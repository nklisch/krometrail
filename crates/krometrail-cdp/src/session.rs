//! Production browser connector and supervised browser session.
//!
//! The connector composes discovery/launch, the replaceable transport, compatibility probing, and
//! the target reducer. The reducer remains the only writer of session/target state; async tasks
//! only translate transport/process observations into inputs and execute its effects.

use std::{
    collections::{BTreeSet, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use futures_util::{StreamExt, stream::FuturesUnordered};

use krometrail_core::{
    AttachBrowser, BrowserCompatibility, BrowserConnectRequest, BrowserConnector,
    BrowserInstallation, BrowserOperationRequest, BrowserOperationResult, BrowserOperationScope,
    BrowserOwnership, BrowserSessionEvent, BrowserSessionEvents, BrowserSessionPort,
    BrowserSessionState, BrowserStatus, BrowserStopOutcome, ErrorCode, IdSource, IdValue,
    InteractionAnchor, InteractionTiming, KrometrailError, MonotonicClock, NonEmptyText,
    ObservationPart, PageChange, PageOperationOutcome, PageOperationResult, PageSelection,
    PageStatus, PortFuture, ProfileRef, Result, SessionId, SessionOrigin, TargetCaptureStatus,
    TargetVisibility,
};
use serde_json::Value;
use tokio::{
    sync::{Notify, mpsc, oneshot},
    task::JoinHandle,
};
use uuid::Uuid;

use crate::{
    capture::{
        CaptureConfig, CaptureCoordinator, CaptureDependencies, CaptureObserver, CaptureStopReason,
        CaptureTarget,
    },
    compatibility::{CompatibilityProbeError, probe_compatibility_with_target_limit},
    control::{PageControl, navigation::OperationCancellation, operation_error},
    launcher::{
        ChromeLauncher, LaunchError, LauncherConfig, ManagedChromeProcess, ProfileLease,
        SystemChromeLauncher, attach_endpoint,
    },
    targets::{
        ReconnectedSnapshot, ReconnectedTarget, SupervisorConfig, SupervisorEffect,
        SupervisorInput, SupervisorState, TransportTargetInfo, reduce,
        supervisor::SubscriberRegistry,
    },
    transport::{
        CdpTransport, CdpTransportFactory, CommandScope, NamedEvent, TransportClose,
        TransportError, TransportEvents, TransportSessionId,
    },
};

struct AdapterMonotonicClock {
    origin: Instant,
}

impl MonotonicClock for AdapterMonotonicClock {
    fn now(&self) -> krometrail_core::ObservedTime {
        krometrail_core::ObservedTime::from_nanos(
            u64::try_from(self.origin.elapsed().as_nanos()).unwrap_or(u64::MAX),
        )
    }
}

struct AdapterIdSource;

impl IdSource for AdapterIdSource {
    fn next(&self) -> IdValue {
        IdValue::from_uuid(Uuid::new_v4())
    }
}

const TARGET_EVENT_NAMES: &[(&str, TargetEventKind)] = &[
    ("Target.targetCreated", TargetEventKind::Created),
    ("Target.targetInfoChanged", TargetEventKind::InfoChanged),
    ("Target.targetDestroyed", TargetEventKind::Destroyed),
    ("Target.attachedToTarget", TargetEventKind::Attached),
    ("Target.detachedFromTarget", TargetEventKind::Detached),
];

#[derive(Clone, Copy, Debug)]
enum TargetEventKind {
    Created,
    InfoChanged,
    Destroyed,
    Attached,
    Detached,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(crate) enum VisibilityProbeError {
    #[error("visibility probe result did not contain a string value")]
    MissingValue,
    #[error("visibility probe returned an unsupported value")]
    UnsupportedValue,
}

/// Decode only the two result envelopes emitted by the supported cdpkit paths. Do not default
/// unknown values to visible: an unresolved initial probe must not allow a target into Ready.
pub(crate) fn parse_visibility_result(
    value: &Value,
) -> std::result::Result<TargetVisibility, VisibilityProbeError> {
    let raw = value
        .pointer("/result/result/value")
        .and_then(Value::as_str)
        .or_else(|| value.pointer("/result/value").and_then(Value::as_str));
    match raw {
        Some("visible") => Ok(TargetVisibility::Visible),
        Some("hidden") => Ok(TargetVisibility::Hidden),
        Some(_) => Err(VisibilityProbeError::UnsupportedValue),
        None => Err(VisibilityProbeError::MissingValue),
    }
}

/// The production composition root for browser sessions. It deliberately accepts the two adapter
/// seams so deterministic tests can replace launch and transport without changing supervision.
pub struct ProductionBrowserConnector {
    launcher: Arc<dyn ChromeLauncher>,
    transport_factory: Arc<dyn CdpTransportFactory>,
    config: SupervisorConfig,
    clock: Arc<dyn MonotonicClock>,
    ids: Arc<dyn IdSource>,
    capture: Option<CaptureAssembly>,
}

#[derive(Clone)]
struct CaptureAssembly {
    clock: Arc<dyn MonotonicClock>,
    ids: Arc<dyn IdSource>,
    sink: Arc<dyn krometrail_core::RecordingSink>,
    retention: Arc<dyn krometrail_core::RetentionStore>,
    config: CaptureConfig,
}

impl ProductionBrowserConnector {
    pub fn new(
        launcher: Arc<dyn ChromeLauncher>,
        transport_factory: Arc<dyn CdpTransportFactory>,
    ) -> Self {
        Self {
            launcher,
            transport_factory,
            config: SupervisorConfig::default(),
            clock: Arc::new(AdapterMonotonicClock {
                origin: Instant::now(),
            }),
            ids: Arc::new(AdapterIdSource),
            capture: None,
        }
    }

    pub fn with_clock(mut self, clock: Arc<dyn MonotonicClock>) -> Self {
        self.clock = clock;
        self
    }

    pub fn with_ids(mut self, ids: Arc<dyn IdSource>) -> Self {
        self.ids = ids;
        self
    }

    pub fn with_capture(
        mut self,
        clock: Arc<dyn MonotonicClock>,
        ids: Arc<dyn IdSource>,
        sink: Arc<dyn krometrail_core::RecordingSink>,
        retention: Arc<dyn krometrail_core::RetentionStore>,
        config: CaptureConfig,
    ) -> Self {
        self.clock = Arc::clone(&clock);
        self.ids = Arc::clone(&ids);
        self.capture = Some(CaptureAssembly {
            clock,
            ids,
            sink,
            retention,
            config,
        });
        self
    }

    pub fn with_config(mut self, config: SupervisorConfig) -> Self {
        self.config = config;
        self
    }

    pub fn launcher(&self) -> &Arc<dyn ChromeLauncher> {
        &self.launcher
    }
}

impl Default for ProductionBrowserConnector {
    fn default() -> Self {
        Self::new(
            Arc::new(SystemChromeLauncher::new(LauncherConfig::default())),
            Arc::new(
                crate::transport::CdpkitTransportFactory::new()
                    .with_command_timeout(Duration::from_secs(3)),
            ),
        )
    }
}

impl BrowserConnector for ProductionBrowserConnector {
    fn installations(&self) -> PortFuture<'_, Result<Vec<BrowserInstallation>>> {
        // Discovery policy lives in launcher/discovery.rs. In particular, doctor must not launch,
        // attach, reserve a port, or acquire a profile as a side effect of this call.
        Box::pin(async move {
            self.launcher
                .installations()
                .await
                .map_err(|error| launch_error_to_core(&error))
        })
    }

    fn connect(
        &self,
        request: BrowserConnectRequest,
    ) -> PortFuture<'_, Result<Arc<dyn BrowserSessionPort>>> {
        let launcher = Arc::clone(&self.launcher);
        let transport_factory = Arc::clone(&self.transport_factory);
        let config = self.config.clone();
        let capture_assembly = self.capture.clone();
        let control_clock = Arc::clone(&self.clock);
        let ids = Arc::clone(&self.ids);
        Box::pin(async move {
            // Keep a launched browser in its paired Drop guard until transport setup succeeds.
            // Splitting process/profile ownership before this point could release a temporary
            // profile while the child is still alive if connection or compatibility setup fails.
            let (endpoint, ownership, mut launched) = match request {
                BrowserConnectRequest::Launch(request) => {
                    let launched = launcher
                        .launch(&request)
                        .await
                        .map_err(|error| launch_error_to_core(&error))?;
                    let endpoint = launched.endpoint.clone();
                    (endpoint, BrowserOwnership::Managed, Some(launched))
                }
                BrowserConnectRequest::Attach(AttachBrowser { endpoint }) => {
                    let endpoint = attach_endpoint(endpoint)
                        .await
                        .map_err(|error| launch_error_to_core(&error))?;
                    (endpoint, BrowserOwnership::Attached, None)
                }
            };
            let transport = transport_factory
                .connect_endpoint(&endpoint)
                .await
                .map_err(|error| transport_error_to_core(error, false))?;
            let setup = setup_connection(Arc::clone(&transport))
                .await
                .map_err(|error| {
                    tracing::debug!(error = ?error, "browser session setup failed");
                    session_setup_error(error)
                })?;
            let (profile, process, profile_lease) = if let Some(launched) = launched.take() {
                let (_endpoint, profile_lease, process) = launched.into_parts();
                (
                    Some(profile_lease.profile_ref().clone()),
                    Some(Arc::new(Mutex::new(Some(process)))),
                    Some(Arc::new(Mutex::new(Some(profile_lease)))),
                )
            } else {
                (Some(ProfileRef::External), None, None)
            };
            let subscribers = Arc::new(SubscriberRegistry::new(config.subscriber_capacity));
            let (command_tx, command_rx) = mpsc::channel(64);
            let session_id = SessionId::from_uuid(*ids.next().as_uuid());
            let session_origin = SessionOrigin::new(control_clock.now());
            let capture = capture_assembly
                .map(|assembly| {
                    let observer: Arc<dyn CaptureObserver> = Arc::new(SessionCaptureObserver {
                        subscribers: Arc::clone(&subscribers),
                        command_tx: command_tx.clone(),
                    });
                    let coordinator = CaptureCoordinator::new(
                        assembly.config.clone(),
                        CaptureDependencies {
                            clock: Arc::clone(&assembly.clock),
                            ids: Arc::clone(&assembly.ids),
                            sink: Arc::clone(&assembly.sink),
                            retention: Arc::clone(&assembly.retention),
                        },
                        observer,
                    )
                    .map_err(|_| {
                        stable_error(ErrorCode::InvalidInput, "capture configuration is invalid")
                    })?;
                    Ok::<_, KrometrailError>(Arc::new(CaptureRuntime {
                        coordinator: Arc::new(coordinator),
                        clock: assembly.clock,
                        session_id,
                        session_origin,
                        retention: assembly.retention,
                        shutdown_timeout: assembly.config.shutdown_timeout,
                    }))
                })
                .transpose()?;
            let compatibility = setup.compatibility.clone();
            let mut state = SupervisorState::new(compatibility.clone());
            let initial = reduce(
                state,
                SupervisorInput::InitialTargets(setup.targets.clone()),
            )?;
            state = initial.state;
            let connection = setup;
            apply_effects(
                &mut state,
                initial.effects,
                Arc::clone(&connection.transport),
                Arc::clone(&subscribers),
                capture.clone(),
                None,
            )
            .await?;
            // Initial reconciliation is complete only after all attach effects and visibility probes.
            // Ready is reduced like every later lifecycle transition, so capture can only start from
            // the committed Ready state and the exact attached/visible target generation.
            let ready = reduce(state, SupervisorInput::InitialReconciliationCompleted)?;
            state = ready.state;
            apply_effects(
                &mut state,
                ready.effects,
                Arc::clone(&connection.transport),
                Arc::clone(&subscribers),
                capture.clone(),
                None,
            )
            .await?;
            let process_death = Arc::new(ProcessDeathSignal::default());
            let shared = Arc::new(SessionShared {
                compatibility,
                ownership,
                profile: profile.unwrap_or(ProfileRef::External),
                state: Mutex::new(state.clone()),
                subscribers,
                command_tx,
                session_id,
                session_origin,
                capture: capture.clone(),
                operation_cancellation: OperationCancellation::default(),
                stop_result: Mutex::new(None),
            });
            let task_shared = Arc::clone(&shared);
            let endpoint = Arc::new(endpoint);
            let page_control = PageControl::new(control_clock, ids, session_id, session_origin);
            let task = tokio::spawn(run_supervisor(
                task_shared,
                state,
                Some(connection),
                page_control,
                SupervisorRuntime {
                    endpoint,
                    factory: transport_factory,
                    process,
                    profile: profile_lease,
                    config,
                    process_death,
                    capture_timeout: capture
                        .as_ref()
                        .map_or(Duration::from_secs(5), |runtime| runtime.shutdown_timeout),
                },
                command_rx,
            ));
            let session = ProductionSession {
                shared,
                task: Mutex::new(Some(task)),
            };
            Ok(Arc::new(session) as Arc<dyn BrowserSessionPort>)
        })
    }
}

struct CaptureRuntime {
    coordinator: Arc<CaptureCoordinator>,
    clock: Arc<dyn MonotonicClock>,
    session_id: SessionId,
    session_origin: SessionOrigin,
    retention: Arc<dyn krometrail_core::RetentionStore>,
    shutdown_timeout: Duration,
}

struct SessionCaptureObserver {
    subscribers: Arc<SubscriberRegistry>,
    command_tx: mpsc::Sender<SupervisorCommand>,
}

impl CaptureObserver for SessionCaptureObserver {
    fn status_changed(&self, status: TargetCaptureStatus) {
        self.subscribers
            .publish(BrowserSessionEvent::CaptureStateChanged { status });
    }

    fn gap_declared(&self, gap: krometrail_core::CaptureGap) {
        self.subscribers
            .publish(BrowserSessionEvent::CaptureGapDeclared { gap });
    }

    fn visibility_changed(
        &self,
        target_id: krometrail_core::TargetId,
        visibility: TargetVisibility,
    ) {
        let _ = self.command_tx.try_send(SupervisorCommand::Input(
            SupervisorInput::CaptureVisibilityChanged {
                target_id,
                visibility,
            },
        ));
    }
}

struct SessionShared {
    compatibility: BrowserCompatibility,
    ownership: BrowserOwnership,
    profile: ProfileRef,
    state: Mutex<SupervisorState>,
    subscribers: Arc<SubscriberRegistry>,
    command_tx: mpsc::Sender<SupervisorCommand>,
    session_id: SessionId,
    session_origin: SessionOrigin,
    capture: Option<Arc<CaptureRuntime>>,
    operation_cancellation: OperationCancellation,
    stop_result: Mutex<Option<Result<BrowserStopOutcome>>>,
}

#[derive(Debug)]
enum SupervisorCommand {
    Input(SupervisorInput),
    Execute(
        BrowserOperationRequest,
        oneshot::Sender<Result<BrowserOperationResult>>,
    ),
    Stop(oneshot::Sender<Result<BrowserStopOutcome>>),
}

struct ProductionSession {
    shared: Arc<SessionShared>,
    task: Mutex<Option<JoinHandle<()>>>,
}

impl BrowserSessionPort for ProductionSession {
    fn session_origin(&self) -> SessionOrigin {
        self.shared.session_origin
    }

    fn status(&self) -> PortFuture<'_, Result<BrowserStatus>> {
        let (session_state, compatibility, selected_target_id, pages) = {
            let state = self.shared.state.lock().expect("session state lock");
            let selected_target_id = if state.session_state == BrowserSessionState::Ended {
                None
            } else {
                state
                    .selected_target_key
                    .as_deref()
                    .and_then(|key| state.targets_by_key.get(key))
                    .map(|target| target.target.target.id())
            };
            let pages = state
                .targets()
                .into_iter()
                .map(|target| PageStatus {
                    selected: Some(target.target.id()) == selected_target_id,
                    target,
                })
                .collect();
            (
                state.session_state,
                state.compatibility.clone(),
                selected_target_id,
                pages,
            )
        };
        let session_id = self.shared.session_id;
        let ownership = self.shared.ownership;
        let profile = self.shared.profile.clone();
        let capture = self.shared.capture.clone();
        Box::pin(async move {
            let capture_statuses = capture
                .as_ref()
                .map_or_else(Vec::new, |runtime| runtime.coordinator.statuses());
            let retention = match capture.as_ref() {
                Some(runtime) => runtime.retention.status().await?,
                None => krometrail_core::RetentionStatus::empty(
                    krometrail_core::DiskBudgetBytes::default(),
                ),
            };
            BrowserStatus::new(
                session_id,
                session_state,
                ownership,
                profile,
                compatibility,
                selected_target_id,
                pages,
                capture_statuses,
                retention,
            )
        })
    }

    fn subscribe(&self) -> PortFuture<'_, Result<Box<dyn BrowserSessionEvents>>> {
        let events = self.shared.subscribers.subscribe();
        Box::pin(std::future::ready(Ok(events)))
    }

    fn execute(
        &self,
        request: BrowserOperationRequest,
    ) -> PortFuture<'_, Result<BrowserOperationResult>> {
        let shared = Arc::clone(&self.shared);
        Box::pin(async move {
            let target_id = direct_request_target(&request);
            let (sender, receiver) = oneshot::channel();
            shared
                .command_tx
                .send(SupervisorCommand::Execute(request, sender))
                .await
                .map_err(|_| {
                    request_operation_error(
                        ErrorCode::Cancelled,
                        target_id,
                        "browser supervision task ended",
                    )
                })?;
            receiver.await.map_err(|_| {
                request_operation_error(
                    ErrorCode::Cancelled,
                    target_id,
                    "browser operation ended without a result",
                )
            })?
        })
    }

    fn stop(&self) -> PortFuture<'_, Result<BrowserStopOutcome>> {
        let shared = Arc::clone(&self.shared);
        Box::pin(async move {
            if let Some(result) = shared.stop_result.lock().expect("stop result lock").clone() {
                return result;
            }
            shared.operation_cancellation.stop();
            let (sender, receiver) = oneshot::channel();
            shared
                .command_tx
                .send(SupervisorCommand::Stop(sender))
                .await
                .map_err(|_| {
                    stable_error(ErrorCode::Cancelled, "browser supervision task ended")
                })?;
            let result = receiver.await.map_err(|_| {
                stable_error(
                    ErrorCode::ShutdownIncomplete,
                    "browser shutdown did not report an outcome",
                )
            })?;
            *shared.stop_result.lock().expect("stop result lock") = Some(result.clone());
            result
        })
    }
}

impl Drop for ProductionSession {
    fn drop(&mut self) {
        // Detach the task rather than aborting it. The task owns the asynchronous shutdown path;
        // the process/profile guards remain alive until that path completes, while their Drop
        // implementations still provide cancellation-safe last-resort cleanup if the runtime ends.
        self.shared.operation_cancellation.stop();
        let cancel_queued = self
            .shared
            .command_tx
            .try_send(SupervisorCommand::Input(SupervisorInput::Cancelled))
            .is_ok();
        if let Some(task) = self.task.lock().expect("session task lock").take() {
            if !cancel_queued {
                // A saturated/closed command channel cannot deliver cancellation. Abort only in
                // that case so the task-owned process/profile guards perform last-resort cleanup.
                task.abort();
            }
            // Otherwise detach and let the bounded async shutdown finish.
        }
    }
}

struct ConnectionResources {
    transport: Arc<dyn CdpTransport>,
    subscriptions: Vec<(TargetEventKind, Box<dyn TransportEvents>)>,
    targets: Vec<TransportTargetInfo>,
    compatibility: BrowserCompatibility,
    pump_handles: Vec<JoinHandle<()>>,
}

impl ConnectionResources {
    fn restart_pumps(
        &mut self,
        sender: mpsc::Sender<SupervisorCommand>,
        generation: u64,
        cancellation: OperationCancellation,
    ) {
        self.abort_pumps();
        let subscriptions = std::mem::take(&mut self.subscriptions);
        self.pump_handles = subscriptions
            .into_iter()
            .map(|(kind, events)| {
                tokio::spawn(pump_events(
                    kind,
                    events,
                    sender.clone(),
                    generation,
                    cancellation.clone(),
                ))
            })
            .collect();
    }

    fn abort_pumps(&mut self) {
        for handle in self.pump_handles.drain(..) {
            handle.abort();
        }
    }
}

impl Drop for ConnectionResources {
    fn drop(&mut self) {
        self.abort_pumps();
    }
}

async fn setup_connection(
    transport: Arc<dyn CdpTransport>,
) -> std::result::Result<ConnectionResources, CompatibilityProbeError> {
    setup_connection_with_target_limit(transport, usize::MAX).await
}

async fn setup_connection_with_target_limit(
    transport: Arc<dyn CdpTransport>,
    target_limit: usize,
) -> std::result::Result<ConnectionResources, CompatibilityProbeError> {
    let mut subscriptions = Vec::with_capacity(TARGET_EVENT_NAMES.len());
    // This happens before any discovery/auto-attach command. Event channels can therefore buffer
    // creation and attachment races while the initial snapshot is being fetched.
    for (name, kind) in TARGET_EVENT_NAMES {
        let events = transport
            .subscribe_named(&CommandScope::Browser, name)
            .await
            .map_err(CompatibilityProbeError::Transport)?;
        subscriptions.push((*kind, events));
    }
    let compatibility =
        probe_compatibility_with_target_limit(transport.as_ref(), target_limit).await?;
    transport
        .send_raw(
            &CommandScope::Browser,
            "Target.setDiscoverTargets",
            serde_json::json!({"discover": true}),
        )
        .await
        .map_err(CompatibilityProbeError::Transport)?;
    transport
		.send_raw(
			&CommandScope::Browser,
			"Target.setAutoAttach",
			serde_json::json!({"autoAttach": true, "waitForDebuggerOnStart": false, "flatten": true}),
		)
		.await
		.map_err(CompatibilityProbeError::Transport)?;
    let targets = transport
        .send_raw(
            &CommandScope::Browser,
            "Target.getTargets",
            Value::Object(Default::default()),
        )
        .await
        .map_err(CompatibilityProbeError::Transport)
        .and_then(|value| {
            parse_target_list(&value).ok_or(CompatibilityProbeError::InvalidIdentity)
        })?;
    Ok(ConnectionResources {
        transport,
        subscriptions,
        targets,
        compatibility,
        pump_handles: Vec::new(),
    })
}

async fn apply_effects(
    state: &mut SupervisorState,
    effects: Vec<SupervisorEffect>,
    transport: Arc<dyn CdpTransport>,
    subscribers: Arc<SubscriberRegistry>,
    capture: Option<Arc<CaptureRuntime>>,
    shutdown_deadline: Option<ShutdownDeadline>,
) -> Result<()> {
    let mut queue = VecDeque::from(effects);
    while let Some(effect) = queue.pop_front() {
        match effect {
            SupervisorEffect::Publish(event) => subscribers.publish(event),
            SupervisorEffect::Attach { target_key } => {
                let result = transport
                    .send_raw(
                        &CommandScope::Browser,
                        "Target.attachToTarget",
                        serde_json::json!({"targetId": target_key, "flatten": true}),
                    )
                    .await;
                let input = result
                    .ok()
                    .and_then(|value| {
                        value
                            .get("sessionId")
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                    })
                    .and_then(|session| TransportSessionId::new(session).ok())
                    .map(|session| SupervisorInput::Attached {
                        target_key: target_key.clone(),
                        session,
                    })
                    .unwrap_or(SupervisorInput::TargetAttachFailed { target_key });
                let compatibility = state.compatibility.clone();
                let previous = std::mem::replace(state, SupervisorState::new(compatibility));
                let reduction = reduce(previous, input)?;
                *state = reduction.state;
                queue.extend(reduction.effects);
            }
            SupervisorEffect::Detach { session } => {
                let _ = transport
                    .send_raw(
                        &CommandScope::Browser,
                        "Target.detachFromTarget",
                        serde_json::json!({"sessionId": session.as_str()}),
                    )
                    .await;
            }
            SupervisorEffect::ProbeInitialVisibility {
                target_key,
                session,
            } => {
                let input = match transport
					.send_raw(
						&CommandScope::Session(session),
						"Runtime.evaluate",
						serde_json::json!({"expression": "document.visibilityState", "returnByValue": true}),
					)
					.await
				{
					Ok(value) => parse_visibility_result(&value)
						.map(|visibility| SupervisorInput::VisibilityChanged {
							target_key: target_key.clone(),
							visibility,
						})
						.unwrap_or_else(|_| SupervisorInput::InitialVisibilityProbeFailed {
							target_key: target_key.clone(),
						}),
					Err(_) => SupervisorInput::InitialVisibilityProbeFailed {
						target_key: target_key.clone(),
					},
				};
                let compatibility = state.compatibility.clone();
                let previous = std::mem::replace(state, SupervisorState::new(compatibility));
                let reduction = reduce(previous, input)?;
                *state = reduction.state;
                queue.extend(reduction.effects);
            }
            SupervisorEffect::StartCapture { context }
            | SupervisorEffect::ResumeCapture { context } => {
                if let Some(capture) = capture.as_ref() {
                    let target = CaptureTarget {
                        session_id: capture.session_id,
                        session_origin: capture.session_origin,
                        target_id: context.target_id,
                        connection_generation: context.connection_generation,
                        attachment_generation: context.attachment_generation,
                        transport_session: context.transport_session,
                    };
                    if capture
                        .coordinator
                        .start_target(target, Arc::clone(&transport))
                        .await
                        .is_err()
                    {
                        let target_key = state
                            .targets_by_key
                            .iter()
                            .find(|(_, target)| target.target.target.id() == context.target_id)
                            .map(|(target_key, _)| target_key.clone());
                        if let Some(target_key) = target_key {
                            let compatibility = state.compatibility.clone();
                            let previous =
                                std::mem::replace(state, SupervisorState::new(compatibility));
                            let reduction = reduce(
                                previous,
                                SupervisorInput::CaptureStartFailed { target_key },
                            )?;
                            *state = reduction.state;
                            queue.extend(reduction.effects);
                        }
                    }
                }
            }
            SupervisorEffect::SuspendCapture { context } => {
                if let Some(capture) = capture.as_ref() {
                    let target = CaptureTarget {
                        session_id: capture.session_id,
                        session_origin: capture.session_origin,
                        target_id: context.target_id,
                        connection_generation: context.connection_generation,
                        attachment_generation: context.attachment_generation,
                        transport_session: context.transport_session,
                    };
                    let at = capture
                        .session_origin
                        .normalize(capture.clock.now())
                        .unwrap_or(krometrail_core::SessionTime::ZERO);
                    capture.coordinator.suspend_target(&target, at).await;
                }
            }
            SupervisorEffect::StopCapture { context } => {
                if let Some(capture) = capture.as_ref() {
                    let target = CaptureTarget {
                        session_id: capture.session_id,
                        session_origin: capture.session_origin,
                        target_id: context.target_id,
                        connection_generation: context.connection_generation,
                        attachment_generation: context.attachment_generation,
                        transport_session: context.transport_session,
                    };
                    let deadline = shutdown_deadline
                        .as_ref()
                        .map_or_else(
                            || ShutdownDeadline::new(capture.shutdown_timeout),
                            Clone::clone,
                        )
                        .instant();
                    let reason = state
                        .targets_by_key
                        .values()
                        .find(|target| target.target.target.id() == context.target_id)
                        .map(|target| match target.target.lifecycle {
                            krometrail_core::TargetLifecycle::Closed => {
                                CaptureStopReason::TargetClosed
                            }
                            krometrail_core::TargetLifecycle::Failed => {
                                CaptureStopReason::TargetFailed
                            }
                            _ => CaptureStopReason::TargetDetached,
                        })
                        .unwrap_or(CaptureStopReason::TargetDetached);
                    let _ = capture
                        .coordinator
                        .stop_target(&target, reason, deadline)
                        .await;
                }
            }
            SupervisorEffect::BeginReconnect => {}
            SupervisorEffect::Shutdown { cause: _ } => {
                // The outer supervisor owns the aggregate shutdown sequencing. Capture effects
                // above have already fenced acceptance before this marker is handled.
            }
        }
    }
    Ok(())
}

struct SupervisorRuntime {
    endpoint: Arc<crate::LocalCdpEndpoint>,
    factory: Arc<dyn CdpTransportFactory>,
    process: Option<Arc<Mutex<Option<ManagedChromeProcess>>>>,
    profile: Option<Arc<Mutex<Option<ProfileLease>>>>,
    config: SupervisorConfig,
    process_death: Arc<ProcessDeathSignal>,
    capture_timeout: Duration,
}

#[derive(Default)]
struct ProcessDeathSignal {
    exit: Mutex<Option<crate::launcher::SanitizedProcessExit>>,
    notify: Notify,
}

impl ProcessDeathSignal {
    fn record(&self, exit: crate::launcher::SanitizedProcessExit) {
        *self.exit.lock().expect("process death lock") = Some(exit);
        self.notify.notify_waiters();
    }

    async fn wait(&self) -> crate::launcher::SanitizedProcessExit {
        loop {
            if let Some(exit) = self.exit.lock().expect("process death lock").take() {
                return exit;
            }
            let notified = self.notify.notified();
            if let Some(exit) = self.exit.lock().expect("process death lock").take() {
                return exit;
            }
            notified.await;
        }
    }
}

async fn run_supervisor(
    shared: Arc<SessionShared>,
    mut state: SupervisorState,
    mut connection: Option<ConnectionResources>,
    mut page_control: PageControl,
    runtime: SupervisorRuntime,
    mut commands: mpsc::Receiver<SupervisorCommand>,
) {
    if let Some(connection) = connection.as_mut() {
        let sender = shared.command_tx.clone();
        connection.restart_pumps(
            sender,
            state.connection_generation,
            shared.operation_cancellation.clone(),
        );
    }
    if let Some(process) = runtime.process.clone() {
        tokio::spawn(watch_process(
            process,
            shared.command_tx.clone(),
            Arc::clone(&runtime.process_death),
        ));
    }
    while let Some(command) = commands.recv().await {
        match command {
            SupervisorCommand::Input(input) => {
                // Keep the last committed state if a late transport event violates a lifecycle
                // invariant; dropping it here would erase every target before reconnect can restore
                // the exact browser keys.
                let previous = std::mem::replace(
                    &mut state,
                    SupervisorState::new(shared.compatibility.clone()),
                );
                match reduce(previous.clone(), input) {
                    Ok(reduction) => {
                        state = reduction.state;
                        *shared.state.lock().expect("session state lock") = state.clone();
                        let should_reconnect = reduction
                            .effects
                            .iter()
                            .any(|effect| matches!(effect, SupervisorEffect::BeginReconnect));
                        let shutdown = reduction.effects.iter().find_map(|effect| match effect {
                            SupervisorEffect::Shutdown { cause } => Some(*cause),
                            _ => None,
                        });
                        let shutdown_deadline =
                            shutdown.map(|_| ShutdownDeadline::new(runtime.capture_timeout));
                        if let Some(connection) = connection.as_ref() {
                            let _ = apply_effects(
                                &mut state,
                                reduction.effects,
                                Arc::clone(&connection.transport),
                                Arc::clone(&shared.subscribers),
                                shared.capture.clone(),
                                shutdown_deadline.clone(),
                            )
                            .await;
                        }
                        *shared.state.lock().expect("session state lock") = state.clone();
                        if should_reconnect {
                            let outcome = reconnect_loop_transactional(
                                &shared,
                                &mut state,
                                &mut connection,
                                &runtime,
                                &mut commands,
                            )
                            .await;
                            if outcome {
                                break;
                            }
                        }
                        if let Some(cause) = shutdown {
                            let _ = perform_shutdown(
                                &mut connection,
                                &runtime.process,
                                &runtime.profile,
                                &state,
                                ShutdownPlan {
                                    cause,
                                    ownership: shared.ownership,
                                    capture: shared.capture.clone(),
                                    deadline: shutdown_deadline
                                        .expect("shutdown cause has an aggregate deadline"),
                                    flush_capture: !matches!(
                                        cause,
                                        crate::targets::ShutdownCause::ReconnectExhausted
                                    ),
                                },
                            )
                            .await;
                            finish_state(&shared, &mut state);
                            break;
                        }
                    }
                    Err(error) => {
                        tracing::debug!(error = ?error, "browser supervisor input rejected");
                        state = previous;
                    }
                }
            }
            SupervisorCommand::Execute(request, sender) => {
                let target_id = direct_request_target(&request);
                let result = match connection.as_ref() {
                    Some(connection) => {
                        execute_operation(
                            &mut page_control,
                            &mut state,
                            Arc::clone(&connection.transport),
                            &shared,
                            request,
                        )
                        .await
                    }
                    None => Err(request_operation_error(
                        ErrorCode::BrowserDisconnected,
                        target_id,
                        "browser transport is unavailable",
                    )),
                };
                let _ = sender.send(result);
            }
            SupervisorCommand::Stop(sender) => {
                let reduction = reduce(
                    std::mem::replace(
                        &mut state,
                        SupervisorState::new(shared.compatibility.clone()),
                    ),
                    SupervisorInput::StopRequested,
                );
                match reduction {
                    Ok(reduction) => {
                        state = reduction.state;
                        let shutdown_deadline = ShutdownDeadline::new(runtime.capture_timeout);
                        if let Some(connection) = connection.as_ref() {
                            let _ = apply_effects(
                                &mut state,
                                reduction.effects,
                                Arc::clone(&connection.transport),
                                Arc::clone(&shared.subscribers),
                                shared.capture.clone(),
                                Some(shutdown_deadline.clone()),
                            )
                            .await;
                        }
                        let result = perform_shutdown(
                            &mut connection,
                            &runtime.process,
                            &runtime.profile,
                            &state,
                            ShutdownPlan {
                                cause: crate::targets::ShutdownCause::StopRequested,
                                ownership: shared.ownership,
                                capture: shared.capture.clone(),
                                deadline: shutdown_deadline,
                                flush_capture: true,
                            },
                        )
                        .await;
                        let outcome = result.map(|_| {
                            if shared.ownership == BrowserOwnership::Managed {
                                BrowserStopOutcome::ManagedBrowserClosed
                            } else {
                                BrowserStopOutcome::Detached
                            }
                        });
                        *shared.stop_result.lock().expect("stop result lock") =
                            Some(outcome.clone());
                        finish_state(&shared, &mut state);
                        let _ = sender.send(outcome);
                    }
                    Err(error) => {
                        let _ = sender.send(Err(error));
                    }
                }
                break;
            }
        }
    }
    // Dropping the connection aborts event pumps. Process/profile Arcs remain owned by this task
    // and are cleaned by the explicit shutdown path or by their guards on cancellation.
}

async fn execute_operation(
    page_control: &mut PageControl,
    state: &mut SupervisorState,
    transport: Arc<dyn CdpTransport>,
    shared: &Arc<SessionShared>,
    request: BrowserOperationRequest,
) -> Result<BrowserOperationResult> {
    if request.kind().is_interaction() {
        return page_control
            .execute_interaction_request(
                transport.as_ref(),
                state,
                request,
                &shared.operation_cancellation,
            )
            .await;
    }
    match request {
        BrowserOperationRequest::CreatePage(request) => {
            let started_at = page_control.session_time()?;
            let dispatched_at = page_control.session_time()?;
            let response = transport
                .send_raw(
                    &CommandScope::Browser,
                    "Target.createTarget",
                    serde_json::json!({"url": request.initial_url.as_ref().map_or("about:blank", |url| url.as_str())}),
                )
                .await
                .map_err(|error| transport_error_to_core(error, true))?;
            let target_key = response
                .get("targetId")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    stable_error(
                        ErrorCode::TargetFailed,
                        "browser returned an invalid created target",
                    )
                })?
                .to_owned();
            let info = transport
                .send_raw(
                    &CommandScope::Browser,
                    "Target.getTargetInfo",
                    serde_json::json!({"targetId": target_key}),
                )
                .await
                .map_err(|error| transport_error_to_core(error, true))?
                .get("targetInfo")
                .and_then(parse_target_info)
                .ok_or_else(|| {
                    stable_error(
                        ErrorCode::TargetFailed,
                        "created browser target could not be reconciled",
                    )
                })?;
            // The browser has supplied a concrete target before the interaction is allocated.
            // Reducing first gives the anchor its real TargetId; every fallible attach/select step
            // after allocation can therefore return an honest anchored failure instead of a plain
            // browser-scoped error or a fabricated target identity.
            let reduction = reduce(state.clone(), SupervisorInput::TargetCreated(info))?;
            *state = reduction.state;
            let target_id = state
                .targets_by_key
                .get(&target_key)
                .map(|target| target.target.target.id())
                .ok_or_else(|| {
                    stable_error(
                        ErrorCode::TargetFailed,
                        "created browser target could not be reconciled",
                    )
                })?;
            let interaction_id = page_control.next_interaction_id();
            let attach = apply_effects(
                state,
                reduction.effects,
                Arc::clone(&transport),
                Arc::clone(&shared.subscribers),
                shared.capture.clone(),
                None,
            )
            .await;
            *shared.state.lock().expect("session state lock") = state.clone();
            if attach.is_err()
                || state
                    .resolve_selection(PageSelection::Target(target_id))
                    .is_err()
            {
                return page_failure_result(
                    page_control,
                    target_id,
                    krometrail_core::BrowserOperationKind::CreatePage,
                    interaction_id,
                    started_at,
                    dispatched_at,
                    operation_error(
                        ErrorCode::TargetFailed,
                        target_id,
                        "created browser target could not be attached",
                    ),
                );
            }
            let activation = transport
                .send_raw(
                    &CommandScope::Browser,
                    "Target.activateTarget",
                    serde_json::json!({"targetId": target_key}),
                )
                .await;
            if let Err(error) = activation {
                return page_failure_result(
                    page_control,
                    target_id,
                    krometrail_core::BrowserOperationKind::CreatePage,
                    interaction_id,
                    started_at,
                    dispatched_at,
                    transport_page_error(error, ErrorCode::TargetFailed, target_id),
                );
            }
            if commit_supervisor_input(
                state,
                SupervisorInput::SelectTarget { target_key },
                Arc::clone(&transport),
                shared,
            )
            .await
            .is_err()
            {
                return page_failure_result(
                    page_control,
                    target_id,
                    krometrail_core::BrowserOperationKind::CreatePage,
                    interaction_id,
                    started_at,
                    dispatched_at,
                    operation_error(
                        ErrorCode::TargetFailed,
                        target_id,
                        "created browser target could not be selected",
                    ),
                );
            }
            page_success_result(
                page_control,
                transport.as_ref(),
                state,
                target_id,
                krometrail_core::BrowserOperationKind::CreatePage,
                interaction_id,
                started_at,
                dispatched_at,
                PageChange::Created { target_id },
                PageSelection::Target(target_id),
                &shared.operation_cancellation,
            )
            .await
            .map(|result| BrowserOperationResult::CreatePage(Box::new(result)))
        }
        BrowserOperationRequest::SelectPage(request) => {
            let target = state.resolve_selection(PageSelection::Target(request.target_id))?;
            let target_key = target.target.target.browser_target_key().to_owned();
            let target_id = target.target.target.id();
            let previous = state
                .selected_target()
                .map(|target| target.target.target.id());
            let started_at = page_control.session_time()?;
            let interaction_id = page_control.next_interaction_id();
            let dispatched_at = page_control.session_time()?;
            if let Err(error) = transport
                .send_raw(
                    &CommandScope::Browser,
                    "Target.activateTarget",
                    serde_json::json!({"targetId": target_key}),
                )
                .await
            {
                return page_failure_result(
                    page_control,
                    target_id,
                    krometrail_core::BrowserOperationKind::SelectPage,
                    interaction_id,
                    started_at,
                    dispatched_at,
                    transport_page_error(error, ErrorCode::TargetFailed, target_id),
                );
            }
            commit_supervisor_input(
                state,
                SupervisorInput::SelectTarget { target_key },
                Arc::clone(&transport),
                shared,
            )
            .await?;
            page_success_result(
                page_control,
                transport.as_ref(),
                state,
                target_id,
                krometrail_core::BrowserOperationKind::SelectPage,
                interaction_id,
                started_at,
                dispatched_at,
                PageChange::Selected {
                    previous,
                    selected: target_id,
                },
                PageSelection::Target(target_id),
                &shared.operation_cancellation,
            )
            .await
            .map(|result| BrowserOperationResult::SelectPage(Box::new(result)))
        }
        BrowserOperationRequest::NavigatePage(request) => {
            page_control
                .navigate(
                    transport.as_ref(),
                    state,
                    request,
                    &shared.operation_cancellation,
                )
                .await
        }
        BrowserOperationRequest::ReloadPage(request) => {
            page_control
                .reload(
                    transport.as_ref(),
                    state,
                    request,
                    &shared.operation_cancellation,
                )
                .await
        }
        BrowserOperationRequest::GoBack(request) => {
            page_control
                .go_back(
                    transport.as_ref(),
                    state,
                    request,
                    &shared.operation_cancellation,
                )
                .await
        }
        BrowserOperationRequest::GoForward(request) => {
            page_control
                .go_forward(
                    transport.as_ref(),
                    state,
                    request,
                    &shared.operation_cancellation,
                )
                .await
        }
        BrowserOperationRequest::ClosePage(request) => {
            let target = state.resolve_selection(request.target)?;
            let target_key = target.target.target.browser_target_key().to_owned();
            let target_id = target.target.target.id();
            let started_at = page_control.session_time()?;
            let interaction_id = page_control.next_interaction_id();
            let dispatched_at = page_control.session_time()?;
            let response = transport
                .send_raw(
                    &CommandScope::Browser,
                    "Target.closeTarget",
                    serde_json::json!({"targetId": target_key}),
                )
                .await;
            let success = match response {
                Ok(response) => response.get("success").and_then(Value::as_bool) == Some(true),
                Err(error) => {
                    return page_failure_result(
                        page_control,
                        target_id,
                        krometrail_core::BrowserOperationKind::ClosePage,
                        interaction_id,
                        started_at,
                        dispatched_at,
                        transport_page_error(error, ErrorCode::TargetFailed, target_id),
                    );
                }
            };
            if !success {
                return page_failure_result(
                    page_control,
                    target_id,
                    krometrail_core::BrowserOperationKind::ClosePage,
                    interaction_id,
                    started_at,
                    dispatched_at,
                    operation_error(
                        ErrorCode::TargetFailed,
                        target_id,
                        "browser did not confirm page closure",
                    ),
                );
            }
            commit_supervisor_input(
                state,
                SupervisorInput::TargetDestroyed { target_key },
                Arc::clone(&transport),
                shared,
            )
            .await?;
            page_control.invalidate_target_snapshot(target_id);
            let selected = state
                .selected_target()
                .map(|target| target.target.target.id());
            let (observation, interruption) = match selected {
                Some(selected) => {
                    let observed = page_control
                        .observe_after_operation(
                            transport.as_ref(),
                            state,
                            PageSelection::Target(selected),
                            &shared.operation_cancellation,
                        )
                        .await?;
                    (observed.observation, observed.interruption)
                }
                None => (
                    ObservationPart::Unavailable(KrometrailError::new(
                        ErrorCode::NotFound,
                        NonEmptyText::new("no browser page remains selected after closure")
                            .unwrap(),
                    )),
                    None,
                ),
            };
            let outcome = interruption.map_or_else(
                || {
                    PageOperationOutcome::Succeeded(PageChange::Closed {
                        closed: target_id,
                        selected,
                    })
                },
                PageOperationOutcome::Failed,
            );
            let result = build_page_result(
                page_control,
                target_id,
                krometrail_core::BrowserOperationKind::ClosePage,
                interaction_id,
                started_at,
                dispatched_at,
                outcome,
                observation,
            )?;
            Ok(BrowserOperationResult::ClosePage(Box::new(result)))
        }
        request => {
            page_control
                .execute(transport.as_ref(), state, request)
                .await
        }
    }
}

async fn commit_supervisor_input(
    state: &mut SupervisorState,
    input: SupervisorInput,
    transport: Arc<dyn CdpTransport>,
    shared: &Arc<SessionShared>,
) -> Result<()> {
    let previous = std::mem::replace(state, SupervisorState::new(shared.compatibility.clone()));
    let reduction = reduce(previous, input)?;
    *state = reduction.state;
    apply_effects(
        state,
        reduction.effects,
        transport,
        Arc::clone(&shared.subscribers),
        shared.capture.clone(),
        None,
    )
    .await?;
    *shared.state.lock().expect("session state lock") = state.clone();
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn page_success_result(
    page_control: &mut PageControl,
    transport: &dyn CdpTransport,
    state: &SupervisorState,
    target_id: krometrail_core::TargetId,
    operation: krometrail_core::BrowserOperationKind,
    interaction_id: krometrail_core::InteractionId,
    started_at: krometrail_core::SessionTime,
    dispatched_at: krometrail_core::SessionTime,
    change: PageChange,
    observation_target: PageSelection,
    cancel: &OperationCancellation,
) -> Result<PageOperationResult> {
    let observation = page_control
        .observe_after_operation(transport, state, observation_target, cancel)
        .await?;
    let outcome = observation.interruption.map_or_else(
        || PageOperationOutcome::Succeeded(change),
        PageOperationOutcome::Failed,
    );
    build_page_result(
        page_control,
        target_id,
        operation,
        interaction_id,
        started_at,
        dispatched_at,
        outcome,
        observation.observation,
    )
}

fn page_failure_result(
    page_control: &PageControl,
    target_id: krometrail_core::TargetId,
    operation: krometrail_core::BrowserOperationKind,
    interaction_id: krometrail_core::InteractionId,
    started_at: krometrail_core::SessionTime,
    dispatched_at: krometrail_core::SessionTime,
    error: KrometrailError,
) -> Result<BrowserOperationResult> {
    let result = build_page_result(
        page_control,
        target_id,
        operation,
        interaction_id,
        started_at,
        dispatched_at,
        PageOperationOutcome::Failed(error.clone()),
        ObservationPart::Unavailable(error),
    )?;
    Ok(match operation {
        krometrail_core::BrowserOperationKind::CreatePage => {
            BrowserOperationResult::CreatePage(Box::new(result))
        }
        krometrail_core::BrowserOperationKind::SelectPage => {
            BrowserOperationResult::SelectPage(Box::new(result))
        }
        krometrail_core::BrowserOperationKind::ClosePage => {
            BrowserOperationResult::ClosePage(Box::new(result))
        }
        krometrail_core::BrowserOperationKind::NavigatePage => {
            BrowserOperationResult::NavigatePage(Box::new(result))
        }
        krometrail_core::BrowserOperationKind::ReloadPage => {
            BrowserOperationResult::ReloadPage(Box::new(result))
        }
        krometrail_core::BrowserOperationKind::GoBack => {
            BrowserOperationResult::GoBack(Box::new(result))
        }
        krometrail_core::BrowserOperationKind::GoForward => {
            BrowserOperationResult::GoForward(Box::new(result))
        }
        _ => unreachable!("only state-changing page operations produce page failures"),
    })
}

#[allow(clippy::too_many_arguments)]
fn build_page_result(
    page_control: &PageControl,
    target_id: krometrail_core::TargetId,
    operation: krometrail_core::BrowserOperationKind,
    interaction_id: krometrail_core::InteractionId,
    started_at: krometrail_core::SessionTime,
    dispatched_at: krometrail_core::SessionTime,
    outcome: PageOperationOutcome,
    observation: ObservationPart<krometrail_core::LiveObservation>,
) -> Result<PageOperationResult> {
    let (completed_at, observed_at) = match &observation {
        ObservationPart::Available(observation) => (
            observation.context.started_at,
            Some(observation.context.completed_at),
        ),
        ObservationPart::Unavailable(_) => (page_control.session_time()?, None),
    };
    let timing = InteractionTiming::new(started_at, dispatched_at, completed_at, observed_at)?;
    let interaction = InteractionAnchor::new(
        interaction_id,
        page_control.session_id,
        target_id,
        operation,
        timing,
    )?;
    let outcome = match outcome {
        PageOperationOutcome::Failed(error) => PageOperationOutcome::failed(error, &interaction),
        outcome => outcome,
    };
    PageOperationResult::new(interaction, outcome, observation)
}

fn transport_page_error(
    error: TransportError,
    fallback: ErrorCode,
    target_id: krometrail_core::TargetId,
) -> KrometrailError {
    crate::control::transport_error(error, fallback, target_id)
}

async fn finish_interrupted_reconnect(
    shared: &Arc<SessionShared>,
    state: &mut SupervisorState,
    connection: &mut Option<ConnectionResources>,
    runtime: &SupervisorRuntime,
    input: SupervisorInput,
    stop_sender: Option<oneshot::Sender<Result<BrowserStopOutcome>>>,
) -> Result<()> {
    let cause = match &input {
        SupervisorInput::StopRequested => crate::targets::ShutdownCause::StopRequested,
        SupervisorInput::BrowserProcessTerminated { .. } => {
            crate::targets::ShutdownCause::BrowserProcessTerminated
        }
        SupervisorInput::Cancelled => crate::targets::ShutdownCause::Cancelled,
        _ => return Ok(()),
    };
    let reduction = reduce(
        std::mem::replace(state, SupervisorState::new(shared.compatibility.clone())),
        input,
    )?;
    *state = reduction.state;
    let deadline = ShutdownDeadline::new(runtime.capture_timeout);
    if let Some(current) = connection.as_ref() {
        let _ = apply_effects(
            state,
            reduction.effects,
            Arc::clone(&current.transport),
            Arc::clone(&shared.subscribers),
            shared.capture.clone(),
            Some(deadline.clone()),
        )
        .await;
    }
    let result = perform_shutdown(
        connection,
        &runtime.process,
        &runtime.profile,
        state,
        ShutdownPlan {
            cause,
            ownership: shared.ownership,
            capture: shared.capture.clone(),
            deadline,
            flush_capture: !matches!(cause, crate::targets::ShutdownCause::ReconnectExhausted),
        },
    )
    .await;
    let outcome: Result<BrowserStopOutcome> = match &result {
        Ok(()) => Ok(if shared.ownership == BrowserOwnership::Managed {
            BrowserStopOutcome::ManagedBrowserClosed
        } else {
            BrowserStopOutcome::Detached
        }),
        Err(error) => Err(error.clone()),
    };
    if let Some(sender) = stop_sender {
        *shared.stop_result.lock().expect("stop result lock") = Some(outcome.clone());
        let _ = sender.send(outcome);
    }
    finish_state(shared, state);
    result
}

#[derive(Clone)]
struct AttemptCancellation {
    cancelled: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

impl AttemptCancellation {
    fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            notify: Arc::new(Notify::new()),
        }
    }

    fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.notify.notify_waiters();
    }

    async fn cancelled(&self) {
        if self.cancelled.load(Ordering::Acquire) {
            return;
        }
        let notified = self.notify.notified();
        if self.cancelled.load(Ordering::Acquire) {
            return;
        }
        notified.await;
    }
}

#[derive(Clone)]
struct AttemptControl {
    cancellation: AttemptCancellation,
    deadline: tokio::time::Instant,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AttemptFailure {
    Failed,
    TimedOut,
    Cancelled,
}

impl AttemptControl {
    async fn race<F, T>(&self, future: F) -> std::result::Result<T, AttemptFailure>
    where
        F: std::future::Future<Output = T>,
    {
        let mut future = Box::pin(future);
        let mut deadline = Box::pin(tokio::time::sleep_until(self.deadline));
        tokio::select! {
            biased;
            _ = self.cancellation.cancelled() => Err(AttemptFailure::Cancelled),
            _ = &mut deadline => Err(AttemptFailure::TimedOut),
            value = &mut future => Ok(value),
        }
    }

    async fn command(
        &self,
        transport: &Arc<dyn CdpTransport>,
        scope: &CommandScope,
        method: &str,
        params: Value,
    ) -> std::result::Result<Value, AttemptFailure> {
        self.race(transport.send_raw(scope, method, params))
            .await?
            .map_err(|_| AttemptFailure::Failed)
    }
}

#[derive(Default)]
struct PartialSessionTracker {
    sessions: Mutex<Vec<TransportSessionId>>,
}

impl PartialSessionTracker {
    fn insert(&self, session: TransportSessionId) {
        let mut sessions = self.sessions.lock().expect("partial session lock");
        if !sessions.iter().any(|existing| existing == &session) {
            sessions.push(session);
        }
    }

    fn take(&self) -> Vec<TransportSessionId> {
        std::mem::take(&mut *self.sessions.lock().expect("partial session lock"))
    }
}

struct PreparedReconnection {
    connection: ConnectionResources,
    state: SupervisorState,
    effects: Vec<SupervisorEffect>,
}

enum ReconnectInterrupt {
    Stop(oneshot::Sender<Result<BrowserStopOutcome>>),
    Input(SupervisorInput),
}

async fn discard_partial_connection(
    connection: &mut ConnectionResources,
    sessions: &PartialSessionTracker,
) {
    connection.abort_pumps();
    // Cleanup has one small global budget, not one budget per target. If the transport itself is
    // wedged, dropping it after this bound closes every remaining temporary flat session.
    let deadline = tokio::time::Instant::now() + Duration::from_millis(50);
    for session in sessions.take() {
        let _ = tokio::time::timeout_at(
            deadline,
            connection.transport.send_raw(
                &CommandScope::Browser,
                "Target.detachFromTarget",
                serde_json::json!({"sessionId": session.as_str()}),
            ),
        )
        .await;
        if tokio::time::Instant::now() >= deadline {
            break;
        }
    }
}

fn recordable_reconnect_targets(
    infos: &[TransportTargetInfo],
    limit: usize,
) -> std::result::Result<Vec<TransportTargetInfo>, AttemptFailure> {
    let mut keys = BTreeSet::new();
    let mut recordable = Vec::new();
    for info in infos.iter().filter(|info| info.is_recordable()) {
        if !keys.insert(info.target_key.clone()) || recordable.len() >= limit {
            return Err(AttemptFailure::Failed);
        }
        recordable.push(info.clone());
    }
    recordable.sort_by(|left, right| left.target_key.cmp(&right.target_key));
    Ok(recordable)
}

async fn restore_one_target(
    attempt: AttemptControl,
    transport: Arc<dyn CdpTransport>,
    info: TransportTargetInfo,
    sessions: Arc<PartialSessionTracker>,
) -> std::result::Result<ReconnectedTarget, AttemptFailure> {
    let value = attempt
        .command(
            &transport,
            &CommandScope::Browser,
            "Target.attachToTarget",
            serde_json::json!({"targetId": info.target_key, "flatten": true}),
        )
        .await?;
    let session = value
        .get("sessionId")
        .and_then(Value::as_str)
        .and_then(|value| TransportSessionId::new(value.to_owned()).ok())
        .ok_or(AttemptFailure::Failed)?;
    sessions.insert(session.clone());
    let scope = CommandScope::Session(session.clone());
    // Restore the control/recording domains without starting a screencast. Capture is a separate,
    // later effect and must not become an implicit side effect of reconnect supervision.
    for method in ["Page.enable", "Runtime.enable", "Accessibility.enable"] {
        attempt
            .command(
                &transport,
                &scope,
                method,
                Value::Object(Default::default()),
            )
            .await?;
    }
    let visibility_value = attempt
        .command(
            &transport,
            &scope,
            "Runtime.evaluate",
            serde_json::json!({
                "expression": "document.visibilityState",
                "returnByValue": true
            }),
        )
        .await?;
    let visibility =
        parse_visibility_result(&visibility_value).map_err(|_| AttemptFailure::Failed)?;
    Ok(ReconnectedTarget {
        info,
        session: Some(session),
        visibility,
    })
}

async fn restore_targets(
    attempt: AttemptControl,
    transport: Arc<dyn CdpTransport>,
    infos: Vec<TransportTargetInfo>,
    concurrency: usize,
    sessions: Arc<PartialSessionTracker>,
) -> std::result::Result<Vec<ReconnectedTarget>, AttemptFailure> {
    let mut pending = FuturesUnordered::new();
    let mut restored = Vec::with_capacity(infos.len());
    for info in infos {
        while pending.len() >= concurrency {
            let result = match pending.next().await {
                Some(result) => result?,
                None => return Err(AttemptFailure::Failed),
            };
            restored.push(result);
        }
        pending.push(restore_one_target(
            attempt.clone(),
            Arc::clone(&transport),
            info,
            Arc::clone(&sessions),
        ));
    }
    while let Some(result) = pending.next().await {
        restored.push(result?);
    }
    restored.sort_by(|left, right| left.info.target_key.cmp(&right.info.target_key));
    Ok(restored)
}

async fn stage_reconnection_effects(
    attempt: &AttemptControl,
    transport: &Arc<dyn CdpTransport>,
    effects: &[SupervisorEffect],
) -> std::result::Result<Vec<SupervisorEffect>, AttemptFailure> {
    let mut staged = Vec::new();
    for effect in effects {
        match effect {
            SupervisorEffect::Publish(event) => {
                staged.push(SupervisorEffect::Publish(event.clone()));
            }
            SupervisorEffect::Detach { session } => {
                attempt
                    .command(
                        transport,
                        &CommandScope::Browser,
                        "Target.detachFromTarget",
                        serde_json::json!({"sessionId": session.as_str()}),
                    )
                    .await?;
            }
            // A successful reconstruction has already attached every bounded target, restored
            // domains, and observed visibility. Any follow-up attach/probe would violate the
            // transaction boundary and make publication depend on an unbounded effect chain.
            SupervisorEffect::StartCapture { context } => {
                staged.push(SupervisorEffect::StartCapture {
                    context: context.clone(),
                });
            }
            SupervisorEffect::ResumeCapture { context } => {
                staged.push(SupervisorEffect::ResumeCapture {
                    context: context.clone(),
                });
            }
            SupervisorEffect::StopCapture { context } => {
                staged.push(SupervisorEffect::StopCapture {
                    context: context.clone(),
                });
            }
            SupervisorEffect::SuspendCapture { context } => {
                staged.push(SupervisorEffect::SuspendCapture {
                    context: context.clone(),
                });
            }
            SupervisorEffect::Attach { .. }
            | SupervisorEffect::ProbeInitialVisibility { .. }
            | SupervisorEffect::BeginReconnect
            | SupervisorEffect::Shutdown { .. } => return Err(AttemptFailure::Failed),
        }
    }
    Ok(staged)
}

async fn reconstruct_connection(
    runtime: &SupervisorRuntime,
    current_state: &SupervisorState,
    attempt: AttemptControl,
) -> std::result::Result<PreparedReconnection, AttemptFailure> {
    let (target_limit, attach_concurrency) = runtime.config.normalized_reconnect_bounds();
    if target_limit == 0 {
        return Err(AttemptFailure::Failed);
    }
    // HTTP endpoints are discovery origins, not immutable WebSocket URLs. Refresh each attempt so
    // a browser may rotate its path; direct WebSocket attaches remain direct.
    let endpoint = attempt
        .race(async {
            match runtime.endpoint.kind() {
                crate::LocalCdpEndpointKind::Http => runtime.endpoint.refresh_http().await,
                crate::LocalCdpEndpointKind::WebSocket => Ok(runtime.endpoint.as_ref().clone()),
            }
        })
        .await?
        .map_err(|_| AttemptFailure::Failed)?;
    let transport = attempt
        .race(runtime.factory.connect_endpoint(&endpoint))
        .await?
        .map_err(|_| AttemptFailure::Failed)?;
    let setup = attempt
        .race(setup_connection_with_target_limit(
            Arc::clone(&transport),
            target_limit,
        ))
        .await?;
    let mut connection = match setup {
        Ok(connection) => connection,
        Err(_) => return Err(AttemptFailure::Failed),
    };
    let infos = match recordable_reconnect_targets(&connection.targets, target_limit) {
        Ok(infos) => infos,
        Err(error) => {
            drop(connection);
            return Err(error);
        }
    };
    let sessions = Arc::new(PartialSessionTracker::default());
    let restored = match restore_targets(
        attempt.clone(),
        Arc::clone(&connection.transport),
        infos,
        attach_concurrency,
        Arc::clone(&sessions),
    )
    .await
    {
        Ok(restored) => restored,
        Err(error) => {
            discard_partial_connection(&mut connection, &sessions).await;
            drop(connection);
            return Err(error);
        }
    };
    let snapshot = ReconnectedSnapshot {
        connection_generation: current_state.connection_generation.saturating_add(1),
        compatibility: connection.compatibility.clone(),
        targets: restored,
    };
    let reduction = match reduce(
        current_state.clone(),
        SupervisorInput::Reconnected(snapshot),
    ) {
        Ok(reduction) => reduction,
        Err(_) => {
            discard_partial_connection(&mut connection, &sessions).await;
            drop(connection);
            return Err(AttemptFailure::Failed);
        }
    };
    let effects =
        match stage_reconnection_effects(&attempt, &connection.transport, &reduction.effects).await
        {
            Ok(effects) => effects,
            Err(error) => {
                discard_partial_connection(&mut connection, &sessions).await;
                drop(connection);
                return Err(error);
            }
        };
    Ok(PreparedReconnection {
        connection,
        state: reduction.state,
        effects,
    })
}

async fn reconnect_loop_transactional(
    shared: &Arc<SessionShared>,
    state: &mut SupervisorState,
    connection: &mut Option<ConnectionResources>,
    runtime: &SupervisorRuntime,
    commands: &mut mpsc::Receiver<SupervisorCommand>,
) -> bool {
    if let Some(old) = connection.as_mut() {
        old.abort_pumps();
    }
    for (attempt_number, delay) in runtime.config.reconnect.delays.iter().copied().enumerate() {
        let backoff_deadline = tokio::time::Instant::now() + delay;
        loop {
            let mut sleep = Box::pin(tokio::time::sleep_until(backoff_deadline));
            tokio::select! {
                _ = &mut sleep => break,
                command = commands.recv() => {
                    match command {
                        Some(SupervisorCommand::Stop(sender)) => {
                            let _ = finish_interrupted_reconnect(shared, state, connection, runtime, SupervisorInput::StopRequested, Some(sender)).await;
                            return true;
                        }
                        Some(SupervisorCommand::Execute(request, sender)) => {
                            let target_id = direct_request_target(&request);
                            let _ = sender.send(Err(request_operation_error(
                                ErrorCode::BrowserDisconnected,
                                target_id,
                                "browser is reconnecting; operation was not replayed",
                            )));
                        }
                        Some(SupervisorCommand::Input(input)) => {
                            let input = match input {
                                SupervisorInput::ForConnectionGeneration { input, .. } => *input,
                                input => input,
                            };
                            if matches!(input, SupervisorInput::Cancelled | SupervisorInput::BrowserProcessTerminated { .. }) {
                                let _ = finish_interrupted_reconnect(shared, state, connection, runtime, input, None).await;
                                return true;
                            }
                            // Old-generation target events are harmless; keep the same absolute
                            // backoff deadline rather than resetting it or consuming an attempt.
                        }
                        None => {
                            let _ = finish_interrupted_reconnect(shared, state, connection, runtime, SupervisorInput::Cancelled, None).await;
                            return true;
                        }
                    }
                }
                exit = runtime.process_death.wait(), if runtime.process.is_some() => {
                    let _ = finish_interrupted_reconnect(
                        shared,
                        state,
                        connection,
                        runtime,
                        SupervisorInput::BrowserProcessTerminated { exit },
                        None,
                    ).await;
                    return true;
                }
            }
        }
        if let Some(process) = &runtime.process {
            let alive = process
                .lock()
                .expect("process lock")
                .as_mut()
                .is_some_and(ManagedChromeProcess::is_alive);
            if !alive {
                let _ = finish_interrupted_reconnect(
                    shared,
                    state,
                    connection,
                    runtime,
                    SupervisorInput::BrowserProcessTerminated {
                        exit: crate::launcher::SanitizedProcessExit::Unknown,
                    },
                    None,
                )
                .await;
                return true;
            }
        }
        tracing::info!(
            reconnect_attempt = attempt_number + 1,
            connection_generation = state.connection_generation,
            target_limit = runtime.config.reconnect_target_limit,
            attach_concurrency = runtime.config.reconnect_attach_concurrency,
            "browser.session.reconnect_attempt"
        );
        let cancellation = AttemptCancellation::new();
        let attempt_control = AttemptControl {
            cancellation: cancellation.clone(),
            deadline: tokio::time::Instant::now() + runtime.config.reconnect.attempt_timeout,
        };
        let reconnect_state = state.clone();
        let mut transaction = Box::pin(reconstruct_connection(
            runtime,
            &reconnect_state,
            attempt_control,
        ));
        let outcome = loop {
            tokio::select! {
                command = commands.recv() => {
                    let interrupt = match command {
                        Some(SupervisorCommand::Stop(sender)) => Some(ReconnectInterrupt::Stop(sender)),
                        Some(SupervisorCommand::Execute(request, sender)) => {
                            let target_id = direct_request_target(&request);
                            let _ = sender.send(Err(request_operation_error(
                                ErrorCode::BrowserDisconnected,
                                target_id,
                                "browser is reconnecting; operation was not replayed",
                            )));
                            None
                        }
                        Some(SupervisorCommand::Input(input)) => {
                            let input = match input {
                                SupervisorInput::ForConnectionGeneration { input, .. } => *input,
                                input => input,
                            };
                            if matches!(input, SupervisorInput::Cancelled | SupervisorInput::BrowserProcessTerminated { .. }) {
                                Some(ReconnectInterrupt::Input(input))
                            } else {
                                None
                            }
                        }
                        None => Some(ReconnectInterrupt::Input(SupervisorInput::Cancelled)),
                    };
                    if let Some(interrupt) = interrupt {
                        cancellation.cancel();
                        let _ = (&mut transaction).await;
                        break Err(interrupt);
                    }
                }
                exit = runtime.process_death.wait(), if runtime.process.is_some() => {
                    cancellation.cancel();
                    let _ = (&mut transaction).await;
                    break Err(ReconnectInterrupt::Input(
                        SupervisorInput::BrowserProcessTerminated { exit }
                    ));
                }
                result = &mut transaction => break Ok(result),
            }
        };
        match outcome {
            Err(ReconnectInterrupt::Stop(sender)) => {
                let _ = finish_interrupted_reconnect(
                    shared,
                    state,
                    connection,
                    runtime,
                    SupervisorInput::StopRequested,
                    Some(sender),
                )
                .await;
                return true;
            }
            Err(ReconnectInterrupt::Input(input)) => {
                let _ =
                    finish_interrupted_reconnect(shared, state, connection, runtime, input, None)
                        .await;
                return true;
            }
            Ok(Err(_)) => continue,
            Ok(Ok(mut prepared)) => {
                prepared.connection.restart_pumps(
                    shared.command_tx.clone(),
                    prepared.state.connection_generation,
                    shared.operation_cancellation.clone(),
                );
                *state = prepared.state;
                let new_transport = Arc::clone(&prepared.connection.transport);
                let effects = std::mem::take(&mut prepared.effects);
                *connection = Some(prepared.connection);
                let _ = apply_effects(
                    state,
                    effects,
                    new_transport,
                    Arc::clone(&shared.subscribers),
                    shared.capture.clone(),
                    None,
                )
                .await;
                *shared.state.lock().expect("session state lock") = state.clone();
                tracing::info!(
                    reconnect_attempt = attempt_number + 1,
                    connection_generation = state.connection_generation,
                    "browser.session.reconnected"
                );
                return false;
            }
        }
    }
    if let Ok(reduction) = reduce(
        std::mem::replace(state, SupervisorState::new(shared.compatibility.clone())),
        SupervisorInput::ReconnectExhausted,
    ) {
        *state = reduction.state;
        let deadline = ShutdownDeadline::new(runtime.capture_timeout);
        if let Some(current) = connection.as_ref() {
            let _ = apply_effects(
                state,
                reduction.effects,
                Arc::clone(&current.transport),
                Arc::clone(&shared.subscribers),
                shared.capture.clone(),
                Some(deadline.clone()),
            )
            .await;
        }
        let _ = perform_shutdown(
            connection,
            &runtime.process,
            &runtime.profile,
            state,
            ShutdownPlan {
                cause: crate::targets::ShutdownCause::ReconnectExhausted,
                ownership: shared.ownership,
                capture: shared.capture.clone(),
                deadline,
                flush_capture: false,
            },
        )
        .await;
        finish_state(shared, state);
    }
    true
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ShutdownPhase {
    Origin,
    CaptureStopDrainFlush,
    TargetDetach,
    BrowserClose,
    ProcessTerminate,
    Complete,
}

trait ShutdownBudgetSource: Send + Sync {
    fn now(&self, phase: ShutdownPhase) -> tokio::time::Instant;
}

struct TokioShutdownBudgetSource;

impl ShutdownBudgetSource for TokioShutdownBudgetSource {
    fn now(&self, _phase: ShutdownPhase) -> tokio::time::Instant {
        tokio::time::Instant::now()
    }
}

#[derive(Clone)]
struct ShutdownDeadline {
    origin: tokio::time::Instant,
    timeout: Duration,
    source: Arc<dyn ShutdownBudgetSource>,
}

impl ShutdownDeadline {
    fn new(timeout: Duration) -> Self {
        Self::with_source(timeout, Arc::new(TokioShutdownBudgetSource))
    }

    fn with_source(timeout: Duration, source: Arc<dyn ShutdownBudgetSource>) -> Self {
        let origin = source.now(ShutdownPhase::Origin);
        Self {
            origin,
            timeout,
            source,
        }
    }

    fn instant(&self) -> tokio::time::Instant {
        self.origin + self.timeout
    }

    fn remaining(&self, phase: ShutdownPhase) -> Duration {
        self.instant()
            .saturating_duration_since(self.source.now(phase))
    }
}

struct ShutdownPlan {
    cause: crate::targets::ShutdownCause,
    ownership: BrowserOwnership,
    capture: Option<Arc<CaptureRuntime>>,
    deadline: ShutdownDeadline,
    flush_capture: bool,
}

async fn perform_shutdown(
    connection: &mut Option<ConnectionResources>,
    process: &Option<Arc<Mutex<Option<ManagedChromeProcess>>>>,
    profile: &Option<Arc<Mutex<Option<ProfileLease>>>>,
    state: &SupervisorState,
    plan: ShutdownPlan,
) -> Result<()> {
    let started = std::time::Instant::now();
    let deadline = plan.deadline.instant();
    let mut failed = false;

    // Capture closes acceptance and drains before transport resources are detached. The same
    // absolute deadline is passed to every phase; the source samples only expose the budget at
    // each boundary and never create a phase-local deadline.
    if plan.flush_capture {
        if let Some(capture) = plan.capture.as_ref() {
            if !plan
                .deadline
                .remaining(ShutdownPhase::CaptureStopDrainFlush)
                .is_zero()
            {
                let outcome = capture
                    .coordinator
                    .shutdown(capture.session_id, deadline)
                    .await;
                failed |= !outcome.complete;
            } else {
                failed = true;
            }
        }
    }

    if let Some(connection) = connection.as_mut() {
        connection.abort_pumps();
        let mut sessions = state
            .target_key_by_session
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        sessions.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        for session in sessions {
            if plan
                .deadline
                .remaining(ShutdownPhase::TargetDetach)
                .is_zero()
            {
                failed = true;
                break;
            }
            let result = tokio::time::timeout_at(
                deadline,
                connection.transport.send_raw(
                    &CommandScope::Browser,
                    "Target.detachFromTarget",
                    serde_json::json!({"sessionId": session.as_str()}),
                ),
            )
            .await;
            if !result.is_ok_and(|result| result.is_ok()) {
                failed = true;
            }
        }
        if plan.ownership == BrowserOwnership::Managed
            && matches!(
                plan.cause,
                crate::targets::ShutdownCause::StopRequested
                    | crate::targets::ShutdownCause::BrowserProcessTerminated
                    | crate::targets::ShutdownCause::ReconnectExhausted
                    | crate::targets::ShutdownCause::Cancelled
            )
            && !plan
                .deadline
                .remaining(ShutdownPhase::BrowserClose)
                .is_zero()
        {
            let result = tokio::time::timeout_at(
                deadline,
                connection.transport.send_raw(
                    &CommandScope::Browser,
                    "Browser.close",
                    Value::Object(Default::default()),
                ),
            )
            .await;
            if !result.is_ok_and(|result| result.is_ok()) {
                failed = true;
            }
        } else if plan.ownership == BrowserOwnership::Managed {
            failed = true;
        }
    }

    if let Some(process) = process {
        let owned = process.lock().expect("process lock").take();
        if let Some(mut owned) = owned {
            let remaining = plan.deadline.remaining(ShutdownPhase::ProcessTerminate);
            if !remaining.is_zero() {
                match tokio::time::timeout_at(deadline, owned.terminate(remaining)).await {
                    Ok(Ok(_)) => {}
                    Ok(Err(_)) | Err(_) => {
                        failed = true;
                        owned.force_kill_now();
                    }
                }
            } else {
                failed = true;
                owned.force_kill_now();
            }
        }
    }
    if let Some(profile) = profile {
        profile.lock().expect("profile lock").take();
    }
    *connection = None;
    let exhausted = plan.deadline.remaining(ShutdownPhase::Complete).is_zero();
    if failed || exhausted {
        tracing::warn!(
            elapsed_ms = started.elapsed().as_millis() as u64,
            forced_termination = true,
            unfinished_task_count = 0_u64,
            "browser.shutdown.incomplete"
        );
        Err(stable_error(
            ErrorCode::ShutdownIncomplete,
            "browser shutdown was incomplete",
        ))
    } else {
        tracing::info!(
            elapsed_ms = started.elapsed().as_millis() as u64,
            forced_termination = false,
            unfinished_task_count = 0_u64,
            "browser.shutdown.completed"
        );
        Ok(())
    }
}

fn finish_state(shared: &Arc<SessionShared>, state: &mut SupervisorState) {
    // Several shutdown inputs can race with transport/process teardown. The first terminal
    // transition owns the single Ended publication and channel closure; later inputs are no-ops.
    if state.session_state == BrowserSessionState::Ended {
        return;
    }
    let previous = state.session_state;
    state.session_state = BrowserSessionState::Ended;
    state.revision = state.revision.saturating_add(1);
    tracing::info!(
        previous_state = previous.as_str(),
        next_state = BrowserSessionState::Ended.as_str(),
        connection_generation = state.connection_generation,
        "browser.session.state_changed"
    );
    *shared.state.lock().expect("session state lock") = state.clone();
    shared
        .subscribers
        .publish(BrowserSessionEvent::SessionStateChanged {
            state: BrowserSessionState::Ended,
        });
}

async fn watch_process(
    process: Arc<Mutex<Option<ManagedChromeProcess>>>,
    sender: mpsc::Sender<SupervisorCommand>,
    death: Arc<ProcessDeathSignal>,
) {
    loop {
        tokio::time::sleep(Duration::from_millis(25)).await;
        let exit = {
            let mut guard = process.lock().expect("process lock");
            let Some(process) = guard.as_mut() else {
                return;
            };
            if process.is_alive() {
                None
            } else {
                Some(
                    process
                        .termination_if_exited()
                        .map(|termination| termination.exit)
                        .unwrap_or(crate::launcher::SanitizedProcessExit::Unknown),
                )
            }
        };
        if let Some(exit) = exit {
            death.record(exit);
            let _ = sender
                .send(SupervisorCommand::Input(
                    SupervisorInput::BrowserProcessTerminated { exit },
                ))
                .await;
            return;
        }
    }
}

async fn pump_events(
    kind: TargetEventKind,
    mut events: Box<dyn TransportEvents>,
    sender: mpsc::Sender<SupervisorCommand>,
    generation: u64,
    cancellation: OperationCancellation,
) {
    loop {
        match events.next().await {
            Ok(Some(event)) => {
                if let Some(input) = parse_event(kind, event) {
                    if sender
                        .send(SupervisorCommand::Input(
                            SupervisorInput::ForConnectionGeneration {
                                generation,
                                input: Box::new(input),
                            },
                        ))
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
            }
            Ok(None) | Err(_) => {
                cancellation.disconnect(generation);
                let _ = sender
                    .send(SupervisorCommand::Input(
                        SupervisorInput::ForConnectionGeneration {
                            generation,
                            input: Box::new(SupervisorInput::ConnectionLost(TransportClose {
                                reason: NonEmptyText::new("transport event stream closed").unwrap(),
                            })),
                        },
                    ))
                    .await;
                return;
            }
        }
    }
}

fn parse_event(kind: TargetEventKind, event: NamedEvent) -> Option<SupervisorInput> {
    match kind {
        TargetEventKind::Created | TargetEventKind::InfoChanged => {
            let info = event.params.get("targetInfo").and_then(parse_target_info)?;
            Some(if matches!(kind, TargetEventKind::Created) {
                SupervisorInput::TargetCreated(info)
            } else {
                SupervisorInput::TargetInfoChanged(info)
            })
        }
        TargetEventKind::Destroyed => Some(SupervisorInput::TargetDestroyed {
            target_key: event.params.get("targetId")?.as_str()?.to_owned(),
        }),
        TargetEventKind::Attached => Some(SupervisorInput::Attached {
            target_key: event
                .params
                .pointer("/targetInfo/targetId")?
                .as_str()?
                .to_owned(),
            session: TransportSessionId::new(event.params.get("sessionId")?.as_str()?.to_owned())
                .ok()?,
        }),
        TargetEventKind::Detached => Some(SupervisorInput::Detached {
            session: TransportSessionId::new(event.params.get("sessionId")?.as_str()?.to_owned())
                .ok()?,
            reason: event
                .params
                .get("reason")
                .and_then(Value::as_str)
                .map(str::to_owned),
        }),
    }
}

fn parse_target_list(value: &Value) -> Option<Vec<TransportTargetInfo>> {
    value
        .get("targetInfos")?
        .as_array()?
        .iter()
        .map(parse_target_info)
        .collect()
}

fn parse_target_info(value: &Value) -> Option<TransportTargetInfo> {
    TransportTargetInfo::new(
        value.get("targetId")?.as_str()?,
        value.get("type")?.as_str()?,
        value.get("url").and_then(Value::as_str).unwrap_or_default(),
        value
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        value
            .get("attached")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        value
            .get("browserContextId")
            .and_then(Value::as_str)
            .map(str::to_owned),
    )
    .ok()
}

fn launch_error_to_core(error: &LaunchError) -> KrometrailError {
    let code = error.stable_code();
    stable_error(
        code,
        match code {
            ErrorCode::BrowserNotFound => "no supported browser installation was found",
            ErrorCode::ProfileInUse => "managed browser profile is already in use",
            ErrorCode::BrowserProcessTerminated => "the managed browser process terminated",
            ErrorCode::ShutdownIncomplete => "browser shutdown was incomplete",
            ErrorCode::Cancelled => "browser launch was cancelled",
            _ => "browser launch failed",
        },
    )
}

fn transport_error_to_core(error: TransportError, target: bool) -> KrometrailError {
    if target {
        return stable_error(ErrorCode::TargetFailed, "browser target operation failed");
    }
    stable_error(
        if error.is_retryable() {
            ErrorCode::BrowserDisconnected
        } else {
            ErrorCode::BrowserCompatibilityFailed
        },
        if error.is_retryable() {
            "browser transport disconnected"
        } else {
            "browser transport command failed"
        },
    )
}

fn session_setup_error(error: CompatibilityProbeError) -> KrometrailError {
    match error {
        CompatibilityProbeError::Transport(error) => transport_error_to_core(error, false),
        _ => stable_error(
            ErrorCode::BrowserCompatibilityFailed,
            "browser does not provide the required renderer capabilities",
        ),
    }
}

fn direct_request_target(request: &BrowserOperationRequest) -> Option<krometrail_core::TargetId> {
    match request.scope() {
        BrowserOperationScope::Page(krometrail_core::PageSelection::Target(target_id)) => {
            Some(target_id)
        }
        BrowserOperationScope::Browser
        | BrowserOperationScope::Page(krometrail_core::PageSelection::Selected) => None,
    }
}

fn request_operation_error(
    code: ErrorCode,
    target_id: Option<krometrail_core::TargetId>,
    message: &'static str,
) -> KrometrailError {
    target_id.map_or_else(
        || stable_error(code, message),
        |target_id| operation_error(code, target_id, message),
    )
}

fn stable_error(code: ErrorCode, message: &'static str) -> KrometrailError {
    KrometrailError::from_browser_failure(code, NonEmptyText::new(message).unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        EndpointResolveFuture, EndpointResolver, LocalCdpEndpoint, transport::TransportFuture,
    };
    use krometrail_core::{
        BrowserProduct, BrowserProductVersion, BrowserVersion, ByteOffset, CapabilitySupport,
        CaptureGap, EncodedFrame, FrameAddress, IdValue, MonotonicClock, PortFuture, RecordingSink,
        RendererCapability, SegmentId, SessionOrigin,
    };
    use std::{
        net::{IpAddr, Ipv4Addr, SocketAddr},
        process::Command,
        sync::atomic::AtomicUsize,
        sync::{Arc, Mutex},
    };

    #[test]
    fn initial_visibility_parser_accepts_only_supported_result_shapes_and_values() {
        assert_eq!(
            parse_visibility_result(&serde_json::json!({
                "result": {"result": {"value": "visible"}}
            })),
            Ok(TargetVisibility::Visible)
        );
        assert_eq!(
            parse_visibility_result(&serde_json::json!({
                "result": {"value": "hidden"}
            })),
            Ok(TargetVisibility::Hidden)
        );
        assert_eq!(
            parse_visibility_result(&serde_json::json!({
                "result": {"value": "prerender"}
            })),
            Err(VisibilityProbeError::UnsupportedValue)
        );
        assert_eq!(
            parse_visibility_result(&serde_json::json!({"result": {}})),
            Err(VisibilityProbeError::MissingValue)
        );
    }

    #[test]
    fn target_event_parsing_uses_opaque_keys_and_ignores_page_content() {
        let input = parse_event(
            TargetEventKind::Created,
            NamedEvent {
                method: "Target.targetCreated".into(),
                params: serde_json::json!({"targetInfo": {"targetId":"target-1","type":"page","url":"https://example.test/private?token=secret","title":"secret"}}),
            },
        );
        assert!(matches!(input, Some(SupervisorInput::TargetCreated(_))));
    }

    fn page_info(key: &str) -> TransportTargetInfo {
        TransportTargetInfo::new(key, "page", format!("https://{key}.test"), key, false, None)
            .unwrap()
    }

    struct DelayedChangedAuthorityResolver {
        address: SocketAddr,
        calls: AtomicUsize,
        started: Arc<Notify>,
    }

    impl EndpointResolver for DelayedChangedAuthorityResolver {
        fn resolve<'a>(&'a self, _host: &'a str, _port: u16) -> EndpointResolveFuture<'a> {
            let call = self.calls.fetch_add(1, Ordering::AcqRel);
            if call < 2 {
                let address = self.address;
                Box::pin(std::future::ready(Ok(vec![address])))
            } else {
                let started = Arc::clone(&self.started);
                Box::pin(async move {
                    started.notify_waiters();
                    std::future::pending::<std::io::Result<Vec<SocketAddr>>>().await
                })
            }
        }
    }

    async fn endpoint_with_delayed_changed_authority() -> (
        LocalCdpEndpoint,
        Arc<DelayedChangedAuthorityResolver>,
        JoinHandle<()>,
    ) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            for host in ["initial.invalid", "changed.invalid"] {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                loop {
                    let mut chunk = [0_u8; 512];
                    let count = stream.read(&mut chunk).await.unwrap();
                    request.extend_from_slice(&chunk[..count]);
                    if request.windows(4).any(|window| window == b"\r\n\r\n") {
                        break;
                    }
                }
                let body = serde_json::json!({
                    "webSocketDebuggerUrl": format!(
                        "ws://{host}:{}/devtools/browser/{}",
                        address.port(),
                        if host == "initial.invalid" { "initial" } else { "changed" }
                    ),
                })
                .to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        });
        let resolver = Arc::new(DelayedChangedAuthorityResolver {
            address: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), address.port()),
            calls: AtomicUsize::new(0),
            started: Arc::new(Notify::new()),
        });
        let endpoint = LocalCdpEndpoint::resolve_with_resolver(
            format!("http://origin.invalid:{}", address.port()),
            Arc::clone(&resolver) as Arc<dyn EndpointResolver>,
        )
        .await
        .unwrap();
        assert_eq!(resolver.calls.load(Ordering::Acquire), 2);
        (endpoint, resolver, server)
    }

    fn test_compatibility() -> BrowserCompatibility {
        BrowserCompatibility::new(
            BrowserVersion::new(
                BrowserProduct::Chrome,
                BrowserProductVersion::new("128").unwrap(),
                "revision",
                "1.3",
                "user-agent",
                "js",
            )
            .unwrap(),
            RendererCapability::ALL
                .iter()
                .map(|capability| CapabilitySupport::new(*capability, true, true, None).unwrap())
                .collect(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn changed_authority_resolution_loses_to_the_twenty_millisecond_attempt_deadline() {
        let (endpoint, resolver, server) = endpoint_with_delayed_changed_authority().await;
        let attempt = AttemptControl {
            cancellation: AttemptCancellation::new(),
            deadline: tokio::time::Instant::now() + Duration::from_millis(20),
        };
        let result = attempt.race(endpoint.refresh_http()).await;
        assert_eq!(result, Err(AttemptFailure::TimedOut));
        assert_eq!(resolver.calls.load(Ordering::Acquire), 3);
        assert_eq!(
            endpoint.browser_websocket_url().path(),
            "/devtools/browser/initial"
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn changed_authority_resolution_loses_to_attempt_cancellation() {
        let (endpoint, resolver, server) = endpoint_with_delayed_changed_authority().await;
        let cancellation = AttemptCancellation::new();
        let attempt = AttemptControl {
            cancellation: cancellation.clone(),
            deadline: tokio::time::Instant::now() + Duration::from_secs(1),
        };
        let task = tokio::spawn(async move { attempt.race(endpoint.refresh_http()).await });
        tokio::time::timeout(Duration::from_millis(100), resolver.started.notified())
            .await
            .unwrap();
        cancellation.cancel();
        assert_eq!(task.await.unwrap(), Err(AttemptFailure::Cancelled));
        server.await.unwrap();
    }

    struct NeverConnectFactory {
        calls: AtomicUsize,
    }

    impl CdpTransportFactory for NeverConnectFactory {
        fn connect(
            &self,
            _browser_websocket_url: &str,
        ) -> TransportFuture<'_, std::result::Result<Arc<dyn CdpTransport>, TransportError>>
        {
            self.calls.fetch_add(1, Ordering::AcqRel);
            Box::pin(async { Err(TransportError::ConnectFailed) })
        }
    }

    #[tokio::test]
    async fn process_death_abandons_changed_authority_resolution_before_connection_commit() {
        let (endpoint, resolver, server) = endpoint_with_delayed_changed_authority().await;
        let process =
            ManagedChromeProcess::from_child(Command::new("sleep").arg("60").spawn().unwrap());
        let process = Arc::new(Mutex::new(Some(process)));
        let process_death = Arc::new(ProcessDeathSignal::default());
        let factory = Arc::new(NeverConnectFactory {
            calls: AtomicUsize::new(0),
        });
        let compatibility = test_compatibility();
        let (command_tx, mut commands) = mpsc::channel(4);
        let shared = Arc::new(SessionShared {
            compatibility: compatibility.clone(),
            ownership: BrowserOwnership::Managed,
            profile: ProfileRef::External,
            state: Mutex::new(SupervisorState::new(compatibility.clone())),
            subscribers: Arc::new(SubscriberRegistry::new(4)),
            command_tx,
            session_id: SessionId::from_uuid(Uuid::new_v4()),
            session_origin: SessionOrigin::new(krometrail_core::ObservedTime::from_nanos(0)),
            capture: None,
            operation_cancellation: OperationCancellation::default(),
            stop_result: Mutex::new(None),
        });
        let runtime = SupervisorRuntime {
            endpoint: Arc::new(endpoint),
            factory: factory.clone(),
            process: Some(process),
            profile: None,
            config: SupervisorConfig {
                reconnect: crate::ReconnectPolicy {
                    delays: vec![Duration::ZERO].into_boxed_slice(),
                    attempt_timeout: Duration::from_secs(1),
                },
                subscriber_capacity: 4,
                reconnect_target_limit: 4,
                reconnect_attach_concurrency: 1,
            },
            process_death: Arc::clone(&process_death),
            capture_timeout: Duration::from_secs(5),
        };
        let task = tokio::spawn(async move {
            let mut state = SupervisorState::new(compatibility);
            let mut connection = None;
            let ended = reconnect_loop_transactional(
                &shared,
                &mut state,
                &mut connection,
                &runtime,
                &mut commands,
            )
            .await;
            (ended, state, connection)
        });
        tokio::time::timeout(Duration::from_millis(100), resolver.started.notified())
            .await
            .unwrap();
        process_death.record(crate::launcher::SanitizedProcessExit::Signaled);
        let (ended, state, connection) = tokio::time::timeout(Duration::from_millis(100), task)
            .await
            .unwrap()
            .unwrap();
        assert!(ended);
        assert_eq!(state.session_state, BrowserSessionState::Ended);
        assert!(connection.is_none());
        assert_eq!(factory.calls.load(Ordering::Acquire), 0);
        server.await.unwrap();
    }

    #[test]
    fn reconnect_target_cap_rejects_extra_recordable_targets_before_attachment() {
        let infos = vec![page_info("one"), page_info("two"), page_info("three")];
        assert_eq!(recordable_reconnect_targets(&infos, 3).unwrap().len(), 3);
        assert_eq!(
            recordable_reconnect_targets(&infos, 2),
            Err(AttemptFailure::Failed)
        );
    }

    #[tokio::test]
    async fn many_target_restore_never_exceeds_configured_concurrency() {
        let transport = Arc::new(ControlledTransport::paced());
        let infos = (0..9)
            .map(|index| page_info(&format!("target-{index}")))
            .collect();
        let attempt = AttemptControl {
            cancellation: AttemptCancellation::new(),
            deadline: tokio::time::Instant::now() + Duration::from_secs(1),
        };
        let restored = restore_targets(
            attempt,
            transport.clone(),
            infos,
            2,
            Arc::new(PartialSessionTracker::default()),
        )
        .await
        .unwrap();
        assert_eq!(restored.len(), 9);
        assert!(transport.maximum_active() <= 2);
        assert!(transport.maximum_active() >= 2);
    }

    #[tokio::test]
    async fn stalled_restore_command_is_cut_off_by_attempt_deadline() {
        let transport = Arc::new(ControlledTransport::stalled("Runtime.enable"));
        let started = transport.started();
        let attempt = AttemptControl {
            cancellation: AttemptCancellation::new(),
            deadline: tokio::time::Instant::now() + Duration::from_millis(20),
        };
        let task = tokio::spawn(restore_one_target(
            attempt,
            transport,
            page_info("stalled"),
            Arc::new(PartialSessionTracker::default()),
        ));
        tokio::time::timeout(Duration::from_millis(100), started.notified())
            .await
            .unwrap();
        assert_eq!(
            tokio::time::timeout(Duration::from_millis(100), task)
                .await
                .unwrap()
                .unwrap(),
            Err(AttemptFailure::TimedOut)
        );
    }

    #[tokio::test]
    async fn cancellation_interrupts_a_stalled_restore_immediately() {
        let transport = Arc::new(ControlledTransport::stalled("Runtime.enable"));
        let started = transport.started();
        let cancellation = AttemptCancellation::new();
        let attempt = AttemptControl {
            cancellation: cancellation.clone(),
            deadline: tokio::time::Instant::now() + Duration::from_secs(1),
        };
        let task = tokio::spawn(restore_one_target(
            attempt,
            transport,
            page_info("cancelled"),
            Arc::new(PartialSessionTracker::default()),
        ));
        tokio::time::timeout(Duration::from_millis(100), started.notified())
            .await
            .unwrap();
        cancellation.cancel();
        assert_eq!(
            tokio::time::timeout(Duration::from_millis(100), task)
                .await
                .unwrap()
                .unwrap(),
            Err(AttemptFailure::Cancelled)
        );
    }

    #[tokio::test]
    async fn managed_process_death_signal_is_observed_without_polling_the_attempt() {
        let signal = Arc::new(ProcessDeathSignal::default());
        let waiter = {
            let signal = Arc::clone(&signal);
            tokio::spawn(async move { signal.wait().await })
        };
        signal.record(crate::launcher::SanitizedProcessExit::Signaled);
        assert_eq!(
            waiter.await.unwrap(),
            crate::launcher::SanitizedProcessExit::Signaled
        );
    }

    #[derive(Clone)]
    struct ControlledTransport {
        state: Arc<ControlledTransportState>,
    }

    struct ControlledTransportState {
        stall_method: Mutex<Option<String>>,
        next_session: AtomicUsize,
        active: AtomicUsize,
        maximum_active: AtomicUsize,
        started: Arc<Notify>,
    }

    impl ControlledTransport {
        fn paced() -> Self {
            Self {
                state: Arc::new(ControlledTransportState {
                    stall_method: Mutex::new(None),
                    next_session: AtomicUsize::new(0),
                    active: AtomicUsize::new(0),
                    maximum_active: AtomicUsize::new(0),
                    started: Arc::new(Notify::new()),
                }),
            }
        }

        fn stalled(method: &str) -> Self {
            let transport = Self::paced();
            *transport
                .state
                .stall_method
                .lock()
                .expect("stall method lock") = Some(method.to_owned());
            transport
        }

        fn started(&self) -> Arc<Notify> {
            Arc::clone(&self.state.started)
        }

        fn maximum_active(&self) -> usize {
            self.state.maximum_active.load(Ordering::Acquire)
        }
    }

    impl CdpTransport for ControlledTransport {
        fn send_raw(
            &self,
            _scope: &CommandScope,
            method: &str,
            _params: Value,
        ) -> TransportFuture<'_, std::result::Result<Value, TransportError>> {
            let stalled = self
                .state
                .stall_method
                .lock()
                .expect("stall method lock")
                .as_deref()
                == Some(method);
            let response = if method == "Target.attachToTarget" {
                let session = self.state.next_session.fetch_add(1, Ordering::Relaxed);
                serde_json::json!({"sessionId": format!("session-{session}")})
            } else if method == "Runtime.evaluate" {
                serde_json::json!({"result": {"result": {"type": "string", "value": "visible"}}})
            } else {
                Value::Object(Default::default())
            };
            let state = Arc::clone(&self.state);
            Box::pin(async move {
                state.started.notify_waiters();
                if stalled {
                    std::future::pending::<std::result::Result<Value, TransportError>>().await
                } else {
                    let active = state.active.fetch_add(1, Ordering::AcqRel) + 1;
                    state.maximum_active.fetch_max(active, Ordering::AcqRel);
                    tokio::time::sleep(Duration::from_millis(2)).await;
                    state.active.fetch_sub(1, Ordering::AcqRel);
                    Ok(response)
                }
            })
        }

        fn subscribe_named(
            &self,
            _scope: &CommandScope,
            _method: &str,
        ) -> TransportFuture<'_, std::result::Result<Box<dyn TransportEvents>, TransportError>>
        {
            Box::pin(async { Err(TransportError::SubscriptionClosed) })
        }

        fn close_reason(&self) -> Option<TransportClose> {
            None
        }

        fn is_closed(&self) -> bool {
            false
        }
    }

    struct ShutdownTestEvents;

    impl TransportEvents for ShutdownTestEvents {
        fn next(&mut self) -> TransportFuture<'_, Result<Option<NamedEvent>, TransportError>> {
            Box::pin(async { std::future::pending().await })
        }
    }

    struct ShutdownTestTransport {
        log: Arc<Mutex<Vec<String>>>,
    }

    impl CdpTransport for ShutdownTestTransport {
        fn send_raw(
            &self,
            _scope: &CommandScope,
            method: &str,
            _params: Value,
        ) -> TransportFuture<'_, std::result::Result<Value, TransportError>> {
            self.log
                .lock()
                .expect("shutdown log lock")
                .push(method.into());
            Box::pin(std::future::ready(Ok(Value::Object(Default::default()))))
        }

        fn subscribe_named(
            &self,
            _scope: &CommandScope,
            _method: &str,
        ) -> TransportFuture<'_, std::result::Result<Box<dyn TransportEvents>, TransportError>>
        {
            Box::pin(std::future::ready(Ok(
                Box::new(ShutdownTestEvents) as Box<dyn TransportEvents>
            )))
        }

        fn close_reason(&self) -> Option<TransportClose> {
            None
        }

        fn is_closed(&self) -> bool {
            false
        }
    }

    struct ShutdownTestObserver;

    impl CaptureObserver for ShutdownTestObserver {
        fn status_changed(&self, _status: krometrail_core::TargetCaptureStatus) {}

        fn gap_declared(&self, _gap: CaptureGap) {}
    }

    struct ShutdownTestClock;

    impl MonotonicClock for ShutdownTestClock {
        fn now(&self) -> krometrail_core::ObservedTime {
            krometrail_core::ObservedTime::from_nanos(1)
        }
    }

    struct ShutdownTestIds;

    impl krometrail_core::IdSource for ShutdownTestIds {
        fn next(&self) -> IdValue {
            IdValue::from_uuid(Uuid::from_u128(42))
        }
    }

    struct ShutdownTestSink {
        log: Arc<Mutex<Vec<String>>>,
    }

    impl RecordingSink for ShutdownTestSink {
        fn append_frame(
            &self,
            _frame: EncodedFrame,
        ) -> PortFuture<'_, krometrail_core::Result<FrameAddress>> {
            Box::pin(std::future::ready(Ok(FrameAddress::new(
                SegmentId::from_uuid(Uuid::from_u128(1)),
                ByteOffset::new(1),
            ))))
        }

        fn append_gap(&self, _gap: CaptureGap) -> PortFuture<'_, krometrail_core::Result<()>> {
            Box::pin(std::future::ready(Ok(())))
        }

        fn flush(&self, _session_id: SessionId) -> PortFuture<'_, krometrail_core::Result<()>> {
            self.log
                .lock()
                .expect("shutdown log lock")
                .push("flush".into());
            Box::pin(std::future::ready(Ok(())))
        }
    }

    impl krometrail_core::RetentionStore for ShutdownTestSink {
        fn pin_range(
            &self,
            request: krometrail_core::RetentionRange,
        ) -> PortFuture<'_, krometrail_core::Result<krometrail_core::PinChange>> {
            Box::pin(std::future::ready(Ok(krometrail_core::PinChange {
                request,
                protected_segments: Vec::new(),
                pinned_usage_bytes: 0,
            })))
        }

        fn unpin_range(
            &self,
            request: krometrail_core::RetentionRange,
        ) -> PortFuture<'_, krometrail_core::Result<krometrail_core::PinChange>> {
            self.pin_range(request)
        }

        fn enforce_budget(
            &self,
        ) -> PortFuture<'_, krometrail_core::Result<krometrail_core::RetentionStatus>> {
            self.status()
        }

        fn status(
            &self,
        ) -> PortFuture<'_, krometrail_core::Result<krometrail_core::RetentionStatus>> {
            Box::pin(std::future::ready(Ok(
                krometrail_core::RetentionStatus::empty(krometrail_core::DiskBudgetBytes::default()),
            )))
        }

        fn delete_session(
            &self,
            session_id: SessionId,
        ) -> PortFuture<'_, krometrail_core::Result<krometrail_core::SessionDeletion>> {
            Box::pin(std::future::ready(Ok(krometrail_core::SessionDeletion {
                session_id,
                removed_segments: 0,
                removed_frames: 0,
                removed_artifacts: 0,
                removed_bytes: 0,
            })))
        }

        fn wait_until_recording_allowed(&self) -> PortFuture<'_, krometrail_core::Result<()>> {
            Box::pin(std::future::ready(Ok(())))
        }
    }

    struct ConsumingShutdownClock {
        current: Mutex<tokio::time::Instant>,
        step: Duration,
        samples: Mutex<Vec<(ShutdownPhase, tokio::time::Instant)>>,
    }

    impl ConsumingShutdownClock {
        fn new(step: Duration) -> Arc<Self> {
            Arc::new(Self {
                current: Mutex::new(tokio::time::Instant::now()),
                step,
                samples: Mutex::new(Vec::new()),
            })
        }

        fn samples(&self) -> Vec<(ShutdownPhase, tokio::time::Instant)> {
            self.samples.lock().expect("deadline samples lock").clone()
        }
    }

    impl ShutdownBudgetSource for ConsumingShutdownClock {
        fn now(&self, phase: ShutdownPhase) -> tokio::time::Instant {
            let mut current = self.current.lock().expect("deadline clock lock");
            let now = *current;
            self.samples
                .lock()
                .expect("deadline samples lock")
                .push((phase, now));
            *current = now + self.step;
            now
        }
    }

    async fn run_shutdown_fixture(
        timeout: Duration,
        step: Duration,
    ) -> (
        Result<()>,
        Arc<ConsumingShutdownClock>,
        ShutdownDeadline,
        Arc<Mutex<Vec<String>>>,
    ) {
        let log = Arc::new(Mutex::new(Vec::new()));
        let transport = Arc::new(ShutdownTestTransport {
            log: Arc::clone(&log),
        });
        let sink = Arc::new(ShutdownTestSink {
            log: Arc::clone(&log),
        });
        let retention = Arc::clone(&sink) as Arc<dyn krometrail_core::RetentionStore>;
        let coordinator = Arc::new(
            CaptureCoordinator::new(
                CaptureConfig::default(),
                CaptureDependencies {
                    clock: Arc::new(ShutdownTestClock),
                    ids: Arc::new(ShutdownTestIds),
                    sink,
                    retention: Arc::clone(&retention),
                },
                Arc::new(ShutdownTestObserver),
            )
            .expect("shutdown capture configuration"),
        );
        let session_id = SessionId::from_uuid(Uuid::from_u128(10));
        let target_id = krometrail_core::TargetId::from_uuid(Uuid::from_u128(11));
        let capture_target = CaptureTarget {
            session_id,
            session_origin: SessionOrigin::new(krometrail_core::ObservedTime::from_nanos(0)),
            target_id,
            connection_generation: 1,
            attachment_generation: 1,
            transport_session: TransportSessionId::new("transport-session").unwrap(),
        };
        coordinator
            .start_target(
                capture_target,
                Arc::clone(&transport) as Arc<dyn CdpTransport>,
            )
            .await
            .expect("start capture fixture");

        let state = reduce(
            SupervisorState::new(test_compatibility()),
            SupervisorInput::InitialTargets(vec![page_info("target")]),
        )
        .unwrap()
        .state;
        let state = reduce(
            state,
            SupervisorInput::Attached {
                target_key: "target".into(),
                session: TransportSessionId::new("transport-session").unwrap(),
            },
        )
        .unwrap()
        .state;
        let connection = ConnectionResources {
            transport,
            subscriptions: Vec::new(),
            targets: Vec::new(),
            compatibility: test_compatibility(),
            pump_handles: Vec::new(),
        };
        let process = ManagedChromeProcess::from_child(
            Command::new("sleep")
                .arg("60")
                .spawn()
                .expect("shutdown fixture process"),
        );
        let process = Some(Arc::new(Mutex::new(Some(process))));
        let source = ConsumingShutdownClock::new(step);
        let deadline = ShutdownDeadline::with_source(
            timeout,
            Arc::clone(&source) as Arc<dyn ShutdownBudgetSource>,
        );
        let capture = Arc::new(CaptureRuntime {
            coordinator,
            clock: Arc::new(ShutdownTestClock),
            session_id,
            session_origin: SessionOrigin::new(krometrail_core::ObservedTime::from_nanos(0)),
            retention,
            shutdown_timeout: timeout,
        });
        let mut connection = Some(connection);
        let result = perform_shutdown(
            &mut connection,
            &process,
            &None,
            &state,
            ShutdownPlan {
                cause: crate::targets::ShutdownCause::StopRequested,
                ownership: BrowserOwnership::Managed,
                capture: Some(capture),
                deadline: deadline.clone(),
                flush_capture: true,
            },
        )
        .await;
        (result, source, deadline, log)
    }

    #[tokio::test]
    async fn shutdown_deadline_is_consumed_once_across_capture_and_browser_cleanup() {
        let (result, source, deadline, log) =
            run_shutdown_fixture(Duration::from_millis(100), Duration::from_millis(10)).await;
        assert!(result.is_ok(), "shutdown fixture failed: {result:?}");
        let samples = source.samples();
        assert_eq!(
            samples.first().map(|sample| sample.0),
            Some(ShutdownPhase::Origin)
        );
        assert_eq!(
            samples.last().map(|sample| sample.0),
            Some(ShutdownPhase::Complete)
        );
        assert_eq!(
            samples.iter().map(|sample| sample.0).collect::<Vec<_>>(),
            vec![
                ShutdownPhase::Origin,
                ShutdownPhase::CaptureStopDrainFlush,
                ShutdownPhase::TargetDetach,
                ShutdownPhase::BrowserClose,
                ShutdownPhase::ProcessTerminate,
                ShutdownPhase::Complete,
            ]
        );
        let budgets = samples
            .iter()
            .skip(1)
            .map(|(_, now)| deadline.instant().saturating_duration_since(*now))
            .collect::<Vec<_>>();
        assert_eq!(
            budgets,
            vec![
                Duration::from_millis(90),
                Duration::from_millis(80),
                Duration::from_millis(70),
                Duration::from_millis(60),
                Duration::from_millis(50),
            ]
        );
        assert!(budgets.windows(2).all(|pair| pair[0] > pair[1]));
        assert_eq!(
            deadline.instant(),
            samples[0].1 + Duration::from_millis(100)
        );
        assert_eq!(
            log.lock().expect("shutdown log lock").as_slice(),
            [
                "Page.startScreencast",
                "Page.stopScreencast",
                "flush",
                "Target.detachFromTarget",
                "Browser.close",
            ]
        );
    }

    #[tokio::test]
    async fn shutdown_deadline_exhaustion_uses_process_force_cleanup() {
        let (result, source, deadline, log) =
            run_shutdown_fixture(Duration::from_millis(100), Duration::from_millis(30)).await;
        assert_eq!(result.unwrap_err().code, ErrorCode::ShutdownIncomplete);
        let samples = source.samples();
        assert_eq!(samples[4].0, ShutdownPhase::ProcessTerminate);
        assert_eq!(
            deadline.instant().saturating_duration_since(samples[4].1),
            Duration::ZERO
        );
        assert!(samples[5].1 >= samples[4].1);
        assert!(
            log.lock()
                .expect("shutdown log lock")
                .contains(&"Browser.close".into())
        );
    }
}
