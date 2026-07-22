use std::{any::Any, sync::Arc, time::Instant};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use krometrail_core::{
    ArtifactCacheDisposition, ArtifactGenerationResult, ArtifactHandle, ArtifactId,
    ArtifactOutcome, BatchOutcome, BatchResult, BrowserOperationKind, BrowserOperationResult,
    BrowserOwnership, BrowserSessionState, BrowserStatus, BrowserStopOutcome, CaptureFailure,
    CaptureStreamState, CssRect, EncodedScreenshot, ErrorCode, EveryNthFrame, InteractionAnchor,
    InteractionId, InteractionTiming, KrometrailError, LiveObservation, NonEmptyText,
    ObservationPart, PageAssetInventory, PageAssetKind, PageAssetMetadata, PageOperationOutcome,
    PageOperationResult, PageSnapshot, ProfileRef, ProgressiveEvidence, ProgressiveEvidenceContext,
    ProgressiveEvidenceRequest, ProgressiveEvidenceResult, RecordingBudgetState,
    ResolvedRangeHandleId, RetrieveArtifactRequest, RetryAdvice, ScreenshotMetadata, SessionId,
    SessionTime, ShutdownQuality, SourceFrameBatch, SourceFrameHandle, TargetId,
    TemporalDebugBundle, TemporalRangeAnchorKind, TemporalRangeResolution,
    TemporalVideoGenerationResult, VideoPresentationPolicy, WaitOutcome,
};
use rmcp::model::JsonObject;
use rmcp::model::{CallToolResult, Content, RawResource};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use temporal_vision::{ArtifactKind, EvidenceClass, PixelDimensions};

use crate::resources::{ResourceKind, ResourceProjection};
use crate::session::SnapshotNovelty;

const MAX_CONCISE_TARGETS: usize = 24;
const MAX_CONCISE_TARGET_JSON_BYTES: usize = 6 * 1024;
const MAX_EXPANDED_TARGETS: usize = 48;
const MAX_EXPANDED_TARGET_JSON_BYTES: usize = 12 * 1024;
const MAX_EXPANDED_CONTEXT_NODES: usize = 96;
const MAX_EXPANDED_SNAPSHOT_JSON_BYTES: usize = 32 * 1024;
// `full` is the most complete *bounded* projection, not an unbounded dump: an ordinary
// encyclopedia article produced a ~930 KB accessibility tree that exceeded an agent's whole
// context. Every full ceiling is four times its expanded counterpart, so the extra detail is
// real while the worst case stays predictable and its omissions stay accounted for.
const MAX_FULL_TARGETS: usize = 192;
const MAX_FULL_TARGET_JSON_BYTES: usize = 48 * 1024;
const MAX_FULL_CONTEXT_NODES: usize = 384;
const MAX_FULL_SNAPSHOT_JSON_BYTES: usize = 128 * 1024;
const MAX_CONCISE_ASSETS: usize = 16;
const MAX_CONCISE_ASSET_JSON_BYTES: usize = 6 * 1024;
const MAX_EXPANDED_ASSETS: usize = 64;
const MAX_EXPANDED_ASSET_JSON_BYTES: usize = 16 * 1024;
const MAX_FULL_ASSETS: usize = 256;
const MAX_FULL_ASSET_JSON_BYTES: usize = 64 * 1024;
const MAX_SEMANTIC_OUTCOMES: usize = 8;
const MAX_SEMANTIC_OUTCOME_JSON_BYTES: usize = 4 * 1024;
// Bounded temporal identifier presentation. Domain values stay exact; only the
// serialized presentation caps id enumeration, always with exact omission
// accounting and named drill-down (range handle, paginated listings, manifest
// resources).
const MAX_EXPANDED_RANGE_EVENT_IDS: usize = 32; // per kind: interaction/navigation/marker
const MAX_FULL_RANGE_EVENT_IDS: usize = 128;
const MAX_FULL_RANGE_FRAME_IDS: usize = 256;
const MAX_FULL_MANIFEST_IDS: usize = 256;
const MAX_CONCISE_PROJECTED_EPOCHS: usize = 8;
const MAX_PROJECTED_EPOCHS: usize = 32;
const MAX_PROJECTED_PIN_FRAME_IDS: usize = 32;
const MAX_FULL_PROJECTED_PIN_FRAME_IDS: usize = 256;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ResponseDetail {
    #[default]
    Concise,
    Expanded,
    Full,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ResponseRequest {
    #[serde(default)]
    pub detail: ResponseDetail,
    #[serde(default)]
    #[schemars(with = "bool")]
    pub inline_images: Option<bool>,
}

impl ResponseRequest {
    pub(crate) fn with_inline_default(mut self, default: bool) -> Self {
        self.inline_images = Some(self.inline_images.unwrap_or(default));
        self
    }

    fn includes_images(self) -> bool {
        self.inline_images.unwrap_or(false)
    }

    fn includes_images_for(self, tool: &str) -> bool {
        self.inline_images.unwrap_or(tool == "fetch_source_frames")
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ConciseCaptureStatus {
    pub target_id: TargetId,
    pub state: CaptureStreamState,
    pub received_frames: u64,
    pub persisted_frames: u64,
    pub dropped_frames: u64,
    pub known_gap_count: u64,
    pub last_frame_session_time: Option<SessionTime>,
    pub failure: Option<CaptureFailure>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ConciseRetentionStatus {
    pub used_bytes: u64,
    pub configured_bytes: u64,
    pub pinned_bytes: u64,
    pub budget_state: RecordingBudgetState,
    pub recording_blocked: bool,
    pub retained_bounds: Option<RetainedBounds>,
}

/// Retained-evidence endpoints projected with their comparability made explicit.
///
/// `session_time` is session-relative, so the two endpoints are only orderable when they share
/// one session and target. Reporting the raw pair alone let an agent read `oldest > newest` and
/// conclude the store was corrupt, so the projection states the scope instead of implying one.
#[derive(Clone, Copy, Debug, Serialize)]
pub(crate) struct RetainedBounds {
    pub oldest: krometrail_core::RetainedPoint,
    pub newest: krometrail_core::RetainedPoint,
    /// True only when both endpoints share one session and target.
    pub comparable_scope: bool,
    /// Elapsed session time between the endpoints; present only within a comparable scope.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span_nanos: Option<u64>,
}

impl RetainedBounds {
    fn project(retention: &krometrail_core::RetentionStatus) -> Option<Self> {
        let (oldest, newest) = (retention.oldest_retained?, retention.newest_retained?);
        let comparable_scope =
            oldest.session_id == newest.session_id && oldest.target_id == newest.target_id;
        Some(Self {
            oldest,
            newest,
            comparable_scope,
            span_nanos: comparable_scope.then(|| {
                newest
                    .session_time
                    .as_nanos()
                    .saturating_sub(oldest.session_time.as_nanos())
            }),
        })
    }
}

/// One page reported as holding an open modal JavaScript dialog.
#[derive(Clone, Copy, Debug, Serialize)]
pub(crate) struct ConciseOpenDialog {
    pub target_id: TargetId,
    pub dialog_type: krometrail_core::BrowserDialogType,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ConciseBrowserStatus {
    pub session_id: SessionId,
    pub state: BrowserSessionState,
    pub ownership: BrowserOwnership,
    pub profile: ProfileRef,
    pub selected_target_id: Option<TargetId>,
    pub page_count: u32,
    /// Pages holding an open dialog. A dialog blocks that page's renderer, so a status call
    /// must surface it at every detail level rather than only where full page rows appear.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub open_dialogs: Vec<ConciseOpenDialog>,
    pub capture: Vec<ConciseCaptureStatus>,
    pub retention: ConciseRetentionStatus,
    pub every_nth_frame: EveryNthFrame,
}

pub(crate) fn split_response_request(
    mut arguments: JsonObject,
) -> krometrail_core::Result<(JsonObject, ResponseRequest)> {
    let preference = match arguments.remove("response") {
        None => ResponseRequest::default(),
        Some(value) => decode_response_request(value)?,
    };
    Ok((arguments, preference))
}

fn decode_response_request(value: Value) -> krometrail_core::Result<ResponseRequest> {
    let encoded = serde_json::to_vec(&value).map_err(|_| invalid_projection("response", None))?;
    let mut deserializer = serde_json::Deserializer::from_slice(&encoded);
    serde_path_to_error::deserialize(&mut deserializer).map_err(|error| {
        let nested = normalize_projection_path(&error.path().to_string());
        let description = bounded_serde_description(&error);
        invalid_projection(&format!("response.{nested}"), Some(&description))
    })
}

fn normalize_projection_path(path: &str) -> String {
    let normalized: String = path
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '.' | '[' | ']' | '$')
        })
        .take(128)
        .collect();
    if normalized.is_empty() || normalized == "." {
        "$".into()
    } else {
        normalized
    }
}

fn invalid_projection(path: &str, description: Option<&str>) -> KrometrailError {
    let message = match description {
        Some(description) => {
            format!(
                "tool arguments do not match the advertised input schema at {path}: {description}"
            )
        }
        None => format!("tool arguments do not match the advertised input schema at {path}"),
    };
    KrometrailError::new(
        ErrorCode::InvalidInput,
        NonEmptyText::new(message).expect("projection validation message is non-empty"),
    )
}

fn bounded_serde_description(error: impl std::fmt::Display) -> String {
    const MAX_SCHEMA_ERROR_BYTES: usize = 512;
    let description = error.to_string();
    let mut safe = String::with_capacity(description.len());
    let mut characters = description.chars().peekable();
    while let Some(character) = characters.next() {
        if character != '"' {
            safe.push(character);
            continue;
        }
        safe.push('"');
        safe.push_str("[redacted]");
        let mut escaped = false;
        for character in characters.by_ref() {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                break;
            }
        }
        safe.push('"');
    }
    let mut description = safe;
    if description.len() > MAX_SCHEMA_ERROR_BYTES {
        let mut end = MAX_SCHEMA_ERROR_BYTES;
        while !description.is_char_boundary(end) {
            end -= 1;
        }
        description.truncate(end);
    }
    description
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToolResponseStatus {
    Succeeded,
    Degraded,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ImageRole {
    RequestedScreenshot,
    LiveObservation,
    PostAction,
    BatchFinal,
    BatchStep,
    TemporalPrimary,
    TemporalSourceFrame,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ResourceRole {
    Artifact,
    ArtifactManifest,
    SourceFrame,
    Video,
    VideoManifest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, JsonSchema)]
pub struct VideoOutputDimensions {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, JsonSchema)]
pub struct TemporalVideoClipHandle {
    pub epoch_index: u32,
    #[schemars(with = "String")]
    pub cache: ArtifactCacheDisposition,
    pub artifact_id: ArtifactId,
    pub media_type: String,
    pub encoded_byte_len: u64,
    pub output_hash: String,
    pub presentation_policy: VideoPresentationPolicy,
    pub presentation_duration_nanos: u64,
    pub source_frame_count: u32,
    pub meaningful_frame_count: u32,
    pub gap_count: u32,
    pub output_dimensions: VideoOutputDimensions,
    pub video_uri: String,
    pub manifest_uri: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, JsonSchema)]
pub struct BundleArtifactHandle {
    pub artifact_id: ArtifactId,
    #[schemars(with = "String")]
    pub cache: ArtifactCacheDisposition,
    pub media_type: String,
    pub encoded_byte_len: u64,
    pub artifact_kind: ArtifactKind,
    pub evidence_class: EvidenceClass,
    // Three narrowing populations. `source` is every retained frame in the epoch, `analyzed` is
    // the frames that contributed to the artifact, and `selected` is the analyzed frames the
    // output actually renders or references. `omitted` is `source - analyzed`. Without
    // `analyzed_frame_count` the other three do not reconcile — an analysis that examined one
    // frame and rendered none reads as `source 1, selected 0, omitted 0`.
    pub source_frame_count: u32,
    pub analyzed_frame_count: u32,
    pub selected_frame_count: u32,
    pub omitted_frame_count: u32,
    /// The manifest's disclosed analysis-sampling mode when the analysis was
    /// decimated (for example `uniform_bounded`); absent for exhaustive or
    /// non-analysis artifacts. Together with `analyzed_frame_count` and
    /// `source_frame_count` this is the structured sampling accounting —
    /// by-design bounded sampling is not a degradation warning.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sampling_mode: Option<String>,
    pub output_dimensions: PixelDimensions,
    pub output_hash: String,
    pub manifest_uri: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, JsonSchema)]
pub struct ResponseResource {
    pub role: ResourceRole,
    pub uri: String,
    pub name: String,
    pub mime_type: Option<String>,
    pub encoded_byte_len: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResponseImageMetadata {
    Screenshot(ScreenshotMetadata),
    Artifact {
        artifact_id: ArtifactId,
        media_type: String,
        encoded_byte_len: u64,
        width: u32,
        height: u32,
    },
    // Source frames are retained images too, but are not generated artifacts.
    // Keeping this arm explicit avoids inventing an artifact identity for a frame.
    SourceFrame {
        frame_id: krometrail_core::FrameId,
        media_type: String,
        encoded_byte_len: u64,
        width: u32,
        height: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, JsonSchema)]
pub struct ResponseImage {
    pub role: ImageRole,
    pub step_index: Option<u32>,
    pub metadata: ResponseImageMetadata,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, JsonSchema)]
pub struct ResponseInteractionAnchor {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interaction_id: Option<InteractionId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    pub target_id: Option<TargetId>,
    pub operation: Option<BrowserOperationKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timing: Option<InteractionTiming>,
}

impl From<InteractionAnchor> for ResponseInteractionAnchor {
    fn from(anchor: InteractionAnchor) -> Self {
        Self {
            interaction_id: Some(anchor.interaction_id),
            session_id: Some(anchor.session_id),
            target_id: Some(anchor.target_id),
            operation: Some(anchor.operation),
            timing: Some(anchor.timing),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, JsonSchema)]
pub struct ResponseDiagnostics {
    pub correlation_id: String,
    pub log_path: Option<String>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct ToolResponse {
    pub tool: String,
    pub status: ToolResponseStatus,
    // Strict MCP clients reject boolean subschemas, so the free-form result
    // payload must advertise an object-form permissive schema.
    #[schemars(schema_with = "tool_result_subschema")]
    pub result: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range_handle: Option<ResolvedRangeHandleId>,
    pub interaction: Option<ResponseInteractionAnchor>,
    pub warnings: Vec<KrometrailError>,
    pub images: Vec<ResponseImage>,
    pub resources: Vec<ResponseResource>,
    pub error: Option<KrometrailError>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostics: Option<ResponseDiagnostics>,
}

fn tool_result_subschema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "description": "Tool-specific result payload; its shape varies by tool and response detail."
    })
}

#[derive(Clone, Debug)]
enum EncodedMcpImage {
    Screenshot {
        role: ImageRole,
        step_index: Option<u32>,
        screenshot: EncodedScreenshot,
    },
    Artifact {
        role: ImageRole,
        step_index: Option<u32>,
        artifact_id: ArtifactId,
        media_type: String,
        encoded_byte_len: u64,
        width: u32,
        height: u32,
        bytes: Arc<[u8]>,
    },
    SourceFrame {
        role: ImageRole,
        step_index: Option<u32>,
        frame_id: krometrail_core::FrameId,
        media_type: String,
        encoded_byte_len: u64,
        width: u32,
        height: u32,
        bytes: Arc<[u8]>,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct MappedResult {
    pub response: ToolResponse,
    pub summary: String,
    images: Vec<EncodedMcpImage>,
    pub is_error: bool,
}

impl MappedResult {
    pub(crate) fn with_range_handle(mut self, handle: ResolvedRangeHandleId) -> Self {
        self.response.range_handle = Some(handle);
        self
    }
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("browser tool result violated the MCP response contract")]
pub(crate) struct ResponseInvariantError;

struct Projection {
    status: ToolResponseStatus,
    result: Value,
    interaction: Option<ResponseInteractionAnchor>,
    warnings: Vec<KrometrailError>,
    images: Vec<EncodedMcpImage>,
    resources: Vec<ResponseResource>,
    error: Option<KrometrailError>,
}

impl Projection {
    fn success(result: Value) -> Self {
        Self {
            status: ToolResponseStatus::Succeeded,
            result,
            interaction: None,
            warnings: Vec::new(),
            images: Vec::new(),
            resources: Vec::new(),
            error: None,
        }
    }

    fn degrade_with(&mut self, warnings: Vec<KrometrailError>) {
        self.degrade_with_stage(warnings, "live_observation");
    }

    fn degrade_with_stage(&mut self, warnings: Vec<KrometrailError>, failure_stage: &str) {
        if !warnings.is_empty() && self.status == ToolResponseStatus::Succeeded {
            self.status = ToolResponseStatus::Degraded;
        }
        for warning in warnings {
            if self.warnings.contains(&warning) {
                continue;
            }
            tracing::warn!(
                event = "mcp.response.degraded",
                failure_stage,
                error_code = warning.code.as_str(),
                "mcp.response.degraded"
            );
            self.warnings.push(warning);
        }
    }

    fn fail_with(&mut self, error: KrometrailError) {
        tracing::warn!(
            event = "mcp.response.failed",
            failure_stage = "operation",
            error_code = error.code.as_str(),
            "mcp.response.failed"
        );
        self.status = ToolResponseStatus::Failed;
        self.error = Some(error);
    }
}

#[cfg(test)]
pub(crate) fn map_operation_result(
    tool: &str,
    result: BrowserOperationResult,
) -> Result<MappedResult, ResponseInvariantError> {
    map_operation_result_with_capture(tool, result, &[], ResponseRequest::default())
}

#[cfg(test)]
pub(crate) fn map_operation_result_with_capture(
    tool: &str,
    result: BrowserOperationResult,
    capture_statuses: &[krometrail_core::TargetCaptureStatus],
    response: ResponseRequest,
) -> Result<MappedResult, ResponseInvariantError> {
    let mut mapped =
        map_operation_result_with_novelty(tool, result, response, SnapshotNovelty::Novel)?;
    apply_capture_health(&mut mapped, capture_statuses);
    Ok(mapped)
}

pub(crate) fn map_operation_result_with_novelty(
    tool: &str,
    result: BrowserOperationResult,
    response: ResponseRequest,
    novelty: SnapshotNovelty,
) -> Result<MappedResult, ResponseInvariantError> {
    let mut projection = project_operation(result, response, novelty)?;
    project_response(tool, &mut projection, response)?;
    // A failed operation states its cause in the same shape as `visible_error`. A bare
    // "{tool} failed" text line was the only thing a caller reading tool text ever saw.
    let summary = match projection.status {
        ToolResponseStatus::Succeeded => format!("{tool} succeeded"),
        ToolResponseStatus::Degraded => {
            format!("{tool} succeeded with incomplete live evidence")
        }
        ToolResponseStatus::Failed => projection.error.as_ref().map_or_else(
            || format!("{tool} failed"),
            |error| format!("{tool} failed: {}", error.message),
        ),
    };
    Ok(mapped(tool, projection, summary))
}

fn capture_failed_warning(status: &krometrail_core::TargetCaptureStatus) -> KrometrailError {
    let failure = status
        .failure()
        .expect("failed capture status is validated with a failure");
    let mut warning = KrometrailError::from_browser_failure(
        krometrail_core::ErrorCode::CaptureFailed,
        krometrail_core::NonEmptyText::new(format!(
            "current-state control may have succeeded, but retained temporal frames are unavailable after {}",
            failure.stage().as_str()
        ))
        .expect("capture failure warning is non-empty"),
    )
    .with_context(krometrail_core::ErrorContext {
        target_id: Some(status.target_id()),
        ..krometrail_core::ErrorContext::default()
    });
    if let Some(persistence) = failure.cause().persistence.clone() {
        warning = warning
            .with_persistence(persistence.clone())
            .with_recovery(capture_recovery(persistence.recoverability()));
    }
    warning
}

fn capture_recovery(recoverability: krometrail_core::PersistenceRecoverability) -> NonEmptyText {
    NonEmptyText::new(match recoverability {
        krometrail_core::PersistenceRecoverability::WriterUsable => {
            "start a new browser session before relying on temporal history"
        }
        krometrail_core::PersistenceRecoverability::WriterTerminal => {
            "restart the Krometrail MCP process, then start a new browser session"
        }
    })
    .expect("capture recovery is non-empty")
}

fn stop_warning(outcome: &BrowserStopOutcome) -> KrometrailError {
    let mut warning = KrometrailError::from_browser_failure(
        ErrorCode::CaptureFailed,
        NonEmptyText::new("browser authority was released with degraded evidence closure")
            .expect("stop warning is non-empty"),
    );
    if let Some(recovery) = outcome.recovery().cloned() {
        warning = warning.with_recovery(recovery);
    }
    if let Some(persistence) = outcome
        .capture_failure()
        .and_then(|failure| failure.cause().persistence.clone())
    {
        warning = warning.with_persistence(persistence);
    }
    warning
}

pub(crate) fn map_lifecycle_result<T: Serialize + 'static>(
    tool: &str,
    value: T,
) -> Result<MappedResult, ResponseInvariantError> {
    let stop_outcome = (&value as &dyn Any)
        .downcast_ref::<BrowserStopOutcome>()
        .cloned();
    let value = serde_json::to_value(value).map_err(|_| ResponseInvariantError)?;
    let mut projection = Projection::success(value);
    if let Some(outcome) = stop_outcome
        .as_ref()
        .filter(|value| value.quality() == ShutdownQuality::Degraded)
    {
        let phase = outcome
            .failed_phase()
            .expect("degraded stop outcome has a failed phase");
        projection.degrade_with_stage(vec![stop_warning(outcome)], phase.as_str());
    }
    Ok(mapped(
        tool,
        projection,
        if stop_outcome.is_some_and(|value| value.quality() == ShutdownQuality::Degraded) {
            format!("{tool} succeeded with degraded evidence closure")
        } else {
            format!("{tool} succeeded")
        },
    ))
}

pub(crate) fn map_temporal_context_result<T: Serialize + 'static>(
    tool: &str,
    value: T,
    response: ResponseRequest,
) -> Result<MappedResult, ResponseInvariantError> {
    let mut mapped = map_lifecycle_result(tool, value)?;
    project_temporal_value(&mut mapped.response.result, response.detail)?;
    Ok(mapped)
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ExpandedBrowserStatus {
    #[serde(flatten)]
    pub concise: ConciseBrowserStatus,
    pub pages: Vec<krometrail_core::PageStatus>,
}

pub(crate) fn map_browser_status(
    tool: &str,
    status: BrowserStatus,
    response: ResponseRequest,
) -> Result<MappedResult, ResponseInvariantError> {
    match response.detail {
        ResponseDetail::Full => {
            let bounds = RetainedBounds::project(&status.retention);
            let mut mapped = map_lifecycle_result(tool, status)?;
            project_retained_bounds(&mut mapped.response.result, bounds)?;
            Ok(mapped)
        }
        ResponseDetail::Concise | ResponseDetail::Expanded => {
            let capture = status
                .capture
                .iter()
                .map(|capture| ConciseCaptureStatus {
                    target_id: capture.target_id(),
                    state: capture.state(),
                    received_frames: capture.statistics().received_frames(),
                    persisted_frames: capture.statistics().persisted_frames(),
                    dropped_frames: capture.statistics().dropped_frames(),
                    known_gap_count: capture.statistics().gap_count(),
                    last_frame_session_time: capture.last_frame_session_time(),
                    failure: capture.failure().cloned(),
                })
                .collect();
            let retention = ConciseRetentionStatus {
                used_bytes: status
                    .retention
                    .usage
                    .total_bytes()
                    .map_err(|_| ResponseInvariantError)?,
                configured_bytes: status.retention.configured_budget.get(),
                pinned_bytes: status.retention.pinned_usage_bytes,
                budget_state: status.retention.budget_state,
                recording_blocked: status.retention.recording_blocked,
                retained_bounds: RetainedBounds::project(&status.retention),
            };
            let page_count =
                u32::try_from(status.pages.len()).map_err(|_| ResponseInvariantError)?;
            let concise = ConciseBrowserStatus {
                session_id: status.session_id,
                state: status.state,
                ownership: status.ownership,
                profile: status.profile,
                selected_target_id: status.selected_target_id,
                page_count,
                open_dialogs: status
                    .pages
                    .iter()
                    .filter_map(|page| {
                        Some(ConciseOpenDialog {
                            target_id: page.target.target.id(),
                            dialog_type: page.open_dialog.dialog_type()?,
                        })
                    })
                    .collect(),
                capture,
                retention,
                every_nth_frame: status.every_nth_frame,
            };
            if response.detail == ResponseDetail::Expanded {
                map_lifecycle_result(
                    tool,
                    ExpandedBrowserStatus {
                        concise,
                        pages: status.pages,
                    },
                )
            } else {
                map_lifecycle_result(tool, concise)
            }
        }
    }
}

/// Replaces the raw retained endpoints in a serialized status with the scoped projection, so no
/// detail tier hands an agent two session-relative times that look comparable but are not.
fn project_retained_bounds(
    value: &mut Value,
    bounds: Option<RetainedBounds>,
) -> Result<(), ResponseInvariantError> {
    let Some(retention) = value.get_mut("retention").and_then(Value::as_object_mut) else {
        return Err(ResponseInvariantError);
    };
    retention.remove("oldest_retained");
    retention.remove("newest_retained");
    retention.insert(
        "retained_bounds".into(),
        serde_json::to_value(bounds).map_err(|_| ResponseInvariantError)?,
    );
    Ok(())
}

pub(crate) fn map_temporal_video_result(
    tool: &str,
    result: TemporalVideoGenerationResult,
) -> Result<MappedResult, ResponseInvariantError> {
    let scope = artifact_scope(&result.range)?;
    let clip_count = result.clips.len();
    let mut clips = Vec::with_capacity(clip_count);
    let mut projection = Projection::success(Value::Null);
    for clip in &result.clips {
        let artifact = &clip.artifact;
        let manifest = &artifact.provenance;
        if artifact.scope != scope
            || artifact.media_type.as_str() != "video/mp4"
            || clip.epoch_index != manifest.plan().epoch().index
        {
            return Err(ResponseInvariantError);
        }
        let video =
            ResourceProjection::from_video(scope, artifact.artifact_id, artifact.encoded_byte_len)
                .map_err(|_| ResponseInvariantError)?;
        let manifest_bytes = serde_json::to_vec(manifest).map_err(|_| ResponseInvariantError)?;
        let manifest_resource = ResourceProjection::from_video_manifest(
            scope,
            artifact.artifact_id,
            manifest_bytes.len() as u64,
        )
        .map_err(|_| ResponseInvariantError)?;
        clips.push(TemporalVideoClipHandle {
            epoch_index: clip.epoch_index,
            cache: clip.cache,
            artifact_id: artifact.artifact_id,
            media_type: artifact.media_type.as_str().to_owned(),
            encoded_byte_len: artifact.encoded_byte_len,
            output_hash: artifact.content_sha256.to_string(),
            presentation_policy: manifest.plan().policy(),
            presentation_duration_nanos: manifest.plan().duration().as_nanos(),
            source_frame_count: u32::try_from(manifest.plan().input_frame_ids().len())
                .map_err(|_| ResponseInvariantError)?,
            meaningful_frame_count: u32::try_from(manifest.plan().meaningful_frame_ids().len())
                .map_err(|_| ResponseInvariantError)?,
            gap_count: u32::try_from(manifest.gap_evidence().len())
                .map_err(|_| ResponseInvariantError)?,
            output_dimensions: VideoOutputDimensions {
                width: manifest.profile().geometry().canvas().width(),
                height: manifest.profile().geometry().canvas().height(),
            },
            video_uri: video.uri.clone(),
            manifest_uri: manifest_resource.uri.clone(),
        });
        add_resource(&mut projection, video)?;
        add_resource(&mut projection, manifest_resource)?;
    }
    projection.result = json!({
        // Video responses are compact handle surfaces; the range presentation
        // is bounded at the expanded tier with exact omission accounting.
        "range": bounded_resolved_range(&result.range, ResponseDetail::Expanded)?,
        "clips": clips,
    });
    Ok(mapped(
        tool,
        projection,
        format!("{tool} generated {clip_count} retained clip(s)"),
    ))
}

pub(crate) fn visible_error(tool: &str, error: KrometrailError) -> CallToolResult {
    visible_error_with_capture(tool, error, &[])
}

pub(crate) fn visible_error_with_capture(
    tool: &str,
    error: KrometrailError,
    capture_statuses: &[krometrail_core::TargetCaptureStatus],
) -> CallToolResult {
    let summary = format!("{tool} failed: {}", error.message);
    let mut projection = Projection::success(json!({}));
    projection.interaction = failure_interaction_anchor(tool, &error);
    projection.fail_with(error);
    into_call_tool_result(mapped(tool, projection, summary), capture_statuses)
        .expect("stable error envelopes always serialize")
}

fn failure_interaction_anchor(
    tool: &str,
    error: &KrometrailError,
) -> Option<ResponseInteractionAnchor> {
    let target_id = error.context.target_id?;
    let operation = BrowserOperationKind::from_stable_name(tool)?;
    if !operation.is_interaction() {
        return None;
    }
    Some(ResponseInteractionAnchor {
        interaction_id: error.context.interaction_id,
        session_id: error.context.session_id,
        target_id: Some(target_id),
        operation: Some(operation),
        timing: None,
    })
}

fn projection_target_id(projection: &Projection) -> Option<TargetId> {
    response_target_id(projection.interaction.as_ref(), &projection.result)
}

fn response_target_id(
    interaction: Option<&ResponseInteractionAnchor>,
    result: &Value,
) -> Option<TargetId> {
    interaction
        .and_then(|interaction| interaction.target_id)
        .or_else(|| {
            ["/context/target_id", "/target_id"]
                .into_iter()
                .find_map(|pointer| result.pointer(pointer)?.as_str()?.parse().ok())
        })
}

/// Applies terminal-capture health to a mapped response.
///
/// This is the single place where the `capture_failed` warning enters a tool response. It runs
/// on the shared exit every tool must pass through, so a surface cannot report healthy capture
/// while the writer is terminal — including surfaces added later, which get this for free rather
/// than having to opt in.
fn apply_capture_health(
    mapped: &mut MappedResult,
    capture_statuses: &[krometrail_core::TargetCaptureStatus],
) {
    let target_id = response_target_id(
        mapped.response.interaction.as_ref(),
        &mapped.response.result,
    )
    .or_else(|| {
        mapped
            .response
            .error
            .as_ref()
            .and_then(|error| error.context.target_id)
    });
    let mut degraded = false;
    for status in capture_statuses
        .iter()
        .filter(|status| status.state() == krometrail_core::CaptureStreamState::Failed)
        .filter(|status| target_id.is_none_or(|target_id| status.target_id() == target_id))
    {
        let warning = capture_failed_warning(status);
        if mapped.response.warnings.contains(&warning) {
            continue;
        }
        let failure = status
            .failure()
            .expect("failed capture status is validated with a failure");
        tracing::warn!(
            event = "mcp.response.degraded",
            failure_stage = failure.stage().as_str(),
            error_code = warning.code.as_str(),
            "mcp.response.degraded"
        );
        mapped.response.warnings.push(warning);
        degraded = true;
    }
    if degraded && mapped.response.status == ToolResponseStatus::Succeeded {
        mapped.response.status = ToolResponseStatus::Degraded;
        mapped.summary = format!(
            "{} succeeded, but retained temporal evidence is unavailable",
            mapped.response.tool
        );
    }
}

pub(crate) fn into_call_tool_result(
    mut mapped: MappedResult,
    capture_statuses: &[krometrail_core::TargetCaptureStatus],
) -> Result<CallToolResult, rmcp::ErrorData> {
    apply_capture_health(&mut mapped, capture_statuses);
    let mut content = Vec::with_capacity(1 + mapped.images.len() + mapped.response.resources.len());
    content.push(Content::text(mapped.summary));
    for image in mapped.images {
        match image {
            EncodedMcpImage::Screenshot { screenshot, .. } => {
                let mime = image_mime_type(screenshot.bytes()).ok_or_else(|| {
                    rmcp::ErrorData::internal_error(
                        "encoded screenshot format is unsupported",
                        None,
                    )
                })?;
                content.push(Content::image(STANDARD.encode(screenshot.bytes()), mime));
            }
            EncodedMcpImage::Artifact {
                media_type, bytes, ..
            }
            | EncodedMcpImage::SourceFrame {
                media_type, bytes, ..
            } => {
                content.push(Content::image(STANDARD.encode(&bytes), media_type));
            }
        }
    }
    content.extend(mapped.response.resources.iter().map(|resource| {
        let raw = RawResource {
            uri: resource.uri.clone(),
            name: resource.name.clone(),
            title: None,
            description: None,
            mime_type: resource.mime_type.clone(),
            size: resource
                .encoded_byte_len
                .and_then(|length| u32::try_from(length).ok()),
            icons: None,
            meta: None,
        };
        Content::resource_link(raw)
    }));
    let structured = serde_json::to_value(mapped.response).map_err(|_| {
        rmcp::ErrorData::internal_error("tool response could not be serialized", None)
    })?;
    let mut result = if mapped.is_error {
        CallToolResult::error(content)
    } else {
        CallToolResult::success(content)
    };
    // Assign directly: rmcp's structured convenience constructors duplicate JSON into text.
    result.structured_content = Some(structured);
    Ok(result)
}

fn encoded_image_metadata(image: &EncodedMcpImage) -> ResponseImage {
    match image {
        EncodedMcpImage::Screenshot {
            role,
            step_index,
            screenshot,
        } => ResponseImage {
            role: role.clone(),
            step_index: *step_index,
            metadata: ResponseImageMetadata::Screenshot(screenshot.metadata().clone()),
        },
        EncodedMcpImage::Artifact {
            role,
            step_index,
            artifact_id,
            media_type,
            encoded_byte_len,
            width,
            height,
            ..
        } => ResponseImage {
            role: role.clone(),
            step_index: *step_index,
            metadata: ResponseImageMetadata::Artifact {
                artifact_id: *artifact_id,
                media_type: media_type.clone(),
                encoded_byte_len: *encoded_byte_len,
                width: *width,
                height: *height,
            },
        },
        EncodedMcpImage::SourceFrame {
            role,
            step_index,
            frame_id,
            media_type,
            encoded_byte_len,
            width,
            height,
            ..
        } => ResponseImage {
            role: role.clone(),
            step_index: *step_index,
            metadata: ResponseImageMetadata::SourceFrame {
                frame_id: *frame_id,
                media_type: media_type.clone(),
                encoded_byte_len: *encoded_byte_len,
                width: *width,
                height: *height,
            },
        },
    }
}

fn mapped(tool: &str, projection: Projection, summary: String) -> MappedResult {
    let is_error = projection.status == ToolResponseStatus::Failed;
    let response_images = projection
        .images
        .iter()
        .map(encoded_image_metadata)
        .collect();
    MappedResult {
        response: ToolResponse {
            tool: tool.to_owned(),
            status: projection.status,
            result: projection.result,
            range_handle: None,
            interaction: projection.interaction,
            warnings: projection.warnings,
            images: response_images,
            resources: projection.resources,
            error: projection.error,
            diagnostics: None,
        },
        summary,
        images: projection.images,
        is_error,
    }
}

fn project_operation(
    result: BrowserOperationResult,
    response: ResponseRequest,
    novelty: SnapshotNovelty,
) -> Result<Projection, ResponseInvariantError> {
    match result {
        BrowserOperationResult::InspectPage(value) => serializable(*value),
        BrowserOperationResult::SnapshotPage(value) => serializable(*value),
        BrowserOperationResult::QueryPage(value) => serializable(*value),
        BrowserOperationResult::TakeScreenshot(value) => {
            let mut projection = serializable(value.metadata().clone())?;
            projection.degrade_with(value.warnings().to_vec());
            projection.images.push(EncodedMcpImage::Screenshot {
                role: ImageRole::RequestedScreenshot,
                step_index: None,
                screenshot: *value,
            });
            Ok(projection)
        }
        BrowserOperationResult::EvaluatePage(value) => serializable(*value),
        BrowserOperationResult::ObserveLive(value) => {
            let (result, warnings, image) = project_live_observation(
                *value,
                ImageRole::LiveObservation,
                None,
                response,
                SnapshotNovelty::Novel,
            )?;
            let mut projection = Projection::success(result);
            projection.degrade_with(warnings);
            projection.images.extend(image);
            Ok(projection)
        }
        BrowserOperationResult::ListPages(value) => serializable(*value),
        BrowserOperationResult::ListPageContexts(value) => serializable(*value),
        BrowserOperationResult::WaitForPage(value) => serializable(*value),
        BrowserOperationResult::ListFrames(value) => serializable(*value),
        BrowserOperationResult::ListPageAssets(value) => {
            serializable(project_page_assets(*value, response.detail)?)
        }
        BrowserOperationResult::ReadClipboard(value) => serializable(*value),
        BrowserOperationResult::ListDownloads(value)
        | BrowserOperationResult::WaitForDownload(value) => serializable(*value),
        BrowserOperationResult::CancelDownload(value) => serializable(*value),
        BrowserOperationResult::CreatePage(value)
        | BrowserOperationResult::SelectPage(value)
        | BrowserOperationResult::ActivatePage(value)
        | BrowserOperationResult::ClosePage(value)
        | BrowserOperationResult::NavigatePage(value)
        | BrowserOperationResult::ReloadPage(value)
        | BrowserOperationResult::GoBack(value)
        | BrowserOperationResult::GoForward(value) => {
            project_page_operation(*value, response, novelty)
        }
        BrowserOperationResult::SetViewport(value) => {
            let mut projection = project_page_operation(value.operation, response, novelty)?;
            let mut warnings = Vec::new();
            let metrics_fallback = matches!(
                &value.effective,
                ObservationPart::Available(effective) if effective.metrics_fallback
            );
            let effective = project_serializable_part(value.effective, &mut warnings)?;
            let result = projection
                .result
                .as_object_mut()
                .ok_or(ResponseInvariantError)?;
            result.insert("effective".to_owned(), effective);
            result.insert(
                "materialization".to_owned(),
                serde_json::to_value(value.materialization).map_err(|_| ResponseInvariantError)?,
            );
            result.insert(
                "guidance".to_owned(),
                serde_json::to_value(value.guidance).map_err(|_| ResponseInvariantError)?,
            );
            if metrics_fallback {
                projection.degrade_with(vec![metrics_fallback_warning(projection_target_id(
                    &projection,
                ))]);
            }
            projection.degrade_with(warnings);
            Ok(projection)
        }
        BrowserOperationResult::WriteClipboard(value) => {
            let bytes = value.utf8_bytes;
            let mut projection = project_page_operation(value.operation, response, novelty)?;
            projection.result["utf8_bytes"] = serde_json::json!(bytes);
            Ok(projection)
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
            let anchor = value.anchor().map_err(|_| ResponseInvariantError)?;
            let (observation, warnings, image) = project_live_observation(
                value.observation,
                ImageRole::PostAction,
                None,
                response,
                novelty,
            )?;
            // The bounded postcondition block is on-by-default at every
            // detail level; the expanded/full record echo carries the same
            // field because the concise block IS the record field.
            let postcondition = serde_json::to_value(&value.record.postcondition)
                .map_err(|_| ResponseInvariantError)?;
            let mut result = json!({
                "observation": observation,
                "postcondition": postcondition,
            });
            if let Some(note) = value.record.expectation_note {
                result["expectation_note"] = json!(note.message());
            }
            if response.detail != ResponseDetail::Concise {
                result["record"] =
                    serde_json::to_value(value.record).map_err(|_| ResponseInvariantError)?;
            }
            let mut projection = Projection::success(result);
            projection.interaction = Some(anchor.into());
            projection.degrade_with(warnings);
            projection.images.extend(image);
            Ok(projection)
        }
        BrowserOperationResult::Wait(value) => {
            let timed_out = matches!(value.outcome, WaitOutcome::TimedOut { .. });
            let target_id = value.context.target_id;
            let mut projection = serializable(*value)?;
            if timed_out {
                projection.fail_with(krometrail_core::wait_timeout_error(target_id));
            }
            Ok(projection)
        }
        BrowserOperationResult::Batch(value) => project_batch(*value, response, novelty),
    }
}

fn metrics_fallback_warning(target_id: Option<TargetId>) -> KrometrailError {
    let mut warning = KrometrailError::from_browser_failure(
        ErrorCode::PageObservationFailed,
        NonEmptyText::new(
            "layout metrics were invalid; the observed JavaScript viewport size was used",
        )
        .unwrap(),
    )
    .with_retry(RetryAdvice::AfterRecovery)
    .with_recovery(
        NonEmptyText::new(
            "reload or navigate the page, then retry after the browser establishes valid layout metrics",
        )
        .unwrap(),
    );
    if let Some(target_id) = target_id {
        warning.context.target_id = Some(target_id);
    }
    warning
}

fn project_page_operation(
    value: PageOperationResult,
    response: ResponseRequest,
    novelty: SnapshotNovelty,
) -> Result<Projection, ResponseInvariantError> {
    let interaction = value.interaction.clone();
    let (observation, warnings, image) =
        project_live_observation_part(value.observation, ImageRole::PostAction, response, novelty)?;
    let outcome = serde_json::to_value(&value.outcome).map_err(|_| ResponseInvariantError)?;
    let mut projection = Projection::success(json!({
        "interaction": interaction,
        "outcome": outcome,
        "observation": observation,
    }));
    projection.interaction = Some(interaction.into());
    projection.degrade_with(warnings);
    projection.images.extend(image);
    if let PageOperationOutcome::Failed(error) = value.outcome {
        projection.fail_with(error);
    }
    Ok(projection)
}

fn project_batch(
    value: BatchResult,
    response: ResponseRequest,
    novelty: SnapshotNovelty,
) -> Result<Projection, ResponseInvariantError> {
    let mut images = Vec::new();
    let mut screenshot_warnings = Vec::new();
    let mut step_values = Vec::with_capacity(value.steps.len());
    let mut first_step_failure: Option<(u32, BrowserOperationKind, KrometrailError)> = None;
    let mut step_failure_seen = false;
    for step in value.steps {
        let result = step
            .result
            .map(|result| {
                project_batch_step(result, step.operation, response, SnapshotNovelty::Novel)
            })
            .transpose()?
            .flatten();
        if step.status != krometrail_core::BatchStepStatus::Succeeded {
            step_failure_seen = true;
        }
        if first_step_failure.is_none()
            && let Some(error) = step.error.clone()
        {
            first_step_failure = Some((step.index, step.operation, error));
        }
        let screenshot = step.screenshot.map(|screenshot| match screenshot {
            ObservationPart::Available(screenshot) => {
                let metadata = screenshot.metadata().clone();
                screenshot_warnings.extend(screenshot.warnings().iter().cloned());
                images.push(EncodedMcpImage::Screenshot {
                    role: ImageRole::BatchStep,
                    step_index: Some(step.index),
                    screenshot,
                });
                json!({"available": metadata})
            }
            ObservationPart::Unavailable(error) => json!({"unavailable": error}),
        });
        let mut step_value = json!({
            "index": step.index,
            "operation": step.operation,
            "target_id": step.target_id,
            "status": step.status,
            "started_at": step.started_at,
            "completed_at": step.completed_at,
            "interaction": step.interaction,
            "result": result,
            "error": step.error,
            "skip_reason": step.skip_reason,
        });
        if let Some(screenshot) = screenshot {
            step_value["screenshot"] = screenshot;
        }
        step_values.push(step_value);
    }

    let (final_observation, final_warnings, final_image) = project_live_observation_part(
        value.final_observation,
        ImageRole::BatchFinal,
        response,
        novelty,
    )?;
    images.extend(final_image);
    let outcome = value.outcome;
    let mut projection = Projection::success(json!({
        "batch_id": value.batch_id,
        "target_id": value.target_id,
        "started_at": value.started_at,
        "completed_at": value.completed_at,
        "outcome": outcome,
        "steps": step_values,
        "final_observation": final_observation,
    }));
    projection.images = images;
    projection.degrade_with(screenshot_warnings);
    projection.degrade_with(final_warnings);
    match outcome {
        BatchOutcome::Completed => {}
        // The domain uses CompletedWithFailures for both failed steps and incomplete final live
        // evidence. If every step succeeded, preserve the already-applied mutations and expose the
        // missing evidence as degradation instead of encouraging callers to replay the batch.
        BatchOutcome::CompletedWithFailures if !step_failure_seen => {}
        _ => projection.fail_with(first_step_failure.map_or_else(
            || batch_outcome_error(outcome),
            |(index, operation, error)| batch_step_error(index, operation, error),
        )),
    }
    Ok(projection)
}

fn project_batch_step(
    result: BrowserOperationResult,
    operation: BrowserOperationKind,
    response: ResponseRequest,
    novelty: SnapshotNovelty,
) -> Result<Option<Value>, ResponseInvariantError> {
    let mut value = project_operation(result, response, novelty)?.result;
    if let Some(object) = value.as_object_mut() {
        object.remove("observation");
    }
    if value.is_object() {
        project_tool_root(operation.stable_name(), &mut value, response)?;
    }
    Ok(Some(value))
}

fn project_live_observation_part(
    value: ObservationPart<LiveObservation>,
    role: ImageRole,
    response: ResponseRequest,
    novelty: SnapshotNovelty,
) -> Result<(Value, Vec<KrometrailError>, Option<EncodedMcpImage>), ResponseInvariantError> {
    match value {
        ObservationPart::Available(observation) => {
            let (value, warnings, image) =
                project_live_observation(observation, role, None, response, novelty)?;
            Ok((json!({"available": value}), warnings, image))
        }
        ObservationPart::Unavailable(error) => {
            Ok((json!({"unavailable": error}), vec![error], None))
        }
    }
}

fn project_live_observation(
    value: LiveObservation,
    role: ImageRole,
    step_index: Option<u32>,
    response: ResponseRequest,
    novelty: SnapshotNovelty,
) -> Result<(Value, Vec<KrometrailError>, Option<EncodedMcpImage>), ResponseInvariantError> {
    let mut warnings = Vec::new();
    let visual_viewport = match &value.page {
        ObservationPart::Available(page) => Some(page.viewport.visual_viewport),
        ObservationPart::Unavailable(_) => None,
    };
    let semantic_outcomes = match &value.snapshot {
        ObservationPart::Available(snapshot) => {
            semantic_outcomes(snapshot, visual_viewport.as_ref())?
        }
        ObservationPart::Unavailable(_) => Vec::new(),
    };
    let mut page = project_serializable_part(value.page, &mut warnings)?;
    project_page_state_part(&mut page, response.detail)?;
    let mut snapshot = project_serializable_part(value.snapshot, &mut warnings)?;
    project_snapshot_part(
        &mut snapshot,
        response.detail,
        novelty,
        visual_viewport.as_ref(),
    )?;
    let (screenshot, image) = match value.screenshot {
        ObservationPart::Available(screenshot) => {
            warnings.extend(screenshot.warnings().iter().cloned());
            (
                json!({"available": screenshot.metadata()}),
                Some(screenshot),
            )
        }
        ObservationPart::Unavailable(error) => {
            warnings.push(error.clone());
            (json!({"unavailable": error}), None)
        }
    };
    Ok((
        json!({
            "context": value.context,
            "page": page,
            "snapshot": snapshot,
            "screenshot": screenshot,
            "semantic_outcomes": semantic_outcomes,
        }),
        warnings,
        image.map(|screenshot| EncodedMcpImage::Screenshot {
            role,
            step_index,
            screenshot,
        }),
    ))
}

#[derive(Clone, Debug, Serialize)]
struct SemanticOutcome {
    role: String,
    name: Option<String>,
    value: Option<String>,
    description: Option<String>,
}

fn semantic_outcomes(
    snapshot: &PageSnapshot,
    visual_viewport: Option<&CssRect>,
) -> Result<Vec<SemanticOutcome>, ResponseInvariantError> {
    const PRIMARY_ROLES: &[&str] = &["alert", "dialog", "status"];
    const TEXT_ROLES: &[&str] = &["statictext", "static_text", "paragraph", "heading"];

    let has_text = |node: &&krometrail_core::SnapshotNode| {
        node.name
            .as_ref()
            .is_some_and(|text| !text.trim().is_empty())
            || node
                .value
                .as_ref()
                .is_some_and(|text| !text.trim().is_empty())
            || node
                .description
                .as_ref()
                .is_some_and(|text| !text.trim().is_empty())
    };
    let mut primary = snapshot
        .nodes
        .iter()
        .filter(|node| {
            PRIMARY_ROLES
                .iter()
                .any(|role| node.role.eq_ignore_ascii_case(role))
        })
        .filter(has_text)
        .collect::<Vec<_>>();
    let mut text = snapshot
        .nodes
        .iter()
        .filter(|node| {
            TEXT_ROLES
                .iter()
                .any(|role| node.role.eq_ignore_ascii_case(role))
        })
        .filter(has_text)
        .collect::<Vec<_>>();
    if geometry_available(snapshot, visual_viewport) {
        primary.sort_by_key(|node| u8::from(!intersects_viewport(node, visual_viewport)));
        text.sort_by_key(|node| u8::from(!intersects_viewport(node, visual_viewport)));
    }
    let mut outcomes = Vec::new();
    for node in primary.into_iter().chain(text) {
        if outcomes.len() == MAX_SEMANTIC_OUTCOMES {
            break;
        }
        let outcome = SemanticOutcome {
            role: node.role.clone(),
            name: node.name.clone(),
            value: node.value.clone(),
            description: node.description.clone(),
        };
        outcomes.push(outcome);
        if serde_json::to_vec(&outcomes)
            .map_err(|_| ResponseInvariantError)?
            .len()
            > MAX_SEMANTIC_OUTCOME_JSON_BYTES
        {
            outcomes.pop();
            break;
        }
    }
    Ok(outcomes)
}

#[derive(Clone, Debug, Serialize)]
struct ExactTarget {
    reference: krometrail_core::NodeReference,
    role: String,
    name: Option<String>,
    value: Option<String>,
    states: Vec<krometrail_core::AccessibleProperty>,
}

#[derive(Clone, Copy, Debug, Serialize)]
struct TargetOmissions {
    source_nodes: u32,
    presentation_targets: u32,
    geometry_omitted: bool,
}

#[derive(Clone, Copy, Debug, Serialize)]
struct BoundedSnapshotOmissions {
    source_nodes: u32,
    presentation_targets: u32,
    presentation_context_nodes: u32,
    geometry_omitted: bool,
}

#[derive(Clone, Debug, Serialize)]
struct ExactTargetIndex {
    context: krometrail_core::ObservationContext,
    generation: krometrail_core::SnapshotGeneration,
    targets: Vec<ExactTarget>,
    omissions: TargetOmissions,
}

#[derive(Clone, Debug, Serialize)]
struct SemanticContextEntry {
    node_id: krometrail_core::SnapshotNodeId,
    parent_node_id: Option<krometrail_core::SnapshotNodeId>,
    depth: u16,
    role: String,
    name: Option<String>,
    value: Option<String>,
    description: Option<String>,
    states: Vec<krometrail_core::AccessibleProperty>,
}

#[derive(Clone, Debug, Serialize)]
struct BoundedSnapshot {
    context: krometrail_core::ObservationContext,
    generation: krometrail_core::SnapshotGeneration,
    targets: Vec<ExactTarget>,
    semantic_context: Vec<SemanticContextEntry>,
    omissions: BoundedSnapshotOmissions,
}

#[derive(Clone, Debug, Serialize)]
struct CompactResolvedRange {
    session_id: SessionId,
    target_id: TargetId,
    anchor_kind: TemporalRangeAnchorKind,
    requested_range: krometrail_core::SessionRange,
    resolved_range: krometrail_core::SessionRange,
    frame_count: u32,
    interaction_count: u32,
    navigation_count: u32,
    marker_count: u32,
    gap_count: u32,
    retention_warning_count: u32,
    /// The window that governed interaction-kind resolved bounds; absent for
    /// other anchor kinds. `options.implicit_interaction_window` is only the
    /// fallback input.
    #[serde(skip_serializing_if = "Option::is_none")]
    applied_interaction_window: Option<krometrail_core::InteractionWindow>,
    options: krometrail_core::RangeResolutionOptions,
}

#[derive(Clone, Debug, Serialize)]
struct CompactSourceFrameRow {
    frame_id: krometrail_core::FrameId,
    resolved_position: u32,
    session_time: SessionTime,
    media_type: NonEmptyText,
    encoded_byte_len: u64,
    #[serde(skip_serializing_if = "is_zero")]
    warning_count: u32,
}

fn is_zero(value: &u32) -> bool {
    *value == 0
}

fn compact_source_frame_row(
    handle: &SourceFrameHandle,
) -> Result<CompactSourceFrameRow, ResponseInvariantError> {
    Ok(CompactSourceFrameRow {
        frame_id: handle.frame_id,
        resolved_position: handle.resolved_position,
        session_time: handle.provenance.session_time(),
        media_type: handle.media_type.clone(),
        encoded_byte_len: handle.encoded_byte_len,
        warning_count: u32::try_from(handle.provenance.warnings().len())
            .map_err(|_| ResponseInvariantError)?,
    })
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
struct AssetKindCounts {
    script: u32,
    stylesheet: u32,
    image: u32,
    font: u32,
    media: u32,
    fetch: u32,
    xml_http_request: u32,
    other: u32,
}

impl AssetKindCounts {
    fn record(&mut self, kind: PageAssetKind) {
        let count = match kind {
            PageAssetKind::Script => &mut self.script,
            PageAssetKind::Stylesheet => &mut self.stylesheet,
            PageAssetKind::Image => &mut self.image,
            PageAssetKind::Font => &mut self.font,
            PageAssetKind::Media => &mut self.media,
            PageAssetKind::Fetch => &mut self.fetch,
            PageAssetKind::XmlHttpRequest => &mut self.xml_http_request,
            PageAssetKind::Other => &mut self.other,
        };
        *count = count.saturating_add(1);
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
struct AssetOmissions {
    source_assets: u32,
    presentation_assets: u32,
}

#[derive(Clone, Debug, Serialize)]
struct ProjectedAssetInventory {
    target_id: TargetId,
    by_kind: AssetKindCounts,
    assets: Vec<PageAssetMetadata>,
    omissions: AssetOmissions,
}

fn project_page_assets(
    inventory: PageAssetInventory,
    detail: ResponseDetail,
) -> Result<Value, ResponseInvariantError> {
    let mut by_kind = AssetKindCounts::default();
    for asset in &inventory.assets {
        by_kind.record(asset.kind);
    }
    let (max_rows, max_bytes) = match detail {
        ResponseDetail::Concise => (MAX_CONCISE_ASSETS, MAX_CONCISE_ASSET_JSON_BYTES),
        ResponseDetail::Expanded => (MAX_EXPANDED_ASSETS, MAX_EXPANDED_ASSET_JSON_BYTES),
        ResponseDetail::Full => (MAX_FULL_ASSETS, MAX_FULL_ASSET_JSON_BYTES),
    };
    let mut assets = Vec::new();
    let mut bytes = 2usize;
    for asset in inventory.assets.iter().cloned() {
        if assets.len() == max_rows {
            break;
        }
        let entry_bytes = serde_json::to_vec(&asset)
            .map_err(|_| ResponseInvariantError)?
            .len();
        let next = bytes
            .checked_add(usize::from(!assets.is_empty()))
            .and_then(|value| value.checked_add(entry_bytes))
            .ok_or(ResponseInvariantError)?;
        if next > max_bytes {
            continue;
        }
        bytes = next;
        assets.push(asset);
    }
    let presentation_assets = inventory.assets.len().saturating_sub(assets.len());
    serde_json::to_value(ProjectedAssetInventory {
        target_id: inventory.target_id,
        by_kind,
        omissions: AssetOmissions {
            source_assets: inventory.omitted_asset_count,
            presentation_assets: u32::try_from(presentation_assets)
                .map_err(|_| ResponseInvariantError)?,
        },
        assets,
    })
    .map_err(|_| ResponseInvariantError)
}

fn compact_resolved_range(
    range: &krometrail_core::ResolvedRange,
) -> Result<CompactResolvedRange, ResponseInvariantError> {
    let count = |length: usize| u32::try_from(length).map_err(|_| ResponseInvariantError);
    Ok(CompactResolvedRange {
        session_id: range.session_id,
        target_id: range.target_id,
        anchor_kind: range.anchor_kind,
        requested_range: range.requested_range,
        resolved_range: range.resolved_range,
        frame_count: count(range.frame_ids.len())?,
        interaction_count: count(range.interaction_ids.len())?,
        navigation_count: count(range.navigation_ids.len())?,
        marker_count: count(range.marker_ids.len())?,
        gap_count: count(range.gaps.len())?,
        retention_warning_count: count(range.retention_warnings.len())?,
        applied_interaction_window: range.applied_interaction_window,
        options: range.options,
    })
}

/// A bounded head slice of an identifier vector with its exact omission count.
#[derive(Serialize)]
struct BoundedIds<T: Serialize> {
    ids: Vec<T>,
    /// Exact count of identifiers beyond the presented head slice.
    omitted_count: u64,
}

fn bounded_ids<T: Serialize + Clone>(ids: &[T], cap: usize) -> BoundedIds<T> {
    BoundedIds {
        ids: ids.iter().take(cap).cloned().collect(),
        omitted_count: u64::try_from(ids.len().saturating_sub(cap)).unwrap_or(u64::MAX),
    }
}

const fn projected_epoch_cap(detail: ResponseDetail) -> usize {
    match detail {
        ResponseDetail::Concise => MAX_CONCISE_PROJECTED_EPOCHS,
        ResponseDetail::Expanded | ResponseDetail::Full => MAX_PROJECTED_EPOCHS,
    }
}

/// One resolved-range projection for every detail tier; replaces every direct
/// serialization of a complete `ResolvedRange`. Concise keeps the compact
/// counts-only shape; expanded adds the anchor, first/last frame ids, bounded
/// per-kind event-id lists, and a drill-down block; full adds a bounded
/// frame-id head slice with the `list_source_frames` continuation offset. The
/// complete sets stay reachable through the range handle, paginated listings,
/// and re-resolution.
fn bounded_resolved_range(
    range: &krometrail_core::ResolvedRange,
    detail: ResponseDetail,
) -> Result<Value, ResponseInvariantError> {
    let mut value =
        serde_json::to_value(compact_resolved_range(range)?).map_err(|_| ResponseInvariantError)?;
    let event_cap = match detail {
        ResponseDetail::Concise => return Ok(value),
        ResponseDetail::Expanded => MAX_EXPANDED_RANGE_EVENT_IDS,
        ResponseDetail::Full => MAX_FULL_RANGE_EVENT_IDS,
    };
    let object = value.as_object_mut().ok_or(ResponseInvariantError)?;
    object.insert(
        "resolved_anchor".into(),
        serde_json::to_value(&range.resolved_anchor).map_err(|_| ResponseInvariantError)?,
    );
    object.insert(
        "first_frame_id".into(),
        serde_json::to_value(range.frame_ids.first()).map_err(|_| ResponseInvariantError)?,
    );
    object.insert(
        "last_frame_id".into(),
        serde_json::to_value(range.frame_ids.last()).map_err(|_| ResponseInvariantError)?,
    );
    object.insert(
        "interaction_ids".into(),
        serde_json::to_value(bounded_ids(&range.interaction_ids, event_cap))
            .map_err(|_| ResponseInvariantError)?,
    );
    object.insert(
        "navigation_ids".into(),
        serde_json::to_value(bounded_ids(&range.navigation_ids, event_cap))
            .map_err(|_| ResponseInvariantError)?,
    );
    object.insert(
        "marker_ids".into(),
        serde_json::to_value(bounded_ids(&range.marker_ids, event_cap))
            .map_err(|_| ResponseInvariantError)?,
    );
    object.insert(
        "gaps".into(),
        serde_json::to_value(&range.gaps).map_err(|_| ResponseInvariantError)?,
    );
    object.insert(
        "retention_warnings".into(),
        serde_json::to_value(&range.retention_warnings).map_err(|_| ResponseInvariantError)?,
    );
    let mut drill_down = serde_json::Map::new();
    drill_down.insert(
        "complete_frame_ids".into(),
        json!("list_source_frames pages the complete resolved frame order with offset"),
    );
    drill_down.insert(
        "range_reference".into(),
        json!(
            "pass the returned range_handle wherever a temporal request accepts a resolved range"
        ),
    );
    if detail == ResponseDetail::Full {
        let frames = bounded_ids(&range.frame_ids, MAX_FULL_RANGE_FRAME_IDS);
        let next_offset =
            (frames.omitted_count > 0).then(|| u64::try_from(frames.ids.len()).unwrap_or(u64::MAX));
        drill_down.insert(
            "next_offset".into(),
            serde_json::to_value(next_offset).map_err(|_| ResponseInvariantError)?,
        );
        object.insert(
            "frame_ids".into(),
            serde_json::to_value(frames).map_err(|_| ResponseInvariantError)?,
        );
    }
    object.insert("drill_down".into(), Value::Object(drill_down));
    Ok(value)
}

/// Bounds the `epochs` rows already present in a serialized capture-quality
/// object, recording the exact omitted count. No-op on unexpected shapes.
fn bound_capture_quality_epochs(quality: &mut Value, detail: ResponseDetail) {
    let cap = projected_epoch_cap(detail);
    let Some(object) = quality.as_object_mut() else {
        return;
    };
    let Some(Value::Array(epochs)) = object.get_mut("epochs") else {
        return;
    };
    let omitted = epochs.len().saturating_sub(cap);
    epochs.truncate(cap);
    object.insert("omitted_epoch_count".into(), json!(omitted));
}

/// Bounded visual-epoch presentation for artifact generations: per-epoch
/// geometry and exact frame counts without inlining the per-epoch frame-id
/// vectors, plus the exact omitted-epoch count.
fn bounded_visual_epochs_value(
    epochs: &[krometrail_core::VisualEpoch],
    detail: ResponseDetail,
) -> Result<(Value, u64), ResponseInvariantError> {
    let cap = projected_epoch_cap(detail);
    let rows = epochs
        .iter()
        .take(cap)
        .map(|epoch| {
            Ok(json!({
                "index": epoch.index,
                "frame_count": epoch.frame_ids.len(),
                "image": serde_json::to_value(epoch.image).map_err(|_| ResponseInvariantError)?,
                "viewport": serde_json::to_value(epoch.viewport)
                    .map_err(|_| ResponseInvariantError)?,
                "device_scale_factor": serde_json::to_value(epoch.device_scale_factor)
                    .map_err(|_| ResponseInvariantError)?,
            }))
        })
        .collect::<Result<Vec<_>, ResponseInvariantError>>()?;
    let omitted = u64::try_from(epochs.len().saturating_sub(cap)).unwrap_or(u64::MAX);
    Ok((Value::Array(rows), omitted))
}

/// The manifest's disclosed analysis-sampling mode, present only when the
/// manifest carries a validated `analysis_sampling` disclosure.
fn manifest_sampling_mode(manifest: &krometrail_core::ArtifactManifest) -> Option<String> {
    match manifest.parameters().get("analysis_sampling") {
        Some(temporal_vision::ParameterValue::Object(values)) => match values.get("mode") {
            Some(temporal_vision::ParameterValue::Text(mode)) => Some(mode.as_ref().to_owned()),
            _ => None,
        },
        _ => None,
    }
}

/// Inline manifest presentation with capped id vectors and the canonical
/// manifest resource URI; the persisted manifest resource stays complete. The
/// presented manifest object carries `omitted_id_counts` with the exact number
/// of identifiers removed from each bounded array, and `manifest_uri` names
/// the complete provenance authority.
fn bounded_manifest_value(
    scope: krometrail_core::EvidenceScope,
    manifest: &krometrail_core::ArtifactManifest,
) -> Result<Value, ResponseInvariantError> {
    let mut value = serde_json::to_value(manifest).map_err(|_| ResponseInvariantError)?;
    let object = value.as_object_mut().ok_or(ResponseInvariantError)?;
    let mut omitted = serde_json::Map::new();
    for key in [
        "source_frame_ids",
        "analyzed_frame_ids",
        "selected_frame_ids",
    ] {
        let Some(Value::Array(ids)) = object.get_mut(key) else {
            continue;
        };
        let omitted_count = ids.len().saturating_sub(MAX_FULL_MANIFEST_IDS);
        ids.truncate(MAX_FULL_MANIFEST_IDS);
        omitted.insert(key.into(), json!(omitted_count));
    }
    if let Some(Value::Array(indices)) = object
        .get_mut("parameters")
        .and_then(Value::as_object_mut)
        .and_then(|parameters| parameters.get_mut("analysis_sampling"))
        .and_then(Value::as_object_mut)
        .and_then(|sampling| sampling.get_mut("value"))
        .and_then(Value::as_object_mut)
        .and_then(|sampling| sampling.get_mut("analyzed_source_indices"))
        .and_then(Value::as_object_mut)
        .and_then(|indices| indices.get_mut("value"))
    {
        let omitted_count = indices.len().saturating_sub(MAX_FULL_MANIFEST_IDS);
        indices.truncate(MAX_FULL_MANIFEST_IDS);
        omitted.insert("analyzed_source_indices".into(), json!(omitted_count));
    }
    object.insert("omitted_id_counts".into(), Value::Object(omitted));
    object.insert(
        "manifest_uri".into(),
        json!(
            crate::resources::EvidenceResourceUri::artifact_manifest(
                scope,
                *manifest.artifact_id()
            )
            .canonical_uri()
        ),
    );
    Ok(value)
}

/// The per-tier artifact presentation: the compact handle at every tier, with
/// the bounded inline manifest added only at full.
fn projected_artifact_value(
    scope: krometrail_core::EvidenceScope,
    artifact: &ArtifactHandle,
    detail: ResponseDetail,
) -> Result<Value, ResponseInvariantError> {
    let mut value = serde_json::to_value(compact_artifact_handle(scope, artifact)?)
        .map_err(|_| ResponseInvariantError)?;
    if detail == ResponseDetail::Full {
        value["manifest"] = bounded_manifest_value(scope, &artifact.manifest)?;
    }
    Ok(value)
}

/// Bounds the pinned-frame enumeration inside a serialized pin state or pin
/// change, recording the exact omitted count. No-op on unexpected shapes.
fn bound_pin_value(value: &mut Value, detail: ResponseDetail) {
    let cap = if detail == ResponseDetail::Full {
        MAX_FULL_PROJECTED_PIN_FRAME_IDS
    } else {
        MAX_PROJECTED_PIN_FRAME_IDS
    };
    let state = match value.get_mut("state") {
        Some(state) => state,
        None => value,
    };
    if let Some(request) = state.get_mut("request").and_then(Value::as_object_mut) {
        bound_pin_ids(
            request,
            "expected_frame_ids",
            "omitted_expected_frame_id_count",
            cap,
        );
    }
    if let Some(evidence) = state.get_mut("evidence").and_then(Value::as_object_mut) {
        bound_pin_ids(
            evidence,
            "retained_frame_ids",
            "omitted_retained_frame_id_count",
            cap,
        );
        bound_pin_ids(
            evidence,
            "missing_frame_ids",
            "omitted_missing_frame_id_count",
            cap,
        );
    }
}

fn bound_pin_ids(
    object: &mut serde_json::Map<String, Value>,
    key: &str,
    omitted_key: &str,
    cap: usize,
) {
    let Some(Value::Array(ids)) = object.get_mut(key) else {
        return;
    };
    let omitted = ids.len().saturating_sub(cap);
    ids.truncate(cap);
    object.insert(omitted_key.into(), json!(omitted));
}

fn exact_target(
    node: &krometrail_core::SnapshotNode,
    concise: bool,
) -> Result<ExactTarget, ResponseInvariantError> {
    let states = node
        .properties
        .iter()
        .filter(|property| {
            !(concise
                && (property.name == "focusable"
                    || matches!(
                        property.value,
                        krometrail_core::AccessibleValue::Boolean(false)
                    )))
        })
        .cloned()
        .collect();
    Ok(ExactTarget {
        reference: node.reference.ok_or(ResponseInvariantError)?,
        role: node.role.clone(),
        name: node.name.clone(),
        value: node.value.clone(),
        states,
    })
}

/// Every detail tier is bounded. `full` is the widest tier, not an unbounded one.
#[derive(Clone, Copy, Debug)]
struct SnapshotBudget {
    max_targets: usize,
    max_target_json_bytes: usize,
    max_context_nodes: usize,
    max_snapshot_json_bytes: usize,
}

impl SnapshotBudget {
    fn for_detail(detail: ResponseDetail) -> Self {
        match detail {
            ResponseDetail::Concise => Self {
                max_targets: MAX_CONCISE_TARGETS,
                max_target_json_bytes: MAX_CONCISE_TARGET_JSON_BYTES,
                max_context_nodes: 0,
                max_snapshot_json_bytes: MAX_CONCISE_TARGET_JSON_BYTES,
            },
            ResponseDetail::Expanded => Self {
                max_targets: MAX_EXPANDED_TARGETS,
                max_target_json_bytes: MAX_EXPANDED_TARGET_JSON_BYTES,
                max_context_nodes: MAX_EXPANDED_CONTEXT_NODES,
                max_snapshot_json_bytes: MAX_EXPANDED_SNAPSHOT_JSON_BYTES,
            },
            ResponseDetail::Full => Self {
                max_targets: MAX_FULL_TARGETS,
                max_target_json_bytes: MAX_FULL_TARGET_JSON_BYTES,
                max_context_nodes: MAX_FULL_CONTEXT_NODES,
                max_snapshot_json_bytes: MAX_FULL_SNAPSHOT_JSON_BYTES,
            },
        }
    }
}

fn bounded_targets(
    snapshot: &PageSnapshot,
    detail: ResponseDetail,
    visual_viewport: Option<&CssRect>,
) -> Result<Vec<ExactTarget>, ResponseInvariantError> {
    let concise = detail == ResponseDetail::Concise;
    let budget = SnapshotBudget::for_detail(detail);
    let mut actions = snapshot
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| node.actionable)
        .collect::<Vec<_>>();
    if geometry_available(snapshot, visual_viewport) {
        actions.sort_by_key(|(index, node)| {
            (
                u8::from(!intersects_viewport(node, visual_viewport)),
                snapshot_action_rank(node),
                u8::from(!snapshot_is_identifiable(node)),
                *index,
            )
        });
    } else {
        actions.sort_by_key(|(index, node)| {
            (
                snapshot_action_rank(node),
                u8::from(!snapshot_is_identifiable(node)),
                *index,
            )
        });
    }
    let (max_targets, max_bytes) = (budget.max_targets, budget.max_target_json_bytes);
    let mut targets = Vec::new();
    let mut bytes = 2usize;
    for (_, node) in actions {
        if targets.len() == max_targets {
            break;
        }
        let target = exact_target(node, concise)?;
        let entry_bytes = serde_json::to_vec(&target)
            .map_err(|_| ResponseInvariantError)?
            .len();
        let next = bytes
            .checked_add(usize::from(!targets.is_empty()))
            .and_then(|value| value.checked_add(entry_bytes))
            .ok_or(ResponseInvariantError)?;
        if next > max_bytes {
            continue;
        }
        bytes = next;
        targets.push(target);
    }
    Ok(targets)
}

fn geometry_available(snapshot: &PageSnapshot, visual_viewport: Option<&CssRect>) -> bool {
    visual_viewport.is_some()
        && snapshot
            .nodes
            .iter()
            .any(|node| node.document_rect.is_some())
}

fn intersects_viewport(
    node: &krometrail_core::SnapshotNode,
    visual_viewport: Option<&CssRect>,
) -> bool {
    let (Some(node_rect), Some(viewport)) = (node.document_rect, visual_viewport) else {
        return false;
    };
    node_rect.origin.x < viewport.right()
        && node_rect.right() > viewport.origin.x
        && node_rect.origin.y < viewport.bottom()
        && node_rect.bottom() > viewport.origin.y
}

fn concise_snapshot(
    snapshot: &PageSnapshot,
    novelty: SnapshotNovelty,
    visual_viewport: Option<&CssRect>,
) -> Result<Value, ResponseInvariantError> {
    let actionable = snapshot.nodes.iter().filter(|node| node.actionable).count();
    let targets = bounded_targets(snapshot, ResponseDetail::Concise, visual_viewport)?;
    let omissions = TargetOmissions {
        source_nodes: snapshot.omitted_node_count,
        presentation_targets: u32::try_from(actionable - targets.len())
            .map_err(|_| ResponseInvariantError)?,
        geometry_omitted: snapshot.geometry_omitted,
    };
    if novelty == SnapshotNovelty::Unchanged {
        return Ok(json!({
            "generation": snapshot.generation,
            "unchanged": true,
            "target_count": actionable,
            "omissions": omissions,
        }));
    }
    serde_json::to_value(ExactTargetIndex {
        context: snapshot.context.clone(),
        generation: snapshot.generation,
        omissions,
        targets,
    })
    .map_err(|_| ResponseInvariantError)
}

/// Shared bounded projection for the `expanded` and `full` detail tiers. Both emit the same
/// omission accounting; only the ceilings differ, so `full` is wider without being unbounded.
fn bounded_snapshot(
    snapshot: &PageSnapshot,
    detail: ResponseDetail,
    novelty: SnapshotNovelty,
    visual_viewport: Option<&CssRect>,
) -> Result<Value, ResponseInvariantError> {
    let budget = SnapshotBudget::for_detail(detail);
    let targets = bounded_targets(snapshot, detail, visual_viewport)?;
    let actionable = snapshot.nodes.iter().filter(|node| node.actionable).count();
    let context_count = snapshot.nodes.len() - actionable;
    let mut candidates = snapshot
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| !node.actionable)
        .collect::<Vec<_>>();
    candidates.sort_by_key(|(index, node)| (semantic_rank(node), *index));
    let mut semantic_context = Vec::new();
    for (_, node) in candidates {
        if semantic_context.len() == budget.max_context_nodes {
            break;
        }
        let entry = SemanticContextEntry {
            node_id: node.id,
            parent_node_id: node.parent,
            depth: node.depth,
            role: node.role.clone(),
            name: node.name.clone(),
            value: node.value.clone(),
            description: node.description.clone(),
            states: node.properties.clone(),
        };
        semantic_context.push(entry);
        let candidate = BoundedSnapshot {
            context: snapshot.context.clone(),
            generation: snapshot.generation,
            targets: targets.clone(),
            semantic_context: semantic_context.clone(),
            omissions: BoundedSnapshotOmissions {
                source_nodes: snapshot.omitted_node_count,
                presentation_targets: u32::try_from(actionable - targets.len())
                    .map_err(|_| ResponseInvariantError)?,
                presentation_context_nodes: u32::try_from(context_count - semantic_context.len())
                    .map_err(|_| ResponseInvariantError)?,
                geometry_omitted: snapshot.geometry_omitted,
            },
        };
        if serde_json::to_vec(&candidate)
            .map_err(|_| ResponseInvariantError)?
            .len()
            > budget.max_snapshot_json_bytes
        {
            semantic_context.pop();
            continue;
        }
    }
    let omissions = BoundedSnapshotOmissions {
        source_nodes: snapshot.omitted_node_count,
        presentation_targets: u32::try_from(actionable - targets.len())
            .map_err(|_| ResponseInvariantError)?,
        presentation_context_nodes: u32::try_from(context_count - semantic_context.len())
            .map_err(|_| ResponseInvariantError)?,
        geometry_omitted: snapshot.geometry_omitted,
    };
    // `full` always materializes. The unchanged-generation summary answers the question a
    // `concise` or `expanded` caller is asking — give me an economical projection — but a caller
    // that explicitly asked for the widest tier and received a summary has had its request
    // silently reinterpreted, with no way to force materialization. The bound, not the
    // short-circuit, is what makes `full` safe.
    if novelty == SnapshotNovelty::Unchanged && detail != ResponseDetail::Full {
        return Ok(json!({
            "generation": snapshot.generation,
            "unchanged": true,
            "target_count": actionable,
            "omissions": omissions,
        }));
    }
    serde_json::to_value(BoundedSnapshot {
        context: snapshot.context.clone(),
        generation: snapshot.generation,
        omissions,
        targets,
        semantic_context,
    })
    .map_err(|_| ResponseInvariantError)
}

fn snapshot_action_rank(node: &krometrail_core::SnapshotNode) -> u8 {
    if snapshot_boolean_property(node, "focused") {
        0
    } else if snapshot_boolean_property(node, "editable") {
        1
    } else if node.role != "link" {
        2
    } else {
        3
    }
}

fn snapshot_boolean_property(node: &krometrail_core::SnapshotNode, name: &str) -> bool {
    node.properties.iter().any(|property| {
        property.name == name
            && matches!(
                property.value,
                krometrail_core::AccessibleValue::Boolean(true)
            )
    })
}

fn semantic_rank(node: &krometrail_core::SnapshotNode) -> u8 {
    if matches!(
        node.role.as_str(),
        "alert" | "dialog" | "heading" | "status"
    ) {
        0
    } else if snapshot_is_identifiable(node) {
        1
    } else {
        2
    }
}

fn snapshot_is_identifiable(node: &krometrail_core::SnapshotNode) -> bool {
    node.name.is_some() || node.value.is_some() || node.description.is_some()
}

fn project_response(
    tool: &str,
    projection: &mut Projection,
    response: ResponseRequest,
) -> Result<(), ResponseInvariantError> {
    if !response.includes_images_for(tool) {
        projection.images.clear();
    }
    project_tool_root(tool, &mut projection.result, response)?;
    Ok(())
}

fn project_tool_root(
    operation: &str,
    result: &mut Value,
    response: ResponseRequest,
) -> Result<(), ResponseInvariantError> {
    if operation == "snapshot_page" {
        let visual_viewport = result
            .get("visual_viewport")
            .map(|value| {
                serde_json::from_value::<CssRect>(value.clone()).map_err(|_| ResponseInvariantError)
            })
            .transpose()?;
        project_root_snapshot(
            result,
            response.detail,
            SnapshotNovelty::Novel,
            visual_viewport.as_ref(),
        )?;
    } else if operation == "inspect_page" {
        project_root_page_state(result, response.detail)?;
    }
    Ok(())
}

fn project_root_snapshot(
    value: &mut Value,
    detail: ResponseDetail,
    novelty: SnapshotNovelty,
    visual_viewport: Option<&CssRect>,
) -> Result<(), ResponseInvariantError> {
    match detail {
        ResponseDetail::Concise => {
            *value = concise_snapshot(
                &serde_json::from_value(value.clone()).map_err(|_| ResponseInvariantError)?,
                novelty,
                visual_viewport,
            )?
        }
        ResponseDetail::Expanded | ResponseDetail::Full => {
            *value = bounded_snapshot(
                &serde_json::from_value(value.clone()).map_err(|_| ResponseInvariantError)?,
                detail,
                novelty,
                visual_viewport,
            )?
        }
    }
    Ok(())
}

fn project_root_page_state(
    value: &mut Value,
    detail: ResponseDetail,
) -> Result<(), ResponseInvariantError> {
    if detail == ResponseDetail::Concise {
        *value = concise_page_state(value)?;
    }
    Ok(())
}

fn project_page_state_part(
    part: &mut Value,
    detail: ResponseDetail,
) -> Result<(), ResponseInvariantError> {
    if detail == ResponseDetail::Concise
        && let Some(value) = part.get_mut("available")
    {
        *value = concise_page_state(value)?;
    }
    Ok(())
}

fn project_snapshot_part(
    part: &mut Value,
    detail: ResponseDetail,
    novelty: SnapshotNovelty,
    visual_viewport: Option<&CssRect>,
) -> Result<(), ResponseInvariantError> {
    let Some(value) = part.get_mut("available") else {
        return Ok(());
    };
    project_root_snapshot(value, detail, novelty, visual_viewport)
}

fn concise_page_state(value: &Value) -> Result<Value, ResponseInvariantError> {
    let Some(object) = value.as_object() else {
        return Ok(value.clone());
    };
    let retained = [
        "context",
        "session_id",
        "target_id",
        "url",
        "title",
        "selection",
        "viewport",
        "effective",
        "navigation",
        "dialog",
        "observed_at",
        "started_at",
        "completed_at",
    ];
    let mut concise = serde_json::Map::new();
    for key in retained {
        if let Some(value) = object.get(key) {
            concise.insert(key.to_owned(), value.clone());
        }
    }
    if concise.is_empty() {
        Ok(value.clone())
    } else {
        Ok(Value::Object(concise))
    }
}

fn project_temporal_value(
    value: &mut Value,
    detail: ResponseDetail,
) -> Result<(), ResponseInvariantError> {
    if detail == ResponseDetail::Concise {
        *value = compact_temporal_value(value)?;
        return Ok(());
    }
    // Expanded and full temporal-context values are bounded at final
    // presentation: the embedded resolved range keeps exact counts while its
    // identifier enumeration is capped with exact omission accounting.
    let Some(object) = value.as_object_mut() else {
        return Ok(());
    };
    if !(object.contains_key("capture_quality") || object.contains_key("browser_events")) {
        return Ok(());
    }
    if let Some(range_value) = object.get("range") {
        let range: krometrail_core::ResolvedRange =
            serde_json::from_value(range_value.clone()).map_err(|_| ResponseInvariantError)?;
        object.insert("range".into(), bounded_resolved_range(&range, detail)?);
    }
    if let Some(quality) = object.get_mut("capture_quality") {
        bound_capture_quality_epochs(quality, detail);
    }
    Ok(())
}

fn compact_temporal_value(value: &Value) -> Result<Value, ResponseInvariantError> {
    let Some(bundle) = value.as_object() else {
        return Ok(value.clone());
    };
    if bundle.contains_key("capture_quality") || bundle.contains_key("browser_events") {
        return compact_temporal_context_value(value);
    }
    let mut compact = serde_json::Map::new();
    for key in ["range", "header", "warnings", "degradations"] {
        if let Some(value) = bundle.get(key) {
            compact.insert(key.to_owned(), value.clone());
        }
    }
    if let Some(effective) = bundle.get("effective").and_then(Value::as_object) {
        compact.insert(
            "effective".into(),
            json!({
                "artifact_anchor": effective.get("artifact_anchor"),
                "epoch_scope": effective.get("epoch_scope"),
                "focus_times": effective.get("focus_times"),
                "artifact_generator_count": effective
                    .get("artifact_generators")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len),
            }),
        );
    }
    if let Some(markers) = bundle.get("markers").and_then(Value::as_array) {
        compact.insert("marker_count".into(), json!(markers.len()));
    }
    if let Some(artifacts) = bundle.get("artifacts") {
        let mut artifacts = artifacts.clone();
        if let Some(object) = artifacts.as_object_mut() {
            object.remove("range");
        }
        compact.insert("artifacts".into(), artifacts);
    }
    if let Some(context) = compact_temporal_context(bundle.get("context")) {
        compact.insert("context".into(), context);
    }
    Ok(Value::Object(compact))
}

fn compact_temporal_context(context: Option<&Value>) -> Option<Value> {
    let context = context?.as_object()?;
    if context.get("status").and_then(Value::as_str) == Some("unavailable") {
        return Some(Value::Object(context.clone()));
    }
    let capture = context.get("capture_quality").and_then(Value::as_object);
    let events = context.get("browser_events").and_then(Value::as_object);
    Some(json!({
        "status": context.get("status"),
        "capture_quality": capture.map(|capture| {
            let mut quality = json!({
                "requested_range": capture.get("requested_range"),
                "retained_range": capture.get("retained_range"),
                "frame_count": capture.get("frame_count"),
                "cadence": capture.get("cadence"),
                "gap_summary": capture.get("gap_summary"),
                "retention_warnings": capture.get("retention_warnings"),
                "epochs": capture.get("epochs"),
                "warnings": capture.get("warnings"),
            });
            bound_capture_quality_epochs(&mut quality, ResponseDetail::Concise);
            quality
        }),
        "browser_events": events.map(|events| json!({
            "effective_range": events.get("effective_range"),
            "matched_count": events.get("matched_count"),
            "returned_count": events.get("returned_count"),
            "events": events.get("events"),
            "next_cursor": events.get("next_cursor"),
            "collection_gaps": events.get("collection_gaps"),
            "unavailable_ranges": events.get("unavailable_ranges"),
            "warnings": events.get("warnings"),
        })),
    }))
}

fn compact_temporal_context_value(value: &Value) -> Result<Value, ResponseInvariantError> {
    let Some(object) = value.as_object() else {
        return Ok(value.clone());
    };
    let mut wrapped = object.clone();
    wrapped.insert("status".into(), Value::String("available".into()));
    let summary =
        compact_temporal_context(Some(&Value::Object(wrapped))).ok_or(ResponseInvariantError)?;
    let range = serde_json::from_value(object.get("range").cloned().ok_or(ResponseInvariantError)?)
        .map_err(|_| ResponseInvariantError)?;
    Ok(json!({
        "range": compact_resolved_range(&range)?,
        "capture_quality": summary.get("capture_quality"),
        "browser_events": summary.get("browser_events"),
    }))
}

fn project_serializable_part<T: Serialize>(
    value: ObservationPart<T>,
    warnings: &mut Vec<KrometrailError>,
) -> Result<Value, ResponseInvariantError> {
    match value {
        ObservationPart::Available(value) => Ok(json!({
            "available": serde_json::to_value(value).map_err(|_| ResponseInvariantError)?
        })),
        ObservationPart::Unavailable(error) => {
            warnings.push(error.clone());
            Ok(json!({"unavailable": error}))
        }
    }
}

fn serializable<T: Serialize>(value: T) -> Result<Projection, ResponseInvariantError> {
    serde_json::to_value(value)
        .map(Projection::success)
        .map_err(|_| ResponseInvariantError)
}

/// Name the failing step on the batch's own error. The step index, operation, and cause are
/// already in `steps`; a caller reading only the top-level failure should not have to correlate.
fn batch_step_error(
    index: u32,
    operation: BrowserOperationKind,
    error: KrometrailError,
) -> KrometrailError {
    let message = NonEmptyText::new(format!(
        "batch step {index} ({}) failed: {}",
        operation.stable_name(),
        error.message.as_str()
    ))
    .expect("batch step failure message is non-empty");
    KrometrailError { message, ..error }
}

fn batch_outcome_error(outcome: BatchOutcome) -> KrometrailError {
    let (code, message) = match outcome {
        BatchOutcome::Cancelled => (
            ErrorCode::Cancelled,
            "browser operation batch was cancelled",
        ),
        BatchOutcome::TimedOut => (ErrorCode::WaitTimedOut, "browser operation batch timed out"),
        BatchOutcome::CompletedWithFailures | BatchOutcome::StoppedOnFailure => (
            ErrorCode::InteractionFailed,
            "browser operation batch did not complete successfully",
        ),
        BatchOutcome::Completed => (ErrorCode::Internal, "completed batch was mapped as failed"),
    };
    KrometrailError::new(code, NonEmptyText::new(message).unwrap()).with_retry(code.default_retry())
}

pub(crate) async fn map_temporal_bundle_result(
    tool: &str,
    bundle: TemporalDebugBundle,
    progressive: &dyn ProgressiveEvidence,
    deadline: Instant,
    cancellation: Arc<dyn krometrail_core::CancellationSignal>,
    response: ResponseRequest,
) -> Result<MappedResult, ResponseInvariantError> {
    let scope = artifact_scope(&bundle.range)?;
    let result = match response.detail {
        ResponseDetail::Concise => concise_bundle_value(&bundle, scope)?,
        ResponseDetail::Expanded | ResponseDetail::Full => {
            bounded_bundle_value(&bundle, scope, response.detail)?
        }
    };
    let mut projection = Projection::success(result);
    let generation = match &bundle.artifacts {
        krometrail_core::BundleArtifactEvidence::Available(generation) => Some(generation),
        krometrail_core::BundleArtifactEvidence::Unavailable { .. } => None,
    };
    let candidate = generation.and_then(primary_artifact);
    if let Some(generation) = generation {
        match response.detail {
            ResponseDetail::Concise => {
                if let Some((_, _, artifact)) = candidate {
                    add_resource(&mut projection, artifact_resource(scope, artifact)?)?;
                }
            }
            ResponseDetail::Expanded | ResponseDetail::Full => {
                add_artifact_generation_resources(&mut projection, generation, scope, true)?;
            }
        }
    }
    if response.includes_images()
        && let Some((_, _, artifact)) = candidate
    {
        add_inline_artifact(
            &mut projection,
            scope,
            artifact,
            progressive,
            deadline,
            &cancellation,
        )
        .await?;
    }
    project_response(tool, &mut projection, response)?;
    Ok(mapped(tool, projection, format!("{tool} succeeded")))
}

pub(crate) fn map_temporal_range_resolution_result(
    tool: &str,
    resolution: TemporalRangeResolution,
    response: ResponseRequest,
) -> Result<MappedResult, ResponseInvariantError> {
    let range = bounded_resolved_range(&resolution.range, response.detail)?;
    let mut capture_quality =
        serde_json::to_value(&resolution.capture_quality).map_err(|_| ResponseInvariantError)?;
    bound_capture_quality_epochs(&mut capture_quality, response.detail);
    let mut projection = Projection::success(json!({
        "range": range,
        "capture_quality": capture_quality,
        "artifacts": {"status": "not_requested"},
        "browser_events": {"status": "not_requested"},
    }));
    project_response(tool, &mut projection, response)?;
    Ok(mapped(tool, projection, format!("{tool} succeeded")))
}

pub(crate) async fn map_progressive_result(
    tool: &str,
    result: ProgressiveEvidenceResult,
    progressive: &dyn ProgressiveEvidence,
    deadline: Instant,
    cancellation: Arc<dyn krometrail_core::CancellationSignal>,
    response: ResponseRequest,
) -> Result<MappedResult, ResponseInvariantError> {
    let inline_artifact = if response.includes_images() {
        match &result {
            ProgressiveEvidenceResult::GenerateArtifacts(generation) => {
                let scope = artifact_scope(&generation.range)?;
                primary_artifact(generation).map(|(_, _, artifact)| (scope, artifact.clone()))
            }
            ProgressiveEvidenceResult::GenerateRegionFilmstrip(evidence) => {
                let scope = artifact_scope(&evidence.generation.range)?;
                primary_artifact(&evidence.generation)
                    .map(|(_, _, artifact)| (scope, artifact.clone()))
            }
            _ => None,
        }
    } else {
        None
    };
    let mut projection = match result {
        ProgressiveEvidenceResult::RetrieveArtifact(read) => {
            // Resource-only operations never become tools, but retain a safe
            // projection if an adapter invokes this boundary directly.
            let mut projection = Projection::success(json!({ "handle": read.handle }));
            add_resource(
                &mut projection,
                ResourceProjection::from_artifact(
                    read.handle.scope,
                    read.handle.artifact_id,
                    read.handle.media_type.as_str(),
                    read.handle.encoded_byte_len,
                )
                .map_err(|_| ResponseInvariantError)?,
            )?;
            projection
        }
        ProgressiveEvidenceResult::RetrieveSourceFrame(read) => {
            let mut projection = Projection::success(json!({ "handle": read.handle }));
            add_resource(
                &mut projection,
                ResourceProjection::from_source_frame(
                    read.handle.scope,
                    read.handle.frame_id,
                    read.handle.media_type.as_str(),
                    read.handle.encoded_byte_len,
                )
                .map_err(|_| ResponseInvariantError)?,
            )?;
            projection
        }
        ProgressiveEvidenceResult::ListSourceFrames(list) => {
            let local_omitted_frame_count = 0;
            let omitted_frame_count = list
                .omitted_frame_count
                .saturating_add(u64::try_from(local_omitted_frame_count).unwrap_or(u64::MAX));
            let range = bounded_resolved_range(&list.range, response.detail)?;
            let frames = if response.detail == ResponseDetail::Concise {
                serde_json::to_value(
                    list.frames
                        .iter()
                        .map(compact_source_frame_row)
                        .collect::<Result<Vec<_>, _>>()?,
                )
                .map_err(|_| ResponseInvariantError)?
            } else {
                serde_json::to_value(&list.frames).map_err(|_| ResponseInvariantError)?
            };
            let mut projection = Projection::success(json!({
                "range": range,
                "frames": frames,
                "omitted_frame_count": omitted_frame_count,
                "next_offset": list.next_offset,
            }));
            for frame in &list.frames {
                add_source_frame_resource(&mut projection, frame)?;
            }
            projection
        }
        ProgressiveEvidenceResult::FetchSourceFrames(batch) => {
            project_source_frame_batch(*batch, response)?
        }
        ProgressiveEvidenceResult::GenerateArtifacts(generation) => {
            let generation = *generation;
            let scope = artifact_scope(&generation.range)?;
            let mut projection = Projection::success(projected_generation_value(
                &generation,
                scope,
                response.detail,
            )?);
            add_artifact_generation_resources(&mut projection, &generation, scope, false)?;
            projection
        }
        ProgressiveEvidenceResult::GenerateRegionFilmstrip(evidence) => {
            let evidence = *evidence;
            let region = evidence.region;
            let generation = evidence.generation;
            let scope = artifact_scope(&generation.range)?;
            let generation_value = projected_generation_value(&generation, scope, response.detail)?;
            let mut projection = Projection::success(json!({
                "region": region,
                "generation": generation_value,
            }));
            add_artifact_generation_resources(&mut projection, &generation, scope, false)?;
            projection
        }
        ProgressiveEvidenceResult::PinResolvedRange(change)
        | ProgressiveEvidenceResult::UnpinResolvedRange(change) => {
            let mut projection = serializable(*change)?;
            bound_pin_value(&mut projection.result, response.detail);
            projection
        }
        ProgressiveEvidenceResult::QueryPinState(state) => {
            let mut projection = serializable(*state)?;
            bound_pin_value(&mut projection.result, response.detail);
            projection
        }
    };
    if let Some((scope, artifact)) = inline_artifact {
        add_inline_artifact(
            &mut projection,
            scope,
            &artifact,
            progressive,
            deadline,
            &cancellation,
        )
        .await?;
    }
    project_response(tool, &mut projection, response)?;
    Ok(mapped(tool, projection, format!("{tool} succeeded")))
}

fn add_artifact_generation_resources(
    projection: &mut Projection,
    generation: &ArtifactGenerationResult,
    scope: krometrail_core::EvidenceScope,
    include_manifest: bool,
) -> Result<(), ResponseInvariantError> {
    for outcome in &generation.outcomes {
        if let ArtifactOutcome::Available { artifact, .. } = outcome {
            add_resource(projection, artifact_resource(scope, artifact)?)?;
            if include_manifest {
                add_resource(projection, artifact_manifest_resource(scope, artifact)?)?;
            }
        }
    }
    Ok(())
}

fn primary_artifact(generation: &ArtifactGenerationResult) -> Option<(u32, u32, &ArtifactHandle)> {
    generation
        .outcomes
        .iter()
        .filter_map(|outcome| match outcome {
            ArtifactOutcome::Available {
                epoch_index,
                generator_index,
                artifact,
            } => Some((*epoch_index, *generator_index, artifact.as_ref())),
            ArtifactOutcome::Unavailable { .. } => None,
        })
        .min_by_key(|(epoch, generator, artifact)| {
            (
                artifact_kind_rank(artifact.manifest.artifact_kind()),
                *epoch,
                *generator,
                artifact.artifact_id,
            )
        })
}

fn concise_bundle_value(
    bundle: &TemporalDebugBundle,
    scope: krometrail_core::EvidenceScope,
) -> Result<Value, ResponseInvariantError> {
    let context_value =
        serde_json::to_value(&bundle.context).map_err(|_| ResponseInvariantError)?;
    let context = compact_temporal_context(Some(&context_value));
    let (
        artifacts,
        selected_epoch_count,
        available,
        unavailable,
        omitted_outcomes,
        resources,
        omitted_resources,
    ) = match &bundle.artifacts {
        krometrail_core::BundleArtifactEvidence::Available(generation) => {
            let available = generation
                .outcomes
                .iter()
                .filter(|outcome| matches!(outcome, ArtifactOutcome::Available { .. }))
                .count();
            let unavailable = generation.outcomes.len() - available;
            let primary = primary_artifact(generation)
                .map(|(epoch_index, generator_index, artifact)| {
                    Ok(json!({
                        "epoch_index": epoch_index,
                        "generator_index": generator_index,
                        "artifact": compact_artifact_handle(scope, artifact)?,
                    }))
                })
                .transpose()?;
            let presented_outcomes = usize::from(primary.is_some());
            let total_resources = available.saturating_mul(2);
            (
                json!({"status": "available", "primary": primary}),
                generation.epochs.len(),
                available,
                unavailable,
                generation.outcomes.len().saturating_sub(presented_outcomes),
                presented_outcomes,
                total_resources.saturating_sub(presented_outcomes),
            )
        }
        krometrail_core::BundleArtifactEvidence::Unavailable { error } => (
            json!({"status": "unavailable", "error": error}),
            0,
            0,
            0,
            0,
            0,
            0,
        ),
    };
    Ok(json!({
        "range": compact_resolved_range(&bundle.range)?,
        "header": bundle.header,
        "effective": {
            "artifact_anchor": bundle.effective.artifact_anchor,
            "epoch_scope": bundle.effective.epoch_scope,
            "focus_times": bundle.effective.focus_times,
            "artifact_generator_count": bundle.effective.artifact_generators.len(),
        },
        "marker_count": bundle.markers.len(),
        "artifacts": artifacts,
        "artifact_counts": {
            "selected_epochs": selected_epoch_count,
            "available_outcomes": available,
            "unavailable_outcomes": unavailable,
            "omitted_outcomes": omitted_outcomes,
            "published_resources": resources,
            "omitted_resources": omitted_resources,
        },
        "context": context,
        "warnings": bundle.warnings,
        "degradations": bundle.degradations,
        "expand_with": {"detail": "expanded"},
    }))
}

/// The expanded/full bundle presentation: the complete bundle structure with
/// every embedded resolved-range and epoch enumeration bounded, compact
/// artifact handles at expanded, and bounded inline manifests at full.
fn bounded_bundle_value(
    bundle: &TemporalDebugBundle,
    scope: krometrail_core::EvidenceScope,
    detail: ResponseDetail,
) -> Result<Value, ResponseInvariantError> {
    let mut value = serde_json::to_value(bundle).map_err(|_| ResponseInvariantError)?;
    value["range"] = bounded_resolved_range(&bundle.range, detail)?;
    if let krometrail_core::BundleContextEvidence::Available(context) = &bundle.context {
        let context_value = value.get_mut("context").ok_or(ResponseInvariantError)?;
        context_value["range"] = bounded_resolved_range(&context.range, detail)?;
        if let Some(quality) = context_value.get_mut("capture_quality") {
            bound_capture_quality_epochs(quality, detail);
        }
    }
    let krometrail_core::BundleArtifactEvidence::Available(generation) = &bundle.artifacts else {
        return Ok(value);
    };
    let artifacts = value.get_mut("artifacts").ok_or(ResponseInvariantError)?;
    artifacts["range"] = bounded_resolved_range(&generation.range, detail)?;
    let (epochs, omitted_epoch_count) = bounded_visual_epochs_value(&generation.epochs, detail)?;
    artifacts["epochs"] = epochs;
    artifacts["omitted_epoch_count"] = json!(omitted_epoch_count);
    let outcomes = artifacts
        .get_mut("outcomes")
        .and_then(Value::as_array_mut)
        .ok_or(ResponseInvariantError)?;
    if outcomes.len() != generation.outcomes.len() {
        return Err(ResponseInvariantError);
    }
    for (projected, outcome) in outcomes.iter_mut().zip(&generation.outcomes) {
        let ArtifactOutcome::Available { artifact, .. } = outcome else {
            continue;
        };
        let artifact_value = projected
            .get_mut("artifact")
            .ok_or(ResponseInvariantError)?;
        *artifact_value = projected_artifact_value(scope, artifact, detail)?;
    }
    Ok(value)
}

/// The per-tier artifact-generation presentation: bounded range, bounded
/// epoch rows at expanded/full, and per-tier artifact presentations. Concise
/// keeps the counts-only range and compact handles without epoch rows.
fn projected_generation_value(
    generation: &ArtifactGenerationResult,
    scope: krometrail_core::EvidenceScope,
    detail: ResponseDetail,
) -> Result<Value, ResponseInvariantError> {
    let outcomes = generation
        .outcomes
        .iter()
        .map(|outcome| match outcome {
            ArtifactOutcome::Available {
                epoch_index,
                generator_index,
                artifact,
            } => Ok(json!({
                "available": {
                    "epoch_index": epoch_index,
                    "generator_index": generator_index,
                    "artifact": projected_artifact_value(scope, artifact, detail)?,
                }
            })),
            ArtifactOutcome::Unavailable {
                epoch_index,
                generator_index,
                artifact_kind,
                error,
            } => Ok(json!({
                "unavailable": {
                    "epoch_index": epoch_index,
                    "generator_index": generator_index,
                    "artifact_kind": artifact_kind,
                    "error": error,
                }
            })),
        })
        .collect::<Result<Vec<_>, ResponseInvariantError>>()?;
    let mut value = json!({
        "range": bounded_resolved_range(&generation.range, detail)?,
        "outcomes": outcomes,
    });
    if detail != ResponseDetail::Concise {
        let (epochs, omitted_epoch_count) =
            bounded_visual_epochs_value(&generation.epochs, detail)?;
        value["epochs"] = epochs;
        value["omitted_epoch_count"] = json!(omitted_epoch_count);
    }
    Ok(value)
}

fn compact_artifact_handle(
    scope: krometrail_core::EvidenceScope,
    artifact: &ArtifactHandle,
) -> Result<BundleArtifactHandle, ResponseInvariantError> {
    let manifest = &artifact.manifest;
    Ok(BundleArtifactHandle {
        artifact_id: artifact.artifact_id,
        cache: artifact.cache,
        media_type: artifact.media_type.as_str().to_owned(),
        encoded_byte_len: artifact.encoded_byte_len,
        artifact_kind: manifest.artifact_kind(),
        evidence_class: manifest.evidence_class(),
        source_frame_count: u32::try_from(manifest.source_frame_count())
            .map_err(|_| ResponseInvariantError)?,
        analyzed_frame_count: u32::try_from(manifest.analyzed_frame_ids().len())
            .map_err(|_| ResponseInvariantError)?,
        selected_frame_count: u32::try_from(manifest.selected_frame_ids().len())
            .map_err(|_| ResponseInvariantError)?,
        omitted_frame_count: u32::try_from(manifest.omitted_frame_count())
            .map_err(|_| ResponseInvariantError)?,
        sampling_mode: manifest_sampling_mode(manifest),
        output_dimensions: manifest.output_dimensions(),
        output_hash: manifest.output_hash().to_string(),
        manifest_uri: crate::resources::EvidenceResourceUri::artifact_manifest(
            scope,
            artifact.artifact_id,
        )
        .canonical_uri(),
    })
}

fn add_source_frame_resource(
    projection: &mut Projection,
    frame: &SourceFrameHandle,
) -> Result<(), ResponseInvariantError> {
    add_resource(
        projection,
        ResourceProjection::from_source_frame(
            frame.scope,
            frame.frame_id,
            frame.media_type.as_str(),
            frame.encoded_byte_len,
        )
        .map_err(|_| ResponseInvariantError)?,
    )
}

fn project_source_frame_batch(
    batch: SourceFrameBatch,
    response: ResponseRequest,
) -> Result<Projection, ResponseInvariantError> {
    let range = bounded_resolved_range(&batch.range, response.detail)?;
    let mut projection = Projection::success(json!({
        "range": range,
        "frames": batch.frames.iter().map(|frame| &frame.handle).collect::<Vec<_>>(),
    }));
    let inline_image_limit = match response.inline_images {
        None => 1,
        Some(true) => 4,
        Some(false) => 0,
    };
    let mut inline_bytes = 0_u64;
    for (index, frame) in batch.frames.into_iter().enumerate() {
        add_source_frame_resource(&mut projection, &frame.handle)?;
        if inline_image_limit == 0 {
            continue;
        }
        let frame_bytes = frame.encoded_bytes();
        let length = frame_bytes.len() as u64;
        if index >= inline_image_limit {
            if response.inline_images == Some(true) && index == inline_image_limit {
                projection.degrade_with(vec![inline_limit_warning()]);
            }
            continue;
        }
        if length > 4 * 1024 * 1024 || inline_bytes.saturating_add(length) > 16 * 1024 * 1024 {
            projection.degrade_with(vec![inline_limit_warning()]);
            continue;
        }
        inline_bytes += length;
        let dimensions = frame.handle.provenance.image();
        projection.images.push(EncodedMcpImage::SourceFrame {
            role: ImageRole::TemporalSourceFrame,
            step_index: Some(index as u32),
            frame_id: frame.handle.frame_id,
            media_type: frame.handle.media_type.as_str().to_owned(),
            encoded_byte_len: frame.handle.encoded_byte_len,
            width: dimensions.width(),
            height: dimensions.height(),
            bytes: Arc::from(frame_bytes),
        });
    }
    Ok(projection)
}

async fn read_inline_artifact(
    scope: krometrail_core::EvidenceScope,
    artifact_id: ArtifactId,
    progressive: &dyn ProgressiveEvidence,
    deadline: Instant,
    cancellation: &Arc<dyn krometrail_core::CancellationSignal>,
) -> std::result::Result<InlineArtifact, KrometrailError> {
    let request = RetrieveArtifactRequest::new(scope, artifact_id, 8 * 1024 * 1024)
        .map_err(|_| inline_limit_warning())?;
    let result = tokio::select! {
        result = progressive.execute(
            ProgressiveEvidenceRequest::RetrieveArtifact(request),
            ProgressiveEvidenceContext {
                deadline: Some(deadline),
                cancellation: Some(Arc::clone(cancellation)),
                current_reference_geometry: None,
            },
        ) => result,
        () = cancellation.cancelled() => Err(inline_cancelled_error()),
        () = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)) => Err(inline_deadline_error()),
    }?;
    match result {
        ProgressiveEvidenceResult::RetrieveArtifact(read) => {
            let encoded_bytes = Arc::from(read.encoded_bytes());
            Ok(InlineArtifact {
                handle: read.handle,
                encoded_bytes,
            })
        }
        _ => Err(internal_mapping_error()),
    }
}

async fn add_inline_artifact(
    projection: &mut Projection,
    scope: krometrail_core::EvidenceScope,
    artifact: &ArtifactHandle,
    progressive: &dyn ProgressiveEvidence,
    deadline: Instant,
    cancellation: &Arc<dyn krometrail_core::CancellationSignal>,
) -> Result<(), ResponseInvariantError> {
    match read_inline_artifact(
        scope,
        artifact.artifact_id,
        progressive,
        deadline,
        cancellation,
    )
    .await
    {
        Ok(bytes) => {
            if bytes.handle.artifact_id != artifact.artifact_id
                || bytes.handle.scope != scope
                || bytes.encoded_bytes.len() as u64 != artifact.encoded_byte_len
                || bytes.handle.media_type != artifact.media_type
                || bytes.handle.content_sha256.as_bytes()
                    != artifact.manifest.output_hash().as_bytes()
            {
                return Err(ResponseInvariantError);
            }
            let dimensions = artifact.manifest.output_dimensions();
            projection.images.push(EncodedMcpImage::Artifact {
                role: ImageRole::TemporalPrimary,
                step_index: None,
                artifact_id: artifact.artifact_id,
                media_type: artifact.media_type.as_str().to_owned(),
                encoded_byte_len: artifact.encoded_byte_len,
                width: dimensions.width(),
                height: dimensions.height(),
                bytes: bytes.encoded_bytes,
            });
        }
        Err(error) if error.code == ErrorCode::Cancelled => projection.fail_with(error),
        Err(error) => projection.degrade_with(vec![error]),
    }
    Ok(())
}

struct InlineArtifact {
    handle: krometrail_core::ArtifactEvidenceHandle,
    encoded_bytes: Arc<[u8]>,
}

fn inline_limit_warning() -> KrometrailError {
    KrometrailError::new(
        ErrorCode::ResourceLimitExceeded,
        NonEmptyText::new("inline temporal evidence exceeded the MCP presentation limit")
            .expect("static warning is non-empty"),
    )
}

fn inline_cancelled_error() -> KrometrailError {
    KrometrailError::new(
        ErrorCode::Cancelled,
        NonEmptyText::new("MCP request was cancelled").expect("static warning is non-empty"),
    )
}

fn inline_deadline_error() -> KrometrailError {
    KrometrailError::new(
        ErrorCode::Cancelled,
        NonEmptyText::new("MCP request deadline elapsed").expect("static warning is non-empty"),
    )
}

fn internal_mapping_error() -> KrometrailError {
    KrometrailError::new(
        ErrorCode::Internal,
        NonEmptyText::new("temporal evidence response mapping failed")
            .expect("static error is non-empty"),
    )
}

fn artifact_kind_rank(kind: ArtifactKind) -> u8 {
    match kind {
        ArtifactKind::BeforeDuringAfter => 0,
        ArtifactKind::Storyboard => 1,
        ArtifactKind::DifferenceMap => 2,
        _ => 3,
    }
}

fn add_resource(
    projection: &mut Projection,
    resource: ResourceProjection,
) -> Result<(), ResponseInvariantError> {
    let parsed = resource.parsed_uri().map_err(|_| ResponseInvariantError)?;
    let role = match resource.role {
        ResourceKind::Artifact => ResourceRole::Artifact,
        ResourceKind::ArtifactManifest => ResourceRole::ArtifactManifest,
        ResourceKind::SourceFrame => ResourceRole::SourceFrame,
        ResourceKind::Video => ResourceRole::Video,
        ResourceKind::VideoManifest => ResourceRole::VideoManifest,
    };
    projection.resources.push(ResponseResource {
        role,
        uri: resource.uri,
        name: resource.name,
        mime_type: Some(resource.mime_type),
        encoded_byte_len: Some(resource.encoded_byte_len),
    });
    // Reparse above, and retain the exact canonical form in the envelope. The
    // actual ResourceLink is assembled from the same descriptor in the final
    // MCP projection, so no arbitrary URI can enter either surface.
    if parsed.canonical_uri() != projection.resources.last().expect("just pushed").uri {
        return Err(ResponseInvariantError);
    }
    Ok(())
}

fn artifact_scope(
    range: &krometrail_core::ResolvedRange,
) -> Result<krometrail_core::EvidenceScope, ResponseInvariantError> {
    krometrail_core::EvidenceScope::new(range.session_id, range.target_id)
        .map_err(|_| ResponseInvariantError)
}

fn artifact_resource(
    scope: krometrail_core::EvidenceScope,
    handle: &ArtifactHandle,
) -> Result<ResourceProjection, ResponseInvariantError> {
    ResourceProjection::from_artifact(
        scope,
        handle.artifact_id,
        handle.media_type.as_str(),
        handle.encoded_byte_len,
    )
    .map_err(|_| ResponseInvariantError)
}

fn artifact_manifest_resource(
    scope: krometrail_core::EvidenceScope,
    handle: &ArtifactHandle,
) -> Result<ResourceProjection, ResponseInvariantError> {
    let encoded_byte_len = serde_json::to_vec(&handle.manifest)
        .map_err(|_| ResponseInvariantError)?
        .len() as u64;
    ResourceProjection::from_artifact_manifest(scope, handle.artifact_id, encoded_byte_len)
        .map_err(|_| ResponseInvariantError)
}

fn image_mime_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krometrail_core::{
        AccessibleProperty, AccessibleValue, BatchSkipReason, BatchStepResult, BatchStepStatus,
        BrowserOperationKind, CaptureFailureStage, CaptureOrdinal, CaptureStatistics,
        CaptureStreamState, CaptureTimingSummary, CapturedFrame, CssPoint, CssRect, CssSize,
        DeviceScaleFactor, DiskBudgetBytes, ErrorContext, EveryNthFrame, FrameId, ImageFormat,
        InteractionId, InteractionLocator, InteractionOutcome, InteractionRecord,
        InteractionResult, InteractionTiming, LocatorSummary, NodeReference, ObservationContext,
        ObservedTime, PageChange, PageOperationResult, PageSelection, PageSnapshot, PageState,
        PinProtectionScope, PinState, PixelDimensions, PresentationRange, PresentationTime,
        ProgressivePinChange, RangeEvidenceAvailability, RangeResolutionOptions,
        RecordingBudgetState, ResolvedRange, RetentionPinRequest, RetentionRange, RetentionStatus,
        SanitizedParameters, ScreenshotTarget, SessionId, SessionRange, SessionTime, Sha256Digest,
        SnapshotGeneration, SnapshotNode, SnapshotNodeId, SourceFrameRead, StorageUsage,
        TargetCaptureStatus, TargetId, TemporalRangeAnchorKind, TemporalVideoGenerationClip,
        TemporalVideoManifest, VideoArtifactEvidenceHandle, VideoEncodedClip, VideoEncoderIdentity,
        VideoEncodingProfile, VideoOutputGeometry, VideoPresentationPlan, VideoPresentationSegment,
        VideoSegmentSource, VideoTimingBasis, ViewportState, VisualEpoch, WaitCondition, WaitProbe,
        WaitRequest, WaitResult,
    };
    use std::time::Duration;

    struct UnusedProgressive;

    impl ProgressiveEvidence for UnusedProgressive {
        fn execute(
            &self,
            _request: ProgressiveEvidenceRequest,
            _context: ProgressiveEvidenceContext,
        ) -> krometrail_core::PortFuture<'_, krometrail_core::Result<ProgressiveEvidenceResult>>
        {
            panic!("inline artifact reads are not expected in this test")
        }
    }

    fn test_cancellation() -> Arc<dyn krometrail_core::CancellationSignal> {
        Arc::new(crate::registry::McpCancellation::new(
            tokio_util::sync::CancellationToken::new(),
        ))
    }

    fn session_id() -> SessionId {
        "00000000-0000-0000-0000-000000000001".parse().unwrap()
    }
    fn target_id() -> TargetId {
        "00000000-0000-0000-0000-000000000002".parse().unwrap()
    }
    fn interaction_id() -> InteractionId {
        "00000000-0000-0000-0000-000000000003".parse().unwrap()
    }
    fn context() -> ObservationContext {
        ObservationContext::new(
            session_id(),
            target_id(),
            1,
            SessionTime::from_nanos(10),
            SessionTime::from_nanos(20),
        )
        .unwrap()
    }
    fn error(code: ErrorCode, message: &str) -> KrometrailError {
        KrometrailError::new(code, NonEmptyText::new(message).unwrap())
    }

    fn generated_sampled_difference_generation() -> ArtifactGenerationResult {
        generated_sampled_difference_generation_with(4, vec![0, 2, 3])
    }

    fn generated_sampled_difference_generation_with(
        source_frame_count: u32,
        analyzed_indices: Vec<usize>,
    ) -> ArtifactGenerationResult {
        use temporal_vision::{
            DifferenceMapLimits, DifferenceMapParameters, Frame, FrameSequence, FrequencyMode,
            IntegerScale, MeasurementParameters, NormalizationParameters, PixelFormat,
            ProcessingLimits, Rgb8, TimePalette, TimeRange, Timestamp, normalize_sequence,
            render_difference_map,
        };

        let dimensions = temporal_vision::PixelDimensions::new(2, 2).unwrap();
        let source_frames = (0..source_frame_count)
            .map(|index| {
                Frame::new(
                    FrameId::from_uuid(uuid::Uuid::from_u128(100 + u128::from(index))),
                    Timestamp::from_nanos(index as u64),
                    dimensions,
                    PixelFormat::Rgba8SrgbStraight,
                    vec![index as u8; 16].into_boxed_slice(),
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        let source_frame_ids = source_frames
            .iter()
            .map(|frame| *frame.id())
            .collect::<Vec<_>>();
        let source_range = TimeRange::new(
            Timestamp::from_nanos(0),
            Timestamp::from_nanos(u64::from(source_frame_count.saturating_sub(1))),
        )
        .unwrap();
        let analyzed: temporal_vision::FrameSequence<
            FrameId,
            krometrail_core::ArtifactMarkerId,
            krometrail_core::GapId,
            Box<[u8]>,
        > = FrameSequence::new(
            analyzed_indices
                .iter()
                .map(|index| source_frames[*index].clone())
                .collect(),
            vec![],
            vec![],
            None,
            None,
        )
        .unwrap()
        .with_source_provenance(source_frame_ids.clone(), analyzed_indices, source_range)
        .unwrap();
        let normalized = normalize_sequence(
            &analyzed,
            NormalizationParameters::new(
                Rgb8::new(0, 0, 0),
                None,
                IntegerScale::IDENTITY,
                ProcessingLimits::default(),
            ),
        )
        .unwrap();
        let generated = render_difference_map(
            ArtifactId::from_uuid(uuid::Uuid::from_u128(200)),
            &analyzed,
            &normalized,
            DifferenceMapParameters::new(
                0,
                FrequencyMode::NormalizedFrequency,
                TimePalette::Spectral,
                None,
                MeasurementParameters::new(0),
                Rgb8::new(0, 0, 0),
                DifferenceMapLimits::default(),
            ),
        )
        .unwrap();
        let artifact = ArtifactHandle {
            artifact_id: *generated.manifest().artifact_id(),
            cache: ArtifactCacheDisposition::Generated,
            media_type: NonEmptyText::new("image/png").unwrap(),
            encoded_byte_len: generated.image().bytes().len() as u64,
            manifest: generated.manifest().clone(),
        };
        let range = SessionRange::new(
            SessionTime::from_nanos(0),
            SessionTime::from_nanos(u64::from(source_frame_count.saturating_sub(1))),
        )
        .unwrap();
        let resolved = ResolvedRange::new(
            session_id(),
            target_id(),
            TemporalRangeAnchorKind::SessionTime,
            range,
            range,
            source_frame_ids,
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            RangeResolutionOptions::DEFAULT,
        )
        .unwrap();
        ArtifactGenerationResult {
            range: resolved,
            epochs: vec![],
            outcomes: vec![ArtifactOutcome::Available {
                epoch_index: 0,
                generator_index: 0,
                artifact: Box::new(artifact),
            }],
        }
    }

    #[tokio::test]
    async fn sampled_analysis_success_carries_structured_accounting_without_degradation() {
        // A by-design bounded (uniform-sampled) analysis is a successful
        // response with structured sampling accounting at every tier — never a
        // `resource_limit_exceeded` degradation warning. Exhaustive over-limit
        // requests keep their hard failure in the artifact service.
        for detail in [
            ResponseDetail::Concise,
            ResponseDetail::Expanded,
            ResponseDetail::Full,
        ] {
            let mapped = map_progressive_result(
                "generate_artifacts",
                ProgressiveEvidenceResult::GenerateArtifacts(Box::new(
                    generated_sampled_difference_generation(),
                )),
                &UnusedProgressive,
                Instant::now() + Duration::from_secs(1),
                test_cancellation(),
                ResponseRequest {
                    detail,
                    inline_images: Some(false),
                },
            )
            .await
            .unwrap();
            assert_eq!(mapped.response.status, ToolResponseStatus::Succeeded);
            assert!(
                mapped
                    .response
                    .warnings
                    .iter()
                    .all(|warning| warning.code != ErrorCode::ResourceLimitExceeded),
                "sampling accounting must not be misreported as a resource limit"
            );
            let artifact = &mapped.response.result["outcomes"][0]["available"]["artifact"];
            assert_eq!(artifact["sampling_mode"], "uniform_bounded");
            assert_eq!(artifact["source_frame_count"], 4);
            assert_eq!(artifact["analyzed_frame_count"], 3);
            if detail == ResponseDetail::Full {
                let manifest = &artifact["manifest"];
                assert!(manifest.get("omitted_id_counts").is_some());
                assert!(manifest["manifest_uri"].is_string());
            } else {
                assert!(artifact.get("manifest").is_none());
            }
        }
    }

    #[test]
    fn bounded_manifest_presentation_caps_ids_with_exact_omission_accounting() {
        let generation = generated_sampled_difference_generation();
        let ArtifactOutcome::Available { artifact, .. } = &generation.outcomes[0] else {
            unreachable!()
        };
        let scope = artifact_scope(&generation.range).unwrap();
        let value = bounded_manifest_value(scope, &artifact.manifest).unwrap();
        // Small fixture: nothing omitted, and every omission exactly counted.
        assert_eq!(value["omitted_id_counts"]["source_frame_ids"], 0);
        assert_eq!(value["omitted_id_counts"]["analyzed_frame_ids"], 0);
        assert_eq!(value["omitted_id_counts"]["selected_frame_ids"], 0);
        assert_eq!(
            value["source_frame_ids"],
            serde_json::to_value(artifact.manifest.source_frame_ids()).unwrap()
        );
        assert!(value["manifest_uri"].is_string());
    }

    #[test]
    fn bounded_manifest_presentation_caps_tagged_sampling_indices() {
        let generation =
            generated_sampled_difference_generation_with(600, (0..600).step_by(2).collect());
        let ArtifactOutcome::Available { artifact, .. } = &generation.outcomes[0] else {
            unreachable!()
        };
        let scope = artifact_scope(&generation.range).unwrap();
        let value = bounded_manifest_value(scope, &artifact.manifest).unwrap();
        let indices =
            &value["parameters"]["analysis_sampling"]["value"]["analyzed_source_indices"]["value"];
        assert_eq!(indices.as_array().unwrap().len(), MAX_FULL_MANIFEST_IDS);
        assert_eq!(
            value["omitted_id_counts"]["analyzed_source_indices"],
            300 - MAX_FULL_MANIFEST_IDS as u64
        );
    }

    fn pin_retention(pinned_usage_bytes: u64) -> RetentionStatus {
        RetentionStatus::new(
            DiskBudgetBytes::new(10_000).unwrap(),
            StorageUsage::new(500, 10, 0, 0, 0, 0, 0).unwrap(),
            pinned_usage_bytes,
            None,
            None,
            RecordingBudgetState::Available,
            false,
            false,
            0,
            0,
            0,
        )
        .unwrap()
    }

    fn large_pin_state() -> (PinState, ProgressivePinChange) {
        let expected = (0..600)
            .map(|index| FrameId::from_uuid(uuid::Uuid::from_u128(10_000 + index)))
            .collect::<Vec<_>>();
        let request = RetentionPinRequest::new(
            RetentionRange {
                session_id: session_id(),
                target_id: target_id(),
                range: SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(600)).unwrap(),
            },
            expected.clone(),
        )
        .unwrap();
        let retained = expected[..300].to_vec();
        let missing = expected[300..].to_vec();
        let state = PinState::new(
            request,
            false,
            RangeEvidenceAvailability::PartiallyUnavailable {
                retained_frame_ids: retained,
                missing_frame_ids: missing,
            },
            PinProtectionScope::SourceSegmentsOnly,
            vec![],
            vec![],
            0,
            pin_retention(0),
        )
        .unwrap();
        let change = ProgressivePinChange {
            changed: true,
            state: state.clone(),
        };
        (state, change)
    }

    #[tokio::test]
    async fn pin_state_and_change_bound_all_frame_id_vectors_per_tier() {
        for detail in [
            ResponseDetail::Concise,
            ResponseDetail::Expanded,
            ResponseDetail::Full,
        ] {
            let (state, change) = large_pin_state();
            let cap = if detail == ResponseDetail::Full {
                MAX_FULL_PROJECTED_PIN_FRAME_IDS
            } else {
                MAX_PROJECTED_PIN_FRAME_IDS
            };
            for result in [
                ProgressiveEvidenceResult::QueryPinState(Box::new(state)),
                ProgressiveEvidenceResult::PinResolvedRange(Box::new(change)),
            ] {
                let mapped = map_progressive_result(
                    "query_pin_state",
                    result,
                    &UnusedProgressive,
                    Instant::now() + Duration::from_secs(1),
                    test_cancellation(),
                    ResponseRequest {
                        detail,
                        inline_images: Some(false),
                    },
                )
                .await
                .unwrap();
                let value = &mapped.response.result;
                let state = value.get("state").unwrap_or(value);
                let request = &state["request"];
                let evidence = &state["evidence"];
                assert_eq!(request["expected_frame_ids"].as_array().unwrap().len(), cap);
                assert_eq!(request["omitted_expected_frame_id_count"], 600 - cap as u64);
                assert_eq!(
                    evidence["retained_frame_ids"].as_array().unwrap().len(),
                    cap
                );
                assert_eq!(evidence["missing_frame_ids"].as_array().unwrap().len(), cap);
                assert_eq!(
                    evidence["omitted_retained_frame_id_count"],
                    300 - cap.min(300) as u64
                );
                assert_eq!(
                    evidence["omitted_missing_frame_id_count"],
                    300 - cap.min(300) as u64
                );
            }
        }
    }

    fn video_result() -> TemporalVideoGenerationResult {
        let first = FrameId::from_uuid(uuid::Uuid::from_u128(30));
        let second = FrameId::from_uuid(uuid::Uuid::from_u128(31));
        let range = SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(10)).unwrap();
        let resolved = ResolvedRange::new(
            session_id(),
            target_id(),
            TemporalRangeAnchorKind::SessionTime,
            range,
            range,
            vec![first, second],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            RangeResolutionOptions::DEFAULT,
        )
        .unwrap();
        video_result_with_range(resolved)
    }

    fn video_result_with_range(resolved: ResolvedRange) -> TemporalVideoGenerationResult {
        let first = FrameId::from_uuid(uuid::Uuid::from_u128(30));
        let second = FrameId::from_uuid(uuid::Uuid::from_u128(31));
        let range = resolved.resolved_range;
        let dimensions = PixelDimensions::new(4, 4).unwrap();
        let geometry = VideoOutputGeometry::new(dimensions, dimensions, dimensions).unwrap();
        let clips = [(0_u32, first, 50_u128), (1, second, 51)]
            .into_iter()
            .map(|(epoch_index, frame_id, artifact_value)| {
                let frame_time = SessionTime::from_nanos(2 + u64::from(epoch_index) * 2);
                let plan = VideoPresentationPlan::new(
                    VideoPresentationPolicy::RealTime,
                    range,
                    range,
                    SessionRange::new(frame_time, frame_time).unwrap(),
                    VisualEpoch {
                        index: epoch_index,
                        frame_ids: vec![frame_id],
                        image: dimensions,
                        viewport: dimensions,
                        device_scale_factor: DeviceScaleFactor::new(1.0).unwrap(),
                    },
                    vec![frame_id],
                    vec![frame_time],
                    vec![],
                    vec![
                        VideoPresentationSegment::new(
                            0,
                            VideoSegmentSource::source_frame(frame_id, frame_time).unwrap(),
                            PresentationRange::new(
                                PresentationTime::ZERO,
                                PresentationTime::from_nanos(250_000_000).unwrap(),
                            )
                            .unwrap(),
                            VideoTimingBasis::TerminalHold,
                        )
                        .unwrap(),
                    ],
                    geometry,
                )
                .unwrap();
                let bytes: Arc<[u8]> = Arc::from(
                    format!("fixture-mp4-{epoch_index}")
                        .into_bytes()
                        .into_boxed_slice(),
                );
                let digest = Sha256Digest::digest(&bytes);
                let encoded = VideoEncodedClip::new(
                    VideoEncoderIdentity::new(
                        "fixture-encoder-1",
                        [epoch_index as u8 + 1; 32],
                        "libx264",
                        "adapter-v1",
                        "args-v1",
                    )
                    .unwrap(),
                    VideoEncodingProfile::new(geometry, 1024).unwrap(),
                    temporal_vision::OutputHash::from_bytes(*digest.as_bytes()),
                    Arc::clone(&bytes),
                )
                .unwrap();
                let manifest = TemporalVideoManifest::new(
                    ArtifactId::from_uuid(uuid::Uuid::from_u128(artifact_value)),
                    &resolved,
                    plan,
                    None,
                    &encoded,
                )
                .unwrap();
                let artifact = VideoArtifactEvidenceHandle::new(
                    manifest.artifact_id(),
                    krometrail_core::EvidenceScope::from_range(&resolved).unwrap(),
                    NonEmptyText::new("video/mp4").unwrap(),
                    Sha256Digest::from_bytes(*manifest.output_hash().as_bytes()),
                    manifest.encoded_byte_len(),
                    manifest,
                )
                .unwrap();
                TemporalVideoGenerationClip {
                    epoch_index,
                    cache: ArtifactCacheDisposition::Generated,
                    artifact,
                }
            })
            .collect();
        TemporalVideoGenerationResult {
            range: resolved,
            clips,
        }
    }

    #[test]
    fn temporal_video_result_is_compact_ordered_and_links_each_clip_twice() {
        let mapped = map_temporal_video_result("generate_temporal_video", video_result()).unwrap();
        assert_eq!(mapped.response.status, ToolResponseStatus::Succeeded);
        assert_eq!(mapped.response.result["clips"].as_array().unwrap().len(), 2);
        assert_eq!(mapped.response.result["clips"][0]["epoch_index"], 0);
        assert_eq!(mapped.response.result["clips"][1]["epoch_index"], 1);
        assert_eq!(mapped.response.resources.len(), 4);
        assert_eq!(mapped.response.resources[0].role, ResourceRole::Video);
        assert_eq!(
            mapped.response.resources[1].role,
            ResourceRole::VideoManifest
        );
        let compact = serde_json::to_string(&mapped.response.result).unwrap();
        for forbidden in ["provenance", "encoded_bytes", "provider", "upload"] {
            assert!(!compact.contains(forbidden), "leaked {forbidden}");
        }
        let result = into_call_tool_result(mapped, &[]).unwrap();
        let links = result
            .content
            .iter()
            .filter(|content| serde_json::to_value(content).unwrap()["type"] == "resource_link")
            .count();
        assert_eq!(links, 4);
    }

    #[test]
    fn temporal_video_result_bounds_a_large_range_projection() {
        let mapped = map_temporal_video_result(
            "generate_temporal_video",
            video_result_with_range(large_synthetic_range(1_000)),
        )
        .unwrap();
        let range = &mapped.response.result["range"];
        assert_eq!(range["frame_count"], 1_000);
        assert!(range.get("frame_ids").is_none());
        assert_eq!(
            range["first_frame_id"],
            serde_json::to_value(FrameId::from_uuid(uuid::Uuid::from_u128(1))).unwrap()
        );
        assert_eq!(
            range["last_frame_id"],
            serde_json::to_value(FrameId::from_uuid(uuid::Uuid::from_u128(1_000))).unwrap()
        );
    }
    fn screenshot(format: ImageFormat) -> EncodedScreenshot {
        let metadata = ScreenshotMetadata::new(
            context(),
            ScreenshotTarget::Viewport,
            CssRect::new(
                CssPoint::new(0.0, 0.0).unwrap(),
                CssSize::new(10.0, 10.0).unwrap(),
            )
            .unwrap(),
            PixelDimensions::new(10, 10).unwrap(),
            DeviceScaleFactor::new(1.0).unwrap(),
        )
        .unwrap();
        let bytes = match format {
            ImageFormat::Png => b"\x89PNG\r\n\x1a\npayload".to_vec(),
            ImageFormat::Jpeg => vec![0xff, 0xd8, 0xff, 0xe0, 1],
        };
        EncodedScreenshot::new(metadata, bytes).unwrap()
    }

    fn complex_snapshot() -> PageSnapshot {
        let generation = SnapshotGeneration::new(1).unwrap();
        let root_id = SnapshotNodeId::new(1).unwrap();
        let mut nodes = vec![SnapshotNode {
            id: root_id,
            parent: None,
            depth: 0,
            role: "document".into(),
            name: Some("Synthetic 403-node page".into()),
            value: None,
            description: None,
            properties: vec![],
            actionable: false,
            reference: None,
            document_rect: None,
        }];
        for value in 2..=400 {
            nodes.push(SnapshotNode {
                id: SnapshotNodeId::new(value).unwrap(),
                parent: Some(root_id),
                depth: 1,
                role: "static_text".into(),
                name: Some(format!("section-{value}-{}", "x".repeat(512))),
                value: None,
                description: None,
                properties: vec![],
                actionable: false,
                reference: None,
                document_rect: None,
            });
        }
        let group_id = SnapshotNodeId::new(401).unwrap();
        nodes.push(SnapshotNode {
            id: group_id,
            parent: Some(root_id),
            depth: 1,
            role: "group".into(),
            name: Some("Late controls".into()),
            value: None,
            description: None,
            properties: vec![],
            actionable: false,
            reference: None,
            document_rect: None,
        });
        let action_id = SnapshotNodeId::new(402).unwrap();
        nodes.push(SnapshotNode {
            id: action_id,
            parent: Some(group_id),
            depth: 2,
            role: "button".into(),
            name: Some("Publish".into()),
            value: None,
            description: None,
            properties: vec![],
            actionable: true,
            reference: Some(NodeReference {
                target_id: target_id(),
                generation,
                node_id: action_id,
            }),
            document_rect: None,
        });
        nodes.push(SnapshotNode {
            id: SnapshotNodeId::new(403).unwrap(),
            parent: Some(root_id),
            depth: 1,
            role: "static_text".into(),
            name: Some("Trailing content".into()),
            value: None,
            description: None,
            properties: vec![],
            actionable: false,
            reference: None,
            document_rect: None,
        });
        PageSnapshot::new(context(), generation, nodes, 7).unwrap()
    }

    /// An ordinary encyclopedia article — 2215 source nodes, 160 actionable targets — produced a
    /// 933 KB `snapshot` at `detail: full` and exceeded an agent's entire context in one call.
    /// `full` is the widest bounded tier, not an unbounded one, so this pins a hard ceiling and
    /// checks that everything dropped is accounted for exactly.
    fn encyclopedia_scale_snapshot() -> PageSnapshot {
        let generation = SnapshotGeneration::new(1).unwrap();
        let root_id = SnapshotNodeId::new(1).unwrap();
        let mut nodes = vec![SnapshotNode {
            id: root_id,
            parent: None,
            depth: 0,
            role: "document".into(),
            name: Some("Temporal logic".into()),
            value: None,
            description: None,
            properties: vec![],
            actionable: false,
            reference: None,
            document_rect: None,
        }];
        for value in 2..=2215 {
            let id = SnapshotNodeId::new(value).unwrap();
            let actionable = value % 14 == 0;
            nodes.push(SnapshotNode {
                id,
                parent: Some(root_id),
                depth: 1,
                role: if actionable { "link" } else { "static_text" }.into(),
                name: Some(format!("node-{value}-{}", "temporal ".repeat(24))),
                value: None,
                description: Some("x".repeat(160)),
                properties: vec![
                    AccessibleProperty::new("focusable", AccessibleValue::Boolean(true)).unwrap(),
                ],
                actionable,
                reference: actionable.then_some(NodeReference {
                    target_id: target_id(),
                    generation,
                    node_id: id,
                }),
                document_rect: None,
            });
        }
        PageSnapshot::new(context(), generation, nodes, 4744).unwrap()
    }

    #[test]
    fn full_snapshot_of_a_large_page_stays_bounded_with_exact_omission_accounting() {
        let snapshot = encyclopedia_scale_snapshot();
        let actionable = snapshot.nodes.iter().filter(|node| node.actionable).count();
        let context_nodes = snapshot.nodes.len() - actionable;

        let full = bounded_snapshot(
            &snapshot,
            ResponseDetail::Full,
            SnapshotNovelty::Novel,
            None,
        )
        .unwrap();
        let encoded = serde_json::to_vec(&full).unwrap();
        assert!(
            encoded.len() <= MAX_FULL_SNAPSHOT_JSON_BYTES,
            "full snapshot projected {} bytes, above the {MAX_FULL_SNAPSHOT_JSON_BYTES} ceiling",
            encoded.len()
        );

        let targets = full["targets"].as_array().unwrap().len();
        let context = full["semantic_context"].as_array().unwrap().len();
        assert!(targets <= MAX_FULL_TARGETS);
        assert!(context <= MAX_FULL_CONTEXT_NODES);
        assert_eq!(
            full["omissions"]["presentation_targets"],
            u64::try_from(actionable - targets).unwrap()
        );
        assert_eq!(
            full["omissions"]["presentation_context_nodes"],
            u64::try_from(context_nodes - context).unwrap()
        );
        // Nodes the acquisition layer never handed us stay distinct from nodes this projection
        // chose to leave out.
        assert_eq!(full["omissions"]["source_nodes"], 4744);
        assert!(full.get("nodes").is_none());

        // `full` must still be strictly richer than `expanded`, or the tier would be pointless.
        let expanded = bounded_snapshot(
            &snapshot,
            ResponseDetail::Expanded,
            SnapshotNovelty::Novel,
            None,
        )
        .unwrap();
        assert!(targets > expanded["targets"].as_array().unwrap().len());
        assert!(context > expanded["semantic_context"].as_array().unwrap().len());
        assert!(encoded.len() > serde_json::to_vec(&expanded).unwrap().len());

        // The economical tiers may answer an unchanged generation with a summary. `full` may not:
        // a caller that asked for the widest tier must not have that request reinterpreted, and
        // has no other way to force materialization.
        let unchanged_full = bounded_snapshot(
            &snapshot,
            ResponseDetail::Full,
            SnapshotNovelty::Unchanged,
            None,
        )
        .unwrap();
        assert!(unchanged_full.get("unchanged").is_none());
        assert_eq!(unchanged_full, full);
        let unchanged_expanded = bounded_snapshot(
            &snapshot,
            ResponseDetail::Expanded,
            SnapshotNovelty::Unchanged,
            None,
        )
        .unwrap();
        assert_eq!(unchanged_expanded["unchanged"], true);
    }

    #[test]
    fn semantic_outcomes_prioritize_current_status_and_stay_bounded() {
        let generation = SnapshotGeneration::new(1).unwrap();
        let mut nodes = Vec::new();
        for index in 1..=12 {
            nodes.push(SnapshotNode {
                id: SnapshotNodeId::new(index).unwrap(),
                parent: None,
                depth: 0,
                role: if index == 12 { "status" } else { "paragraph" }.into(),
                name: Some(format!("outcome {index}")),
                value: None,
                description: None,
                properties: vec![],
                actionable: false,
                reference: None,
                document_rect: None,
            });
        }
        let snapshot = PageSnapshot::new(context(), generation, nodes, 0).unwrap();
        let outcomes = semantic_outcomes(&snapshot, None).unwrap();
        assert_eq!(outcomes[0].role, "status");
        assert!(outcomes.len() <= MAX_SEMANTIC_OUTCOMES);
        assert!(serde_json::to_vec(&outcomes).unwrap().len() <= MAX_SEMANTIC_OUTCOME_JSON_BYTES);
    }

    fn live_with_snapshot(snapshot: PageSnapshot) -> LiveObservation {
        let unavailable = error(ErrorCode::PageObservationFailed, "component unavailable");
        LiveObservation {
            context: context(),
            page: ObservationPart::Unavailable(unavailable.clone()),
            snapshot: ObservationPart::Available(snapshot),
            screenshot: ObservationPart::Unavailable(unavailable),
        }
    }

    fn page_state() -> PageState {
        let rect = CssRect::new(
            CssPoint::new(0.0, 0.0).unwrap(),
            CssSize::new(1280.0, 720.0).unwrap(),
        )
        .unwrap();
        PageState::new(
            context(),
            "https://example.test",
            "Projection fixture",
            ViewportState::new(
                rect,
                rect,
                CssSize::new(1280.0, 2400.0).unwrap(),
                DeviceScaleFactor::new(1.0).unwrap(),
                1.0,
            )
            .unwrap(),
            krometrail_core::NavigationState::new(
                0,
                1,
                krometrail_core::DocumentReadiness::Complete,
            )
            .unwrap(),
        )
        .unwrap()
    }

    fn source_frame_batch(frame_count: u32) -> SourceFrameBatch {
        source_frame_batch_for_range(video_result().range, frame_count)
    }

    fn source_frame_batch_for_range(range: ResolvedRange, frame_count: u32) -> SourceFrameBatch {
        let scope = krometrail_core::EvidenceScope::from_range(&range).unwrap();
        let frames = (0..frame_count)
            .map(|index| {
                let bytes: Arc<[u8]> = Arc::from(b"\x89PNG\r\n\x1a\nfixture".as_slice());
                let frame_id = FrameId::from_uuid(uuid::Uuid::from_u128(100 + u128::from(index)));
                let provenance = CapturedFrame::new(
                    frame_id,
                    session_id(),
                    target_id(),
                    CaptureOrdinal::new(u64::from(index) + 1).unwrap(),
                    None,
                    ObservedTime::from_nanos(u64::from(index) + 1),
                    SessionTime::from_nanos(u64::from(index) + 1),
                    ImageFormat::Png,
                    PixelDimensions::new(1, 1).unwrap(),
                    PixelDimensions::new(1, 1).unwrap(),
                    DeviceScaleFactor::new(1.0).unwrap(),
                    Vec::new(),
                )
                .unwrap();
                let handle = SourceFrameHandle::new(
                    frame_id,
                    scope,
                    index,
                    index,
                    NonEmptyText::new("image/png").unwrap(),
                    Sha256Digest::digest(&bytes),
                    bytes.len() as u64,
                    provenance,
                )
                .unwrap();
                SourceFrameRead::new(handle, bytes).unwrap()
            })
            .collect();
        SourceFrameBatch { range, frames }
    }

    #[tokio::test]
    async fn fetch_source_frame_range_is_bounded_at_every_detail_tier() {
        let batch = source_frame_batch_for_range(large_synthetic_range(1_000), 5);
        for detail in [
            ResponseDetail::Concise,
            ResponseDetail::Expanded,
            ResponseDetail::Full,
        ] {
            let mapped = map_progressive_result(
                "fetch_source_frames",
                ProgressiveEvidenceResult::FetchSourceFrames(Box::new(batch.clone())),
                &UnusedProgressive,
                Instant::now() + Duration::from_secs(1),
                test_cancellation(),
                ResponseRequest {
                    detail,
                    inline_images: Some(false),
                },
            )
            .await
            .unwrap();
            let range = &mapped.response.result["range"];
            assert_eq!(range["frame_count"], 1_000);
            if detail == ResponseDetail::Full {
                assert_eq!(
                    range["frame_ids"]["ids"].as_array().unwrap().len(),
                    MAX_FULL_RANGE_FRAME_IDS
                );
                assert_eq!(
                    range["frame_ids"]["omitted_count"],
                    1_000 - MAX_FULL_RANGE_FRAME_IDS as u64
                );
            } else {
                assert!(range.get("frame_ids").is_none());
            }
        }
    }

    fn source_frame_list(frame_count: u32) -> krometrail_core::SourceFrameList {
        let batch = source_frame_batch(frame_count);
        krometrail_core::SourceFrameList {
            range: batch.range,
            frames: batch.frames.into_iter().map(|frame| frame.handle).collect(),
            omitted_frame_count: 0,
            next_offset: None,
        }
    }

    fn failed_capture_for(target_id: TargetId) -> TargetCaptureStatus {
        let cause = KrometrailError::new(
            ErrorCode::PersistenceFailed,
            NonEmptyText::new("frame persistence failed").unwrap(),
        )
        .with_persistence(krometrail_core::PersistenceFailure::new(
            krometrail_core::PersistenceOperation::SealedSegmentPublicationSync,
            krometrail_core::PersistenceFailureCategory::PermissionDenied,
            krometrail_core::PersistenceRecoverability::WriterUsable,
        ));
        TargetCaptureStatus::new(
            target_id,
            1,
            CaptureStreamState::Failed,
            CaptureStatistics::default(),
            1,
            0,
            None,
            CaptureTimingSummary::empty(),
            CaptureTimingSummary::empty(),
            EveryNthFrame::default(),
            Some(CaptureFailure::new(CaptureFailureStage::FramePersistence, cause).unwrap()),
        )
        .unwrap()
    }

    fn failed_capture() -> TargetCaptureStatus {
        failed_capture_for(target_id())
    }

    #[test]
    fn degraded_stop_is_a_success_with_typed_warning_and_recovery() {
        let capture_failure = failed_capture().failure().expect("fixture failure").clone();
        let outcome = BrowserStopOutcome::new(
            krometrail_core::BrowserClosure::ManagedBrowserClosed,
            ShutdownQuality::Degraded,
            Some(krometrail_core::ShutdownFailurePhase::CaptureStopDrainFlush),
            Some(capture_failure),
            Some(
                NonEmptyText::new("start a new browser session before relying on temporal history")
                    .unwrap(),
            ),
        )
        .unwrap();
        let mapped = map_lifecycle_result("stop_browser", outcome).unwrap();
        assert_eq!(mapped.response.status, ToolResponseStatus::Degraded);
        assert!(!mapped.is_error);
        assert_eq!(mapped.response.result["closure"], "managed_browser_closed");
        assert_eq!(mapped.response.result["quality"], "degraded");
        assert_eq!(
            mapped.response.warnings[0]
                .persistence
                .as_ref()
                .unwrap()
                .operation(),
            krometrail_core::PersistenceOperation::SealedSegmentPublicationSync
        );
        assert!(
            mapped.response.warnings[0]
                .recovery
                .as_ref()
                .unwrap()
                .as_str()
                .contains("start a new browser session")
        );
    }

    #[test]
    fn failed_capture_degrades_success_without_removing_current_image() {
        let mapped = map_operation_result_with_capture(
            "take_screenshot",
            BrowserOperationResult::TakeScreenshot(Box::new(screenshot(ImageFormat::Png))),
            &[failed_capture(), failed_capture()],
            ResponseRequest {
                inline_images: Some(true),
                ..ResponseRequest::default()
            },
        )
        .unwrap();
        assert_eq!(mapped.response.status, ToolResponseStatus::Degraded);
        assert_eq!(mapped.response.images.len(), 1);
        assert_eq!(mapped.response.warnings.len(), 1);
        assert_eq!(mapped.response.warnings[0].code, ErrorCode::CaptureFailed);
        assert_eq!(
            mapped.response.warnings[0].context.target_id,
            Some(target_id())
        );
        assert_eq!(
            mapped.response.warnings[0]
                .persistence
                .as_ref()
                .unwrap()
                .recoverability(),
            krometrail_core::PersistenceRecoverability::WriterUsable
        );
        assert!(
            mapped.response.warnings[0]
                .recovery
                .as_ref()
                .unwrap()
                .as_str()
                .contains("start a new browser session")
        );
        assert!(mapped.response.error.is_none());
    }

    #[test]
    fn tall_screenshot_guidance_is_projected_as_one_warning() {
        let mapped = map_operation_result_with_capture(
            "take_screenshot",
            BrowserOperationResult::TakeScreenshot(Box::new(
                screenshot(ImageFormat::Png).with_warning(error(
                    ErrorCode::ResourceLimitExceeded,
                    "captured screenshot height: 8193 exceeds limit 8192, try ≤ 8192",
                )),
            )),
            &[],
            ResponseRequest {
                inline_images: Some(true),
                ..ResponseRequest::default()
            },
        )
        .unwrap();
        assert_eq!(mapped.response.status, ToolResponseStatus::Degraded);
        assert_eq!(mapped.response.warnings.len(), 1);
        assert_eq!(
            mapped.response.warnings[0].code,
            ErrorCode::ResourceLimitExceeded
        );
        assert_eq!(mapped.response.images.len(), 1);
    }

    #[test]
    fn failed_capture_on_another_target_does_not_degrade_page_result() {
        let other = TargetId::from_uuid(uuid::Uuid::from_u128(99));
        let mapped = map_operation_result_with_capture(
            "take_screenshot",
            BrowserOperationResult::TakeScreenshot(Box::new(screenshot(ImageFormat::Png))),
            &[failed_capture_for(other)],
            ResponseRequest::default(),
        )
        .unwrap();
        assert_eq!(mapped.response.status, ToolResponseStatus::Succeeded);
        assert!(mapped.response.warnings.is_empty());
    }
    #[test]
    fn response_request_defaults_to_concise_and_rejects_removed_fields() {
        let (remaining, response) = split_response_request(
            serde_json::json!({
                "target_id": target_id(),
                "response": {"detail": "expanded", "inline_images": true}
            })
            .as_object()
            .unwrap()
            .clone(),
        )
        .unwrap();
        assert!(remaining.contains_key("target_id"));
        assert_eq!(response.detail, ResponseDetail::Expanded);
        assert_eq!(response.inline_images, Some(true));

        let (_, defaulted) = split_response_request(JsonObject::new()).unwrap();
        assert_eq!(defaulted, ResponseRequest::default());

        let error = split_response_request(
            json!({"response": {"extra": true}})
                .as_object()
                .unwrap()
                .clone(),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidInput);
        assert!(!error.message.as_str().contains("true"));

        let error = split_response_request(
            json!({"response": {"inline_images": "yes"}})
                .as_object()
                .unwrap()
                .clone(),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidInput);
        assert!(error.message.as_str().contains("response.inline_images"));
        assert!(error.message.as_str().contains("invalid type"));
    }

    #[test]
    fn concise_snapshot_is_flat_bounded_and_action_ranked() {
        let generation = SnapshotGeneration::new(1).unwrap();
        let root_id = SnapshotNodeId::new(1).unwrap();
        let mut nodes = vec![SnapshotNode {
            id: root_id,
            parent: None,
            depth: 0,
            role: "document".into(),
            name: Some("news".into()),
            value: None,
            description: None,
            properties: vec![],
            actionable: false,
            reference: None,
            document_rect: None,
        }];
        for value in 2..=80 {
            let id = SnapshotNodeId::new(value).unwrap();
            nodes.push(SnapshotNode {
                id,
                parent: Some(root_id),
                depth: 1,
                role: "link".into(),
                name: Some(format!("early link {value}")),
                value: None,
                description: None,
                properties: vec![],
                actionable: true,
                reference: Some(NodeReference {
                    target_id: target_id(),
                    generation,
                    node_id: id,
                }),
                document_rect: None,
            });
        }
        let focused_id = SnapshotNodeId::new(81).unwrap();
        nodes.push(SnapshotNode {
            id: focused_id,
            parent: Some(root_id),
            depth: 1,
            role: "textbox".into(),
            name: Some("late focused editor".into()),
            value: Some("draft".into()),
            description: None,
            properties: vec![
                AccessibleProperty::new("focused", AccessibleValue::Boolean(true)).unwrap(),
                AccessibleProperty::new("editable", AccessibleValue::Boolean(true)).unwrap(),
                AccessibleProperty::new("focusable", AccessibleValue::Boolean(true)).unwrap(),
                AccessibleProperty::new("disabled", AccessibleValue::Boolean(false)).unwrap(),
            ],
            actionable: true,
            reference: Some(NodeReference {
                target_id: target_id(),
                generation,
                node_id: focused_id,
            }),
            document_rect: None,
        });
        let snapshot = PageSnapshot::new(context(), generation, nodes, 9).unwrap();
        let concise = concise_snapshot(&snapshot, SnapshotNovelty::Novel, None).unwrap();
        assert!(concise["targets"].as_array().unwrap().len() <= MAX_CONCISE_TARGETS);
        assert!(
            serde_json::to_vec(&concise["targets"]).unwrap().len() <= MAX_CONCISE_TARGET_JSON_BYTES
        );
        assert_eq!(
            concise["targets"][0]["reference"]["node_id"],
            serde_json::to_value(focused_id).unwrap()
        );
        assert_eq!(concise["omissions"]["source_nodes"], 9);
        assert_eq!(concise["omissions"]["presentation_targets"], 56);
        assert!(
            !concise["targets"][0]["states"]
                .as_array()
                .unwrap()
                .iter()
                .any(|state| state["name"] == "focusable")
        );
        assert!(
            !concise["targets"][0]["states"]
                .as_array()
                .unwrap()
                .iter()
                .any(|state| state["name"] == "disabled")
        );
        let expanded = bounded_snapshot(
            &snapshot,
            ResponseDetail::Expanded,
            SnapshotNovelty::Novel,
            None,
        )
        .unwrap();
        assert!(
            expanded["targets"][0]["states"]
                .as_array()
                .unwrap()
                .iter()
                .any(|state| state["name"] == "disabled")
        );
    }

    #[test]
    fn concise_target_ranking_prefers_identifiable_controls_without_filtering_anonymous_nodes() {
        let generation = SnapshotGeneration::new(1).unwrap();
        let root_id = SnapshotNodeId::new(1).unwrap();
        let mut nodes = vec![SnapshotNode {
            id: root_id,
            parent: None,
            depth: 0,
            role: "document".into(),
            name: Some("controls".into()),
            value: None,
            description: None,
            properties: vec![],
            actionable: false,
            reference: None,
            document_rect: None,
        }];
        for value in 2..=61 {
            let id = SnapshotNodeId::new(value).unwrap();
            nodes.push(SnapshotNode {
                id,
                parent: Some(root_id),
                depth: 1,
                role: "button".into(),
                name: (value <= 13).then(|| format!("button {value}")),
                value: None,
                description: None,
                properties: vec![],
                actionable: true,
                reference: Some(NodeReference {
                    target_id: target_id(),
                    generation,
                    node_id: id,
                }),
                document_rect: None,
            });
        }
        let snapshot = PageSnapshot::new(context(), generation, nodes, 0).unwrap();
        let concise = concise_snapshot(&snapshot, SnapshotNovelty::Novel, None).unwrap();
        let targets = concise["targets"].as_array().unwrap();
        assert_eq!(targets.len(), MAX_CONCISE_TARGETS);
        assert_eq!(
            targets
                .iter()
                .filter(|target| target["name"].is_string())
                .count(),
            12
        );
        assert!(targets.iter().all(|target| target["reference"].is_object()));

        let focused_id = SnapshotNodeId::new(62).unwrap();
        let named_link_id = SnapshotNodeId::new(63).unwrap();
        let focused = SnapshotNode {
            id: focused_id,
            parent: Some(root_id),
            depth: 1,
            role: "textbox".into(),
            name: None,
            value: None,
            description: None,
            properties: vec![
                AccessibleProperty::new("focused", AccessibleValue::Boolean(true)).unwrap(),
            ],
            actionable: true,
            reference: Some(NodeReference {
                target_id: target_id(),
                generation,
                node_id: focused_id,
            }),
            document_rect: None,
        };
        let named_link = SnapshotNode {
            id: named_link_id,
            parent: Some(root_id),
            depth: 1,
            role: "link".into(),
            name: Some("named link".into()),
            value: None,
            description: None,
            properties: vec![],
            actionable: true,
            reference: Some(NodeReference {
                target_id: target_id(),
                generation,
                node_id: named_link_id,
            }),
            document_rect: None,
        };
        let snapshot = PageSnapshot::new(
            context(),
            generation,
            vec![snapshot.nodes[0].clone(), focused, named_link],
            0,
        )
        .unwrap();
        let targets = bounded_targets(&snapshot, ResponseDetail::Concise, None).unwrap();
        assert_eq!(targets[0].reference.node_id, focused_id);
        assert_eq!(targets[1].reference.node_id, named_link_id);
    }

    #[test]
    fn viewport_geometry_prioritizes_intersecting_targets_and_text() {
        let generation = SnapshotGeneration::new(1).unwrap();
        let root = SnapshotNodeId::new(1).unwrap();
        let outside = SnapshotNodeId::new(2).unwrap();
        let inside = SnapshotNodeId::new(3).unwrap();
        let outside_text = SnapshotNodeId::new(4).unwrap();
        let inside_text = SnapshotNodeId::new(5).unwrap();
        let reference = |node_id| NodeReference {
            target_id: target_id(),
            generation,
            node_id,
        };
        let rect = |x, y, width, height| {
            CssRect::new(
                krometrail_core::CssPoint::new(x, y).unwrap(),
                krometrail_core::CssSize::new(width, height).unwrap(),
            )
            .unwrap()
        };
        let nodes = vec![
            SnapshotNode {
                id: root,
                parent: None,
                depth: 0,
                role: "document".into(),
                name: None,
                value: None,
                description: None,
                properties: vec![],
                actionable: false,
                reference: None,
                document_rect: None,
            },
            SnapshotNode {
                id: outside,
                parent: Some(root),
                depth: 1,
                role: "button".into(),
                name: Some("outside".into()),
                value: None,
                description: None,
                properties: vec![],
                actionable: true,
                reference: Some(reference(outside)),
                document_rect: Some(rect(0.0, 200.0, 20.0, 20.0)),
            },
            SnapshotNode {
                id: inside,
                parent: Some(root),
                depth: 1,
                role: "button".into(),
                name: Some("inside".into()),
                value: None,
                description: None,
                properties: vec![],
                actionable: true,
                reference: Some(reference(inside)),
                document_rect: Some(rect(0.0, 10.0, 20.0, 20.0)),
            },
            SnapshotNode {
                id: outside_text,
                parent: Some(root),
                depth: 1,
                role: "paragraph".into(),
                name: Some("outside text".into()),
                value: None,
                description: None,
                properties: vec![],
                actionable: false,
                reference: None,
                document_rect: Some(rect(0.0, 200.0, 20.0, 20.0)),
            },
            SnapshotNode {
                id: inside_text,
                parent: Some(root),
                depth: 1,
                role: "paragraph".into(),
                name: Some("inside text".into()),
                value: None,
                description: None,
                properties: vec![],
                actionable: false,
                reference: None,
                document_rect: Some(rect(0.0, 10.0, 20.0, 20.0)),
            },
        ];
        let snapshot = PageSnapshot::new(context(), generation, nodes, 0).unwrap();
        let viewport = rect(0.0, 0.0, 100.0, 100.0);
        let targets = bounded_targets(&snapshot, ResponseDetail::Concise, Some(&viewport)).unwrap();
        assert_eq!(targets[0].reference.node_id, inside);
        let outcomes = semantic_outcomes(&snapshot, Some(&viewport)).unwrap();
        assert_eq!(outcomes[0].name.as_deref(), Some("inside text"));

        let anchored = snapshot.clone().with_visual_viewport(viewport);
        let mapped = map_operation_result_with_capture(
            "snapshot_page",
            BrowserOperationResult::SnapshotPage(Box::new(anchored)),
            &[],
            ResponseRequest {
                inline_images: Some(false),
                ..ResponseRequest::default()
            },
        )
        .unwrap();
        assert_eq!(mapped.response.status, ToolResponseStatus::Succeeded);
        assert_eq!(
            mapped.response.result["targets"][0]["reference"]["node_id"],
            serde_json::to_value(inside).unwrap()
        );
    }

    #[test]
    fn geometryless_snapshot_projection_keeps_legacy_concise_and_expanded_json() {
        let generation = SnapshotGeneration::new(1).unwrap();
        let root = SnapshotNodeId::new(1).unwrap();
        let action = SnapshotNodeId::new(2).unwrap();
        let reference = NodeReference {
            target_id: target_id(),
            generation,
            node_id: action,
        };
        let snapshot = PageSnapshot::new(
            context(),
            generation,
            vec![
                SnapshotNode {
                    id: root,
                    parent: None,
                    depth: 0,
                    role: "document".into(),
                    name: Some("Legacy page".into()),
                    value: None,
                    description: None,
                    properties: vec![],
                    actionable: false,
                    reference: None,
                    document_rect: None,
                },
                SnapshotNode {
                    id: action,
                    parent: Some(root),
                    depth: 1,
                    role: "button".into(),
                    name: Some("Save".into()),
                    value: None,
                    description: None,
                    properties: vec![],
                    actionable: true,
                    reference: Some(reference),
                    document_rect: None,
                },
            ],
            0,
        )
        .unwrap();

        let concise = concise_snapshot(&snapshot, SnapshotNovelty::Novel, None).unwrap();
        let expected_concise = json!({
            "context": context(),
            "generation": 1,
            "targets": [{
                "reference": reference,
                "role": "button",
                "name": "Save",
                "value": null,
                "states": []
            }],
            "omissions": {
                "source_nodes": 0,
                "presentation_targets": 0,
                "geometry_omitted": false
            }
        });
        assert_eq!(
            serde_json::to_vec(&concise).unwrap(),
            serde_json::to_vec(&expected_concise).unwrap()
        );

        let expanded = bounded_snapshot(
            &snapshot,
            ResponseDetail::Expanded,
            SnapshotNovelty::Novel,
            None,
        )
        .unwrap();
        let expected_expanded = json!({
            "context": context(),
            "generation": 1,
            "targets": [{
                "reference": reference,
                "role": "button",
                "name": "Save",
                "value": null,
                "states": []
            }],
            "semantic_context": [{
                "node_id": root,
                "parent_node_id": null,
                "depth": 0,
                "role": "document",
                "name": "Legacy page",
                "value": null,
                "description": null,
                "states": []
            }],
            "omissions": {
                "source_nodes": 0,
                "presentation_targets": 0,
                "presentation_context_nodes": 0,
                "geometry_omitted": false
            }
        });
        assert_eq!(
            serde_json::to_vec(&expanded).unwrap(),
            serde_json::to_vec(&expected_expanded).unwrap()
        );
    }

    #[test]
    fn page_asset_detail_is_aggregated_and_progressively_bounded() {
        let inventory = || PageAssetInventory {
            target_id: target_id(),
            assets: (0..100)
                .map(|index| PageAssetMetadata {
                    url: krometrail_core::SanitizedUrl::sanitize(&format!(
                        "https://example.test/asset-{index}.js"
                    ))
                    .unwrap(),
                    kind: match index % 4 {
                        0 => PageAssetKind::Script,
                        1 => PageAssetKind::Stylesheet,
                        2 => PageAssetKind::Image,
                        _ => PageAssetKind::Fetch,
                    },
                    duration_ms: index as f64,
                    transfer_bytes: Some(100),
                    encoded_body_bytes: Some(90),
                    decoded_body_bytes: Some(120),
                })
                .collect(),
            omitted_asset_count: 7,
        };
        let project = |detail| {
            project_operation(
                BrowserOperationResult::ListPageAssets(Box::new(inventory())),
                ResponseRequest {
                    detail,
                    inline_images: None,
                },
                SnapshotNovelty::Novel,
            )
            .unwrap()
            .result
        };
        let concise = project(ResponseDetail::Concise);
        let expanded = project(ResponseDetail::Expanded);
        let full = project(ResponseDetail::Full);

        assert_eq!(concise["by_kind"]["script"], 25);
        assert_eq!(concise["by_kind"]["stylesheet"], 25);
        assert_eq!(concise["by_kind"]["image"], 25);
        assert_eq!(concise["by_kind"]["fetch"], 25);
        assert_eq!(concise["omissions"]["source_assets"], 7);
        let concise_rows = concise["assets"].as_array().unwrap().len();
        let expanded_rows = expanded["assets"].as_array().unwrap().len();
        assert!(concise_rows <= MAX_CONCISE_ASSETS);
        assert!(expanded_rows <= MAX_EXPANDED_ASSETS);
        assert!(expanded_rows > concise_rows);
        assert!(
            serde_json::to_vec(&concise["assets"]).unwrap().len() <= MAX_CONCISE_ASSET_JSON_BYTES
        );
        assert_eq!(
            concise["omissions"]["presentation_assets"],
            u32::try_from(100 - concise_rows).unwrap()
        );
        assert_eq!(
            expanded["omissions"]["presentation_assets"],
            u32::try_from(100 - expanded_rows).unwrap()
        );
        // `full` is the widest bounded tier, not an unbounded dump: it still aggregates by kind
        // and still accounts for what it left out.
        let full_rows = full["assets"].as_array().unwrap().len();
        assert!(full_rows <= MAX_FULL_ASSETS);
        assert!(full_rows >= expanded_rows);
        assert!(serde_json::to_vec(&full["assets"]).unwrap().len() <= MAX_FULL_ASSET_JSON_BYTES);
        assert_eq!(full["omissions"]["source_assets"], 7);
        assert_eq!(
            full["omissions"]["presentation_assets"],
            u32::try_from(100 - full_rows).unwrap()
        );
        assert_eq!(full["by_kind"]["script"], 25);
    }

    #[test]
    fn expanded_snapshot_complete_json_stays_within_its_budget() {
        let mut snapshot = complex_snapshot();
        snapshot
            .nodes
            .iter_mut()
            .find(|node| node.actionable)
            .unwrap()
            .properties
            .push(AccessibleProperty::new("focusable", AccessibleValue::Boolean(true)).unwrap());
        let expanded = bounded_snapshot(
            &snapshot,
            ResponseDetail::Expanded,
            SnapshotNovelty::Novel,
            None,
        )
        .unwrap();
        let encoded = serde_json::to_vec(&expanded).unwrap();
        assert!(encoded.len() <= MAX_EXPANDED_SNAPSHOT_JSON_BYTES);
        assert!(
            expanded["omissions"]["presentation_context_nodes"]
                .as_u64()
                .unwrap()
                > 0
        );
        assert!(
            expanded["targets"][0]["states"]
                .as_array()
                .unwrap()
                .iter()
                .any(|state| state["name"] == "focusable")
        );
    }

    #[test]
    fn automatic_live_projection_dedupes_unchanged_generation() {
        let snapshot = complex_snapshot();
        let (first, _, _) = project_live_observation(
            live_with_snapshot(snapshot.clone()),
            ImageRole::PostAction,
            None,
            ResponseRequest::default(),
            SnapshotNovelty::Novel,
        )
        .unwrap();
        let (second, _, _) = project_live_observation(
            live_with_snapshot(snapshot),
            ImageRole::PostAction,
            None,
            ResponseRequest::default(),
            SnapshotNovelty::Unchanged,
        )
        .unwrap();
        assert!(first["snapshot"]["available"]["targets"].is_array());
        assert_eq!(second["snapshot"]["available"]["unchanged"], true);
        assert_eq!(second["snapshot"]["available"]["target_count"], 1);
        assert!(second["snapshot"]["available"]["targets"].is_null());

        let next_generation = SnapshotGeneration::new(2).unwrap();
        let mut next_nodes = complex_snapshot().nodes;
        for node in &mut next_nodes {
            if let Some(reference) = &mut node.reference {
                reference.generation = next_generation;
            }
        }
        let next = PageSnapshot::new(context(), next_generation, next_nodes, 7).unwrap();
        let next_projection = concise_snapshot(&next, SnapshotNovelty::Novel, None).unwrap();
        assert!(next_projection["targets"].is_array());
    }

    #[tokio::test]
    async fn source_frame_image_defaults_distinguish_omitted_true_and_false() {
        let defaulted = map_progressive_result(
            "fetch_source_frames",
            ProgressiveEvidenceResult::FetchSourceFrames(Box::new(source_frame_batch(5))),
            &UnusedProgressive,
            Instant::now() + Duration::from_secs(1),
            test_cancellation(),
            ResponseRequest::default(),
        )
        .await
        .unwrap();
        assert_eq!(defaulted.response.status, ToolResponseStatus::Succeeded);
        assert!(defaulted.response.warnings.is_empty());
        assert_eq!(defaulted.images.len(), 1);
        assert_eq!(defaulted.response.resources.len(), 5);

        let with = map_progressive_result(
            "fetch_source_frames",
            ProgressiveEvidenceResult::FetchSourceFrames(Box::new(source_frame_batch(5))),
            &UnusedProgressive,
            Instant::now() + Duration::from_secs(1),
            test_cancellation(),
            ResponseRequest {
                inline_images: Some(true),
                ..ResponseRequest::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(with.response.status, ToolResponseStatus::Degraded);
        assert_eq!(
            with.response.warnings[0].code,
            ErrorCode::ResourceLimitExceeded
        );
        assert_eq!(with.images.len(), 4);
        assert_eq!(with.response.resources.len(), 5);

        let suppressed = map_progressive_result(
            "fetch_source_frames",
            ProgressiveEvidenceResult::FetchSourceFrames(Box::new(source_frame_batch(5))),
            &UnusedProgressive,
            Instant::now() + Duration::from_secs(1),
            test_cancellation(),
            ResponseRequest {
                inline_images: Some(false),
                ..ResponseRequest::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(suppressed.response.status, ToolResponseStatus::Succeeded);
        assert!(suppressed.response.warnings.is_empty());
        assert!(suppressed.images.is_empty());
        assert_eq!(suppressed.response.resources.len(), 5);
    }

    #[test]
    fn response_detail_grows_without_changing_authoritative_envelope() {
        let snapshot = complex_snapshot();
        let operation = || BrowserOperationResult::SnapshotPage(Box::new(snapshot.clone()));
        let concise = map_operation_result_with_capture(
            "snapshot_page",
            operation(),
            &[],
            ResponseRequest::default(),
        )
        .unwrap();
        let expanded = map_operation_result_with_capture(
            "snapshot_page",
            operation(),
            &[],
            ResponseRequest {
                detail: ResponseDetail::Expanded,
                inline_images: Some(false),
            },
        )
        .unwrap();
        let full = map_operation_result_with_capture(
            "snapshot_page",
            operation(),
            &[],
            ResponseRequest {
                detail: ResponseDetail::Full,
                inline_images: Some(false),
            },
        )
        .unwrap();
        for mapped in [&concise, &expanded, &full] {
            assert_eq!(mapped.response.status, ToolResponseStatus::Succeeded);
            assert!(mapped.response.warnings.is_empty());
            assert!(mapped.response.resources.is_empty());
        }
        assert!(concise.response.result.get("targets").is_some());
        assert!(concise.response.result.get("nodes").is_none());
        assert!(expanded.response.result.get("semantic_context").is_some());
        // `full` grows the projection without abandoning the bound: no raw node array, and the
        // same omission accounting `expanded` emits.
        assert!(full.response.result.get("nodes").is_none());
        assert!(full.response.result.get("semantic_context").is_some());
        assert!(full.response.result.get("omissions").is_some());
        assert!(
            full.response.result["semantic_context"]
                .as_array()
                .unwrap()
                .len()
                >= expanded.response.result["semantic_context"]
                    .as_array()
                    .unwrap()
                    .len()
        );
    }

    #[test]
    fn batch_step_root_projection_matches_standalone_tools() {
        let snapshot_step = BatchStepResult::new(
            0,
            BrowserOperationKind::SnapshotPage,
            target_id(),
            BatchStepStatus::Succeeded,
            Some(SessionTime::from_nanos(10)),
            Some(SessionTime::from_nanos(15)),
            None,
            Some(BrowserOperationResult::SnapshotPage(Box::new(
                complex_snapshot(),
            ))),
            None,
            None,
            None,
        )
        .unwrap();
        let batch = BatchResult::new(
            interaction_id(),
            target_id(),
            SessionTime::from_nanos(10),
            SessionTime::from_nanos(20),
            BatchOutcome::Completed,
            vec![snapshot_step],
            ObservationPart::Available(live_with_snapshot(complex_snapshot())),
        )
        .unwrap();

        let concise = map_operation_result_with_capture(
            "batch",
            BrowserOperationResult::Batch(Box::new(batch.clone())),
            &[],
            ResponseRequest::default(),
        )
        .unwrap();
        let step_result = &concise.response.result["steps"][0]["result"];
        assert!(step_result.get("targets").is_some());
        assert!(step_result.get("nodes").is_none());
        assert!(serde_json::to_vec(step_result).unwrap().len() < 32 * 1024);

        let full_response = ResponseRequest {
            detail: ResponseDetail::Full,
            inline_images: Some(false),
        };
        let batch_full = map_operation_result_with_capture(
            "batch",
            BrowserOperationResult::Batch(Box::new(batch)),
            &[],
            full_response,
        )
        .unwrap();
        let standalone_full = map_operation_result_with_capture(
            "snapshot_page",
            BrowserOperationResult::SnapshotPage(Box::new(complex_snapshot())),
            &[],
            full_response,
        )
        .unwrap();
        assert_eq!(
            batch_full.response.result["steps"][0]["result"],
            standalone_full.response.result
        );

        let inspect_step = BatchStepResult::new(
            0,
            BrowserOperationKind::InspectPage,
            target_id(),
            BatchStepStatus::Succeeded,
            Some(SessionTime::from_nanos(10)),
            Some(SessionTime::from_nanos(15)),
            None,
            Some(BrowserOperationResult::InspectPage(Box::new(page_state()))),
            None,
            None,
            None,
        )
        .unwrap();
        let inspect_batch = BatchResult::new(
            interaction_id(),
            target_id(),
            SessionTime::from_nanos(10),
            SessionTime::from_nanos(20),
            BatchOutcome::Completed,
            vec![inspect_step],
            ObservationPart::Available(live_with_snapshot(complex_snapshot())),
        )
        .unwrap();
        let inspect_batch = map_operation_result_with_capture(
            "batch",
            BrowserOperationResult::Batch(Box::new(inspect_batch)),
            &[],
            ResponseRequest::default(),
        )
        .unwrap();
        let inspect_standalone = map_operation_result_with_capture(
            "inspect_page",
            BrowserOperationResult::InspectPage(Box::new(page_state())),
            &[],
            ResponseRequest::default(),
        )
        .unwrap();
        assert_eq!(
            inspect_batch.response.result["steps"][0]["result"],
            inspect_standalone.response.result
        );
    }

    #[test]
    fn concise_interactions_omit_record_but_expanded_and_full_retain_it() {
        let operation = || {
            let pre = krometrail_core::NodeStateFacts {
                connected: true,
                checked: Some(false),
                ..krometrail_core::NodeStateFacts::default()
            };
            let post = krometrail_core::NodeStateFacts {
                connected: true,
                checked: Some(false),
                ..krometrail_core::NodeStateFacts::default()
            };
            let reference = NodeReference {
                target_id: target_id(),
                generation: SnapshotGeneration::new(1).unwrap(),
                node_id: SnapshotNodeId::new(1).unwrap(),
            };
            let locator =
                InteractionLocator::Element(krometrail_core::ElementLocator::Reference(reference));
            let record = InteractionRecord::new(
                interaction_id(),
                context(),
                SessionTime::from_nanos(12),
                SessionTime::from_nanos(15),
                BrowserOperationKind::Click,
                SanitizedParameters::new(json!({"button": "left"})).unwrap(),
                LocatorSummary::from_locator(Some(&locator)),
                Some(krometrail_core::ExpectationTargetRole::Checkbox),
                InteractionOutcome::Dispatched,
                krometrail_core::InteractionPostcondition::from_facts(
                    Some(&pre),
                    Some(&post),
                    Some(false),
                    false,
                    None,
                    krometrail_core::SideChannelSignals::unobserved(),
                ),
                None,
            )
            .unwrap();
            BrowserOperationResult::Click(Box::new(InteractionResult {
                record,
                observation: live_with_snapshot(complex_snapshot()),
            }))
        };
        let concise = map_operation_result_with_capture(
            "click",
            operation(),
            &[],
            ResponseRequest {
                inline_images: Some(false),
                ..ResponseRequest::default()
            },
        )
        .unwrap();
        let expanded = map_operation_result_with_capture(
            "click",
            operation(),
            &[],
            ResponseRequest {
                detail: ResponseDetail::Expanded,
                inline_images: Some(false),
            },
        )
        .unwrap();
        let full = map_operation_result_with_capture(
            "click",
            operation(),
            &[],
            ResponseRequest {
                detail: ResponseDetail::Full,
                inline_images: Some(false),
            },
        )
        .unwrap();

        assert_eq!(concise.response.status, expanded.response.status);
        assert_eq!(expanded.response.status, full.response.status);
        assert!(concise.response.result.get("observation").is_some());
        assert!(concise.response.result.get("record").is_none());
        assert!(expanded.response.result.get("record").is_some());
        assert!(full.response.result.get("record").is_some());

        // The bounded postcondition block is on-by-default at every detail
        // level, concise included, and the expanded/full record echo carries
        // the identical field: one authority projected twice.
        let expected = json!({
            "page": {
                "url_changed": false,
                "navigation_lifecycle_observed": false,
                "main_frame_navigation_observed": null,
            },
            "target": {
                "node": "present",
                "checked": {"before": false, "after": false, "changed": false},
                "expanded": {"before": null, "after": null, "changed": null},
                "selected": {"before": null, "after": null, "changed": null},
                "pressed": {"before": null, "after": null, "changed": null},
                "value_length_changed": null,
            },
            "signals": {"window_open_attempts": null, "download_requests": null},
            "new_pages": null,
            "downloads": null,
            "clipboard_write_confirmed": null,
        });
        for projection in [&concise, &expanded, &full] {
            assert_eq!(projection.response.result["postcondition"], expected);
            assert_eq!(
                projection.response.result["expectation_note"],
                "The target's checked state was unchanged by the observation point."
            );
        }
        assert_eq!(
            expanded.response.result["record"]["postcondition"],
            expected
        );
        assert_eq!(
            expanded.response.result["record"]["expectation_note"],
            "checked_state_unchanged"
        );
        assert_eq!(
            full.response.result["record"]["expectation_note"],
            "checked_state_unchanged"
        );
    }

    #[test]
    fn inline_images_is_orthogonal_to_structured_detail() {
        let operation =
            || BrowserOperationResult::TakeScreenshot(Box::new(screenshot(ImageFormat::Png)));
        let without = map_operation_result_with_capture(
            "take_screenshot",
            operation(),
            &[],
            ResponseRequest::default(),
        )
        .unwrap();
        let with = map_operation_result_with_capture(
            "take_screenshot",
            operation(),
            &[],
            ResponseRequest {
                detail: ResponseDetail::Concise,
                inline_images: Some(true),
            },
        )
        .unwrap();
        assert_eq!(without.response.result, with.response.result);
        assert!(without.images.is_empty());
        assert_eq!(with.images.len(), 1);
    }

    #[test]
    fn temporal_detail_defaults_to_concise_and_every_tier_bounds_the_range() {
        let range = video_result().range;
        let value = json!({
            "range": range,
            "capture_quality": {"status": "available", "cadence": {}, "gap_summary": {}, "retention_warnings": [], "warnings": []},
            "browser_events": {"status": "available", "effective_range": {}, "matched_count": 50, "returned_count": 2, "events": [{}, {}], "collection_gaps": [], "unavailable_ranges": [], "warnings": []}
        });
        let mut concise = value.clone();
        project_temporal_value(&mut concise, ResponseDetail::Concise).unwrap();
        assert_eq!(concise["browser_events"]["events"], json!([{}, {}]));
        assert_eq!(concise["range"]["frame_count"], 2);
        assert!(concise["range"].get("frame_ids").is_none());
        let mut expanded = value.clone();
        project_temporal_value(&mut expanded, ResponseDetail::Expanded).unwrap();
        assert_eq!(expanded["browser_events"], value["browser_events"]);
        assert_eq!(expanded["range"]["frame_count"], 2);
        assert!(expanded["range"].get("frame_ids").is_none());
        assert!(expanded["range"]["first_frame_id"].is_string());
        let mut full = value.clone();
        project_temporal_value(&mut full, ResponseDetail::Full).unwrap();
        assert_eq!(full["browser_events"], value["browser_events"]);
        assert_eq!(
            full["range"]["frame_ids"]["ids"].as_array().unwrap().len(),
            2
        );
        assert_eq!(full["range"]["frame_ids"]["omitted_count"], 0);
    }

    #[tokio::test]
    async fn concise_source_frame_listing_is_small_and_keeps_only_drilldown_fields() {
        let list = source_frame_list(64);
        let mapped = map_progressive_result(
            "list_source_frames",
            ProgressiveEvidenceResult::ListSourceFrames(Box::new(list.clone())),
            &UnusedProgressive,
            Instant::now() + Duration::from_secs(1),
            test_cancellation(),
            ResponseRequest::default(),
        )
        .await
        .expect("projection succeeds");
        let resource_count = mapped.response.resources.len();
        let concise = mapped.response.result;
        assert!(serde_json::to_vec(&concise).unwrap().len() < 16 * 1024);
        let row = &concise["frames"][0];
        assert_eq!(
            row.as_object().unwrap().keys().collect::<Vec<_>>(),
            vec![
                "encoded_byte_len",
                "frame_id",
                "media_type",
                "resolved_position",
                "session_time",
            ]
        );
        assert!(row.get("provenance").is_none());
        assert!(row.get("content_sha256").is_none());
        assert!(row.get("request_position").is_none());
        assert_eq!(resource_count, 64);

        let expanded = map_progressive_result(
            "list_source_frames",
            ProgressiveEvidenceResult::ListSourceFrames(Box::new(list)),
            &UnusedProgressive,
            Instant::now() + Duration::from_secs(1),
            test_cancellation(),
            ResponseRequest {
                detail: ResponseDetail::Expanded,
                ..ResponseRequest::default()
            },
        )
        .await
        .expect("expanded projection succeeds")
        .response
        .result;
        assert!(expanded["frames"][0].get("provenance").is_some());
        assert!(expanded["frames"][0].get("content_sha256").is_some());
    }

    #[test]
    fn compact_resolved_range_is_bounded_while_full_keeps_ordered_frame_ids() {
        let frame_ids = (1..=29)
            .map(|value| FrameId::from_uuid(uuid::Uuid::from_u128(value)))
            .collect::<Vec<_>>();
        let interval = SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(100)).unwrap();
        let range = krometrail_core::ResolvedRange::new(
            session_id(),
            target_id(),
            TemporalRangeAnchorKind::SessionTime,
            interval,
            interval,
            frame_ids.clone(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            RangeResolutionOptions::DEFAULT,
        )
        .unwrap();

        let concise = serde_json::to_value(compact_resolved_range(&range).unwrap()).unwrap();
        assert_eq!(concise["frame_count"], 29);
        assert!(concise.get("frame_ids").is_none());
        assert!(serde_json::to_vec(&concise).unwrap().len() < 1_000);

        let full = serde_json::to_value(range).unwrap();
        assert_eq!(full["frame_ids"], serde_json::to_value(frame_ids).unwrap());
    }

    fn large_synthetic_range(frame_count: u128) -> krometrail_core::ResolvedRange {
        let interval = SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(100)).unwrap();
        krometrail_core::ResolvedRange::new(
            session_id(),
            target_id(),
            TemporalRangeAnchorKind::SessionTime,
            interval,
            interval,
            (1..=frame_count)
                .map(|value| FrameId::from_uuid(uuid::Uuid::from_u128(value)))
                .collect(),
            (1..=60)
                .map(|value| InteractionId::from_uuid(uuid::Uuid::from_u128(10_000 + value)))
                .collect(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            RangeResolutionOptions::DEFAULT,
        )
        .unwrap()
    }

    #[test]
    fn expanded_range_projection_never_enumerates_frames_and_accounts_omissions_exactly() {
        // Regression for issue #14 finding #7: the expanded response of a
        // long every-frame range must not enumerate thousands of identifiers.
        let range = large_synthetic_range(1_000);
        let expanded = bounded_resolved_range(&range, ResponseDetail::Expanded).unwrap();
        assert_eq!(expanded["frame_count"], 1_000);
        assert!(expanded.get("frame_ids").is_none());
        assert_eq!(
            expanded["first_frame_id"],
            serde_json::to_value(range.frame_ids.first()).unwrap()
        );
        assert_eq!(
            expanded["last_frame_id"],
            serde_json::to_value(range.frame_ids.last()).unwrap()
        );
        assert_eq!(
            expanded["interaction_ids"]["ids"].as_array().unwrap().len(),
            MAX_EXPANDED_RANGE_EVENT_IDS
        );
        assert_eq!(
            expanded["interaction_ids"]["omitted_count"],
            60 - MAX_EXPANDED_RANGE_EVENT_IDS as u64
        );
        assert!(expanded["drill_down"]["complete_frame_ids"].is_string());
        assert!(serde_json::to_vec(&expanded).unwrap().len() < 16 * 1024);
    }

    #[test]
    fn full_range_projection_caps_the_frame_head_with_exact_omission_and_offset() {
        let range = large_synthetic_range(1_000);
        let full = bounded_resolved_range(&range, ResponseDetail::Full).unwrap();
        assert_eq!(
            full["frame_ids"]["ids"].as_array().unwrap().len(),
            MAX_FULL_RANGE_FRAME_IDS
        );
        assert_eq!(
            full["frame_ids"]["omitted_count"],
            1_000 - MAX_FULL_RANGE_FRAME_IDS as u64
        );
        assert_eq!(
            full["drill_down"]["next_offset"],
            MAX_FULL_RANGE_FRAME_IDS as u64
        );
        assert_eq!(full["interaction_ids"]["ids"].as_array().unwrap().len(), 60);
        assert_eq!(full["interaction_ids"]["omitted_count"], 0);
        assert!(serde_json::to_vec(&full).unwrap().len() < 32 * 1024);

        // Short ranges keep their complete head with zero omissions and no
        // continuation offset.
        let short = large_synthetic_range(3);
        let full = bounded_resolved_range(&short, ResponseDetail::Full).unwrap();
        assert_eq!(full["frame_ids"]["ids"].as_array().unwrap().len(), 3);
        assert_eq!(full["frame_ids"]["omitted_count"], 0);
        assert!(full["drill_down"]["next_offset"].is_null());
    }

    #[test]
    fn capture_quality_epoch_presentation_is_bounded_with_exact_accounting() {
        let epochs = (0..50)
            .map(|index| {
                json!({
                    "epoch_index": index,
                    "range": {"start": index, "end": index},
                    "frame_count": 1,
                })
            })
            .collect::<Vec<_>>();
        let mut quality = json!({"frame_count": 50, "epochs": epochs});
        bound_capture_quality_epochs(&mut quality, ResponseDetail::Concise);
        assert_eq!(
            quality["epochs"].as_array().unwrap().len(),
            MAX_CONCISE_PROJECTED_EPOCHS
        );
        assert_eq!(
            quality["omitted_epoch_count"],
            50 - MAX_CONCISE_PROJECTED_EPOCHS as u64
        );
    }

    #[test]
    fn visible_errors_are_structured_without_json_text_duplication() {
        let result = visible_error(
            "inspect_page",
            error(ErrorCode::InvalidInput, "invalid request"),
        );
        assert_eq!(result.is_error, Some(true));
        let text = result.content[0].as_text().unwrap().text.as_str();
        assert_eq!(text, "inspect_page failed: invalid request");
        assert!(!text.contains("{\""));
        assert_eq!(
            result.structured_content.as_ref().unwrap()["status"],
            "failed"
        );
        assert_eq!(
            result.structured_content.as_ref().unwrap()["error"]["code"],
            "invalid_input"
        );
    }

    #[test]
    fn pre_dispatch_interaction_errors_keep_a_context_anchor_without_a_record() {
        let result = visible_error(
            "click",
            error(ErrorCode::TargetHidden, "page is hidden").with_context(ErrorContext {
                session_id: Some(session_id()),
                target_id: Some(target_id()),
                interaction_id: Some(interaction_id()),
                range: None,
            }),
        );
        let structured = result.structured_content.unwrap();
        assert_eq!(structured["status"], "failed");
        assert_eq!(
            structured["interaction"]["session_id"],
            session_id().to_string()
        );
        assert_eq!(
            structured["interaction"]["target_id"],
            target_id().to_string()
        );
        assert_eq!(structured["interaction"]["operation"], "click");
        assert!(structured["interaction"].get("timing").is_none());
        assert!(structured["result"].get("record").is_none());
    }

    #[test]
    fn screenshot_bytes_are_only_image_content_with_matching_metadata() {
        for (format, mime) in [
            (ImageFormat::Png, "image/png"),
            (ImageFormat::Jpeg, "image/jpeg"),
        ] {
            let mapped = map_operation_result_with_capture(
                "take_screenshot",
                BrowserOperationResult::TakeScreenshot(Box::new(screenshot(format))),
                &[],
                ResponseRequest {
                    inline_images: Some(true),
                    ..ResponseRequest::default()
                },
            )
            .unwrap();
            let result = into_call_tool_result(mapped, &[]).unwrap();
            assert_eq!(result.content.len(), 2);
            assert_eq!(result.content[1].as_image().unwrap().mime_type, mime);
            let structured = result.structured_content.unwrap();
            assert_eq!(structured["images"][0]["role"], "requested_screenshot");
            assert!(!structured.to_string().contains("iVBOR"));
            assert!(!result.content[0].as_text().unwrap().text.contains("result"));
        }
    }

    #[test]
    fn degradation_wait_timeout_page_anchor_and_batch_failure_remain_distinct() {
        let unavailable = error(ErrorCode::PageObservationFailed, "page unavailable");
        let live = LiveObservation {
            context: context(),
            page: ObservationPart::Unavailable(unavailable.clone()),
            snapshot: ObservationPart::Unavailable(unavailable.clone()),
            screenshot: ObservationPart::Unavailable(unavailable.clone()),
        };
        let degraded = map_operation_result(
            "observe_live",
            BrowserOperationResult::ObserveLive(Box::new(live)),
        )
        .unwrap();
        assert_eq!(degraded.response.status, ToolResponseStatus::Degraded);
        assert!(!degraded.is_error);
        assert_eq!(degraded.response.warnings, vec![unavailable]);
        for component in ["page", "snapshot", "screenshot"] {
            assert!(
                degraded.response.result[component]
                    .get("unavailable")
                    .is_some(),
                "{component} remains explicitly unavailable"
            );
        }

        let wait = WaitResult::new(
            context(),
            WaitCondition::Elapsed {
                duration: Duration::from_millis(10),
            },
            WaitOutcome::TimedOut {
                last_probe_at: SessionTime::from_nanos(20),
            },
            Some(WaitProbe::Elapsed {
                matched: false,
                elapsed_ms: 10,
            }),
        )
        .unwrap();
        let timed_out =
            map_operation_result("wait", BrowserOperationResult::Wait(Box::new(wait))).unwrap();
        assert_eq!(timed_out.response.status, ToolResponseStatus::Failed);
        assert_eq!(
            timed_out.response.error.unwrap().code,
            ErrorCode::WaitTimedOut
        );

        let timing = InteractionTiming::new(
            SessionTime::from_nanos(10),
            SessionTime::from_nanos(11),
            SessionTime::from_nanos(12),
            None,
        )
        .unwrap();
        let anchor = InteractionAnchor::new(
            interaction_id(),
            session_id(),
            target_id(),
            BrowserOperationKind::NavigatePage,
            timing,
        )
        .unwrap();
        let page_error = error(ErrorCode::NavigationFailed, "navigation failed");
        let page = PageOperationResult::new(
            anchor.clone(),
            PageOperationOutcome::Failed(page_error.clone()),
            ObservationPart::Unavailable(page_error),
        )
        .unwrap();
        let page = map_operation_result(
            "navigate_page",
            BrowserOperationResult::NavigatePage(Box::new(page)),
        )
        .unwrap();
        assert_eq!(page.response.interaction, Some(anchor.into()));
        assert_eq!(page.response.status, ToolResponseStatus::Failed);

        let batch_error = error(ErrorCode::InteractionFailed, "step failed");
        let step = BatchStepResult::new(
            0,
            BrowserOperationKind::Click,
            target_id(),
            BatchStepStatus::Failed,
            Some(SessionTime::from_nanos(10)),
            Some(SessionTime::from_nanos(15)),
            None,
            None,
            Some(batch_error.clone()),
            None,
            Some(ObservationPart::Unavailable(error(
                ErrorCode::ScreenshotFailed,
                "requested capture failed",
            ))),
        )
        .unwrap();
        let skipped = BatchStepResult::new(
            1,
            BrowserOperationKind::Wait,
            target_id(),
            BatchStepStatus::Skipped,
            None,
            None,
            None,
            None,
            None,
            Some(BatchSkipReason::PriorFailure),
            None,
        )
        .unwrap();
        let batch = BatchResult::new(
            interaction_id(),
            target_id(),
            SessionTime::from_nanos(10),
            SessionTime::from_nanos(20),
            BatchOutcome::StoppedOnFailure,
            vec![step, skipped],
            ObservationPart::Unavailable(batch_error),
        )
        .unwrap();
        let batch =
            map_operation_result("batch", BrowserOperationResult::Batch(Box::new(batch))).unwrap();
        assert_eq!(batch.response.status, ToolResponseStatus::Failed);
        // A failed batch carries the same evidence the degraded path builds: partial step
        // results, the failing step's index and operation, and the step's stable error code.
        assert_eq!(
            batch.summary,
            "batch failed: batch step 0 (click) failed: step failed"
        );
        let reported = batch
            .response
            .error
            .as_ref()
            .expect("failure carries error");
        assert_eq!(reported.code, ErrorCode::InteractionFailed);
        assert_eq!(batch.response.result["steps"].as_array().unwrap().len(), 2);
        assert!(batch.response.result["steps"][0]["screenshot"]["unavailable"].is_object());
        assert!(
            batch.response.result["steps"][1]
                .get("screenshot")
                .is_none()
        );

        let timing = InteractionTiming::new(
            SessionTime::from_nanos(10),
            SessionTime::from_nanos(11),
            SessionTime::from_nanos(12),
            Some(SessionTime::from_nanos(13)),
        )
        .unwrap();
        let anchor = InteractionAnchor::new(
            interaction_id(),
            session_id(),
            target_id(),
            BrowserOperationKind::NavigatePage,
            timing,
        )
        .unwrap();
        let page = PageOperationResult::new(
            anchor.clone(),
            PageOperationOutcome::Succeeded(PageChange::Navigated),
            ObservationPart::Available(live_with_snapshot(complex_snapshot())),
        )
        .unwrap();
        let succeeded_page = BatchStepResult::new(
            0,
            BrowserOperationKind::NavigatePage,
            target_id(),
            BatchStepStatus::Succeeded,
            Some(SessionTime::from_nanos(10)),
            Some(SessionTime::from_nanos(15)),
            Some(anchor),
            Some(BrowserOperationResult::NavigatePage(Box::new(page))),
            None,
            None,
            None,
        )
        .unwrap();
        let compact = BatchResult::new(
            interaction_id(),
            target_id(),
            SessionTime::from_nanos(10),
            SessionTime::from_nanos(20),
            BatchOutcome::Completed,
            vec![succeeded_page],
            ObservationPart::Available(live_with_snapshot(complex_snapshot())),
        )
        .unwrap();
        let compact =
            map_operation_result("batch", BrowserOperationResult::Batch(Box::new(compact)))
                .unwrap();
        let step_result = &compact.response.result["steps"][0]["result"];
        assert!(step_result.get("observation").is_none());
        assert!(compact.response.result["final_observation"]["available"].is_object());
        assert!(
            compact.response.result["final_observation"]["available"]["semantic_outcomes"]
                .as_array()
                .is_some_and(|outcomes| !outcomes.is_empty())
        );

        let satisfied_wait = WaitResult::new(
            context(),
            WaitCondition::Elapsed {
                duration: Duration::from_millis(10),
            },
            WaitOutcome::Satisfied {
                matched_at: SessionTime::from_nanos(15),
            },
            Some(WaitProbe::Elapsed {
                matched: true,
                elapsed_ms: 10,
            }),
        )
        .unwrap();
        let succeeded = BatchStepResult::new(
            0,
            BrowserOperationKind::Wait,
            target_id(),
            BatchStepStatus::Succeeded,
            Some(SessionTime::from_nanos(10)),
            Some(SessionTime::from_nanos(15)),
            None,
            Some(BrowserOperationResult::Wait(Box::new(satisfied_wait))),
            None,
            None,
            None,
        )
        .unwrap();
        let incomplete_evidence = error(
            ErrorCode::PageObservationFailed,
            "final observation unavailable",
        );
        let degraded_batch = BatchResult::new(
            interaction_id(),
            target_id(),
            SessionTime::from_nanos(10),
            SessionTime::from_nanos(20),
            BatchOutcome::CompletedWithFailures,
            vec![succeeded],
            ObservationPart::Unavailable(incomplete_evidence),
        )
        .unwrap();
        let degraded_batch = map_operation_result(
            "batch",
            BrowserOperationResult::Batch(Box::new(degraded_batch)),
        )
        .unwrap();
        assert_eq!(degraded_batch.response.status, ToolResponseStatus::Degraded);
        assert!(!degraded_batch.is_error);
        assert_eq!(
            degraded_batch.response.result["outcome"],
            "completed_with_failures"
        );
        assert_eq!(degraded_batch.response.warnings.len(), 1);
    }

    #[test]
    fn request_wire_shape_stays_integer_millisecond_for_response_test_fixture() {
        let request = WaitRequest::new(
            PageSelection::Target(target_id()),
            WaitCondition::Elapsed {
                duration: Duration::from_millis(10),
            },
            Duration::from_millis(20),
            Duration::from_millis(10),
        )
        .unwrap();
        assert_eq!(serde_json::to_value(request).unwrap()["timeout"], 20);
    }
}
