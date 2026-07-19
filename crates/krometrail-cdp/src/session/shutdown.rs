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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RemainingResource {
    ManagedProcess,
    ManagedProfile,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ShutdownReport {
    pub(super) quality: ShutdownQuality,
    pub(super) failed_phase: Option<ShutdownFailurePhase>,
    pub(super) capture_failure: Option<krometrail_core::CaptureFailure>,
    pub(super) remaining: Vec<RemainingResource>,
}

pub(super) fn stop_outcome(
    report: &ShutdownReport,
    ownership: BrowserOwnership,
) -> BrowserStopOutcome {
    let closure = match ownership {
        BrowserOwnership::Attached => BrowserClosure::Detached,
        BrowserOwnership::Managed => BrowserClosure::ManagedBrowserClosed,
    };
    let recovery = (report.quality == ShutdownQuality::Degraded).then(|| {
        let guidance = match report
            .capture_failure
            .as_ref()
            .and_then(|failure| failure.cause().persistence.as_ref())
            .map(|failure| failure.recoverability())
        {
            Some(PersistenceRecoverability::WriterUsable) => {
                "start a new browser session before relying on temporal history"
            }
            Some(PersistenceRecoverability::WriterTerminal) => {
                "restart the Krometrail MCP process, then start a new browser session"
            }
            None if report.capture_failure.is_some() => {
                "inspect browser status and diagnostics, then start a new browser session"
            }
            None => "inspect shutdown diagnostics before starting a new browser session",
        };
        NonEmptyText::new(guidance).expect("shutdown recovery is non-empty")
    });
    BrowserStopOutcome::new(
        closure,
        report.quality,
        report.failed_phase,
        report.capture_failure.clone(),
        recovery,
    )
    .expect("shutdown report maintains stop outcome invariants")
}

pub(super) async fn perform_shutdown(
    connection: &mut Option<ConnectionResources>,
    process: &Option<Arc<Mutex<Option<ManagedChromeProcess>>>>,
    profile: &Option<Arc<Mutex<Option<ProfileLease>>>>,
    state: &SupervisorState,
    plan: ShutdownPlan,
) -> Result<ShutdownReport> {
    let started = std::time::Instant::now();
    let deadline = plan.deadline.instant();
    let mut failed = false;
    let mut failed_phase: Option<ShutdownFailurePhase> = None;
    let mut capture_failure = None;

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
                capture_failure = outcome.capture_failure;
                if capture_failure.is_some() {
                    failed = true;
                    failed_phase.get_or_insert(ShutdownFailurePhase::CaptureStopDrainFlush);
                }
                failed |= !outcome.complete;
                if !outcome.complete {
                    failed_phase.get_or_insert(ShutdownFailurePhase::CaptureStopDrainFlush);
                }
            } else {
                failed = true;
                failed_phase.get_or_insert(ShutdownFailurePhase::CaptureStopDrainFlush);
            }
        }
    }

    if !plan
        .deadline
        .remaining(ShutdownPhase::BrowserEventDrainFlush)
        .is_zero()
    {
        if !plan.browser_events.shutdown(deadline).await {
            failed = true;
            failed_phase.get_or_insert(ShutdownFailurePhase::BrowserEventDrainFlush);
        }
    } else {
        failed = true;
        failed_phase.get_or_insert(ShutdownFailurePhase::BrowserEventDrainFlush);
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
                failed_phase.get_or_insert(ShutdownFailurePhase::TargetDetach);
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
                failed_phase.get_or_insert(ShutdownFailurePhase::TargetDetach);
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
                failed_phase.get_or_insert(ShutdownFailurePhase::BrowserClose);
            }
        } else if plan.ownership == BrowserOwnership::Managed {
            failed = true;
            failed_phase.get_or_insert(ShutdownFailurePhase::BrowserClose);
        }
    }

    let mut process_remains = false;
    if let Some(process) = process {
        let owned = process.lock().expect("process lock").take();
        if let Some(mut owned) = owned {
            let remaining = plan.deadline.remaining(ShutdownPhase::ProcessTerminate);
            if !remaining.is_zero() {
                match tokio::time::timeout_at(deadline, owned.terminate(remaining)).await {
                    Ok(Ok(_)) => {}
                    Ok(Err(_)) | Err(_) => {
                        failed = true;
                        failed_phase.get_or_insert(ShutdownFailurePhase::ProcessTerminate);
                        if !owned.force_kill_now() {
                            process_remains = true;
                            *process.lock().expect("process lock") = Some(owned);
                        }
                    }
                }
            } else {
                failed = true;
                failed_phase.get_or_insert(ShutdownFailurePhase::ProcessTerminate);
                if !owned.force_kill_now() {
                    process_remains = true;
                    *process.lock().expect("process lock") = Some(owned);
                }
            }
        }
    }
    if !process_remains {
        if let Some(profile) = profile {
            profile.lock().expect("profile lock").take();
        }
    }
    *connection = None;
    let exhausted = plan.deadline.remaining(ShutdownPhase::Complete).is_zero();
    let profile_remains = profile
        .as_ref()
        .is_some_and(|profile| profile.lock().expect("profile lock").is_some());
    let mut remaining = Vec::new();
    if process_remains {
        remaining.push(RemainingResource::ManagedProcess);
    }
    if profile_remains {
        remaining.push(RemainingResource::ManagedProfile);
    }
    if !remaining.is_empty() {
        let failure_stage = failed_phase.unwrap_or(ShutdownFailurePhase::DeadlineComplete);
        tracing::warn!(
            event = "browser.shutdown.incomplete",
            failure_stage = failure_stage.as_str(),
            error_code = "shutdown_incomplete",
            elapsed_ms = started.elapsed().as_millis() as u64,
            forced_termination = true,
            unfinished_task_count = 0_u64,
            "browser.shutdown.incomplete"
        );
        Err(stable_error(
            ErrorCode::ShutdownIncomplete,
            if process_remains {
                "managed browser process remains after the shutdown deadline"
            } else {
                "managed browser profile remains after the shutdown deadline"
            },
        ))
    } else {
        tracing::info!(
            elapsed_ms = started.elapsed().as_millis() as u64,
            forced_termination = false,
            unfinished_task_count = 0_u64,
            "browser.shutdown.completed"
        );
        Ok(ShutdownReport {
            quality: if failed || exhausted {
                ShutdownQuality::Degraded
            } else {
                ShutdownQuality::Clean
            },
            failed_phase: if failed || exhausted {
                Some(failed_phase.unwrap_or(ShutdownFailurePhase::DeadlineComplete))
            } else {
                None
            },
            capture_failure,
            remaining,
        })
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
