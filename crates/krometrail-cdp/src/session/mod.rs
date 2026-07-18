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
    AttachBrowser, BrowserCompatibility, BrowserConnectRequest, BrowserConnector, BrowserEventSink,
    BrowserInstallation, BrowserOperationContext, BrowserOperationRequest, BrowserOperationResult,
    BrowserOperationScope, BrowserOwnership, BrowserSessionEvent, BrowserSessionEvents,
    BrowserSessionPort, BrowserSessionState, BrowserStatus, BrowserStopOutcome,
    CurrentReferenceGeometryRequest, ErrorCode, EveryNthFrame, IdSource, IdValue,
    InteractionAnchor, InteractionTiming, KrometrailError, MonotonicClock, NonEmptyText,
    ObservationPart, PageChange, PageOperationOutcome, PageOperationResult, PageSelection,
    PageStatus, PortFuture, ProfileRef, ResolvedReferenceGeometry, Result, SessionId,
    SessionOrigin, TargetCaptureStatus, TargetVisibility, ViewportOperationResult,
    ViewportOverride,
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
    compatibility::CompatibilityProbeError,
    control::{PageControl, navigation::OperationCancellation, operation_error},
    events::{BrowserEventConfig, EventTargetBinding, SessionDomainAuthority},
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

mod evidence;
mod operations;
mod reconnect;
mod runtime;
mod shutdown;

pub(crate) use operations::{OperationExecutionContext, execute_operation};
use reconnect::reconnect_loop_transactional;
#[cfg(test)]
use reconnect::{
    AttemptCancellation, AttemptControl, AttemptFailure, PartialSessionTracker,
    recordable_reconnect_targets, restore_event_domains_and_visibility, restore_one_target,
    restore_targets, stage_reconnection_effects,
};
#[allow(unused_imports)]
pub(crate) use runtime::VisibilityProbeError;
pub(crate) use runtime::parse_visibility_result;
use runtime::{
    ConnectionResources, ProcessDeathSignal, SupervisorRuntime, apply_effects, parse_target_info,
    run_supervisor, setup_connection, setup_connection_with_target_limit,
};
#[cfg(test)]
use runtime::{TargetEventKind, parse_event, restore_session_domains};
#[cfg(test)]
use shutdown::{ShutdownBudgetSource, ShutdownPhase};
use shutdown::{ShutdownDeadline, ShutdownPlan, finish_state, perform_shutdown, stop_outcome};

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

/// The production composition root for browser sessions. It deliberately accepts the two adapter
/// seams so deterministic tests can replace launch and transport without changing supervision.
pub struct ProductionBrowserConnector {
    launcher: Arc<dyn ChromeLauncher>,
    transport_factory: Arc<dyn CdpTransportFactory>,
    config: SupervisorConfig,
    clock: Arc<dyn MonotonicClock>,
    ids: Arc<dyn IdSource>,
    capture: Option<CaptureAssembly>,
    browser_events: Option<BrowserEventAssembly>,
    interaction_evidence: Option<Arc<dyn krometrail_core::InteractionEvidenceSink>>,
}

#[derive(Clone)]
struct BrowserEventAssembly {
    clock: Arc<dyn MonotonicClock>,
    ids: Arc<dyn IdSource>,
    sink: Arc<dyn BrowserEventSink>,
    config: BrowserEventConfig,
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
            browser_events: None,
            interaction_evidence: None,
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

    pub fn with_browser_events(
        mut self,
        clock: Arc<dyn MonotonicClock>,
        ids: Arc<dyn IdSource>,
        sink: Arc<dyn BrowserEventSink>,
        config: BrowserEventConfig,
    ) -> Self {
        self.clock = Arc::clone(&clock);
        self.ids = Arc::clone(&ids);
        self.browser_events = Some(BrowserEventAssembly {
            clock,
            ids,
            sink,
            config,
        });
        self
    }

    pub fn with_interaction_evidence(
        mut self,
        sink: Arc<dyn krometrail_core::InteractionEvidenceSink>,
    ) -> Self {
        self.interaction_evidence = Some(sink);
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
        let browser_event_assembly = self.browser_events.clone();
        let interaction_evidence = self.interaction_evidence.clone();
        let control_clock = Arc::clone(&self.clock);
        let ids = Arc::clone(&self.ids);
        let every_nth_frame = requested_every_nth_frame(&request);
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
                BrowserConnectRequest::Attach(AttachBrowser { endpoint, .. }) => {
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
            let browser_events = Arc::new(
                match browser_event_assembly {
                    Some(assembly) => SessionDomainAuthority::new(
                        session_id,
                        session_origin,
                        assembly.clock,
                        assembly.ids,
                        Some(assembly.sink),
                        assembly.config,
                    ),
                    None => SessionDomainAuthority::new(
                        session_id,
                        session_origin,
                        Arc::clone(&control_clock),
                        Arc::clone(&ids),
                        None,
                        BrowserEventConfig::disabled(),
                    ),
                }
                .map_err(|_| {
                    stable_error(
                        ErrorCode::InvalidInput,
                        "browser event configuration is invalid",
                    )
                })?,
            );
            let capture = capture_assembly
                .map(|assembly| {
                    let observer: Arc<dyn CaptureObserver> = Arc::new(SessionCaptureObserver {
                        subscribers: Arc::clone(&subscribers),
                        command_tx: command_tx.clone(),
                        browser_events: Arc::clone(&browser_events),
                    });
                    let coordinator = CaptureCoordinator::new(
                        assembly.config.clone(),
                        every_nth_frame,
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
                Arc::clone(&browser_events),
                connection.browser_event_support,
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
                Arc::clone(&browser_events),
                connection.browser_event_support,
                None,
            )
            .await?;
            let process_death = Arc::new(ProcessDeathSignal::default());
            let shared = Arc::new(SessionShared {
                compatibility,
                browser_event_support: Mutex::new(connection.browser_event_support),
                ownership,
                profile: profile.unwrap_or(ProfileRef::External),
                state: Mutex::new(state.clone()),
                subscribers,
                command_tx,
                session_id,
                session_origin,
                every_nth_frame,
                capture: capture.clone(),
                browser_events: Arc::clone(&browser_events),
                interaction_evidence,
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

fn requested_every_nth_frame(request: &BrowserConnectRequest) -> EveryNthFrame {
    match request {
        BrowserConnectRequest::Launch(request) => request.every_nth_frame,
        BrowserConnectRequest::Attach(request) => request.every_nth_frame,
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
    browser_events: Arc<SessionDomainAuthority>,
}

impl CaptureObserver for SessionCaptureObserver {
    fn status_changed(&self, status: TargetCaptureStatus) {
        self.browser_events.observe_capture_status(status.clone());
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
        self.browser_events
            .observe_visibility(target_id, None, visibility);
        let _ = self.command_tx.try_send(SupervisorCommand::Input(
            SupervisorInput::CaptureVisibilityChanged {
                target_id,
                visibility,
            },
        ));
    }
}

pub(crate) struct SessionShared {
    compatibility: BrowserCompatibility,
    browser_event_support: Mutex<crate::compatibility::BrowserEventSupport>,
    ownership: BrowserOwnership,
    profile: ProfileRef,
    state: Mutex<SupervisorState>,
    subscribers: Arc<SubscriberRegistry>,
    command_tx: mpsc::Sender<SupervisorCommand>,
    session_id: SessionId,
    session_origin: SessionOrigin,
    every_nth_frame: EveryNthFrame,
    capture: Option<Arc<CaptureRuntime>>,
    browser_events: Arc<SessionDomainAuthority>,
    interaction_evidence: Option<Arc<dyn krometrail_core::InteractionEvidenceSink>>,
    pub(crate) operation_cancellation: OperationCancellation,
    stop_result: Mutex<Option<Result<BrowserStopOutcome>>>,
}

#[derive(Debug)]
enum SupervisorCommand {
    Input(SupervisorInput),
    CurrentReferenceGeometry(
        CurrentReferenceGeometryRequest,
        oneshot::Sender<Result<ResolvedReferenceGeometry>>,
    ),
    Execute(
        BrowserOperationRequest,
        BrowserOperationContext,
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

    fn capture_statuses(&self) -> Vec<TargetCaptureStatus> {
        self.shared
            .capture
            .as_ref()
            .map_or_else(Vec::new, |runtime| runtime.coordinator.statuses())
    }

    fn resolve_current_reference_geometry(
        &self,
        request: CurrentReferenceGeometryRequest,
    ) -> PortFuture<'_, Result<ResolvedReferenceGeometry>> {
        let shared = Arc::clone(&self.shared);
        Box::pin(async move {
            let (sender, receiver) = oneshot::channel();
            shared
                .command_tx
                .send(SupervisorCommand::CurrentReferenceGeometry(request, sender))
                .await
                .map_err(|_| {
                    crate::control::current_reference_error(
                        request,
                        ErrorCode::StaleReference,
                        "browser session no longer owns the current reference",
                    )
                })?;
            receiver.await.map_err(|_| {
                crate::control::current_reference_error(
                    request,
                    ErrorCode::StaleReference,
                    "current reference geometry ended without a result",
                )
            })?
        })
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
        let every_nth_frame = self.shared.every_nth_frame;
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
                every_nth_frame,
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
        context: BrowserOperationContext,
    ) -> PortFuture<'_, Result<BrowserOperationResult>> {
        let shared = Arc::clone(&self.shared);
        Box::pin(async move {
            let target_id = direct_request_target(&request);
            if context.is_cancelled() {
                return Err(request_operation_error(
                    ErrorCode::Cancelled,
                    target_id,
                    "browser operation was cancelled before dispatch",
                ));
            }
            let request_signal = context.cancellation().cloned();
            let (sender, receiver) = oneshot::channel();
            let send = shared
                .command_tx
                .send(SupervisorCommand::Execute(request, context, sender));
            match request_signal {
                Some(signal) => tokio::select! {
                    biased;
                    () = signal.cancelled() => {
                        return Err(request_operation_error(
                            ErrorCode::Cancelled,
                            target_id,
                            "browser operation was cancelled before dispatch",
                        ));
                    }
                    result = send => result,
                },
                None => send.await,
            }
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
    fn request_stride_is_extracted_before_launch_or_attach_consumes_the_request() {
        let stride = EveryNthFrame::new(23).unwrap();
        let launch = BrowserConnectRequest::Launch(krometrail_core::LaunchBrowser {
            executable: None,
            profile: krometrail_core::ManagedProfile::Temporary,
            initial_url: None,
            every_nth_frame: stride,
        });
        let attach = BrowserConnectRequest::Attach(
            krometrail_core::AttachBrowser::new("ws://127.0.0.1:9222/devtools/browser/fake")
                .unwrap()
                .with_every_nth_frame(stride),
        );
        assert_eq!(requested_every_nth_frame(&launch), stride);
        assert_eq!(requested_every_nth_frame(&attach), stride);
    }

    #[tokio::test]
    async fn session_domain_restore_is_ordered_and_has_no_redundant_commands() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        restore_session_domains(|method| {
            let calls = Arc::clone(&calls);
            async move {
                calls.lock().unwrap().push(method);
                Ok::<(), ()>(())
            }
        })
        .await
        .unwrap();
        assert_eq!(
            *calls.lock().unwrap(),
            vec!["Page.enable", "Runtime.enable", "Accessibility.enable"]
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
            every_nth_frame: EveryNthFrame::default(),
            capture: None,
            interaction_evidence: None,
            operation_cancellation: OperationCancellation::default(),
            browser_events: Arc::new(
                SessionDomainAuthority::new(
                    SessionId::from_uuid(Uuid::new_v4()),
                    SessionOrigin::new(krometrail_core::ObservedTime::from_nanos(0)),
                    Arc::new(AdapterMonotonicClock {
                        origin: Instant::now(),
                    }),
                    Arc::new(AdapterIdSource),
                    None,
                    BrowserEventConfig::default(),
                )
                .unwrap(),
            ),
            browser_event_support: Mutex::new(crate::BrowserEventSupport::default()),
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

    fn reconnect_reduction_fixture() -> (SupervisorState, Vec<SupervisorEffect>) {
        let state = reduce(
            SupervisorState::new(test_compatibility()),
            SupervisorInput::InitialTargets(vec![page_info("restored")]),
        )
        .unwrap()
        .state;
        let state = reduce(
            state,
            SupervisorInput::Attached {
                target_key: "restored".into(),
                session: TransportSessionId::new("old-session").unwrap(),
            },
        )
        .unwrap()
        .state;
        let state = reduce(
            state,
            SupervisorInput::VisibilityChanged {
                target_key: "restored".into(),
                visibility: TargetVisibility::Visible,
            },
        )
        .unwrap()
        .state;
        let state = reduce(
            state,
            SupervisorInput::ViewportOverrideApplied {
                target_key: "restored".into(),
                viewport: Some(
                    krometrail_core::ViewportMetrics::new(390, 844, 3.0, true, true).unwrap(),
                ),
            },
        )
        .unwrap()
        .state;
        let state = reduce(
            state,
            SupervisorInput::ConnectionLost(TransportClose {
                reason: NonEmptyText::new("fixture disconnect").unwrap(),
            }),
        )
        .unwrap()
        .state;
        let reduction = reduce(
            state,
            SupervisorInput::Reconnected(ReconnectedSnapshot {
                connection_generation: 1,
                compatibility: test_compatibility(),
                targets: vec![ReconnectedTarget {
                    info: page_info("restored"),
                    session: Some(TransportSessionId::new("new-session").unwrap()),
                    visibility: TargetVisibility::Unknown,
                }],
            }),
        )
        .unwrap();
        (reduction.state, reduction.effects)
    }

    #[tokio::test]
    async fn reconnect_restores_authority_domains_then_probes_visibility_with_one_deadline() {
        let transport = Arc::new(ControlledTransport::paced());
        let transport_dyn = transport.clone() as Arc<dyn CdpTransport>;
        let authority = Arc::new(
            SessionDomainAuthority::new(
                SessionId::from_uuid(Uuid::from_u128(40)),
                SessionOrigin::new(krometrail_core::ObservedTime::from_nanos(0)),
                Arc::new(AdapterMonotonicClock {
                    origin: Instant::now(),
                }),
                Arc::new(AdapterIdSource),
                None,
                BrowserEventConfig::disabled(),
            )
            .unwrap(),
        );
        let (mut state, mut effects) = reconnect_reduction_fixture();
        let attempt = AttemptControl {
            cancellation: AttemptCancellation::new(),
            deadline: tokio::time::Instant::now() + Duration::from_secs(1),
        };
        restore_event_domains_and_visibility(
            &attempt,
            &authority,
            &transport_dyn,
            crate::BrowserEventSupport::default(),
            &mut state,
            &mut effects,
        )
        .await
        .unwrap();
        assert_eq!(
            state.targets_by_key["restored"].target.visibility,
            TargetVisibility::Visible
        );
        assert_eq!(
            transport.commands(),
            [
                "Page.enable",
                "Runtime.enable",
                "Accessibility.enable",
                "Runtime.evaluate",
            ]
        );
    }

    #[tokio::test]
    async fn reconnect_replays_mobile_page_scale_before_capture_and_fails_target_locally() {
        let authority = Arc::new(
            SessionDomainAuthority::new(
                SessionId::from_uuid(Uuid::from_u128(42)),
                SessionOrigin::new(krometrail_core::ObservedTime::from_nanos(0)),
                Arc::new(AdapterMonotonicClock {
                    origin: Instant::now(),
                }),
                Arc::new(AdapterIdSource),
                None,
                BrowserEventConfig::disabled(),
            )
            .unwrap(),
        );
        let attempt = AttemptControl {
            cancellation: AttemptCancellation::new(),
            deadline: tokio::time::Instant::now() + Duration::from_secs(1),
        };

        let transport = Arc::new(ControlledTransport::paced());
        let transport_dyn = transport.clone() as Arc<dyn CdpTransport>;
        let (mut state, mut effects) = reconnect_reduction_fixture();
        restore_event_domains_and_visibility(
            &attempt,
            &authority,
            &transport_dyn,
            crate::BrowserEventSupport::default(),
            &mut state,
            &mut effects,
        )
        .await
        .unwrap();
        let staged = stage_reconnection_effects(&attempt, &transport_dyn, &mut state, &effects)
            .await
            .unwrap();
        assert!(matches!(
            transport.commands().as_slice(),
            [.., metrics, touch, scale]
                if metrics == "Emulation.setDeviceMetricsOverride"
                    && touch == "Emulation.setTouchEmulationEnabled"
                    && scale == "Emulation.setPageScaleFactor"
        ));
        assert!(staged.iter().any(|effect| matches!(
            effect,
            SupervisorEffect::StartCapture { .. } | SupervisorEffect::ResumeCapture { .. }
        )));

        let transport = Arc::new(ControlledTransport::failed("Emulation.setPageScaleFactor"));
        let transport_dyn = transport as Arc<dyn CdpTransport>;
        let failed_authority = Arc::new(
            SessionDomainAuthority::new(
                SessionId::from_uuid(Uuid::from_u128(43)),
                SessionOrigin::new(krometrail_core::ObservedTime::from_nanos(0)),
                Arc::new(AdapterMonotonicClock {
                    origin: Instant::now(),
                }),
                Arc::new(AdapterIdSource),
                None,
                BrowserEventConfig::disabled(),
            )
            .unwrap(),
        );
        let (mut state, mut effects) = reconnect_reduction_fixture();
        restore_event_domains_and_visibility(
            &attempt,
            &failed_authority,
            &transport_dyn,
            crate::BrowserEventSupport::default(),
            &mut state,
            &mut effects,
        )
        .await
        .unwrap();
        let staged = stage_reconnection_effects(&attempt, &transport_dyn, &mut state, &effects)
            .await
            .unwrap();
        let failed_target = state.targets_by_key["restored"].target.target.id();
        assert_eq!(
            state.targets_by_key["restored"].target.lifecycle,
            krometrail_core::TargetLifecycle::Failed
        );
        assert!(!staged.iter().any(|effect| matches!(
            effect,
            SupervisorEffect::StartCapture { context }
                | SupervisorEffect::ResumeCapture { context }
                if context.target_id == failed_target
        )));
    }

    #[tokio::test]
    async fn reconnect_domain_restore_is_cut_off_by_attempt_deadline() {
        let transport = Arc::new(ControlledTransport::stalled("Page.enable"));
        let transport_dyn = transport as Arc<dyn CdpTransport>;
        let authority = Arc::new(
            SessionDomainAuthority::new(
                SessionId::from_uuid(Uuid::from_u128(41)),
                SessionOrigin::new(krometrail_core::ObservedTime::from_nanos(0)),
                Arc::new(AdapterMonotonicClock {
                    origin: Instant::now(),
                }),
                Arc::new(AdapterIdSource),
                None,
                BrowserEventConfig::disabled(),
            )
            .unwrap(),
        );
        let (mut state, mut effects) = reconnect_reduction_fixture();
        let attempt = AttemptControl {
            cancellation: AttemptCancellation::new(),
            deadline: tokio::time::Instant::now() + Duration::from_millis(20),
        };
        assert_eq!(
            restore_event_domains_and_visibility(
                &attempt,
                &authority,
                &transport_dyn,
                crate::BrowserEventSupport::default(),
                &mut state,
                &mut effects,
            )
            .await,
            Err(AttemptFailure::TimedOut)
        );
    }

    #[tokio::test]
    async fn stalled_target_attachment_is_cut_off_by_attempt_deadline() {
        let transport = Arc::new(ControlledTransport::stalled("Target.attachToTarget"));
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
    async fn cancellation_interrupts_a_stalled_target_attachment_immediately() {
        let transport = Arc::new(ControlledTransport::stalled("Target.attachToTarget"));
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
        fail_method: Mutex<Option<String>>,
        commands: Mutex<Vec<String>>,
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
                    fail_method: Mutex::new(None),
                    commands: Mutex::new(Vec::new()),
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

        fn failed(method: &str) -> Self {
            let transport = Self::paced();
            *transport
                .state
                .fail_method
                .lock()
                .expect("fail method lock") = Some(method.to_owned());
            transport
        }

        fn started(&self) -> Arc<Notify> {
            Arc::clone(&self.state.started)
        }

        fn maximum_active(&self) -> usize {
            self.state.maximum_active.load(Ordering::Acquire)
        }

        fn commands(&self) -> Vec<String> {
            self.state
                .commands
                .lock()
                .expect("controlled command lock")
                .clone()
        }
    }

    impl CdpTransport for ControlledTransport {
        fn send_raw(
            &self,
            _scope: &CommandScope,
            method: &str,
            _params: Value,
        ) -> TransportFuture<'_, std::result::Result<Value, TransportError>> {
            self.state
                .commands
                .lock()
                .expect("controlled command lock")
                .push(method.to_owned());
            let stalled = self
                .state
                .stall_method
                .lock()
                .expect("stall method lock")
                .as_deref()
                == Some(method);
            let failed = self
                .state
                .fail_method
                .lock()
                .expect("fail method lock")
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
                if failed {
                    Err(TransportError::CommandFailed)
                } else if stalled {
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
        Result<shutdown::ShutdownReport>,
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
                EveryNthFrame::default(),
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
            device_scale_factor: krometrail_core::DeviceScaleFactor::new(1.0).unwrap(),
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
            browser_event_support: crate::BrowserEventSupport::default(),
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
                browser_events: Arc::new(
                    SessionDomainAuthority::new(
                        session_id,
                        SessionOrigin::new(krometrail_core::ObservedTime::from_nanos(0)),
                        Arc::new(ShutdownTestClock),
                        Arc::new(AdapterIdSource),
                        None,
                        BrowserEventConfig::default(),
                    )
                    .unwrap(),
                ),
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
                ShutdownPhase::BrowserEventDrainFlush,
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
                Duration::from_millis(40),
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
        assert_eq!(result.unwrap().quality, shutdown::ShutdownQuality::Degraded);
        let samples = source.samples();
        assert_eq!(samples[5].0, ShutdownPhase::ProcessTerminate);
        assert_eq!(
            deadline.instant().saturating_duration_since(samples[5].1),
            Duration::ZERO
        );
        assert!(samples[6].1 >= samples[5].1);
        assert!(
            !log.lock()
                .expect("shutdown log lock")
                .contains(&"Browser.close".into())
        );
    }
}
