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
    time::Duration,
};

use futures_util::{StreamExt, stream::FuturesUnordered};

use krometrail_core::{
    AttachBrowser, BrowserCompatibility, BrowserConnectRequest, BrowserConnector,
    BrowserInstallation, BrowserOwnership, BrowserSessionEvent, BrowserSessionEvents,
    BrowserSessionPort, BrowserSessionState, BrowserStopOutcome, ErrorCode, KrometrailError,
    NonEmptyText, PortFuture, ProfileRef, Result, SupervisedTarget, TargetVisibility,
};
use serde_json::Value;
use tokio::{
    sync::{Notify, mpsc, oneshot},
    task::JoinHandle,
};

use crate::{
    compatibility::{CompatibilityProbeError, probe_compatibility_with_target_limit},
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

/// The production composition root for browser sessions. It deliberately accepts the two adapter
/// seams so deterministic tests can replace launch and transport without changing supervision.
pub struct ProductionBrowserConnector {
    launcher: Arc<dyn ChromeLauncher>,
    transport_factory: Arc<dyn CdpTransportFactory>,
    config: SupervisorConfig,
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
        }
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
                false,
            )
            .await?;
            // Initial reconciliation is complete only after all attach effects and visibility probes.
            // Publishing Ready here keeps the returned port truthful: callers cannot observe a session
            // that claims readiness while discovery is still being rebuilt.
            let previous_state = state.session_state;
            state.session_state = BrowserSessionState::Ready;
            state.revision = state.revision.saturating_add(1);
            tracing::info!(
                previous_state = previous_state.as_str(),
                next_state = BrowserSessionState::Ready.as_str(),
                connection_generation = state.connection_generation,
                "browser.session.state_changed"
            );
            subscribers.publish(BrowserSessionEvent::SessionStateChanged {
                state: BrowserSessionState::Ready,
            });
            let (command_tx, command_rx) = mpsc::channel(64);
            let process_death = Arc::new(ProcessDeathSignal::default());
            let shared = Arc::new(SessionShared {
                compatibility,
                ownership,
                profile: profile.unwrap_or(ProfileRef::External),
                state: Mutex::new(state.clone()),
                subscribers,
                command_tx,
                stop_result: Mutex::new(None),
            });
            let task_shared = Arc::clone(&shared);
            let endpoint = Arc::new(endpoint);
            let task = tokio::spawn(run_supervisor(
                task_shared,
                state,
                Some(connection),
                SupervisorRuntime {
                    endpoint,
                    factory: transport_factory,
                    process,
                    profile: profile_lease,
                    config,
                    process_death,
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

struct SessionShared {
    compatibility: BrowserCompatibility,
    ownership: BrowserOwnership,
    profile: ProfileRef,
    state: Mutex<SupervisorState>,
    subscribers: Arc<SubscriberRegistry>,
    command_tx: mpsc::Sender<SupervisorCommand>,
    stop_result: Mutex<Option<Result<BrowserStopOutcome>>>,
}

#[derive(Debug)]
enum SupervisorCommand {
    Input(SupervisorInput),
    Stop(oneshot::Sender<Result<BrowserStopOutcome>>),
}

struct ProductionSession {
    shared: Arc<SessionShared>,
    task: Mutex<Option<JoinHandle<()>>>,
}

impl BrowserSessionPort for ProductionSession {
    fn compatibility(&self) -> &BrowserCompatibility {
        &self.shared.compatibility
    }

    fn ownership(&self) -> BrowserOwnership {
        self.shared.ownership
    }

    fn profile(&self) -> &ProfileRef {
        &self.shared.profile
    }

    fn state(&self) -> BrowserSessionState {
        self.shared
            .state
            .lock()
            .expect("session state lock")
            .session_state
    }

    fn targets(&self) -> PortFuture<'_, Result<Vec<SupervisedTarget>>> {
        let targets = self
            .shared
            .state
            .lock()
            .expect("session state lock")
            .targets();
        Box::pin(std::future::ready(Ok(targets)))
    }

    fn subscribe(&self) -> PortFuture<'_, Result<Box<dyn BrowserSessionEvents>>> {
        let events = self.shared.subscribers.subscribe();
        Box::pin(std::future::ready(Ok(events)))
    }

    fn stop(&self) -> PortFuture<'_, Result<BrowserStopOutcome>> {
        let shared = Arc::clone(&self.shared);
        Box::pin(async move {
            if let Some(result) = shared.stop_result.lock().expect("stop result lock").clone() {
                return result;
            }
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
    fn restart_pumps(&mut self, sender: mpsc::Sender<SupervisorCommand>, generation: u64) {
        self.abort_pumps();
        let subscriptions = std::mem::take(&mut self.subscriptions);
        self.pump_handles = subscriptions
            .into_iter()
            .map(|(kind, events)| {
                tokio::spawn(pump_events(kind, events, sender.clone(), generation))
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
    allow_shutdown: bool,
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
					.and_then(|value| value.get("sessionId").and_then(Value::as_str).map(str::to_owned))
					.and_then(|session| TransportSessionId::new(session).ok())
					.map(|session| SupervisorInput::Attached { target_key: target_key.clone(), session })
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
			SupervisorEffect::ProbeInitialVisibility { target_key, session } => {
				if let Ok(value) = transport
					.send_raw(
						&CommandScope::Session(session),
						"Runtime.evaluate",
						serde_json::json!({"expression": "document.visibilityState", "returnByValue": true}),
					)
					.await
				{
					let visibility = value
						.pointer("/result/result/value")
						.and_then(Value::as_str)
						.map(|value| if value == "hidden" { TargetVisibility::Hidden } else { TargetVisibility::Visible });
					if let Some(visibility) = visibility {
						let compatibility = state.compatibility.clone();
						let previous = std::mem::replace(state, SupervisorState::new(compatibility));
						let reduction = reduce(
							previous,
							SupervisorInput::VisibilityChanged { target_key, visibility },
						)?;
						*state = reduction.state;
						queue.extend(reduction.effects);
					}
				}
			}
			SupervisorEffect::BeginReconnect => {}
			SupervisorEffect::Shutdown { cause: _ } if allow_shutdown => {
				// The outer supervisor owns shutdown sequencing. The flag prevents an initial
				// reconciliation failure from trying to close a resource it has not returned yet.
			}
			SupervisorEffect::Shutdown { cause: _ } => {}
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
    runtime: SupervisorRuntime,
    mut commands: mpsc::Receiver<SupervisorCommand>,
) {
    if let Some(connection) = connection.as_mut() {
        let sender = shared.command_tx.clone();
        connection.restart_pumps(sender, state.connection_generation);
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
                        if let Some(connection) = connection.as_ref() {
                            let _ = apply_effects(
                                &mut state,
                                reduction.effects,
                                Arc::clone(&connection.transport),
                                Arc::clone(&shared.subscribers),
                                false,
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
                                cause,
                                shared.ownership,
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
                        if let Some(connection) = connection.as_ref() {
                            let _ = apply_effects(
                                &mut state,
                                reduction.effects,
                                Arc::clone(&connection.transport),
                                Arc::clone(&shared.subscribers),
                                false,
                            )
                            .await;
                        }
                        let result = perform_shutdown(
                            &mut connection,
                            &runtime.process,
                            &runtime.profile,
                            &state,
                            crate::targets::ShutdownCause::StopRequested,
                            shared.ownership,
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
    if let Some(current) = connection.as_ref() {
        let _ = apply_effects(
            state,
            reduction.effects,
            Arc::clone(&current.transport),
            Arc::clone(&shared.subscribers),
            false,
        )
        .await;
    }
    let result = perform_shutdown(
        connection,
        &runtime.process,
        &runtime.profile,
        state,
        cause,
        shared.ownership,
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
    events: Vec<BrowserSessionEvent>,
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
    let visibility = match visibility_value
        .pointer("/result/result/value")
        .or_else(|| visibility_value.pointer("/result/value"))
        .and_then(Value::as_str)
    {
        Some("hidden") => TargetVisibility::Hidden,
        Some(_) => TargetVisibility::Visible,
        None => return Err(AttemptFailure::Failed),
    };
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
) -> std::result::Result<Vec<BrowserSessionEvent>, AttemptFailure> {
    let mut events = Vec::new();
    for effect in effects {
        match effect {
            SupervisorEffect::Publish(event) => events.push(event.clone()),
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
            SupervisorEffect::Attach { .. }
            | SupervisorEffect::ProbeInitialVisibility { .. }
            | SupervisorEffect::BeginReconnect
            | SupervisorEffect::Shutdown { .. } => return Err(AttemptFailure::Failed),
        }
    }
    Ok(events)
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
    let events =
        match stage_reconnection_effects(&attempt, &connection.transport, &reduction.effects).await
        {
            Ok(events) => events,
            Err(error) => {
                discard_partial_connection(&mut connection, &sessions).await;
                drop(connection);
                return Err(error);
            }
        };
    Ok(PreparedReconnection {
        connection,
        state: reduction.state,
        events,
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
                );
                *state = prepared.state;
                *connection = Some(prepared.connection);
                *shared.state.lock().expect("session state lock") = state.clone();
                for event in prepared.events.drain(..) {
                    shared.subscribers.publish(event);
                }
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
        if let Some(current) = connection.as_ref() {
            let _ = apply_effects(
                state,
                reduction.effects,
                Arc::clone(&current.transport),
                Arc::clone(&shared.subscribers),
                false,
            )
            .await;
        }
        let _ = perform_shutdown(
            connection,
            &runtime.process,
            &runtime.profile,
            state,
            crate::targets::ShutdownCause::ReconnectExhausted,
            shared.ownership,
        )
        .await;
        finish_state(shared, state);
    }
    true
}

async fn perform_shutdown(
    connection: &mut Option<ConnectionResources>,
    process: &Option<Arc<Mutex<Option<ManagedChromeProcess>>>>,
    profile: &Option<Arc<Mutex<Option<ProfileLease>>>>,
    state: &SupervisorState,
    cause: crate::targets::ShutdownCause,
    ownership: BrowserOwnership,
) -> Result<()> {
    let started = std::time::Instant::now();
    let mut failed = false;
    if let Some(connection) = connection.as_mut() {
        connection.abort_pumps();
        for session in state.target_key_by_session.keys() {
            if connection
                .transport
                .send_raw(
                    &CommandScope::Browser,
                    "Target.detachFromTarget",
                    serde_json::json!({"sessionId": session.as_str()}),
                )
                .await
                .is_err()
            {
                failed = true;
            }
        }
        if ownership == BrowserOwnership::Managed
            && matches!(
                cause,
                crate::targets::ShutdownCause::StopRequested
                    | crate::targets::ShutdownCause::BrowserProcessTerminated
                    | crate::targets::ShutdownCause::ReconnectExhausted
            )
            && connection
                .transport
                .send_raw(
                    &CommandScope::Browser,
                    "Browser.close",
                    Value::Object(Default::default()),
                )
                .await
                .is_err()
            && !matches!(
                cause,
                crate::targets::ShutdownCause::BrowserProcessTerminated
            )
        {
            failed = true;
        }
    }
    if let Some(process) = process {
        let owned = process.lock().expect("process lock").take();
        if let Some(mut owned) = owned {
            if owned.terminate(Duration::from_secs(3)).await.is_err() {
                failed = true;
            }
        }
    }
    if let Some(profile) = profile {
        profile.lock().expect("profile lock").take();
    }
    *connection = None;
    if failed {
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

fn stable_error(code: ErrorCode, message: &'static str) -> KrometrailError {
    KrometrailError::from_browser_failure(code, NonEmptyText::new(message).unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::TransportFuture;
    use std::sync::atomic::AtomicUsize;

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
}
