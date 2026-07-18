use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::{
    ErrorCode, InteractionId, KrometrailError, NonEmptyText, Result, SessionTime, TargetId,
    error::invalid,
    validation::{delegate_json_schema, deserialize_validated},
};

use super::{
    BROWSER_OPERATION_REGISTRY, BrowserOperationKind, BrowserOperationRequest,
    BrowserOperationResult, BrowserOperationScope, ElementLocator, EncodedScreenshot,
    InteractionAnchor, InteractionLocator, LiveObservation, ObservationPart, PageSelection,
    ScreenshotTarget, WaitCondition, validate_operation_timeout,
};

const MAX_BATCH_STEPS: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BatchFailurePolicy {
    StopOnFailure,
    ContinueOnFailure,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BatchOptions {
    pub failure_policy: BatchFailurePolicy,
    pub include_step_screenshots: bool,
}

impl Default for BatchOptions {
    fn default() -> Self {
        Self {
            failure_policy: BatchFailurePolicy::StopOnFailure,
            include_step_screenshots: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BatchRequest {
    pub target: PageSelection,
    pub steps: Vec<BrowserOperationRequest>,
    #[serde(serialize_with = "serialize_duration")]
    pub timeout: Duration,
    pub options: BatchOptions,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct BatchRequestWire {
    #[serde(default)]
    target: PageSelection,
    steps: Vec<BrowserOperationRequest>,
    timeout: u64,
    #[serde(default)]
    options: BatchOptions,
}

impl BatchRequest {
    pub fn new(
        target: PageSelection,
        mut steps: Vec<BrowserOperationRequest>,
        timeout: Duration,
        options: BatchOptions,
    ) -> Result<Self> {
        validate_operation_timeout(timeout)?;
        if steps.is_empty() || steps.len() > MAX_BATCH_STEPS {
            return Err(invalid("batch must contain between one and 64 steps"));
        }
        if let PageSelection::Target(target_id) = target {
            for step in &mut steps {
                step.inherit_selected_target(target_id);
            }
        }
        for step in &steps {
            let definition = BROWSER_OPERATION_REGISTRY
                .iter()
                .find(|definition| definition.kind == step.kind())
                .ok_or_else(|| invalid("batch step operation is not registered"))?;
            if !definition.batchable {
                return Err(invalid(format!(
                    "{} is not admitted as a batch step",
                    step.stable_name()
                )));
            }
            let BrowserOperationScope::Page(step_target) = step.scope() else {
                return Err(invalid(
                    "browser-scoped operations are not admitted in a batch",
                ));
            };
            if let (PageSelection::Target(batch_target), PageSelection::Target(step_target)) =
                (target, step_target)
            {
                if batch_target != step_target {
                    return Err(invalid("batch step targets another page"));
                }
            }
            for reference_target in reference_targets(step) {
                if let PageSelection::Target(batch_target) = target {
                    if reference_target != batch_target {
                        return Err(invalid("batch step reference targets another page"));
                    }
                }
                if let PageSelection::Target(step_target) = step_target {
                    if reference_target != step_target {
                        return Err(invalid("batch step reference contradicts its page target"));
                    }
                }
            }
        }
        Ok(Self {
            target,
            steps,
            timeout,
            options,
        })
    }
}

delegate_json_schema!(BatchRequest => BatchRequestWire);

impl<'de> Deserialize<'de> for BatchRequest {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        deserialize_validated(deserializer, |wire: BatchRequestWire| {
            if wire.timeout == 0 {
                return Err(invalid("batch timeout must be non-zero"));
            }
            Self::new(
                wire.target,
                wire.steps,
                Duration::from_millis(wire.timeout),
                wire.options,
            )
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchStepStatus {
    Succeeded,
    Failed,
    Skipped,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchSkipReason {
    PriorFailure,
    BatchCancelled,
    BatchTimedOut,
    TargetUnavailable,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BatchStepResult {
    pub index: u32,
    pub operation: BrowserOperationKind,
    pub target_id: TargetId,
    pub status: BatchStepStatus,
    /// Skipped steps have no execution interval; their timing remains absent rather than fabricated.
    pub started_at: Option<SessionTime>,
    pub completed_at: Option<SessionTime>,
    pub interaction: Option<InteractionAnchor>,
    pub result: Option<BrowserOperationResult>,
    pub error: Option<KrometrailError>,
    pub skip_reason: Option<BatchSkipReason>,
    pub screenshot: ObservationPart<EncodedScreenshot>,
}

impl BatchStepResult {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        index: u32,
        operation: BrowserOperationKind,
        target_id: TargetId,
        status: BatchStepStatus,
        started_at: Option<SessionTime>,
        completed_at: Option<SessionTime>,
        interaction: Option<InteractionAnchor>,
        result: Option<BrowserOperationResult>,
        error: Option<KrometrailError>,
        skip_reason: Option<BatchSkipReason>,
        screenshot: ObservationPart<EncodedScreenshot>,
    ) -> Result<Self> {
        if let (Some(started), Some(completed)) = (started_at, completed_at) {
            if started > completed {
                return Err(invalid("batch step times must be monotonically ordered"));
            }
        }
        if interaction
            .as_ref()
            .is_some_and(|anchor| anchor.target_id != target_id || anchor.operation != operation)
        {
            return Err(invalid(
                "batch step interaction anchor does not match the step",
            ));
        }
        if result
            .as_ref()
            .is_some_and(|result| result.kind() != operation)
        {
            return Err(invalid(
                "batch step result kind does not match the operation",
            ));
        }
        match status {
            BatchStepStatus::Succeeded => {
                if started_at.is_none()
                    || completed_at.is_none()
                    || result.is_none()
                    || error.is_some()
                    || skip_reason.is_some()
                {
                    return Err(invalid("successful batch step has inconsistent fields"));
                }
            }
            BatchStepStatus::Failed => {
                if started_at.is_none()
                    || completed_at.is_none()
                    || error.is_none()
                    || skip_reason.is_some()
                {
                    return Err(invalid("failed batch step has inconsistent fields"));
                }
            }
            BatchStepStatus::Skipped => {
                if started_at.is_some()
                    || completed_at.is_some()
                    || interaction.is_some()
                    || result.is_some()
                    || error.is_some()
                    || skip_reason.is_none()
                    || matches!(screenshot, ObservationPart::Available(_))
                {
                    return Err(invalid("skipped batch step has inconsistent fields"));
                }
            }
        }
        Ok(Self {
            index,
            operation,
            target_id,
            status,
            started_at,
            completed_at,
            interaction,
            result,
            error,
            skip_reason,
            screenshot,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchOutcome {
    Completed,
    CompletedWithFailures,
    StoppedOnFailure,
    Cancelled,
    TimedOut,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BatchResult {
    pub batch_id: InteractionId,
    pub target_id: TargetId,
    pub started_at: SessionTime,
    pub completed_at: SessionTime,
    pub outcome: BatchOutcome,
    pub steps: Vec<BatchStepResult>,
    pub final_observation: ObservationPart<LiveObservation>,
}

impl BatchResult {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        batch_id: InteractionId,
        target_id: TargetId,
        started_at: SessionTime,
        completed_at: SessionTime,
        outcome: BatchOutcome,
        steps: Vec<BatchStepResult>,
        final_observation: ObservationPart<LiveObservation>,
    ) -> Result<Self> {
        if started_at > completed_at {
            return Err(invalid("batch times must be monotonically ordered"));
        }
        if steps.is_empty() || steps.len() > MAX_BATCH_STEPS {
            return Err(invalid(
                "batch result must contain between one and 64 steps",
            ));
        }
        for (expected, step) in steps.iter().enumerate() {
            if usize::try_from(step.index).ok() != Some(expected) {
                return Err(invalid("batch step indexes must be contiguous and ordered"));
            }
            if step.target_id != target_id {
                return Err(invalid("batch result contains a step for another target"));
            }
            if step.started_at.is_some_and(|value| value < started_at)
                || step.completed_at.is_some_and(|value| value > completed_at)
            {
                return Err(invalid("batch step timing lies outside the batch interval"));
            }
        }
        Ok(Self {
            batch_id,
            target_id,
            started_at,
            completed_at,
            outcome,
            steps,
            final_observation,
        })
    }
}

pub fn wait_timeout_error(target_id: TargetId) -> KrometrailError {
    let code = ErrorCode::WaitTimedOut;
    KrometrailError::new(
        code,
        NonEmptyText::new("wait condition was not satisfied before its deadline").unwrap(),
    )
    .with_context(crate::ErrorContext {
        target_id: Some(target_id),
        ..crate::ErrorContext::default()
    })
    .with_retry(code.default_retry())
    .with_recovery(NonEmptyText::new(code.default_recovery().unwrap()).unwrap())
}

fn reference_targets(request: &BrowserOperationRequest) -> Vec<TargetId> {
    let mut targets = Vec::new();
    match request {
        BrowserOperationRequest::TakeScreenshot(request) => {
            if let ScreenshotTarget::Element(locator) = &request.target {
                push_locator_target(locator, &mut targets);
            }
        }
        BrowserOperationRequest::Click(request) => {
            push_interaction_target(&request.locator, &mut targets);
        }
        BrowserOperationRequest::Fill(request) => {
            push_interaction_target(&request.locator, &mut targets);
        }
        BrowserOperationRequest::PressKeys(request) => {
            if let Some(locator) = &request.locator {
                push_interaction_target(locator, &mut targets);
            }
        }
        BrowserOperationRequest::SelectOption(request) => {
            push_interaction_target(&request.locator, &mut targets);
        }
        BrowserOperationRequest::Hover(request) => {
            push_interaction_target(&request.locator, &mut targets);
        }
        BrowserOperationRequest::Drag(request) => {
            push_interaction_target(&request.source, &mut targets);
            push_interaction_target(&request.destination, &mut targets);
        }
        BrowserOperationRequest::Scroll(request) => {
            if let super::ScrollDelta::ToElement(locator) = &request.delta {
                push_locator_target(locator, &mut targets);
            }
        }
        BrowserOperationRequest::UploadFiles(request) => {
            push_interaction_target(&request.locator, &mut targets);
        }
        BrowserOperationRequest::Wait(request) => match &request.condition {
            WaitCondition::Text {
                locator: Some(locator),
                ..
            }
            | WaitCondition::Element { locator, .. } => push_locator_target(locator, &mut targets),
            _ => {}
        },
        _ => {}
    }
    targets
}

fn push_interaction_target(locator: &InteractionLocator, targets: &mut Vec<TargetId>) {
    if let InteractionLocator::Element(locator) = locator {
        push_locator_target(locator, targets);
    }
}

fn push_locator_target(locator: &ElementLocator, targets: &mut Vec<TargetId>) {
    if let ElementLocator::Reference(reference) = locator {
        targets.push(reference.target_id);
    }
}

fn serialize_duration<S: serde::Serializer>(
    value: &Duration,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error> {
    u64::try_from(value.as_millis())
        .map_err(serde::ser::Error::custom)?
        .serialize(serializer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BrowserOperationScopeKind, ImageFormat, InspectPageRequest, ScreenshotRequest,
        SnapshotGeneration, SnapshotNodeId, TargetId,
    };
    use uuid::Uuid;

    fn target(value: u128) -> TargetId {
        TargetId::from_uuid(Uuid::from_u128(value))
    }

    #[test]
    fn batch_admission_uses_registry_metadata_and_one_target() {
        let target_id = target(1);
        let accepted = BatchRequest::new(
            PageSelection::Target(target_id),
            vec![
                BrowserOperationRequest::InspectPage(InspectPageRequest::new(target_id)),
                BrowserOperationRequest::TakeScreenshot(
                    ScreenshotRequest::new(
                        target_id,
                        ScreenshotTarget::Viewport,
                        ImageFormat::Png,
                        None,
                    )
                    .unwrap(),
                ),
            ],
            Duration::from_secs(2),
            BatchOptions::default(),
        )
        .unwrap();
        assert_eq!(accepted.steps.len(), 2);
        assert!(
            BatchRequest::new(
                PageSelection::Target(target_id),
                vec![BrowserOperationRequest::ClosePage(
                    super::super::ClosePageRequest {
                        target: PageSelection::Target(target_id),
                    }
                )],
                Duration::from_secs(1),
                BatchOptions::default(),
            )
            .is_err()
        );
        assert!(
            BatchRequest::new(
                PageSelection::Target(target_id),
                vec![BrowserOperationRequest::InspectPage(
                    InspectPageRequest::new(target(2))
                )],
                Duration::from_secs(1),
                BatchOptions::default(),
            )
            .is_err()
        );
        assert!(BROWSER_OPERATION_REGISTRY.iter().any(|definition| {
            definition.kind == BrowserOperationKind::Batch
                && definition.scope == BrowserOperationScopeKind::Page
                && !definition.batchable
        }));
    }

    #[test]
    fn explicit_batch_target_is_inherited_by_targetless_steps() {
        let target_id = target(1);
        let accepted = BatchRequest::new(
            PageSelection::Target(target_id),
            vec![BrowserOperationRequest::InspectPage(InspectPageRequest {
                target: PageSelection::Selected,
            })],
            Duration::from_secs(1),
            BatchOptions::default(),
        )
        .unwrap();

        assert_eq!(
            accepted.steps[0].scope(),
            BrowserOperationScope::Page(PageSelection::Target(target_id))
        );
    }

    #[test]
    fn batch_rejects_nested_and_cross_target_reference_steps_at_wire_boundary() {
        let target_id = target(1);
        let reference = super::super::NodeReference {
            target_id: target(2),
            generation: SnapshotGeneration::new(1).unwrap(),
            node_id: SnapshotNodeId::new(1).unwrap(),
        };
        let cross_target = BrowserOperationRequest::TakeScreenshot(
            ScreenshotRequest::new(
                target_id,
                ScreenshotTarget::Element(ElementLocator::Reference(reference)),
                ImageFormat::Png,
                None,
            )
            .unwrap(),
        );
        assert!(
            BatchRequest::new(
                PageSelection::Target(target_id),
                vec![cross_target],
                Duration::from_secs(1),
                BatchOptions::default(),
            )
            .is_err()
        );

        let inner = BatchRequest::new(
            PageSelection::Target(target_id),
            vec![BrowserOperationRequest::InspectPage(
                InspectPageRequest::new(target_id),
            )],
            Duration::from_secs(1),
            BatchOptions::default(),
        )
        .unwrap();
        let json = serde_json::json!({
            "target":{"selection":"target","target_id":target_id},
            "steps":[{"operation":"batch","request":inner}],
            "timeout":1000,
            "options":{"failure_policy":"stop_on_failure","include_step_screenshots":false}
        });
        assert!(serde_json::from_value::<BatchRequest>(json).is_err());
    }
}
