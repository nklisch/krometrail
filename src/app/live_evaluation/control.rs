//! Test-only browser-control qualification over the production connector.
//!
//! The registry is derived from `krometrail-core`'s operation registry.  This module owns only
//! fixture recipes and accounting; browser dispatch, interaction evidence, capture, and interval
//! reads remain production ports supplied by `QualificationRuntime`.

use std::{fs, sync::Arc, time::Duration};

use krometrail_core::{
    AnchorScope, BROWSER_OPERATION_REGISTRY, BatchOptions, BatchOutcome, BatchRequest,
    BatchStepStatus, BrowserOperationKind, BrowserOperationRequest, BrowserOperationResult,
    BrowserSessionPort, BrowserSessionState, BrowserStatus, ClickRequest, CreatePageRequest,
    DialogAction, DragRequest, ElementLocator, ErrorCode, FillMode, FillRequest, GoBackRequest,
    GoForwardRequest, HandleDialogRequest, HoverRequest, ImageFormat, InspectPageRequest,
    InteractionAnchorSource, InteractionId, InteractionLocator, KeyChord, LiveObservation,
    LiveObservationRequest, Modifiers, MouseButton, NavigatePageRequest, ObservationPart,
    PageOperationOutcome, PageSelection, ReadOnlyEvaluationRequest, ReloadPageRequest,
    ScreenshotRequest, ScreenshotTarget, ScrollDelta, ScrollRequest, SelectOptionRequest,
    SelectPageRequest, SelectValue, SessionRange, SessionTime, SnapshotPageRequest, TargetId,
    TemporalQueryRequest, TemporalRangeAnchor, WaitCondition, WaitOutcome, WaitRequest,
};
use serde::Serialize;
use temporal_evaluation::{
    ControlQualificationMeasurements, EvaluationStatus, RunFailureCode, VIEWPORT_HEIGHT,
    VIEWPORT_WIDTH, Viewport,
};

use super::{
    BrowserPreflight, LiveQualificationConfig, OptInDecision, QualificationLifecycle,
    QualificationRuntime,
    barriers::{
        BarrierProtocolError, BarrierTrace, ControlBarrier, DEFAULT_BARRIER_TIMEOUT, bounded,
    },
    capture::{IntervalAuthorities, source_interval_for_interaction},
    live_error,
};

const CONTROL_POLL_INTERVAL: Duration = Duration::from_millis(25);
const CONTROL_RELIABILITY_THRESHOLD_BASIS_POINTS: u16 = 9_500;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlFixture {
    VerifiedInteractions,
    WaitsAndBatches,
}

impl ControlFixture {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::VerifiedInteractions => "verified-interactions",
            Self::WaitsAndBatches => "waits-and-batches",
        }
    }

    fn url(self) -> String {
        match self {
            Self::VerifiedInteractions => {
                krometrail_cdp::qualification_support::verified_interactions_fixture_url()
            }
            Self::WaitsAndBatches => {
                krometrail_cdp::qualification_support::waits_and_batches_fixture_url("index.html")
            }
        }
    }
}

/// A stable, fixture-specific recipe attached to one operation definition.  Operation identity is
/// never copied into this type: callers obtain it from `BrowserOperationDefinition.kind`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ControlScenario {
    pub scenario_id: String,
    pub operation: BrowserOperationKind,
    pub fixture: Option<ControlFixture>,
}

impl ControlScenario {
    fn from_definition(definition: &krometrail_core::BrowserOperationDefinition) -> Self {
        let fixture = fixture_for_operation(definition.kind);
        let fixture_name = fixture.map_or("unsupported", ControlFixture::as_str);
        Self {
            scenario_id: format!("control:{fixture_name}:{}", definition.stable_name),
            operation: definition.kind,
            fixture,
        }
    }
}

/// The operation registry is the only source of operation identity and order.  A missing recipe
/// remains an explicit unavailable scenario instead of disappearing from the qualification.
pub fn control_scenarios() -> Vec<ControlScenario> {
    BROWSER_OPERATION_REGISTRY
        .iter()
        .map(ControlScenario::from_definition)
        .collect()
}

fn fixture_for_operation(operation: BrowserOperationKind) -> Option<ControlFixture> {
    use BrowserOperationKind::*;
    match operation {
        InspectPage | SnapshotPage | TakeScreenshot | EvaluatePage | ObserveLive | Click | Fill
        | PressKeys | SelectOption | Hover | Drag | Scroll | UploadFiles | HandleDialog => {
            Some(ControlFixture::VerifiedInteractions)
        }
        ListPages | CreatePage | SelectPage | ClosePage | NavigatePage | ReloadPage | GoBack
        | GoForward | Wait | Batch => Some(ControlFixture::WaitsAndBatches),
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationAvailability {
    Available,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlOperationOutcome {
    Succeeded,
    Failed,
    Unavailable,
}

/// Safe failure vocabulary for the manifest-facing control record.  It deliberately contains no
/// adapter message, selector, URL, path, or transport identifier.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlFailureCode {
    UnsupportedCapability,
    StaleReference,
    TargetReplacement,
    Timeout,
    TransportLoss,
    MissingPreObservation,
    MissingPostObservation,
    FixtureSettle,
    CaptureFence,
    IntervalQuery,
    OperationFailed,
    RecoveryFailed,
}

impl ControlFailureCode {
    pub const fn status(self) -> EvaluationStatus {
        match self {
            Self::OperationFailed => EvaluationStatus::Fail,
            Self::UnsupportedCapability
            | Self::StaleReference
            | Self::TargetReplacement
            | Self::Timeout
            | Self::TransportLoss
            | Self::MissingPreObservation
            | Self::MissingPostObservation
            | Self::FixtureSettle
            | Self::CaptureFence
            | Self::IntervalQuery
            | Self::RecoveryFailed => EvaluationStatus::Inconclusive,
        }
    }

    pub const fn run_failure_code(self) -> RunFailureCode {
        match self {
            Self::UnsupportedCapability => RunFailureCode::Unsupported,
            Self::StaleReference
            | Self::TargetReplacement
            | Self::Timeout
            | Self::TransportLoss
            | Self::MissingPreObservation
            | Self::MissingPostObservation
            | Self::FixtureSettle
            | Self::CaptureFence
            | Self::IntervalQuery
            | Self::RecoveryFailed => RunFailureCode::InsufficientEvidence,
            Self::OperationFailed => RunFailureCode::Unavailable,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ControlAttempt {
    pub scenario_id: String,
    pub operation: BrowserOperationKind,
    pub interaction_id: Option<InteractionId>,
    pub pre_observation: ObservationAvailability,
    pub operation_outcome: ControlOperationOutcome,
    pub post_observation: ObservationAvailability,
    pub failure_code: Option<ControlFailureCode>,
    pub barriers: BarrierTrace,
    pub recovered: bool,
}

impl ControlAttempt {
    pub fn is_success(&self) -> bool {
        self.pre_observation == ObservationAvailability::Available
            && self.operation_outcome == ControlOperationOutcome::Succeeded
            && self.post_observation == ObservationAvailability::Available
            && self.failure_code.is_none()
            && self.barriers.is_complete()
    }

    fn unavailable(
        scenario: &ControlScenario,
        barriers: BarrierTrace,
        failure_code: ControlFailureCode,
    ) -> Self {
        Self {
            scenario_id: scenario.scenario_id.clone(),
            operation: scenario.operation,
            interaction_id: None,
            pre_observation: ObservationAvailability::Unavailable,
            operation_outcome: ControlOperationOutcome::Unavailable,
            post_observation: ObservationAvailability::Unavailable,
            failure_code: Some(failure_code),
            barriers,
            recovered: false,
        }
    }

    pub fn canonical_bytes(&self) -> krometrail_core::Result<Vec<u8>> {
        temporal_evaluation::canonical_json(self).map_err(|_| {
            live_error(
                ErrorCode::PersistenceFailed,
                "control attempt could not be canonicalized",
            )
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ControlQualificationRun {
    pub attempts: Vec<ControlAttempt>,
    pub control: ControlQualificationMeasurements,
    pub status: EvaluationStatus,
}

impl ControlQualificationRun {
    pub fn canonical_bytes(&self) -> krometrail_core::Result<Vec<u8>> {
        temporal_evaluation::canonical_json(self).map_err(|_| {
            live_error(
                ErrorCode::PersistenceFailed,
                "control qualification could not be canonicalized",
            )
        })
    }
}

pub fn summarize_control(attempts: &[ControlAttempt]) -> ControlQualificationRun {
    let successes = attempts
        .iter()
        .filter(|attempt| attempt.is_success())
        .count() as u64;
    let failed_observation_ids = attempts
        .iter()
        .filter(|attempt| attempt.post_observation == ObservationAvailability::Unavailable)
        .map(|attempt| attempt.scenario_id.clone())
        .collect::<Vec<_>>();
    let success_rate_basis_points = if attempts.is_empty() {
        0
    } else {
        ((successes * 10_000) / attempts.len() as u64) as u16
    };
    let measurements = ControlQualificationMeasurements {
        scenario_ids: attempts
            .iter()
            .map(|attempt| attempt.scenario_id.clone())
            .collect(),
        attempts: attempts.len() as u64,
        successes,
        failed_observation_ids,
        success_rate_basis_points,
    };
    let status = if attempts.is_empty()
        || attempts.iter().any(|attempt| {
            attempt
                .failure_code
                .is_some_and(|code| code.status() == EvaluationStatus::Inconclusive)
        }) {
        EvaluationStatus::Inconclusive
    } else if success_rate_basis_points < CONTROL_RELIABILITY_THRESHOLD_BASIS_POINTS {
        EvaluationStatus::Fail
    } else {
        EvaluationStatus::Pass
    };
    ControlQualificationRun {
        attempts: attempts.to_vec(),
        control: measurements,
        status,
    }
}

pub(crate) struct LiveTrialContext<'a> {
    pub runtime: &'a QualificationRuntime,
    pub lifecycle: &'a QualificationLifecycle,
    pub session: Arc<dyn BrowserSessionPort>,
    pub fixture: ControlFixture,
    pub target_id: TargetId,
    pub primary_target_id: TargetId,
}

pub struct ReadyObservation {
    pub target_id: TargetId,
    pub viewport: Viewport,
    pub barriers: BarrierTrace,
}

pub async fn wait_for_ready_barrier(
    context: &mut LiveTrialContext<'_>,
) -> krometrail_core::Result<ReadyObservation> {
    let mut barriers = BarrierTrace::new();
    if !context.lifecycle.lock_held() {
        return Err(live_error(
            ErrorCode::ProfileInUse,
            "qualification browser lock is not held",
        ));
    }
    barriers
        .record(ControlBarrier::BrowserLockAcquired)
        .map_err(BarrierProtocolError::into_error)?;
    if !context.lifecycle.server_ready() {
        return Err(live_error(
            ErrorCode::BrowserLaunchFailed,
            "qualification loopback server is not ready",
        ));
    }
    barriers
        .record(ControlBarrier::LoopbackServerReady)
        .map_err(BarrierProtocolError::into_error)?;

    let status = bounded(
        ControlBarrier::TargetAttached,
        DEFAULT_BARRIER_TIMEOUT,
        context.session.status(),
    )
    .await?;
    let target_id = ready_target(&status)?;
    context.target_id = target_id;
    barriers
        .record(ControlBarrier::TargetAttached)
        .map_err(BarrierProtocolError::into_error)?;

    let viewport = inspect_viewport(context, target_id).await?;
    barriers
        .record(ControlBarrier::ViewportReported)
        .map_err(BarrierProtocolError::into_error)?;
    wait_for_page_ready(context, target_id).await?;
    barriers
        .record(ControlBarrier::PageReady)
        .map_err(BarrierProtocolError::into_error)?;
    Ok(ReadyObservation {
        target_id,
        viewport,
        barriers,
    })
}

fn ready_target(status: &BrowserStatus) -> krometrail_core::Result<TargetId> {
    if status.state != BrowserSessionState::Ready {
        return Err(live_error(
            ErrorCode::InvalidLifecycleTransition,
            "qualification browser session is not ready",
        ));
    }
    status.selected_target_id.ok_or_else(|| {
        live_error(
            ErrorCode::TargetFailed,
            "qualification browser session has no selected target",
        )
    })
}

async fn inspect_viewport(
    context: &LiveTrialContext<'_>,
    target_id: TargetId,
) -> krometrail_core::Result<Viewport> {
    let result = bounded(
        ControlBarrier::ViewportReported,
        DEFAULT_BARRIER_TIMEOUT,
        context.session.execute(
            BrowserOperationRequest::InspectPage(InspectPageRequest::new(target_id)),
            krometrail_core::BrowserOperationContext::default(),
        ),
    )
    .await?;
    let BrowserOperationResult::InspectPage(page) = result else {
        return Err(live_error(
            ErrorCode::InvalidInput,
            "viewport barrier returned the wrong operation",
        ));
    };
    let width = page.viewport.layout_viewport.size.width.round() as u32;
    let height = page.viewport.layout_viewport.size.height.round() as u32;
    if width != VIEWPORT_WIDTH
        || height != VIEWPORT_HEIGHT
        || (page.viewport.device_scale_factor.get() - 1.0).abs() > f64::EPSILON
    {
        return Err(live_error(
            ErrorCode::InvalidInput,
            "control qualification viewport does not match the canonical profile",
        ));
    }
    Ok(Viewport { width, height })
}

async fn wait_for_page_ready(
    context: &LiveTrialContext<'_>,
    target_id: TargetId,
) -> krometrail_core::Result<()> {
    let request = WaitRequest::new(
        PageSelection::Target(target_id),
        WaitCondition::Page {
            expression: krometrail_core::NonEmptyText::new(
                "document.readyState === 'complete' && typeof window.fixtureState === 'object'",
            )
            .expect("static page readiness expression"),
        },
        DEFAULT_BARRIER_TIMEOUT,
        CONTROL_POLL_INTERVAL,
    )?;
    let result = bounded(
        ControlBarrier::PageReady,
        DEFAULT_BARRIER_TIMEOUT,
        context.session.execute(
            BrowserOperationRequest::Wait(request),
            krometrail_core::BrowserOperationContext::default(),
        ),
    )
    .await?;
    if !matches!(
        result,
        BrowserOperationResult::Wait(value)
            if matches!(value.outcome, WaitOutcome::Satisfied { .. })
    ) {
        return Err(live_error(
            ErrorCode::WaitTimedOut,
            "fixture page did not report readiness",
        ));
    }
    Ok(())
}

pub async fn execute_control_trial(
    context: &mut LiveTrialContext<'_>,
    scenario: &ControlScenario,
) -> krometrail_core::Result<ControlAttempt> {
    let ready = match wait_for_ready_barrier(context).await {
        Ok(ready) => ready,
        Err(error) => {
            return Ok(ControlAttempt::unavailable(
                scenario,
                BarrierTrace::new(),
                failure_code_for_error(error.code),
            ));
        }
    };
    let pre_observation = observe_live(context, ready.target_id).await;
    if pre_observation != ObservationAvailability::Available {
        return Ok(ControlAttempt {
            scenario_id: scenario.scenario_id.clone(),
            operation: scenario.operation,
            interaction_id: None,
            pre_observation,
            operation_outcome: ControlOperationOutcome::Unavailable,
            post_observation: ObservationAvailability::Unavailable,
            failure_code: Some(ControlFailureCode::MissingPreObservation),
            barriers: ready.barriers,
            recovered: false,
        });
    }

    let Some(request) = request_for_scenario(scenario, context)? else {
        return Ok(ControlAttempt {
            scenario_id: scenario.scenario_id.clone(),
            operation: scenario.operation,
            interaction_id: None,
            pre_observation,
            operation_outcome: ControlOperationOutcome::Unavailable,
            post_observation: ObservationAvailability::Unavailable,
            failure_code: Some(ControlFailureCode::UnsupportedCapability),
            barriers: ready.barriers,
            recovered: false,
        });
    };
    let mut barriers = ready.barriers;
    barriers
        .record(ControlBarrier::StructuredOperationSubmitted)
        .map_err(BarrierProtocolError::into_error)?;
    let operation_result = bounded(
        ControlBarrier::StructuredOperationSubmitted,
        DEFAULT_BARRIER_TIMEOUT,
        context
            .session
            .execute(request, krometrail_core::BrowserOperationContext::default()),
    )
    .await;
    let operation_result = match operation_result {
        Ok(result) => result,
        Err(error) => {
            return Ok(ControlAttempt {
                scenario_id: scenario.scenario_id.clone(),
                operation: scenario.operation,
                interaction_id: error.context.interaction_id,
                pre_observation,
                operation_outcome: if error.code == ErrorCode::Unsupported {
                    ControlOperationOutcome::Unavailable
                } else {
                    ControlOperationOutcome::Failed
                },
                post_observation: ObservationAvailability::Unavailable,
                failure_code: Some(failure_code_for_error(error.code)),
                barriers,
                recovered: false,
            });
        }
    };
    let analysis = analyze_operation(&operation_result);
    let mut operation_outcome = if analysis.operation_succeeded {
        ControlOperationOutcome::Succeeded
    } else {
        ControlOperationOutcome::Failed
    };
    if !analysis.operation_succeeded {
        return Ok(ControlAttempt {
            scenario_id: scenario.scenario_id.clone(),
            operation: scenario.operation,
            interaction_id: analysis.interaction_id,
            pre_observation,
            operation_outcome,
            post_observation: ObservationAvailability::Unavailable,
            failure_code: Some(
                analysis
                    .failure_code
                    .unwrap_or(ControlFailureCode::OperationFailed),
            ),
            barriers,
            recovered: false,
        });
    }
    if analysis.requires_embedded_observation && !analysis.embedded_observation_available {
        operation_outcome = ControlOperationOutcome::Failed;
        return Ok(ControlAttempt {
            scenario_id: scenario.scenario_id.clone(),
            operation: scenario.operation,
            interaction_id: analysis.interaction_id,
            pre_observation,
            operation_outcome,
            post_observation: ObservationAvailability::Unavailable,
            failure_code: Some(ControlFailureCode::MissingPostObservation),
            barriers,
            recovered: false,
        });
    }

    let observation_target = match &operation_result {
        BrowserOperationResult::ClosePage(_) => selected_target_after(context).await?,
        BrowserOperationResult::CreatePage(value)
        | BrowserOperationResult::SelectPage(value)
        | BrowserOperationResult::NavigatePage(value)
        | BrowserOperationResult::ReloadPage(value)
        | BrowserOperationResult::GoBack(value)
        | BrowserOperationResult::GoForward(value) => Some(value.interaction.target_id),
        _ => Some(context.target_id),
    };
    let Some(observation_target) = observation_target else {
        return Ok(ControlAttempt {
            scenario_id: scenario.scenario_id.clone(),
            operation: scenario.operation,
            interaction_id: analysis.interaction_id,
            pre_observation,
            operation_outcome,
            post_observation: ObservationAvailability::Unavailable,
            failure_code: Some(ControlFailureCode::TargetReplacement),
            barriers,
            recovered: false,
        });
    };
    context.target_id = observation_target;
    let post_observation = observe_live(context, observation_target).await;
    if post_observation != ObservationAvailability::Available {
        return Ok(ControlAttempt {
            scenario_id: scenario.scenario_id.clone(),
            operation: scenario.operation,
            interaction_id: analysis.interaction_id,
            pre_observation,
            operation_outcome,
            post_observation,
            failure_code: Some(ControlFailureCode::MissingPostObservation),
            barriers,
            recovered: false,
        });
    }
    barriers
        .record(ControlBarrier::PostActionObservationPresent)
        .map_err(BarrierProtocolError::into_error)?;
    if let Some(target_id) = selected_target_after(context).await? {
        context.target_id = target_id;
    }
    if !fixture_settled(context).await? {
        return Ok(ControlAttempt {
            scenario_id: scenario.scenario_id.clone(),
            operation: scenario.operation,
            interaction_id: analysis.interaction_id,
            pre_observation,
            operation_outcome,
            post_observation,
            failure_code: Some(ControlFailureCode::FixtureSettle),
            barriers,
            recovered: false,
        });
    }
    barriers
        .record(ControlBarrier::FixtureSettled)
        .map_err(BarrierProtocolError::into_error)?;
    let session_id = match context.session.status().await {
        Ok(status) => status.session_id,
        Err(_) => {
            return Ok(ControlAttempt {
                scenario_id: scenario.scenario_id.clone(),
                operation: scenario.operation,
                interaction_id: analysis.interaction_id,
                pre_observation,
                operation_outcome,
                post_observation,
                failure_code: Some(ControlFailureCode::CaptureFence),
                barriers,
                recovered: false,
            });
        }
    };
    if bounded(
        ControlBarrier::CaptureFenceAcknowledged,
        DEFAULT_BARRIER_TIMEOUT,
        context.runtime.dependencies.recording.flush(session_id),
    )
    .await
    .is_err()
    {
        return Ok(ControlAttempt {
            scenario_id: scenario.scenario_id.clone(),
            operation: scenario.operation,
            interaction_id: analysis.interaction_id,
            pre_observation,
            operation_outcome,
            post_observation,
            failure_code: Some(ControlFailureCode::CaptureFence),
            barriers,
            recovered: false,
        });
    }
    barriers
        .record(ControlBarrier::CaptureFenceAcknowledged)
        .map_err(BarrierProtocolError::into_error)?;
    if query_interval(context, analysis.interaction_id)
        .await
        .is_err()
    {
        return Ok(ControlAttempt {
            scenario_id: scenario.scenario_id.clone(),
            operation: scenario.operation,
            interaction_id: analysis.interaction_id,
            pre_observation,
            operation_outcome,
            post_observation,
            failure_code: Some(ControlFailureCode::IntervalQuery),
            barriers,
            recovered: false,
        });
    }
    barriers
        .record(ControlBarrier::IntervalQueryComplete)
        .map_err(BarrierProtocolError::into_error)?;
    Ok(ControlAttempt {
        scenario_id: scenario.scenario_id.clone(),
        operation: scenario.operation,
        interaction_id: analysis.interaction_id,
        pre_observation,
        operation_outcome,
        post_observation,
        failure_code: None,
        barriers,
        recovered: false,
    })
}

async fn observe_live(
    context: &LiveTrialContext<'_>,
    target_id: TargetId,
) -> ObservationAvailability {
    let request = BrowserOperationRequest::ObserveLive(LiveObservationRequest::new(target_id));
    let Ok(result) = bounded(
        ControlBarrier::PostActionObservationPresent,
        DEFAULT_BARRIER_TIMEOUT,
        context
            .session
            .execute(request, krometrail_core::BrowserOperationContext::default()),
    )
    .await
    else {
        return ObservationAvailability::Unavailable;
    };
    match result {
        BrowserOperationResult::ObserveLive(observation)
            if live_observation_available(&observation) =>
        {
            ObservationAvailability::Available
        }
        _ => ObservationAvailability::Unavailable,
    }
}

fn live_observation_available(observation: &LiveObservation) -> bool {
    matches!(observation.page, ObservationPart::Available(_))
        && matches!(observation.snapshot, ObservationPart::Available(_))
        && matches!(observation.screenshot, ObservationPart::Available(_))
}

async fn fixture_settled(context: &LiveTrialContext<'_>) -> krometrail_core::Result<bool> {
    let expression = match context.fixture {
        ControlFixture::VerifiedInteractions | ControlFixture::WaitsAndBatches => {
            "document.readyState === 'complete' && typeof window.fixtureState === 'object' && (!('running' in window.fixtureState) || window.fixtureState.running === false) && (!document.querySelector('#run') || !document.querySelector('#run').disabled)"
        }
    };
    let request = WaitRequest::new(
        PageSelection::Target(context.target_id),
        WaitCondition::Page {
            expression: krometrail_core::NonEmptyText::new(expression)
                .expect("static settle expression"),
        },
        DEFAULT_BARRIER_TIMEOUT,
        CONTROL_POLL_INTERVAL,
    )?;
    let result = bounded(
        ControlBarrier::FixtureSettled,
        DEFAULT_BARRIER_TIMEOUT,
        context.session.execute(
            BrowserOperationRequest::Wait(request),
            krometrail_core::BrowserOperationContext::default(),
        ),
    )
    .await?;
    Ok(matches!(
        result,
        BrowserOperationResult::Wait(value)
            if matches!(value.outcome, WaitOutcome::Satisfied { .. })
    ))
}

async fn selected_target_after(
    context: &LiveTrialContext<'_>,
) -> krometrail_core::Result<Option<TargetId>> {
    Ok(bounded(
        ControlBarrier::PostActionObservationPresent,
        DEFAULT_BARRIER_TIMEOUT,
        context.session.status(),
    )
    .await?
    .selected_target_id)
}

struct OperationAnalysis {
    operation_succeeded: bool,
    requires_embedded_observation: bool,
    embedded_observation_available: bool,
    interaction_id: Option<InteractionId>,
    failure_code: Option<ControlFailureCode>,
}

fn analyze_operation(result: &BrowserOperationResult) -> OperationAnalysis {
    match result {
        BrowserOperationResult::CreatePage(value)
        | BrowserOperationResult::SelectPage(value)
        | BrowserOperationResult::ClosePage(value)
        | BrowserOperationResult::NavigatePage(value)
        | BrowserOperationResult::ReloadPage(value)
        | BrowserOperationResult::GoBack(value)
        | BrowserOperationResult::GoForward(value) => OperationAnalysis {
            operation_succeeded: matches!(value.outcome, PageOperationOutcome::Succeeded(_)),
            requires_embedded_observation: true,
            embedded_observation_available: matches!(
                &value.observation,
                ObservationPart::Available(observation) if live_observation_available(observation)
            ),
            interaction_id: Some(value.interaction.interaction_id),
            failure_code: match &value.outcome {
                PageOperationOutcome::Failed(error) => Some(failure_code_for_error(error.code)),
                PageOperationOutcome::Succeeded(_) => None,
            },
        },
        BrowserOperationResult::Click(value)
        | BrowserOperationResult::Fill(value)
        | BrowserOperationResult::PressKeys(value)
        | BrowserOperationResult::SelectOption(value)
        | BrowserOperationResult::Hover(value)
        | BrowserOperationResult::Drag(value)
        | BrowserOperationResult::Scroll(value)
        | BrowserOperationResult::UploadFiles(value)
        | BrowserOperationResult::HandleDialog(value) => OperationAnalysis {
            operation_succeeded: true,
            requires_embedded_observation: true,
            embedded_observation_available: live_observation_available(&value.observation),
            interaction_id: Some(value.record.id),
            failure_code: None,
        },
        BrowserOperationResult::Batch(value) => {
            let steps_succeeded = value
                .steps
                .iter()
                .all(|step| step.status == BatchStepStatus::Succeeded);
            OperationAnalysis {
                operation_succeeded: value.outcome == BatchOutcome::Completed && steps_succeeded,
                requires_embedded_observation: true,
                embedded_observation_available: matches!(
                    &value.final_observation,
                    ObservationPart::Available(observation) if live_observation_available(observation)
                ),
                interaction_id: value.steps.iter().find_map(|step| {
                    step.interaction
                        .as_ref()
                        .map(|anchor| anchor.interaction_id)
                }),
                failure_code: (value.outcome != BatchOutcome::Completed)
                    .then_some(ControlFailureCode::OperationFailed),
            }
        }
        BrowserOperationResult::ObserveLive(value) => OperationAnalysis {
            operation_succeeded: live_observation_available(value),
            requires_embedded_observation: true,
            embedded_observation_available: live_observation_available(value),
            interaction_id: None,
            failure_code: (!live_observation_available(value))
                .then_some(ControlFailureCode::MissingPostObservation),
        },
        BrowserOperationResult::Wait(value) => OperationAnalysis {
            operation_succeeded: matches!(value.outcome, WaitOutcome::Satisfied { .. }),
            requires_embedded_observation: false,
            embedded_observation_available: false,
            interaction_id: None,
            failure_code: (!matches!(value.outcome, WaitOutcome::Satisfied { .. }))
                .then_some(ControlFailureCode::Timeout),
        },
        BrowserOperationResult::InspectPage(_)
        | BrowserOperationResult::SnapshotPage(_)
        | BrowserOperationResult::TakeScreenshot(_)
        | BrowserOperationResult::EvaluatePage(_)
        | BrowserOperationResult::ListPages(_) => OperationAnalysis {
            operation_succeeded: true,
            requires_embedded_observation: false,
            embedded_observation_available: false,
            interaction_id: None,
            failure_code: None,
        },
    }
}

fn request_for_scenario(
    scenario: &ControlScenario,
    context: &LiveTrialContext<'_>,
) -> krometrail_core::Result<Option<BrowserOperationRequest>> {
    let Some(fixture) = scenario.fixture else {
        return Ok(None);
    };
    let target = PageSelection::Target(context.target_id);
    let selector = |value: &str| {
        InteractionLocator::Element(ElementLocator::CssSelector(
            krometrail_core::NonEmptyText::new(value).expect("static fixture selector"),
        ))
    };
    let request = match scenario.operation {
        BrowserOperationKind::InspectPage => {
            BrowserOperationRequest::InspectPage(InspectPageRequest::new(context.target_id))
        }
        BrowserOperationKind::SnapshotPage => {
            BrowserOperationRequest::SnapshotPage(SnapshotPageRequest::new(context.target_id))
        }
        BrowserOperationKind::TakeScreenshot => {
            BrowserOperationRequest::TakeScreenshot(ScreenshotRequest::new(
                context.target_id,
                ScreenshotTarget::Viewport,
                ImageFormat::Png,
                None,
            )?)
        }
        BrowserOperationKind::EvaluatePage => BrowserOperationRequest::EvaluatePage(
            ReadOnlyEvaluationRequest::new(context.target_id, "window.fixtureState", false)?,
        ),
        BrowserOperationKind::ObserveLive => {
            BrowserOperationRequest::ObserveLive(LiveObservationRequest::new(context.target_id))
        }
        BrowserOperationKind::ListPages => {
            BrowserOperationRequest::ListPages(krometrail_core::ListPagesRequest)
        }
        BrowserOperationKind::CreatePage => {
            BrowserOperationRequest::CreatePage(CreatePageRequest::new(Some(fixture.url()))?)
        }
        BrowserOperationKind::SelectPage => {
            BrowserOperationRequest::SelectPage(SelectPageRequest {
                target_id: context.primary_target_id,
            })
        }
        BrowserOperationKind::ClosePage => {
            BrowserOperationRequest::ClosePage(krometrail_core::ClosePageRequest { target })
        }
        BrowserOperationKind::NavigatePage => {
            BrowserOperationRequest::NavigatePage(NavigatePageRequest::new(target, fixture.url())?)
        }
        BrowserOperationKind::ReloadPage => {
            BrowserOperationRequest::ReloadPage(ReloadPageRequest {
                target,
                bypass_cache: false,
            })
        }
        BrowserOperationKind::GoBack => BrowserOperationRequest::GoBack(GoBackRequest { target }),
        BrowserOperationKind::GoForward => {
            BrowserOperationRequest::GoForward(GoForwardRequest { target })
        }
        BrowserOperationKind::Click => BrowserOperationRequest::Click(ClickRequest::new(
            target,
            selector(if fixture == ControlFixture::WaitsAndBatches {
                "#increment"
            } else {
                "#click-target"
            }),
            MouseButton::Left,
            Modifiers::default(),
            1,
            false,
        )?),
        BrowserOperationKind::Fill => BrowserOperationRequest::Fill(FillRequest::new(
            target,
            selector("#text-input"),
            "qualification",
            FillMode::Replace,
            false,
        )?),
        BrowserOperationKind::PressKeys => {
            BrowserOperationRequest::PressKeys(krometrail_core::PressKeysRequest::new(
                target,
                Some(selector("#text-input")),
                vec![KeyChord::new("Control+S")?],
                false,
            )?)
        }
        BrowserOperationKind::SelectOption => {
            BrowserOperationRequest::SelectOption(SelectOptionRequest::new(
                target,
                selector("#select"),
                SelectValue::Label(krometrail_core::NonEmptyText::new("Two").unwrap()),
            )?)
        }
        BrowserOperationKind::Hover => BrowserOperationRequest::Hover(HoverRequest {
            target,
            locator: selector("#hover-target"),
        }),
        BrowserOperationKind::Drag => BrowserOperationRequest::Drag(DragRequest {
            target,
            source: selector("#drag-source"),
            destination: selector("#drop-target"),
        }),
        BrowserOperationKind::Scroll => BrowserOperationRequest::Scroll(ScrollRequest {
            target,
            delta: ScrollDelta::ByOffset { dx: 0.0, dy: 100.0 },
        }),
        BrowserOperationKind::UploadFiles => {
            let path = std::env::temp_dir().join(format!(
                "krometrail-control-qualification-{}.txt",
                std::process::id()
            ));
            fs::write(&path, b"qualification upload").map_err(|_| {
                live_error(
                    ErrorCode::PersistenceFailed,
                    "control upload fixture could not be prepared",
                )
            })?;
            BrowserOperationRequest::UploadFiles(krometrail_core::UploadFilesRequest::new(
                target,
                selector("#file-input"),
                vec![krometrail_core::ValidatedFilePath::new(
                    path.to_string_lossy(),
                )?],
            )?)
        }
        BrowserOperationKind::HandleDialog => {
            BrowserOperationRequest::HandleDialog(HandleDialogRequest {
                target,
                action: DialogAction::Accept { prompt_text: None },
            })
        }
        BrowserOperationKind::Wait => BrowserOperationRequest::Wait(WaitRequest::new(
            target,
            WaitCondition::Page {
                expression: krometrail_core::NonEmptyText::new(
                    "document.readyState === 'complete'",
                )
                .unwrap(),
            },
            DEFAULT_BARRIER_TIMEOUT,
            CONTROL_POLL_INTERVAL,
        )?),
        BrowserOperationKind::Batch => {
            let click = BrowserOperationRequest::Click(ClickRequest::new(
                target,
                selector("#increment"),
                MouseButton::Left,
                Modifiers::default(),
                1,
                false,
            )?);
            let wait = BrowserOperationRequest::Wait(WaitRequest::new(
                target,
                WaitCondition::Page {
                    expression: krometrail_core::NonEmptyText::new(
                        "window.fixtureState.count >= 1",
                    )
                    .unwrap(),
                },
                DEFAULT_BARRIER_TIMEOUT,
                CONTROL_POLL_INTERVAL,
            )?);
            let evaluate = BrowserOperationRequest::EvaluatePage(ReadOnlyEvaluationRequest::new(
                context.target_id,
                "window.fixtureState.count",
                false,
            )?);
            BrowserOperationRequest::Batch(BatchRequest::new(
                target,
                vec![click, wait, evaluate],
                DEFAULT_BARRIER_TIMEOUT,
                BatchOptions::default(),
            )?)
        }
    };
    Ok(Some(request))
}

async fn query_interval(
    context: &LiveTrialContext<'_>,
    interaction_id: Option<InteractionId>,
) -> krometrail_core::Result<()> {
    let status = bounded(
        ControlBarrier::TargetAttached,
        DEFAULT_BARRIER_TIMEOUT,
        context.session.status(),
    )
    .await?;
    if let Some(interaction_id) = interaction_id {
        let target_id = context
            .runtime
            .store
            .interaction_anchor(interaction_id)
            .await?
            .map(|anchor| anchor.target_id)
            .unwrap_or(context.target_id);
        let authorities = IntervalAuthorities {
            query: context.runtime.dependencies.temporal_queries.as_ref(),
            frames: context.runtime.dependencies.frames.as_ref(),
            gaps: context.runtime.dependencies.gaps.as_ref(),
            interactions: context.runtime.store.as_ref(),
        };
        bounded(
            ControlBarrier::IntervalQueryComplete,
            DEFAULT_BARRIER_TIMEOUT,
            source_interval_for_interaction(
                &authorities,
                status.session_id,
                target_id,
                interaction_id,
            ),
        )
        .await
        .map(|_| ())
    } else {
        let target_id = status.selected_target_id.unwrap_or(context.target_id);
        let now = context.runtime.dependencies.clock.now();
        let range = SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(now.as_nanos()))?;
        let request = TemporalQueryRequest::strict(TemporalRangeAnchor::SessionTime {
            scope: AnchorScope::new(Some(status.session_id), Some(target_id)),
            range,
        })?;
        bounded(
            ControlBarrier::IntervalQueryComplete,
            DEFAULT_BARRIER_TIMEOUT,
            context
                .runtime
                .dependencies
                .temporal_queries
                .resolve_range(request),
        )
        .await
        .map(|_| ())
    }
}

fn failure_code_for_error(code: ErrorCode) -> ControlFailureCode {
    match code {
        ErrorCode::Unsupported => ControlFailureCode::UnsupportedCapability,
        ErrorCode::StaleReference => ControlFailureCode::StaleReference,
        ErrorCode::TargetFailed => ControlFailureCode::TargetReplacement,
        ErrorCode::WaitTimedOut => ControlFailureCode::Timeout,
        ErrorCode::BrowserDisconnected
        | ErrorCode::BrowserProcessTerminated
        | ErrorCode::ReconnectExhausted
        | ErrorCode::Cancelled => ControlFailureCode::TransportLoss,
        ErrorCode::PageObservationFailed | ErrorCode::ScreenshotFailed => {
            ControlFailureCode::MissingPostObservation
        }
        _ => ControlFailureCode::OperationFailed,
    }
}

pub async fn recover_after_failure(
    context: &mut LiveTrialContext<'_>,
    _failure: ControlFailureCode,
) -> bool {
    wait_for_ready_barrier(context).await.is_ok()
}

/// Authorized real-browser control entry point.  It is test-only and has two independent opt-in
/// gates; ordinary qualification tests call only the scripted seams below.
pub async fn run_opted_in_control(
    config: LiveQualificationConfig,
) -> krometrail_core::Result<ControlQualificationRun> {
    if OptInDecision::from_environment() != OptInDecision::Authorized {
        return Err(live_error(
            ErrorCode::InvalidLifecycleTransition,
            "live control qualification requires both explicit opt-in environment gates",
        ));
    }
    let preflight = super::run_preflight(config.clone()).await?;
    let BrowserPreflight::Ready(installation) = preflight.browser.as_ref().ok_or_else(|| {
        live_error(
            ErrorCode::BrowserNotFound,
            "live browser preflight did not run",
        )
    })?
    else {
        return Err(live_error(
            ErrorCode::BrowserNotFound,
            "live browser preflight did not find a required browser",
        ));
    };
    let lifecycle = QualificationLifecycle::start(&config, &preflight).await?;
    let runtime = super::build_qualification_runtime(&config, OptInDecision::Authorized)?;
    let wrapper = super::capture::qualification_wrapper(installation, lifecycle.viewport());
    let initial_url = krometrail_cdp::qualification_support::verified_interactions_fixture_url();
    let session = runtime
        .dependencies
        .browser
        .connect(krometrail_core::BrowserConnectRequest::Launch(
            krometrail_core::LaunchBrowser {
                executable: wrapper.as_ref().map(|wrapper| wrapper.path.clone()),
                profile: krometrail_core::ManagedProfile::Temporary,
                initial_url: Some(initial_url),
            },
        ))
        .await?;
    let result = run_control_session(&runtime, Arc::clone(&session), &lifecycle).await;
    let stop = session.stop().await;
    let cleanup = lifecycle.cleanup();
    let _ = wrapper;
    let _ = runtime.cleanup();
    result.and_then(|result| {
        stop?;
        if cleanup.is_clean() {
            Ok(result)
        } else {
            Err(live_error(
                ErrorCode::PersistenceFailed,
                "live control qualification cleanup did not complete",
            ))
        }
    })
}

pub async fn run_control_session(
    runtime: &QualificationRuntime,
    session: Arc<dyn BrowserSessionPort>,
    lifecycle: &QualificationLifecycle,
) -> krometrail_core::Result<ControlQualificationRun> {
    let status = bounded(
        ControlBarrier::TargetAttached,
        DEFAULT_BARRIER_TIMEOUT,
        session.status(),
    )
    .await?;
    let primary_target_id = ready_target(&status)?;
    let mut current_target_id = primary_target_id;
    let mut current_fixture = None;
    let mut attempts = Vec::new();
    for scenario in control_scenarios() {
        let Some(fixture) = scenario.fixture else {
            attempts.push(ControlAttempt::unavailable(
                &scenario,
                BarrierTrace::new(),
                ControlFailureCode::UnsupportedCapability,
            ));
            continue;
        };
        let mut context = LiveTrialContext {
            runtime,
            lifecycle,
            session: Arc::clone(&session),
            fixture,
            target_id: current_target_id,
            primary_target_id,
        };
        if current_fixture != Some(fixture) {
            if let Err(error) = navigate_setup(&context, fixture.url()).await {
                let mut attempt = ControlAttempt::unavailable(
                    &scenario,
                    BarrierTrace::new(),
                    failure_code_for_error(error.code),
                );
                attempt.recovered = recover_after_failure(
                    &mut context,
                    attempt
                        .failure_code
                        .unwrap_or(ControlFailureCode::RecoveryFailed),
                )
                .await;
                attempts.push(attempt);
                current_target_id = context.target_id;
                continue;
            }
            current_fixture = Some(fixture);
        }
        if let Err(error) = prepare_scenario(&mut context, &scenario).await {
            let mut attempt = ControlAttempt::unavailable(
                &scenario,
                BarrierTrace::new(),
                failure_code_for_error(error.code),
            );
            attempt.recovered = recover_after_failure(
                &mut context,
                attempt
                    .failure_code
                    .unwrap_or(ControlFailureCode::RecoveryFailed),
            )
            .await;
            attempts.push(attempt);
            current_target_id = context.target_id;
            continue;
        }
        let mut attempt = execute_control_trial(&mut context, &scenario).await?;
        if let Some(failure) = attempt.failure_code {
            attempt.recovered = recover_after_failure(&mut context, failure).await;
        }
        current_target_id = context.target_id;
        attempts.push(attempt);
    }
    Ok(summarize_control(&attempts))
}

async fn prepare_scenario(
    context: &mut LiveTrialContext<'_>,
    scenario: &ControlScenario,
) -> krometrail_core::Result<()> {
    match scenario.operation {
        BrowserOperationKind::NavigatePage | BrowserOperationKind::ReloadPage => {
            navigate_setup(context, context.fixture.url()).await?;
        }
        BrowserOperationKind::GoBack | BrowserOperationKind::GoForward => {
            navigate_setup(context, context.fixture.url()).await?;
            let second =
                krometrail_cdp::qualification_support::waits_and_batches_fixture_url("second.html");
            navigate_setup(context, second).await?;
            if scenario.operation == BrowserOperationKind::GoForward {
                run_setup_operation(
                    context,
                    BrowserOperationRequest::GoBack(GoBackRequest {
                        target: PageSelection::Target(context.target_id),
                    }),
                )
                .await?;
            }
        }
        BrowserOperationKind::SelectPage | BrowserOperationKind::ClosePage => {
            let created = run_setup_operation(
                context,
                BrowserOperationRequest::CreatePage(CreatePageRequest::new(Some(
                    context.fixture.url(),
                ))?),
            )
            .await?;
            let BrowserOperationResult::CreatePage(value) = created else {
                return Err(live_error(
                    ErrorCode::TargetFailed,
                    "control target setup returned the wrong operation",
                ));
            };
            context.target_id = value.interaction.target_id;
        }
        BrowserOperationKind::HandleDialog => {
            run_setup_operation(
                context,
                BrowserOperationRequest::Click(ClickRequest::new(
                    PageSelection::Target(context.target_id),
                    InteractionLocator::Element(ElementLocator::CssSelector(
                        krometrail_core::NonEmptyText::new("#confirm-target")
                            .expect("static dialog selector"),
                    )),
                    MouseButton::Left,
                    Modifiers::default(),
                    1,
                    false,
                )?),
            )
            .await?;
        }
        _ => {}
    }
    Ok(())
}

async fn run_setup_operation(
    context: &LiveTrialContext<'_>,
    request: BrowserOperationRequest,
) -> krometrail_core::Result<BrowserOperationResult> {
    let result = bounded(
        ControlBarrier::StructuredOperationSubmitted,
        DEFAULT_BARRIER_TIMEOUT,
        context
            .session
            .execute(request, krometrail_core::BrowserOperationContext::default()),
    )
    .await?;
    let valid = match &result {
        BrowserOperationResult::CreatePage(value)
        | BrowserOperationResult::SelectPage(value)
        | BrowserOperationResult::ClosePage(value)
        | BrowserOperationResult::NavigatePage(value)
        | BrowserOperationResult::ReloadPage(value)
        | BrowserOperationResult::GoBack(value)
        | BrowserOperationResult::GoForward(value) => {
            matches!(value.outcome, PageOperationOutcome::Succeeded(_))
                && matches!(&value.observation, ObservationPart::Available(observation) if live_observation_available(observation))
        }
        BrowserOperationResult::Click(value)
        | BrowserOperationResult::Fill(value)
        | BrowserOperationResult::PressKeys(value)
        | BrowserOperationResult::SelectOption(value)
        | BrowserOperationResult::Hover(value)
        | BrowserOperationResult::Drag(value)
        | BrowserOperationResult::Scroll(value)
        | BrowserOperationResult::UploadFiles(value)
        | BrowserOperationResult::HandleDialog(value) => {
            live_observation_available(&value.observation)
        }
        _ => true,
    };
    if valid {
        Ok(result)
    } else {
        Err(live_error(
            ErrorCode::TargetFailed,
            "control setup operation did not produce live evidence",
        ))
    }
}

async fn navigate_setup(
    context: &LiveTrialContext<'_>,
    url: String,
) -> krometrail_core::Result<()> {
    let request = BrowserOperationRequest::NavigatePage(NavigatePageRequest::new(
        PageSelection::Target(context.target_id),
        url,
    )?);
    let result = bounded(
        ControlBarrier::StructuredOperationSubmitted,
        DEFAULT_BARRIER_TIMEOUT,
        context
            .session
            .execute(request, krometrail_core::BrowserOperationContext::default()),
    )
    .await?;
    let BrowserOperationResult::NavigatePage(value) = result else {
        return Err(live_error(
            ErrorCode::InvalidInput,
            "control fixture setup returned the wrong operation",
        ));
    };
    if !matches!(value.outcome, PageOperationOutcome::Succeeded(_))
        || !matches!(&value.observation, ObservationPart::Available(observation) if live_observation_available(observation))
    {
        return Err(live_error(
            ErrorCode::NavigationFailed,
            "control fixture setup did not produce a live observation",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::live_evaluation::barriers::ControlBarrier;

    fn attempt(
        id: &str,
        operation: BrowserOperationKind,
        success: bool,
        failure: Option<ControlFailureCode>,
    ) -> ControlAttempt {
        let mut barriers = BarrierTrace::new();
        if success {
            for stage in ControlBarrier::ORDER {
                barriers.record(stage).unwrap();
            }
        }
        ControlAttempt {
            scenario_id: id.into(),
            operation,
            interaction_id: None,
            pre_observation: if success {
                ObservationAvailability::Available
            } else {
                ObservationAvailability::Unavailable
            },
            operation_outcome: if success {
                ControlOperationOutcome::Succeeded
            } else {
                ControlOperationOutcome::Failed
            },
            post_observation: if success {
                ObservationAvailability::Available
            } else {
                ObservationAvailability::Unavailable
            },
            failure_code: failure,
            barriers,
            recovered: !success,
        }
    }

    #[test]
    fn scenarios_follow_the_operation_registry_without_a_second_identity_list() {
        let scenarios = control_scenarios();
        assert_eq!(scenarios.len(), BROWSER_OPERATION_REGISTRY.len());
        assert_eq!(
            scenarios
                .iter()
                .map(|scenario| scenario.operation)
                .collect::<Vec<_>>(),
            BROWSER_OPERATION_REGISTRY
                .iter()
                .map(|definition| definition.kind)
                .collect::<Vec<_>>()
        );
        assert!(
            scenarios
                .windows(2)
                .all(|pair| pair[0].scenario_id != pair[1].scenario_id)
        );
        assert!(scenarios.iter().all(|scenario| {
            BROWSER_OPERATION_REGISTRY
                .iter()
                .any(|definition| definition.kind == scenario.operation)
        }));
    }

    #[test]
    fn transport_acknowledgement_without_observation_is_not_success() {
        let scenario = control_scenarios()
            .into_iter()
            .find(|scenario| scenario.operation == BrowserOperationKind::Click)
            .unwrap();
        let mut barriers = BarrierTrace::new();
        for stage in ControlBarrier::ORDER[..6].iter().copied() {
            barriers.record(stage).unwrap();
        }
        let attempt = ControlAttempt {
            scenario_id: scenario.scenario_id,
            operation: scenario.operation,
            interaction_id: Some(InteractionId::from_uuid(uuid::Uuid::from_u128(1))),
            pre_observation: ObservationAvailability::Available,
            operation_outcome: ControlOperationOutcome::Succeeded,
            post_observation: ObservationAvailability::Unavailable,
            failure_code: Some(ControlFailureCode::MissingPostObservation),
            barriers,
            recovered: true,
        };
        assert!(!attempt.is_success());
    }

    #[test]
    fn accounting_is_exact_and_unavailable_failures_are_not_passes() {
        let attempts = vec![
            attempt("a", BrowserOperationKind::Click, true, None),
            attempt(
                "b",
                BrowserOperationKind::Wait,
                false,
                Some(ControlFailureCode::Timeout),
            ),
            attempt(
                "c",
                BrowserOperationKind::UploadFiles,
                false,
                Some(ControlFailureCode::UnsupportedCapability),
            ),
        ];
        let run = summarize_control(&attempts);
        assert_eq!(run.control.attempts, 3);
        assert_eq!(run.control.successes, 1);
        assert_eq!(run.control.success_rate_basis_points, 3_333);
        assert_eq!(run.control.failed_observation_ids, vec!["b", "c"]);
        assert_eq!(run.status, EvaluationStatus::Inconclusive);
        assert_eq!(
            run.canonical_bytes().unwrap(),
            run.canonical_bytes().unwrap()
        );
    }

    #[test]
    fn failure_categories_are_stable_and_safe() {
        for (error, expected) in [
            (
                ErrorCode::Unsupported,
                ControlFailureCode::UnsupportedCapability,
            ),
            (
                ErrorCode::StaleReference,
                ControlFailureCode::StaleReference,
            ),
            (
                ErrorCode::TargetFailed,
                ControlFailureCode::TargetReplacement,
            ),
            (ErrorCode::WaitTimedOut, ControlFailureCode::Timeout),
            (
                ErrorCode::BrowserDisconnected,
                ControlFailureCode::TransportLoss,
            ),
        ] {
            assert_eq!(failure_code_for_error(error), expected);
        }
        let text = serde_json::to_string(&ControlFailureCode::TransportLoss).unwrap();
        assert_eq!(text, "\"transport_loss\"");
    }
}
