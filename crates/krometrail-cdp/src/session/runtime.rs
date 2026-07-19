use super::*;

const TARGET_EVENT_NAMES: &[(&str, TargetEventKind)] = &[
    ("Target.targetCreated", TargetEventKind::Created),
    ("Target.targetInfoChanged", TargetEventKind::InfoChanged),
    ("Target.targetDestroyed", TargetEventKind::Destroyed),
    ("Target.attachedToTarget", TargetEventKind::Attached),
    ("Target.detachedFromTarget", TargetEventKind::Detached),
];

// These domains are required by both the first control operation and a reconstructed target. Keep
// the order stable: Page must be enabled before its dialog events can be associated with a flat
// session, while Runtime and Accessibility are prerequisites for visibility and live observation.
#[cfg(test)]
const SESSION_RESTORE_DOMAINS: [&str; 3] =
    ["Page.enable", "Runtime.enable", "Accessibility.enable"];

#[cfg(test)]
pub(super) async fn restore_session_domains<F, Fut, E>(mut send: F) -> std::result::Result<(), E>
where
    F: FnMut(&'static str) -> Fut,
    Fut: std::future::Future<Output = std::result::Result<(), E>>,
{
    for method in SESSION_RESTORE_DOMAINS {
        send(method).await?;
    }
    Ok(())
}

#[derive(Clone, Copy, Debug)]
pub(super) enum TargetEventKind {
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

pub(super) struct ConnectionResources {
    pub(super) transport: Arc<dyn CdpTransport>,
    pub(super) subscriptions: Vec<(TargetEventKind, Box<dyn TransportEvents>)>,
    pub(super) targets: Vec<TransportTargetInfo>,
    pub(super) compatibility: BrowserCompatibility,
    pub(super) browser_event_support: crate::compatibility::BrowserEventSupport,
    pub(super) pump_handles: Vec<JoinHandle<()>>,
}

impl ConnectionResources {
    pub(super) fn restart_pumps(
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

    pub(super) fn abort_pumps(&mut self) {
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

pub(super) async fn setup_connection(
    transport: Arc<dyn CdpTransport>,
) -> std::result::Result<ConnectionResources, CompatibilityProbeError> {
    setup_connection_with_target_limit(transport, usize::MAX).await
}

pub(super) async fn setup_connection_with_target_limit(
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
    let probe = crate::compatibility::probe_compatibility_details_with_target_limit(
        transport.as_ref(),
        target_limit,
    )
    .await?;
    let compatibility = probe.compatibility;
    let browser_event_support = probe.browser_events;
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
        browser_event_support,
        pump_handles: Vec::new(),
    })
}

fn fallback_capture_geometry() -> crate::capture::CaptureGeometry {
    crate::capture::CaptureGeometry {
        viewport: krometrail_core::PixelDimensions::new(1, 1)
            .expect("one-by-one fallback capture geometry is valid"),
        device_scale_factor: krometrail_core::DeviceScaleFactor::new(1.0)
            .expect("one is a valid scale"),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn apply_effects(
    state: &mut SupervisorState,
    effects: Vec<SupervisorEffect>,
    transport: Arc<dyn CdpTransport>,
    subscribers: Arc<SubscriberRegistry>,
    capture: Option<Arc<CaptureRuntime>>,
    browser_events: Arc<SessionDomainAuthority>,
    browser_event_support: crate::compatibility::BrowserEventSupport,
    shutdown_deadline: Option<ShutdownDeadline>,
) -> Result<()> {
    let mut queue = VecDeque::from(effects);
    while let Some(effect) = queue.pop_front() {
        match effect {
            SupervisorEffect::Publish(event) => {
                observe_supervisor_event(browser_events.as_ref(), &event);
                let terminal_target = match &event {
                    BrowserSessionEvent::TargetClosed { target_id }
                    | BrowserSessionEvent::TargetFailed { target_id, .. } => Some(*target_id),
                    _ => None,
                };
                subscribers.publish(event);
                if let Some(target_id) = terminal_target {
                    // Close acceptance immediately, but leave the per-target writer registered
                    // so aggregate shutdown remains the only blocking flush boundary.
                    browser_events.retire_target(target_id, None);
                }
            }
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
                            .and_then(|session| TransportSessionId::new(session.to_owned()).ok())
                    })
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
            SupervisorEffect::RestoreViewport { context, viewport } => {
                let current = state
                    .targets_by_key
                    .get(&context.target_key)
                    .is_some_and(|target| {
                        state.connection_generation == context.connection_generation
                            && target.target.target.id() == context.target_id
                            && target.target.attachment_generation == context.attachment_generation
                            && target.transport_session.as_ref() == Some(&context.transport_session)
                    });
                let bound = crate::control::BoundTarget {
                    target_id: context.target_id,
                    browser_target_key: context.target_key.clone(),
                    attachment_generation: context.attachment_generation,
                    transport_session: context.transport_session,
                    visibility: krometrail_core::TargetVisibility::Unknown,
                };
                if !current
                    || crate::control::viewport::apply_viewport(
                        transport.as_ref(),
                        &bound,
                        Some(viewport),
                    )
                    .await
                    .is_err()
                {
                    queue.retain(|effect| match effect {
                        SupervisorEffect::RestoreSessionDomains { target_key, .. }
                        | SupervisorEffect::ProbeInitialVisibility { target_key, .. }
                        | SupervisorEffect::Attach { target_key } => {
                            target_key != &context.target_key
                        }
                        SupervisorEffect::RestoreViewport {
                            context: queued, ..
                        } => queued.target_key != context.target_key,
                        SupervisorEffect::StartCapture { context: queued }
                        | SupervisorEffect::ResumeCapture { context: queued }
                        | SupervisorEffect::SuspendCapture { context: queued }
                        | SupervisorEffect::StopCapture { context: queued } => {
                            queued.target_id != context.target_id
                        }
                        SupervisorEffect::Detach { .. }
                        | SupervisorEffect::Publish(_)
                        | SupervisorEffect::BeginReconnect
                        | SupervisorEffect::Shutdown { .. } => true,
                    });
                    let compatibility = state.compatibility.clone();
                    let previous = std::mem::replace(state, SupervisorState::new(compatibility));
                    let reduction = reduce(
                        previous,
                        SupervisorInput::TargetAttachFailed {
                            target_key: context.target_key,
                        },
                    )?;
                    *state = reduction.state;
                    queue.extend(reduction.effects);
                }
            }
            SupervisorEffect::RestoreSessionDomains {
                target_key,
                session,
            } => {
                let binding =
                    state
                        .targets_by_key
                        .get(&target_key)
                        .map(|target| EventTargetBinding {
                            target_id: target.target.target.id(),
                            connection_generation: state.connection_generation,
                            attachment_generation: target.target.attachment_generation,
                            transport_session: session.clone(),
                        });
                let restored = match binding {
                    Some(binding) => browser_events
                        .restore_target(binding, transport.as_ref(), browser_event_support)
                        .await
                        .map(|_| ())
                        .map_err(|_| ()),
                    None => Err(()),
                };
                if restored.is_ok() {
                    queue.push_front(SupervisorEffect::ProbeInitialVisibility {
                        target_key,
                        session,
                    });
                } else {
                    let compatibility = state.compatibility.clone();
                    let previous = std::mem::replace(state, SupervisorState::new(compatibility));
                    let reduction =
                        reduce(previous, SupervisorInput::TargetAttachFailed { target_key })?;
                    *state = reduction.state;
                    queue.extend(reduction.effects);
                }
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
							observed_at: browser_events
								.session_time()
								.unwrap_or(krometrail_core::SessionTime::ZERO),
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
                    let target_key = state
                        .targets_by_key
                        .iter()
                        .find(|(_, target)| target.target.target.id() == context.target_id)
                        .map(|(key, _)| key.clone())
                        .unwrap_or_default();
                    let bound = crate::control::BoundTarget {
                        target_id: context.target_id,
                        browser_target_key: target_key,
                        attachment_generation: context.attachment_generation,
                        transport_session: context.transport_session.clone(),
                        visibility: krometrail_core::TargetVisibility::Unknown,
                    };
                    let declared_override = state
                        .targets_by_key
                        .values()
                        .find(|target| target.target.target.id() == context.target_id)
                        .and_then(|target| target.viewport_override);
                    let geometry = crate::control::viewport::observe_effective_viewport(
                        transport.as_ref(),
                        &bound,
                        declared_override,
                    )
                    .await
                    .and_then(crate::control::viewport::capture_geometry);
                    let Ok(geometry) = geometry else {
                        let target_key = state
                            .targets_by_key
                            .iter()
                            .find(|(_, target)| target.target.target.id() == context.target_id)
                            .map(|(key, _)| key.clone());
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
                        continue;
                    };
                    let target = CaptureTarget {
                        session_id: capture.session_id,
                        session_origin: capture.session_origin,
                        target_id: context.target_id,
                        connection_generation: context.connection_generation,
                        attachment_generation: context.attachment_generation,
                        transport_session: context.transport_session,
                        geometry,
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
                        geometry: fallback_capture_geometry(),
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
                        geometry: fallback_capture_geometry(),
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
                browser_events
                    .retire_target(context.target_id, Some(context.attachment_generation));
            }
            SupervisorEffect::BeginReconnect => {
                browser_events.suspend_connection(state.connection_generation);
            }
            SupervisorEffect::Shutdown { cause: _ } => {
                // The outer supervisor owns the aggregate shutdown sequencing. Capture effects
                // above have already fenced acceptance before this marker is handled.
            }
        }
    }
    Ok(())
}

fn observe_supervisor_event(browser_events: &SessionDomainAuthority, event: &BrowserSessionEvent) {
    match event {
        BrowserSessionEvent::TargetDiscovered { target }
        | BrowserSessionEvent::TargetChanged { target } => {
            browser_events.observe_target_lifecycle(
                target.target.id(),
                target.attachment_generation,
                target.lifecycle,
            );
            browser_events.observe_visibility(
                target.target.id(),
                Some(target.attachment_generation),
                target.visibility,
            );
        }
        BrowserSessionEvent::TargetClosed { target_id } => {
            browser_events.observe_current_target_lifecycle(
                *target_id,
                krometrail_core::TargetLifecycle::Closed,
            );
        }
        BrowserSessionEvent::TargetFailed { target_id, .. } => {
            browser_events.observe_current_target_lifecycle(
                *target_id,
                krometrail_core::TargetLifecycle::Failed,
            );
        }
        BrowserSessionEvent::SessionStateChanged { .. }
        | BrowserSessionEvent::SessionFailed { .. }
        | BrowserSessionEvent::SelectedTargetChanged { .. }
        | BrowserSessionEvent::CaptureStateChanged { .. }
        | BrowserSessionEvent::CaptureGapDeclared { .. }
        | BrowserSessionEvent::DownloadStateChanged { .. } => {}
    }
}

pub(super) struct SupervisorRuntime {
    pub(super) endpoint: Arc<crate::LocalCdpEndpoint>,
    pub(super) factory: Arc<dyn CdpTransportFactory>,
    pub(super) process: Option<Arc<Mutex<Option<ManagedChromeProcess>>>>,
    pub(super) profile: Option<Arc<Mutex<Option<ProfileLease>>>>,
    pub(super) config: SupervisorConfig,
    pub(super) process_death: Arc<ProcessDeathSignal>,
    pub(super) capture_timeout: Duration,
}

#[derive(Default)]
pub(super) struct ProcessDeathSignal {
    exit: Mutex<Option<crate::launcher::SanitizedProcessExit>>,
    notify: Notify,
}

impl ProcessDeathSignal {
    pub(super) fn record(&self, exit: crate::launcher::SanitizedProcessExit) {
        *self.exit.lock().expect("process death lock") = Some(exit);
        self.notify.notify_waiters();
    }

    pub(super) async fn wait(&self) -> crate::launcher::SanitizedProcessExit {
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

pub(super) async fn run_supervisor(
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
                                Arc::clone(&shared.browser_events),
                                connection.browser_event_support,
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
                            if let Some(downloads) = shared.downloads.as_ref() {
                                let transport =
                                    connection.as_ref().map(|value| value.transport.as_ref());
                                let _ = downloads.shutdown(transport).await;
                            }
                            let _ = perform_shutdown(
                                &mut connection,
                                &runtime.process,
                                &runtime.profile,
                                &state,
                                ShutdownPlan {
                                    cause,
                                    ownership: shared.ownership,
                                    capture: shared.capture.clone(),
                                    browser_events: Arc::clone(&shared.browser_events),
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
            SupervisorCommand::CurrentReferenceGeometry(request, sender) => {
                let result = match connection.as_ref() {
                    Some(connection) => {
                        page_control
                            .current_reference_geometry(
                                connection.transport.as_ref(),
                                &state,
                                request,
                            )
                            .await
                    }
                    None => Err(crate::control::current_reference_error(
                        request,
                        ErrorCode::StaleReference,
                        "browser session has no current reference generation",
                    )),
                };
                let _ = sender.send(result);
            }
            SupervisorCommand::RefreshCaptureGeometry { transition } => {
                if let (Some(connection), Some(capture)) =
                    (connection.as_ref(), shared.capture.as_ref())
                {
                    let _ = refresh_capture_geometry(
                        &state,
                        connection.transport.as_ref(),
                        capture,
                        transition,
                    )
                    .await;
                }
            }
            SupervisorCommand::Execute(request, context, sender) => {
                let target_id = direct_request_target(&request);
                let cancellation = shared.operation_cancellation.for_request(&context);
                let result = if cancellation.request_is_cancelled() {
                    Err(request_operation_error(
                        ErrorCode::Cancelled,
                        target_id,
                        "browser operation was cancelled before dispatch",
                    ))
                } else {
                    match connection.as_ref() {
                        Some(connection) => {
                            execute_operation(
                                &mut page_control,
                                &mut state,
                                Arc::clone(&connection.transport),
                                &shared,
                                request,
                                &cancellation,
                                OperationExecutionContext::default(),
                            )
                            .await
                        }
                        None => Err(request_operation_error(
                            ErrorCode::BrowserDisconnected,
                            target_id,
                            "browser transport is unavailable",
                        )),
                    }
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
                                Arc::clone(&shared.browser_events),
                                connection.browser_event_support,
                                Some(shutdown_deadline.clone()),
                            )
                            .await;
                        }
                        let download_cleanup = match shared.downloads.as_ref() {
                            Some(downloads) => {
                                downloads
                                    .shutdown(
                                        connection.as_ref().map(|value| value.transport.as_ref()),
                                    )
                                    .await
                            }
                            None => Ok(()),
                        };
                        let result = perform_shutdown(
                            &mut connection,
                            &runtime.process,
                            &runtime.profile,
                            &state,
                            ShutdownPlan {
                                cause: crate::targets::ShutdownCause::StopRequested,
                                ownership: shared.ownership,
                                capture: shared.capture.clone(),
                                browser_events: Arc::clone(&shared.browser_events),
                                deadline: shutdown_deadline,
                                flush_capture: true,
                            },
                        )
                        .await;
                        let outcome = match (result, download_cleanup) {
                            (Ok(report), Ok(())) => Ok(stop_outcome(&report, shared.ownership)),
                            (_, Err(error)) => Err(error),
                            (Err(error), Ok(())) => Err(error),
                        };
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

pub(super) async fn refresh_capture_geometry(
    state: &SupervisorState,
    transport: &dyn CdpTransport,
    capture: &CaptureRuntime,
    transition: crate::capture::CaptureGeometryTransition,
) -> bool {
    let Some((target_key, target)) = state.targets_by_key.iter().find(|(_, target)| {
        target.target.target.id() == transition.target_id()
            && target.target.attachment_generation == transition.attachment_generation()
    }) else {
        return false;
    };
    let Some(transport_session) = target.transport_session.clone() else {
        return false;
    };
    let bound = crate::control::BoundTarget {
        target_id: transition.target_id(),
        browser_target_key: target_key.clone(),
        attachment_generation: transition.attachment_generation(),
        transport_session,
        visibility: target.target.visibility,
    };
    let declared_override = target.viewport_override;
    const ATTEMPTS: usize = 5;
    const RETRY_DELAY: Duration = Duration::from_millis(50);
    let mut last_error = None;
    let mut override_replayed = false;
    for attempt in 0..ATTEMPTS {
        let geometry = crate::control::viewport::observe_effective_viewport(
            transport,
            &bound,
            declared_override,
        )
        .await
        .and_then(crate::control::viewport::capture_geometry);
        match geometry {
            Ok(geometry) => {
                return capture
                    .coordinator
                    .commit_geometry_transition(transition, geometry);
            }
            Err(error)
                if declared_override.is_some()
                    && error.code == krometrail_core::ErrorCode::TargetFailed
                    && !override_replayed =>
            {
                override_replayed = true;
                last_error =
                    crate::control::viewport::apply_viewport(transport, &bound, declared_override)
                        .await
                        .err()
                        .or(Some(error));
            }
            Err(error) => last_error = Some(error),
        }
        if attempt + 1 < ATTEMPTS {
            tokio::time::sleep(RETRY_DELAY).await;
        }
    }
    if let Some(error) = last_error {
        tracing::warn!(
            event = "capture.geometry_refresh.pending",
            error_code = error.code.as_str(),
            error_message = %error.message,
            attempts = ATTEMPTS,
            target_id = %transition.target_id(),
            attachment_generation = transition.attachment_generation(),
            "capture.geometry_refresh.pending"
        );
    }
    false
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

pub(super) fn parse_event(kind: TargetEventKind, event: NamedEvent) -> Option<SupervisorInput> {
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

pub(super) fn parse_target_info(value: &Value) -> Option<TransportTargetInfo> {
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
    .map(|info| {
        info.with_opener_target_key(
            value
                .get("openerId")
                .and_then(Value::as_str)
                .map(str::to_owned),
        )
    })
}
