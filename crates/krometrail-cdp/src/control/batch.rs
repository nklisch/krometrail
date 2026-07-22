use std::sync::Arc;

use krometrail_core::{
    BatchFailurePolicy, BatchOutcome, BatchRequest, BatchResult, BatchSkipReason, BatchStepResult,
    BatchStepStatus, BrowserOperationRequest, BrowserOperationResult, BrowserOperationScope,
    EncodedScreenshot, ErrorCode, ImageFormat, InteractionAnchor, KrometrailError,
    LiveObservationRequest, ObservationPart, PageOperationOutcome, PageSelection, Result,
    ScreenshotRequest, ScreenshotTarget, TargetId, WaitOutcome, wait_timeout_error,
};

use super::{PageControl, bind_target, operation_error};
use crate::{
    SupervisorState,
    control::navigation::OperationCancellation,
    session::{OperationExecutionContext, SessionShared, execute_operation},
    transport::CdpTransport,
};

const BATCH_TIMEOUT_GRACE: std::time::Duration = std::time::Duration::from_millis(500);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BatchTermination {
    Cancelled,
    TimedOut,
    TargetUnavailable,
}

impl BatchTermination {
    const fn skip_reason(self) -> BatchSkipReason {
        match self {
            Self::Cancelled => BatchSkipReason::BatchCancelled,
            Self::TimedOut => BatchSkipReason::BatchTimedOut,
            Self::TargetUnavailable => BatchSkipReason::TargetUnavailable,
        }
    }

    const fn outcome(self) -> BatchOutcome {
        match self {
            Self::Cancelled => BatchOutcome::Cancelled,
            Self::TimedOut => BatchOutcome::TimedOut,
            Self::TargetUnavailable => BatchOutcome::StoppedOnFailure,
        }
    }
}

enum DispatchOutcome {
    Completed(Result<BrowserOperationResult>),
    Interrupted(KrometrailError),
    TimedOut,
}

impl PageControl {
    pub(crate) async fn execute_batch(
        &mut self,
        transport: Arc<dyn CdpTransport>,
        state: &mut SupervisorState,
        shared: &Arc<SessionShared>,
        request: BatchRequest,
        cancellation: &OperationCancellation,
        parent_context: OperationExecutionContext,
    ) -> Result<BatchResult> {
        let bound = bind_target(state, request.target)?;
        let target_id = bound.target_id;
        let generation = state.connection_generation;
        let batch_id = self.next_interaction_id();
        let started_at = self.session_time()?;
        let own_deadline = tokio::time::Instant::now() + request.timeout;
        let deadline = parent_context
            .deadline
            .map_or(own_deadline, |parent| parent.min(own_deadline));
        let step_count = request.steps.len();
        let mut steps = Vec::with_capacity(step_count);
        let mut termination: Option<BatchTermination> = None;
        let mut failure_seen = false;
        let mut stopped_on_failure = false;

        for (index, child) in request.steps.into_iter().enumerate() {
            let operation = child.kind();
            if let Some(termination) = termination {
                steps.push(skipped_step(
                    index,
                    operation,
                    target_id,
                    termination.skip_reason(),
                )?);
                continue;
            }
            if stopped_on_failure {
                steps.push(skipped_step(
                    index,
                    operation,
                    target_id,
                    BatchSkipReason::PriorFailure,
                )?);
                continue;
            }
            let child_started = self.session_time()?;
            if !child_resolves_to(state, &child, target_id) {
                let error = operation_error(
                    ErrorCode::TargetFailed,
                    target_id,
                    "batch step no longer resolves to the admitted target",
                );
                let completed_at = self.session_time()?;
                steps.push(BatchStepResult::new(
                    u32::try_from(index).map_err(|_| batch_internal(target_id))?,
                    operation,
                    target_id,
                    BatchStepStatus::Failed,
                    Some(child_started),
                    Some(completed_at),
                    None,
                    None,
                    Some(error),
                    None,
                    None,
                )?);
                failure_seen = true;
                termination = Some(BatchTermination::TargetUnavailable);
                continue;
            }

            let context = OperationExecutionContext {
                deadline: Some(deadline),
                parent_batch: Some(batch_id),
            };
            let dispatched = dispatch_bounded(
                self,
                state,
                Arc::clone(&transport),
                shared,
                child,
                cancellation,
                context,
                generation,
                target_id,
                deadline,
            )
            .await;

            let (result, mut error, mut child_termination) = match dispatched {
                DispatchOutcome::Completed(Ok(result)) => {
                    let wait_timed_out = matches!(
                        &result,
                        BrowserOperationResult::Wait(value)
                            if matches!(value.outcome, WaitOutcome::TimedOut { .. })
                    );
                    let error = result_failure(&result, target_id);
                    let terminal = if wait_timed_out || tokio::time::Instant::now() >= deadline {
                        Some(BatchTermination::TimedOut)
                    } else {
                        error.as_ref().and_then(error_termination)
                    };
                    (Some(result), error, terminal)
                }
                DispatchOutcome::Completed(Err(error)) | DispatchOutcome::Interrupted(error) => {
                    let terminal = error_termination(&error);
                    (None, Some(error), terminal)
                }
                DispatchOutcome::TimedOut => (
                    None,
                    Some(wait_timeout_error(target_id)),
                    Some(BatchTermination::TimedOut),
                ),
            };
            let interaction = result.as_ref().map(result_anchor).transpose()?.flatten();
            let mut screenshot = request
                .options
                .include_step_screenshots
                .then(|| result.as_ref().and_then(existing_screenshot))
                .flatten();

            if request.options.include_step_screenshots
                && result.as_ref().and_then(existing_screenshot).is_none()
                && child_termination.is_none()
            {
                let screenshot_request =
                    BrowserOperationRequest::TakeScreenshot(ScreenshotRequest::new(
                        target_id,
                        ScreenshotTarget::Viewport,
                        ImageFormat::Png,
                        None,
                    )?);
                match dispatch_bounded(
                    self,
                    state,
                    Arc::clone(&transport),
                    shared,
                    screenshot_request,
                    cancellation,
                    context,
                    generation,
                    target_id,
                    deadline,
                )
                .await
                {
                    DispatchOutcome::Completed(Ok(BrowserOperationResult::TakeScreenshot(
                        value,
                    ))) => {
                        screenshot = Some(ObservationPart::Available(*value));
                    }
                    DispatchOutcome::Completed(Ok(_)) => unreachable!("screenshot dispatch result"),
                    DispatchOutcome::Completed(Err(screenshot_error)) => {
                        screenshot = Some(ObservationPart::Unavailable(screenshot_error));
                    }
                    DispatchOutcome::Interrupted(screenshot_error) => {
                        child_termination = error_termination(&screenshot_error)
                            .or(Some(BatchTermination::Cancelled));
                        screenshot = Some(ObservationPart::Unavailable(screenshot_error));
                    }
                    DispatchOutcome::TimedOut => {
                        child_termination = Some(BatchTermination::TimedOut);
                        screenshot =
                            Some(ObservationPart::Unavailable(wait_timeout_error(target_id)));
                    }
                }
            }

            let completed_at = self.session_time()?;
            let status = if error.is_some() {
                failure_seen = true;
                BatchStepStatus::Failed
            } else {
                BatchStepStatus::Succeeded
            };
            if status == BatchStepStatus::Failed
                && request.options.failure_policy == BatchFailurePolicy::StopOnFailure
                && child_termination.is_none()
            {
                stopped_on_failure = true;
            }
            if child_termination.is_some() && error.is_none() {
                // Evidence acquisition consumes the same budget. Preserve the successful child
                // result while making the terminal batch condition explicit on that step.
                error = Some(match child_termination {
                    Some(BatchTermination::TimedOut) => wait_timeout_error(target_id),
                    Some(BatchTermination::Cancelled) => operation_error(
                        ErrorCode::Cancelled,
                        target_id,
                        "batch was cancelled while collecting per-step evidence",
                    ),
                    Some(BatchTermination::TargetUnavailable) => operation_error(
                        ErrorCode::TargetFailed,
                        target_id,
                        "batch target became unavailable while collecting per-step evidence",
                    ),
                    None => unreachable!(),
                });
                failure_seen = true;
            }
            let final_status = if error.is_some() {
                BatchStepStatus::Failed
            } else {
                status
            };
            steps.push(BatchStepResult::new(
                u32::try_from(index).map_err(|_| batch_internal(target_id))?,
                operation,
                target_id,
                final_status,
                Some(child_started),
                Some(completed_at),
                interaction,
                result,
                error,
                None,
                screenshot,
            )?);
            termination = child_termination;
        }

        let final_observation = if let Some(reason) = termination {
            ObservationPart::Unavailable(termination_error(reason, target_id))
        } else if tokio::time::Instant::now() >= deadline {
            termination = Some(BatchTermination::TimedOut);
            ObservationPart::Unavailable(wait_timeout_error(target_id))
        } else {
            let final_request = BrowserOperationRequest::ObserveLive(LiveObservationRequest {
                target: PageSelection::Target(target_id),
            });
            let context = OperationExecutionContext {
                deadline: Some(deadline),
                parent_batch: Some(batch_id),
            };
            match dispatch_bounded(
                self,
                state,
                transport,
                shared,
                final_request,
                cancellation,
                context,
                generation,
                target_id,
                deadline,
            )
            .await
            {
                DispatchOutcome::Completed(Ok(BrowserOperationResult::ObserveLive(value))) => {
                    ObservationPart::Available(*value)
                }
                DispatchOutcome::Completed(Ok(_)) => {
                    unreachable!("live observation dispatch result")
                }
                DispatchOutcome::Completed(Err(error)) => {
                    if let Some(reason) = error_termination(&error) {
                        termination = Some(reason);
                    }
                    ObservationPart::Unavailable(error)
                }
                DispatchOutcome::Interrupted(error) => {
                    termination = error_termination(&error).or(Some(BatchTermination::Cancelled));
                    ObservationPart::Unavailable(error)
                }
                DispatchOutcome::TimedOut => {
                    termination = Some(BatchTermination::TimedOut);
                    ObservationPart::Unavailable(wait_timeout_error(target_id))
                }
            }
        };
        let completed_at = self.session_time()?;
        let outcome = termination.map_or_else(
            || {
                if stopped_on_failure {
                    BatchOutcome::StoppedOnFailure
                } else if failure_seen || final_observation_degraded(&final_observation) {
                    BatchOutcome::CompletedWithFailures
                } else {
                    BatchOutcome::Completed
                }
            },
            BatchTermination::outcome,
        );
        BatchResult::new(
            batch_id,
            target_id,
            started_at,
            completed_at,
            outcome,
            steps,
            final_observation,
        )
    }
}

#[allow(clippy::too_many_arguments)]
async fn dispatch_bounded(
    page_control: &mut PageControl,
    state: &mut SupervisorState,
    transport: Arc<dyn CdpTransport>,
    shared: &Arc<SessionShared>,
    request: BrowserOperationRequest,
    cancellation: &OperationCancellation,
    context: OperationExecutionContext,
    generation: u64,
    target_id: TargetId,
    deadline: tokio::time::Instant,
) -> DispatchOutcome {
    let execution = Box::pin(execute_operation(
        page_control,
        state,
        transport,
        shared,
        request,
        cancellation,
        context,
    ));
    tokio::select! {
        biased;
        error = cancellation.wait(generation, target_id) => {
            DispatchOutcome::Interrupted(error)
        }
        _ = tokio::time::sleep_until(deadline + BATCH_TIMEOUT_GRACE) => DispatchOutcome::TimedOut,
        result = execution => DispatchOutcome::Completed(result),
    }
}

fn child_resolves_to(
    state: &SupervisorState,
    request: &BrowserOperationRequest,
    target_id: TargetId,
) -> bool {
    let BrowserOperationScope::Page(selection) = request.scope() else {
        return false;
    };
    state
        .resolve_selection(selection)
        .is_ok_and(|target| target.target.target.id() == target_id)
}

fn result_failure(result: &BrowserOperationResult, target_id: TargetId) -> Option<KrometrailError> {
    match result {
        BrowserOperationResult::CreatePage(value)
        | BrowserOperationResult::SelectPage(value)
        | BrowserOperationResult::ActivatePage(value)
        | BrowserOperationResult::ClosePage(value)
        | BrowserOperationResult::NavigatePage(value)
        | BrowserOperationResult::ReloadPage(value)
        | BrowserOperationResult::GoBack(value)
        | BrowserOperationResult::GoForward(value) => match &value.outcome {
            PageOperationOutcome::Succeeded(_) => None,
            PageOperationOutcome::Failed(error) => Some(error.clone()),
        },
        BrowserOperationResult::SetViewport(value) => match &value.operation.outcome {
            PageOperationOutcome::Succeeded(_) => None,
            PageOperationOutcome::Failed(error) => Some(error.clone()),
        },
        BrowserOperationResult::Wait(value)
            if matches!(value.outcome, WaitOutcome::TimedOut { .. }) =>
        {
            Some(wait_timeout_error(target_id))
        }
        _ => None,
    }
}

fn result_anchor(result: &BrowserOperationResult) -> Result<Option<InteractionAnchor>> {
    let anchor = match result {
        BrowserOperationResult::CreatePage(value)
        | BrowserOperationResult::SelectPage(value)
        | BrowserOperationResult::ActivatePage(value)
        | BrowserOperationResult::ClosePage(value)
        | BrowserOperationResult::NavigatePage(value)
        | BrowserOperationResult::ReloadPage(value)
        | BrowserOperationResult::GoBack(value)
        | BrowserOperationResult::GoForward(value) => Some(value.interaction.clone()),
        BrowserOperationResult::SetViewport(value) => Some(value.operation.interaction.clone()),
        BrowserOperationResult::Click(value)
        | BrowserOperationResult::Fill(value)
        | BrowserOperationResult::PressKeys(value)
        | BrowserOperationResult::SelectOption(value)
        | BrowserOperationResult::Hover(value)
        | BrowserOperationResult::Drag(value)
        | BrowserOperationResult::Scroll(value)
        | BrowserOperationResult::UploadFiles(value)
        | BrowserOperationResult::HandleDialog(value) => Some(value.anchor()?),
        _ => None,
    };
    Ok(anchor)
}

fn existing_screenshot(
    result: &BrowserOperationResult,
) -> Option<ObservationPart<EncodedScreenshot>> {
    match result {
        BrowserOperationResult::TakeScreenshot(value) => {
            Some(ObservationPart::Available((**value).clone()))
        }
        BrowserOperationResult::CreatePage(value)
        | BrowserOperationResult::SelectPage(value)
        | BrowserOperationResult::ActivatePage(value)
        | BrowserOperationResult::ClosePage(value)
        | BrowserOperationResult::NavigatePage(value)
        | BrowserOperationResult::ReloadPage(value)
        | BrowserOperationResult::GoBack(value)
        | BrowserOperationResult::GoForward(value) => Some(match &value.observation {
            ObservationPart::Available(observation) => clone_screenshot(&observation.screenshot),
            ObservationPart::Unavailable(error) => ObservationPart::Unavailable(error.clone()),
        }),
        BrowserOperationResult::SetViewport(value) => Some(match &value.operation.observation {
            ObservationPart::Available(observation) => clone_screenshot(&observation.screenshot),
            ObservationPart::Unavailable(error) => ObservationPart::Unavailable(error.clone()),
        }),
        BrowserOperationResult::Click(value)
        | BrowserOperationResult::Fill(value)
        | BrowserOperationResult::PressKeys(value)
        | BrowserOperationResult::SelectOption(value)
        | BrowserOperationResult::Hover(value)
        | BrowserOperationResult::Drag(value)
        | BrowserOperationResult::Scroll(value)
        | BrowserOperationResult::UploadFiles(value)
        | BrowserOperationResult::HandleDialog(value) => {
            Some(clone_screenshot(&value.observation.screenshot))
        }
        _ => None,
    }
}

fn final_observation_degraded(value: &ObservationPart<krometrail_core::LiveObservation>) -> bool {
    match value {
        ObservationPart::Unavailable(_) => true,
        ObservationPart::Available(observation) => {
            matches!(observation.page, ObservationPart::Unavailable(_))
                || matches!(observation.snapshot, ObservationPart::Unavailable(_))
                || matches!(observation.screenshot, ObservationPart::Unavailable(_))
        }
    }
}

fn clone_screenshot(
    value: &ObservationPart<EncodedScreenshot>,
) -> ObservationPart<EncodedScreenshot> {
    match value {
        ObservationPart::Available(value) => ObservationPart::Available(value.clone()),
        ObservationPart::Unavailable(error) => ObservationPart::Unavailable(error.clone()),
    }
}

fn error_termination(error: &KrometrailError) -> Option<BatchTermination> {
    match error.code {
        ErrorCode::Cancelled => Some(BatchTermination::Cancelled),
        ErrorCode::BrowserDisconnected | ErrorCode::TargetFailed => {
            Some(BatchTermination::TargetUnavailable)
        }
        ErrorCode::WaitTimedOut => Some(BatchTermination::TimedOut),
        _ => None,
    }
}

fn termination_error(reason: BatchTermination, target_id: TargetId) -> KrometrailError {
    match reason {
        BatchTermination::Cancelled => operation_error(
            ErrorCode::Cancelled,
            target_id,
            "batch final observation was prevented by cancellation",
        ),
        BatchTermination::TimedOut => wait_timeout_error(target_id),
        BatchTermination::TargetUnavailable => operation_error(
            ErrorCode::TargetFailed,
            target_id,
            "batch final observation was prevented because the target is unavailable",
        ),
    }
}

fn skipped_step(
    index: usize,
    operation: krometrail_core::BrowserOperationKind,
    target_id: TargetId,
    reason: BatchSkipReason,
) -> Result<BatchStepResult> {
    BatchStepResult::new(
        u32::try_from(index).map_err(|_| batch_internal(target_id))?,
        operation,
        target_id,
        BatchStepStatus::Skipped,
        None,
        None,
        None,
        None,
        None,
        Some(reason),
        None,
    )
}

fn batch_internal(target_id: TargetId) -> KrometrailError {
    operation_error(
        ErrorCode::Internal,
        target_id,
        "batch step index exceeds the supported range",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn termination_reasons_have_stable_skip_and_outcome_mappings() {
        assert_eq!(
            BatchTermination::Cancelled.skip_reason(),
            BatchSkipReason::BatchCancelled
        );
        assert_eq!(BatchTermination::TimedOut.outcome(), BatchOutcome::TimedOut);
        assert_eq!(
            BatchTermination::TargetUnavailable.skip_reason(),
            BatchSkipReason::TargetUnavailable
        );
    }
}
