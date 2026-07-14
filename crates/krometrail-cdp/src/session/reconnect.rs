use super::*;

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
pub(super) struct AttemptCancellation {
    cancelled: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

impl AttemptCancellation {
    pub(super) fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            notify: Arc::new(Notify::new()),
        }
    }

    pub(super) fn cancel(&self) {
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
pub(super) struct AttemptControl {
    pub(super) cancellation: AttemptCancellation,
    pub(super) deadline: tokio::time::Instant,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum AttemptFailure {
    Failed,
    TimedOut,
    Cancelled,
}

impl AttemptControl {
    pub(super) async fn race<F, T>(&self, future: F) -> std::result::Result<T, AttemptFailure>
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
pub(super) struct PartialSessionTracker {
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

pub(super) fn recordable_reconnect_targets(
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

pub(super) async fn restore_one_target(
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
    // later effect and must not become an implicit side effect of reconnect supervision. The same
    // ordered helper is used by initial attachment so dialog events and visibility probing have one
    // restoration contract.
    restore_session_domains(|method| async {
        attempt
            .command(
                &transport,
                &scope,
                method,
                Value::Object(Default::default()),
            )
            .await
            .map(|_| ())
    })
    .await?;
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

pub(super) async fn restore_targets(
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
            | SupervisorEffect::RestoreSessionDomains { .. }
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

fn reject_operation_during_reconnect(
    request: BrowserOperationRequest,
    sender: oneshot::Sender<Result<BrowserOperationResult>>,
) {
    let target_id = direct_request_target(&request);
    let _ = sender.send(Err(request_operation_error(
        ErrorCode::BrowserDisconnected,
        target_id,
        "browser is reconnecting; operation was not replayed",
    )));
}

pub(super) async fn reconnect_loop_transactional(
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
                        Some(SupervisorCommand::Execute(request, _context, sender)) => {
                            reject_operation_during_reconnect(request, sender);
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
                        Some(SupervisorCommand::Execute(request, _context, sender)) => {
                            reject_operation_during_reconnect(request, sender);
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
