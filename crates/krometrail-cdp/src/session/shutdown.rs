use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ShutdownPhase {
    Origin,
    CaptureStopDrainFlush,
    BrowserEventDrainFlush,
    TargetDetach,
    BrowserClose,
    ProcessTerminate,
    Complete,
}

pub(super) trait ShutdownBudgetSource: Send + Sync {
    fn now(&self, phase: ShutdownPhase) -> tokio::time::Instant;
}

struct TokioShutdownBudgetSource;

impl ShutdownBudgetSource for TokioShutdownBudgetSource {
    fn now(&self, _phase: ShutdownPhase) -> tokio::time::Instant {
        tokio::time::Instant::now()
    }
}

#[derive(Clone)]
pub(super) struct ShutdownDeadline {
    origin: tokio::time::Instant,
    timeout: Duration,
    source: Arc<dyn ShutdownBudgetSource>,
}

impl ShutdownDeadline {
    pub(super) fn new(timeout: Duration) -> Self {
        Self::with_source(timeout, Arc::new(TokioShutdownBudgetSource))
    }

    pub(super) fn with_source(timeout: Duration, source: Arc<dyn ShutdownBudgetSource>) -> Self {
        let origin = source.now(ShutdownPhase::Origin);
        Self {
            origin,
            timeout,
            source,
        }
    }

    pub(super) fn instant(&self) -> tokio::time::Instant {
        self.origin + self.timeout
    }

    pub(super) fn remaining(&self, phase: ShutdownPhase) -> Duration {
        self.instant()
            .saturating_duration_since(self.source.now(phase))
    }
}

pub(super) struct ShutdownPlan {
    pub(super) cause: crate::targets::ShutdownCause,
    pub(super) ownership: BrowserOwnership,
    pub(super) capture: Option<Arc<CaptureRuntime>>,
    pub(super) browser_events: Arc<SessionDomainAuthority>,
    pub(super) deadline: ShutdownDeadline,
    pub(super) flush_capture: bool,
}

pub(super) async fn perform_shutdown(
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

    if !plan
        .deadline
        .remaining(ShutdownPhase::BrowserEventDrainFlush)
        .is_zero()
    {
        failed |= !plan.browser_events.shutdown(deadline).await;
    } else {
        failed = true;
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

pub(super) fn finish_state(shared: &Arc<SessionShared>, state: &mut SupervisorState) {
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
