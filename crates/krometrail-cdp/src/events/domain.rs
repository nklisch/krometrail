use std::{
    collections::{BTreeSet, HashMap, HashSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use krometrail_core::{
    BrowserDialogType, BrowserEventClass, BrowserEventGapReason, BrowserEventPayload,
    BrowserEventSink, IdSource, MonotonicClock, OpenDialogState, SessionId, SessionOrigin,
    SessionTime, TargetCaptureStatus, TargetId, TargetLifecycle, TargetVisibility,
};
use serde_json::{Value, json};
use tokio::{sync::broadcast, task::JoinHandle};

use crate::{
    compatibility::BrowserEventSupport,
    transport::{CdpTransport, CommandScope, TransportEvents, TransportSessionId},
};

use super::{
    BrowserEventConfig,
    network::{NetworkActivity, NetworkActivityReceiver},
    normalize::{EventNormalizer, SEMANTIC_SOURCE_REGISTRY, SourceDomain},
    pipeline::{EventPipeline, SubmitOutcome, TargetGeneration, TargetIngress},
    privacy,
    signals::{PageSignalKind, PageSignalReceiver},
};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct EventTargetKey {
    target_id: TargetId,
    connection_generation: u64,
    attachment_generation: u64,
    transport_session: TransportSessionId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct EventTargetBinding {
    pub(crate) target_id: TargetId,
    pub(crate) connection_generation: u64,
    pub(crate) attachment_generation: u64,
    pub(crate) transport_session: TransportSessionId,
}

impl EventTargetBinding {
    fn key(&self) -> EventTargetKey {
        EventTargetKey {
            target_id: self.target_id,
            connection_generation: self.connection_generation,
            attachment_generation: self.attachment_generation,
            transport_session: self.transport_session.clone(),
        }
    }

    fn generation(&self) -> TargetGeneration {
        TargetGeneration {
            connection: self.connection_generation,
            attachment: self.attachment_generation,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct EventRestoreOutcome {
    pub(crate) unavailable_classes: Vec<BrowserEventClass>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EventRestoreError {
    MandatoryDomain,
    StaleGeneration,
    TargetLimit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NetworkSetupError {
    StaleGeneration,
    Unsupported,
    Subscription,
    Enable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PageSignalSetupError {
    StaleGeneration,
    Unsupported,
}

pub(crate) struct SessionDomainAuthority {
    config: BrowserEventConfig,
    clock: Arc<dyn MonotonicClock>,
    session_origin: SessionOrigin,
    pipeline: EventPipeline,
    targets: Mutex<HashMap<EventTargetKey, Arc<TargetEventRuntime>>>,
    current: Mutex<HashMap<TargetId, EventTargetKey>>,
}

struct TargetEventRuntime {
    binding: EventTargetBinding,
    support: BrowserEventSupport,
    ingress: Arc<TargetIngress>,
    normalizer: Arc<EventNormalizer>,
    persist_events: bool,
    accepting: AtomicBool,
    installed: Mutex<HashSet<&'static str>>,
    pumps: Mutex<Vec<JoinHandle<()>>>,
    network_sender: broadcast::Sender<NetworkActivity>,
    page_signal_sender: broadcast::Sender<PageSignalKind>,
    // The one piece of reported open-dialog state. It is maintained from the always-installed
    // dialog signal sources, independent of whether semantic events are persisted, because the
    // blocked-observation, handle_dialog, and page-status sites all read it.
    open_dialog: Mutex<Option<BrowserDialogType>>,
    network_enabled: AtomicBool,
    network_setup: tokio::sync::Mutex<()>,
}

impl TargetEventRuntime {
    fn abort(&self) {
        self.accepting.store(false, Ordering::Release);
        for handle in self.pumps.lock().expect("event pump lock").drain(..) {
            handle.abort();
        }
    }

    fn is_current(&self, binding: &EventTargetBinding) -> bool {
        self.binding == *binding && self.accepting.load(Ordering::Acquire)
    }
}

impl SessionDomainAuthority {
    pub(crate) fn new(
        session_id: SessionId,
        session_origin: SessionOrigin,
        clock: Arc<dyn MonotonicClock>,
        ids: Arc<dyn IdSource>,
        sink: Option<Arc<dyn BrowserEventSink>>,
        config: BrowserEventConfig,
    ) -> krometrail_core::Result<Self> {
        let pipeline = EventPipeline::new(
            session_id,
            session_origin,
            Arc::clone(&clock),
            Arc::clone(&ids),
            sink,
            config.clone(),
        )?;
        Ok(Self {
            config: config.clone(),
            clock,
            session_origin,
            pipeline,
            targets: Mutex::new(HashMap::new()),
            current: Mutex::new(HashMap::new()),
        })
    }

    pub(crate) fn session_time(&self) -> krometrail_core::Result<SessionTime> {
        self.session_origin.normalize(self.clock.now())
    }

    pub(crate) async fn restore_target(
        &self,
        binding: EventTargetBinding,
        transport: &dyn CdpTransport,
        support: BrowserEventSupport,
    ) -> Result<EventRestoreOutcome, EventRestoreError> {
        if binding.attachment_generation == 0 {
            return Err(EventRestoreError::StaleGeneration);
        }
        let ingress = self
            .pipeline
            .begin_target(binding.target_id, binding.generation())
            .map_err(|_| EventRestoreError::TargetLimit)?;
        let key = binding.key();
        let (network_sender, _) = broadcast::channel(self.config.network_fanout_capacity.get());
        let (page_signal_sender, _) = broadcast::channel(self.config.network_fanout_capacity.get());
        let runtime = Arc::new(TargetEventRuntime {
            binding: binding.clone(),
            support,
            ingress,
            normalizer: Arc::new(EventNormalizer::new(
                // Event IDs and network request identities share the session-owned source.
                self.pipeline_ids(),
                self.config.request_map_capacity.get(),
            )),
            persist_events: self.pipeline.semantic_enabled(),
            accepting: AtomicBool::new(true),
            installed: Mutex::new(HashSet::new()),
            pumps: Mutex::new(Vec::new()),
            network_sender,
            page_signal_sender,
            open_dialog: Mutex::new(None),
            network_enabled: AtomicBool::new(false),
            network_setup: tokio::sync::Mutex::new(()),
        });
        {
            let mut current = self.current.lock().expect("event current-target lock");
            if let Some(previous_key) = current.insert(binding.target_id, key.clone()) {
                if let Some(previous) = self
                    .targets
                    .lock()
                    .expect("event runtime registry lock")
                    .remove(&previous_key)
                {
                    previous.abort();
                }
            }
            self.targets
                .lock()
                .expect("event runtime registry lock")
                .insert(key, Arc::clone(&runtime));
        }

        let mut unavailable = BTreeClassSet::default();
        for source in SEMANTIC_SOURCE_REGISTRY
            .iter()
            .filter(|source| source.domain != SourceDomain::Network)
        {
            let operation_signal = matches!(
                source.method,
                "Page.lifecycleEvent"
                    | "Page.javascriptDialogOpening"
                    | "Page.javascriptDialogClosed"
            );
            if !self.pipeline.semantic_enabled() && !operation_signal {
                continue;
            }
            let supported = match source.domain {
                SourceDomain::Log => support.log,
                SourceDomain::Page if source.method == "Page.lifecycleEvent" => support.lifecycle,
                _ => true,
            };
            if !supported {
                unavailable.insert(source.class);
                self.source_unavailable(&runtime, source.class);
                continue;
            }
            if install_non_network_source(
                Arc::clone(&runtime),
                transport,
                source.method,
                source.class,
                Arc::clone(&self.clock),
            )
            .await
            .is_err()
            {
                unavailable.insert(source.class);
                self.source_unavailable(&runtime, source.class);
            }
        }
        if self.pipeline.semantic_enabled() {
            if support.network {
                if self
                    .install_network_sources(Arc::clone(&runtime), transport)
                    .await
                    .is_err()
                {
                    unavailable.insert(BrowserEventClass::Network);
                    self.source_unavailable(&runtime, BrowserEventClass::Network);
                }
            } else {
                unavailable.insert(BrowserEventClass::Network);
                self.source_unavailable(&runtime, BrowserEventClass::Network);
            }
        }

        // Every configured stream is installed and draining before this exact
        // restore sequence begins.
        mandatory_command(transport, &binding, "Page.enable", json!({})).await?;
        if support.lifecycle
            && optional_command(
                transport,
                &binding,
                "Page.setLifecycleEventsEnabled",
                json!({"enabled": true}),
            )
            .await
            .is_err()
        {
            unavailable.insert(BrowserEventClass::Lifecycle);
            self.source_unavailable(&runtime, BrowserEventClass::Lifecycle);
        }
        mandatory_command(transport, &binding, "Runtime.enable", json!({})).await?;
        if self.pipeline.semantic_enabled()
            && support.log
            && optional_command(transport, &binding, "Log.enable", json!({}))
                .await
                .is_err()
        {
            unavailable.insert(BrowserEventClass::Console);
            self.source_unavailable(&runtime, BrowserEventClass::Console);
        }
        if self.pipeline.semantic_enabled()
            && support.network
            && self
                .enable_network(Arc::clone(&runtime), transport)
                .await
                .is_err()
        {
            unavailable.insert(BrowserEventClass::Network);
            self.source_unavailable(&runtime, BrowserEventClass::Network);
        }
        mandatory_command(transport, &binding, "Accessibility.enable", json!({})).await?;

        for class in unavailable.iter().copied() {
            self.pipeline.mark_degraded(class);
        }
        if unavailable.is_empty() {
            self.pipeline.mark_operational();
        }
        if let Some(payload) = self.pipeline.collection_state_payload() {
            let _ = runtime.ingress.submit_payload(
                runtime.binding.generation(),
                self.clock.now(),
                None,
                payload,
            );
        }
        Ok(EventRestoreOutcome {
            unavailable_classes: unavailable.into_vec(),
        })
    }

    /// The single reported open-dialog state for a page.
    ///
    /// Returns `Unknown` when no current event runtime has the dialog sources installed, so a
    /// caller never reads absence of evidence as evidence of absence.
    pub(crate) fn open_dialog_state(&self, target_id: TargetId) -> OpenDialogState {
        let key = self
            .current
            .lock()
            .expect("event current-target lock")
            .get(&target_id)
            .cloned();
        let Some(runtime) = key.and_then(|key| {
            self.targets
                .lock()
                .expect("event runtime registry lock")
                .get(&key)
                .map(Arc::clone)
        }) else {
            return OpenDialogState::Unknown;
        };
        if !runtime.accepting.load(Ordering::Acquire) {
            return OpenDialogState::Unknown;
        }
        let installed = runtime.installed.lock().expect("installed source lock");
        if !installed.contains("Page.javascriptDialogOpening")
            || !installed.contains("Page.javascriptDialogClosed")
        {
            return OpenDialogState::Unknown;
        }
        drop(installed);
        runtime
            .open_dialog
            .lock()
            .expect("open dialog lock")
            .map_or(OpenDialogState::None, OpenDialogState::Open)
    }

    pub(crate) fn page_signal(
        &self,
        binding: &EventTargetBinding,
        kind: PageSignalKind,
    ) -> Result<PageSignalReceiver, PageSignalSetupError> {
        let runtime = self
            .runtime(binding)
            .ok_or(PageSignalSetupError::StaleGeneration)?;
        let method = match kind {
            PageSignalKind::Lifecycle => "Page.lifecycleEvent",
            PageSignalKind::DialogOpening => "Page.javascriptDialogOpening",
        };
        if !runtime
            .installed
            .lock()
            .expect("installed source lock")
            .contains(method)
        {
            return Err(PageSignalSetupError::Unsupported);
        }
        Ok(PageSignalReceiver::new(
            kind,
            runtime.page_signal_sender.subscribe(),
        ))
    }

    pub(crate) async fn network_activity(
        &self,
        binding: &EventTargetBinding,
        transport: &dyn CdpTransport,
    ) -> Result<NetworkActivityReceiver, NetworkSetupError> {
        let runtime = self
            .runtime(binding)
            .ok_or(NetworkSetupError::StaleGeneration)?;
        if !runtime.support.network {
            return Err(NetworkSetupError::Unsupported);
        }
        // Subscribe to the bounded fanout before installing/enabling Network so
        // the wait cannot miss activity emitted by the enable command.
        let receiver = NetworkActivityReceiver::new(runtime.network_sender.subscribe());
        self.install_network_sources(Arc::clone(&runtime), transport)
            .await?;
        self.enable_network(runtime, transport).await?;
        Ok(receiver)
    }

    async fn install_network_sources(
        &self,
        runtime: Arc<TargetEventRuntime>,
        transport: &dyn CdpTransport,
    ) -> Result<(), NetworkSetupError> {
        let _guard = runtime.network_setup.lock().await;
        for source in SEMANTIC_SOURCE_REGISTRY
            .iter()
            .filter(|source| source.domain == SourceDomain::Network)
        {
            if runtime
                .installed
                .lock()
                .expect("installed source lock")
                .contains(source.method)
            {
                continue;
            }
            let events = transport
                .subscribe_named(
                    &CommandScope::Session(runtime.binding.transport_session.clone()),
                    source.method,
                )
                .await
                .map_err(|_| NetworkSetupError::Subscription)?;
            runtime
                .installed
                .lock()
                .expect("installed source lock")
                .insert(source.method);
            let runtime_for_pump = Arc::clone(&runtime);
            let method = source.method;
            let clock = Arc::clone(&self.clock);
            runtime
                .pumps
                .lock()
                .expect("event pump lock")
                .push(tokio::spawn(async move {
                    pump_network_source(runtime_for_pump, method, events, clock).await;
                }));
        }
        Ok(())
    }

    async fn enable_network(
        &self,
        runtime: Arc<TargetEventRuntime>,
        transport: &dyn CdpTransport,
    ) -> Result<(), NetworkSetupError> {
        let _guard = runtime.network_setup.lock().await;
        if runtime.network_enabled.load(Ordering::Acquire) {
            return Ok(());
        }
        transport
            .send_raw(
                &CommandScope::Session(runtime.binding.transport_session.clone()),
                "Network.enable",
                json!({}),
            )
            .await
            .map_err(|_| NetworkSetupError::Enable)?;
        runtime.network_enabled.store(true, Ordering::Release);
        Ok(())
    }

    pub(crate) fn observe_target_lifecycle(
        &self,
        target_id: TargetId,
        attachment_generation: u64,
        lifecycle: TargetLifecycle,
    ) {
        if !self.pipeline.semantic_enabled() {
            return;
        }
        let Some(runtime) = self.current_runtime(target_id) else {
            return;
        };
        if runtime.binding.attachment_generation != attachment_generation {
            return;
        }
        let _ = runtime.ingress.submit_payload(
            runtime.binding.generation(),
            self.clock.now(),
            None,
            BrowserEventPayload::TargetLifecycle(krometrail_core::TargetLifecycleEvent::new(
                lifecycle,
            )),
        );
    }

    pub(crate) fn observe_current_target_lifecycle(
        &self,
        target_id: TargetId,
        lifecycle: TargetLifecycle,
    ) {
        let Some(runtime) = self.current_runtime(target_id) else {
            return;
        };
        self.observe_target_lifecycle(target_id, runtime.binding.attachment_generation, lifecycle);
    }

    pub(crate) fn retire_target(&self, target_id: TargetId, attachment_generation: Option<u64>) {
        let Some(runtime) = self.current_runtime(target_id) else {
            return;
        };
        if attachment_generation
            .is_some_and(|generation| generation != runtime.binding.attachment_generation)
        {
            return;
        }
        runtime.abort();
        runtime.ingress.close();
        let key = runtime.binding.key();
        self.targets
            .lock()
            .expect("event runtime registry lock")
            .remove(&key);
        self.current
            .lock()
            .expect("event current-target lock")
            .remove(&target_id);
    }

    pub(crate) fn observe_visibility(
        &self,
        target_id: TargetId,
        attachment_generation: Option<u64>,
        observed_at: Option<krometrail_core::SessionTime>,
        visibility: TargetVisibility,
    ) {
        if !self.pipeline.semantic_enabled() {
            return;
        }
        let Some(runtime) = self.current_runtime(target_id) else {
            return;
        };
        if attachment_generation
            .is_some_and(|generation| generation != runtime.binding.attachment_generation)
        {
            return;
        }
        let observed = match observed_at {
            Some(session_time) => {
                let Some(observed) = self
                    .session_origin
                    .observed()
                    .as_nanos()
                    .checked_add(session_time.as_nanos())
                    .map(krometrail_core::ObservedTime::from_nanos)
                else {
                    return;
                };
                observed
            }
            None => self.clock.now(),
        };
        let _ = runtime.ingress.submit_payload(
            runtime.binding.generation(),
            observed,
            None,
            BrowserEventPayload::TargetVisibility(krometrail_core::TargetVisibilityEvent::new(
                visibility,
            )),
        );
    }

    pub(crate) fn observe_capture_status(&self, status: TargetCaptureStatus) {
        if !self.pipeline.has_sink() {
            return;
        }
        let Some(runtime) = self.current_runtime(status.target_id()) else {
            return;
        };
        if runtime.binding.attachment_generation != status.attachment_generation() {
            return;
        }
        let _ = runtime.ingress.submit_payload(
            runtime.binding.generation(),
            self.clock.now(),
            None,
            BrowserEventPayload::CaptureStatusChanged(status),
        );
    }

    pub(crate) fn suspend_connection(&self, connection_generation: u64) {
        let runtimes: Vec<_> = self
            .targets
            .lock()
            .expect("event runtime registry lock")
            .values()
            .filter(|runtime| runtime.binding.connection_generation == connection_generation)
            .cloned()
            .collect();
        for runtime in runtimes {
            runtime.abort();
            runtime.ingress.suspend_generation(
                runtime.binding.generation(),
                BrowserEventGapReason::ReconnectBoundary,
            );
        }
        self.pipeline.mark_suspended();
    }

    pub(crate) async fn shutdown(&self, deadline: tokio::time::Instant) -> bool {
        let runtimes: Vec<_> = self
            .targets
            .lock()
            .expect("event runtime registry lock")
            .values()
            .cloned()
            .collect();
        for runtime in runtimes {
            runtime.abort();
        }
        self.pipeline.shutdown(deadline).await
    }

    fn source_unavailable(&self, runtime: &TargetEventRuntime, class: BrowserEventClass) {
        self.pipeline.mark_degraded(class);
        let _ = runtime.ingress.record_observed_drop(
            runtime.binding.generation(),
            BrowserEventGapReason::SourceUnavailable,
            Some(class),
            self.clock.now(),
        );
    }

    fn current_runtime(&self, target_id: TargetId) -> Option<Arc<TargetEventRuntime>> {
        let key = self
            .current
            .lock()
            .expect("event current-target lock")
            .get(&target_id)
            .cloned()?;
        self.targets
            .lock()
            .expect("event runtime registry lock")
            .get(&key)
            .cloned()
    }

    fn runtime(&self, binding: &EventTargetBinding) -> Option<Arc<TargetEventRuntime>> {
        let runtime = self
            .targets
            .lock()
            .expect("event runtime registry lock")
            .get(&binding.key())
            .cloned()?;
        runtime.is_current(binding).then_some(runtime)
    }

    fn pipeline_ids(&self) -> Arc<dyn IdSource> {
        self.pipeline.ids()
    }
}

async fn mandatory_command(
    transport: &dyn CdpTransport,
    binding: &EventTargetBinding,
    method: &'static str,
    params: Value,
) -> Result<(), EventRestoreError> {
    transport
        .send_raw(
            &CommandScope::Session(binding.transport_session.clone()),
            method,
            params,
        )
        .await
        .map(|_| ())
        .map_err(|_| EventRestoreError::MandatoryDomain)
}

async fn optional_command(
    transport: &dyn CdpTransport,
    binding: &EventTargetBinding,
    method: &'static str,
    params: Value,
) -> Result<(), ()> {
    transport
        .send_raw(
            &CommandScope::Session(binding.transport_session.clone()),
            method,
            params,
        )
        .await
        .map(|_| ())
        .map_err(|_| ())
}

async fn install_non_network_source(
    runtime: Arc<TargetEventRuntime>,
    transport: &dyn CdpTransport,
    method: &'static str,
    class: BrowserEventClass,
    clock: Arc<dyn MonotonicClock>,
) -> Result<(), ()> {
    if runtime
        .installed
        .lock()
        .expect("installed source lock")
        .contains(method)
    {
        return Ok(());
    }
    let events = transport
        .subscribe_named(
            &CommandScope::Session(runtime.binding.transport_session.clone()),
            method,
        )
        .await
        .map_err(|_| ())?;
    runtime
        .installed
        .lock()
        .expect("installed source lock")
        .insert(method);
    let runtime_for_pump = Arc::clone(&runtime);
    runtime
        .pumps
        .lock()
        .expect("event pump lock")
        .push(tokio::spawn(async move {
            pump_non_network(runtime_for_pump, method, class, events, clock).await;
        }));
    Ok(())
}

async fn pump_non_network(
    runtime: Arc<TargetEventRuntime>,
    method: &'static str,
    class: BrowserEventClass,
    mut events: Box<dyn TransportEvents>,
    clock: Arc<dyn MonotonicClock>,
) {
    while runtime.accepting.load(Ordering::Acquire) {
        match events.next().await {
            Ok(Some(event)) => {
                match method {
                    "Page.javascriptDialogOpening" => {
                        *runtime.open_dialog.lock().expect("open dialog lock") =
                            Some(privacy::dialog_type(event.params.get("type")));
                    }
                    "Page.javascriptDialogClosed" => {
                        runtime.open_dialog.lock().expect("open dialog lock").take();
                    }
                    _ => {}
                }
                let signal = match method {
                    "Page.lifecycleEvent" => Some(PageSignalKind::Lifecycle),
                    "Page.javascriptDialogOpening" => Some(PageSignalKind::DialogOpening),
                    _ => None,
                };
                if let Some(signal) = signal {
                    let _ = runtime.page_signal_sender.send(signal);
                }
                if !runtime.persist_events {
                    continue;
                }
                let observed = clock.now();
                match runtime
                    .normalizer
                    .normalize_non_network(method, &event.params)
                {
                    Ok(normalized) => {
                        for normalized in normalized {
                            if runtime.ingress.submit_payload(
                                runtime.binding.generation(),
                                observed,
                                normalized.source_time,
                                normalized.payload,
                            ) == SubmitOutcome::StaleGeneration
                            {
                                return;
                            }
                        }
                    }
                    Err(error) => {
                        let _ = runtime.ingress.record_observed_drop(
                            runtime.binding.generation(),
                            error.gap_reason(),
                            Some(class),
                            observed,
                        );
                    }
                }
            }
            Ok(None) | Err(_) => {
                if runtime.persist_events {
                    let _ = runtime.ingress.record_observed_drop(
                        runtime.binding.generation(),
                        BrowserEventGapReason::SubscriptionClosed,
                        Some(class),
                        clock.now(),
                    );
                }
                return;
            }
        }
    }
}

async fn pump_network_source(
    runtime: Arc<TargetEventRuntime>,
    method: &'static str,
    mut events: Box<dyn TransportEvents>,
    clock: Arc<dyn MonotonicClock>,
) {
    while runtime.accepting.load(Ordering::Acquire) {
        match events.next().await {
            Ok(Some(event)) => {
                let observed = clock.now();
                match runtime.normalizer.normalize_network(method, &event.params) {
                    Ok(activity) => {
                        // Persistence takes the same normalized activity as waits, but its
                        // bounded enqueue is direct so a lagging wait receiver can never steal
                        // or delay durable evidence.
                        if runtime.persist_events {
                            for normalized in &activity.normalized {
                                if runtime.ingress.submit_payload(
                                    runtime.binding.generation(),
                                    observed,
                                    normalized.source_time.clone(),
                                    normalized.payload.clone(),
                                ) == SubmitOutcome::StaleGeneration
                                {
                                    return;
                                }
                            }
                        }
                        let _ = runtime.network_sender.send(activity);
                    }
                    Err(error) => {
                        let _ = runtime.ingress.record_observed_drop(
                            runtime.binding.generation(),
                            error.gap_reason(),
                            Some(BrowserEventClass::Network),
                            observed,
                        );
                    }
                }
            }
            Ok(None) | Err(_) => {
                if runtime.persist_events {
                    let _ = runtime.ingress.record_observed_drop(
                        runtime.binding.generation(),
                        BrowserEventGapReason::SubscriptionClosed,
                        Some(BrowserEventClass::Network),
                        clock.now(),
                    );
                }
                return;
            }
        }
    }
}

#[derive(Default)]
struct BTreeClassSet(BTreeSet<BrowserEventClass>);

impl BTreeClassSet {
    fn insert(&mut self, class: BrowserEventClass) {
        self.0.insert(class);
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn iter(&self) -> impl Iterator<Item = &BrowserEventClass> {
        self.0.iter()
    }

    fn into_vec(self) -> Vec<BrowserEventClass> {
        self.0.into_iter().collect()
    }
}
