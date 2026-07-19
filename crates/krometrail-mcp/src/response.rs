use std::{any::Any, sync::Arc, time::Instant};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use krometrail_core::{
    ArtifactCacheDisposition, ArtifactGenerationResult, ArtifactHandle, ArtifactId,
    ArtifactOutcome, BatchOutcome, BatchResult, BrowserOperationResult, BrowserOwnership,
    BrowserSessionState, BrowserStatus, BrowserStopOutcome, CaptureFailure, CaptureStreamState,
    EncodedScreenshot, ErrorCode, EveryNthFrame, InteractionAnchor, KrometrailError,
    LiveObservation, NonEmptyText, ObservationPart, PageOperationOutcome, PageOperationResult,
    PageSnapshot, ProfileRef, ProgressiveEvidence, ProgressiveEvidenceContext,
    ProgressiveEvidenceRequest, ProgressiveEvidenceResult, RecordingBudgetState,
    ResolvedRangeHandleId, RetrieveArtifactRequest, ScreenshotMetadata, SessionId, SessionTime,
    ShutdownQuality, SourceFrameBatch, SourceFrameHandle, TargetId, TemporalDebugBundle,
    TemporalVideoGenerationResult, VideoPresentationPolicy, WaitOutcome,
};
use rmcp::model::JsonObject;
use rmcp::model::{CallToolResult, Content, RawResource};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use temporal_vision::{ArtifactKind, EvidenceClass, PixelDimensions};

use crate::resources::{ResourceKind, ResourceProjection};

const MAX_CONCISE_TARGETS: usize = 48;
const MAX_CONCISE_TARGET_JSON_BYTES: usize = 12 * 1024;
const MAX_EXPANDED_CONTEXT_NODES: usize = 96;
const MAX_EXPANDED_SNAPSHOT_JSON_BYTES: usize = 32 * 1024;

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
    pub inline_images: bool,
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
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ConciseBrowserStatus {
    pub session_id: SessionId,
    pub state: BrowserSessionState,
    pub ownership: BrowserOwnership,
    pub profile: ProfileRef,
    pub selected_target_id: Option<TargetId>,
    pub page_count: u32,
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
    let encoded = serde_json::to_vec(&value).map_err(|_| invalid_projection("response"))?;
    let mut deserializer = serde_json::Deserializer::from_slice(&encoded);
    serde_path_to_error::deserialize(&mut deserializer).map_err(|error| {
        let nested = normalize_projection_path(&error.path().to_string());
        invalid_projection(&format!("response.{nested}"))
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

fn invalid_projection(path: &str) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::InvalidInput,
        NonEmptyText::new(format!(
            "tool arguments do not match the advertised input schema at {path}"
        ))
        .expect("projection validation message is non-empty"),
    )
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
    pub source_frame_count: u32,
    pub selected_frame_count: u32,
    pub omitted_frame_count: u32,
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
pub struct ResponseDiagnostics {
    pub correlation_id: String,
    pub log_path: Option<String>,
}

#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct ToolResponse {
    pub tool: String,
    pub status: ToolResponseStatus,
    pub result: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range_handle: Option<ResolvedRangeHandleId>,
    pub interaction: Option<InteractionAnchor>,
    pub warnings: Vec<KrometrailError>,
    pub images: Vec<ResponseImage>,
    pub resources: Vec<ResponseResource>,
    pub error: Option<KrometrailError>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagnostics: Option<ResponseDiagnostics>,
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
    interaction: Option<InteractionAnchor>,
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

pub(crate) fn map_operation_result_with_capture(
    tool: &str,
    result: BrowserOperationResult,
    capture_statuses: &[krometrail_core::TargetCaptureStatus],
    response: ResponseRequest,
) -> Result<MappedResult, ResponseInvariantError> {
    let mut projection = project_operation(result, response)?;
    let target_id = projection_target_id(&projection);
    add_capture_warnings(&mut projection, capture_statuses, target_id);
    project_response(tool, &mut projection, response)?;
    let status = projection.status;
    Ok(mapped(
        tool,
        projection,
        match status {
            ToolResponseStatus::Succeeded => format!("{tool} succeeded"),
            ToolResponseStatus::Degraded => {
                format!("{tool} succeeded with incomplete live evidence")
            }
            ToolResponseStatus::Failed => format!("{tool} failed"),
        },
    ))
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
        ResponseDetail::Full => map_lifecycle_result(tool, status),
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
        "range": result.range,
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
    let target_id = error.context.target_id;
    let mut projection = Projection::success(json!({}));
    projection.fail_with(error);
    add_capture_warnings(&mut projection, capture_statuses, target_id);
    into_call_tool_result(mapped(tool, projection, summary))
        .expect("stable error envelopes always serialize")
}

fn add_capture_warnings(
    projection: &mut Projection,
    capture_statuses: &[krometrail_core::TargetCaptureStatus],
    target_id: Option<TargetId>,
) {
    for status in capture_statuses
        .iter()
        .filter(|status| status.state() == krometrail_core::CaptureStreamState::Failed)
        .filter(|status| target_id.is_none_or(|target_id| status.target_id() == target_id))
    {
        let failure = status
            .failure()
            .expect("failed capture status is validated with a failure");
        projection.degrade_with_stage(
            vec![capture_failed_warning(status)],
            failure.stage().as_str(),
        );
    }
}

fn projection_target_id(projection: &Projection) -> Option<TargetId> {
    projection
        .interaction
        .as_ref()
        .map(|interaction| interaction.target_id)
        .or_else(|| {
            ["/context/target_id", "/target_id"]
                .into_iter()
                .find_map(|pointer| projection.result.pointer(pointer)?.as_str()?.parse().ok())
        })
}

pub(crate) fn into_call_tool_result(
    mapped: MappedResult,
) -> Result<CallToolResult, rmcp::ErrorData> {
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
) -> Result<Projection, ResponseInvariantError> {
    match result {
        BrowserOperationResult::InspectPage(value) => serializable(*value),
        BrowserOperationResult::SnapshotPage(value) => serializable(*value),
        BrowserOperationResult::QueryPage(value) => serializable(*value),
        BrowserOperationResult::TakeScreenshot(value) => {
            let mut projection = serializable(value.metadata().clone())?;
            projection.images.push(EncodedMcpImage::Screenshot {
                role: ImageRole::RequestedScreenshot,
                step_index: None,
                screenshot: *value,
            });
            Ok(projection)
        }
        BrowserOperationResult::EvaluatePage(value) => serializable(*value),
        BrowserOperationResult::ObserveLive(value) => {
            let (result, warnings, image) =
                project_live_observation(*value, ImageRole::LiveObservation, None, response)?;
            let mut projection = Projection::success(result);
            projection.degrade_with(warnings);
            projection.images.extend(image);
            Ok(projection)
        }
        BrowserOperationResult::ListPages(value) => serializable(*value),
        BrowserOperationResult::ListPageContexts(value) => serializable(*value),
        BrowserOperationResult::WaitForPage(value) => serializable(*value),
        BrowserOperationResult::ListFrames(value) => serializable(*value),
        BrowserOperationResult::ListPageAssets(value) => serializable(*value),
        BrowserOperationResult::ReadClipboard(value) => serializable(*value),
        BrowserOperationResult::ListDownloads(value)
        | BrowserOperationResult::WaitForDownload(value) => serializable(*value),
        BrowserOperationResult::CancelDownload(value) => serializable(*value),
        BrowserOperationResult::CreatePage(value)
        | BrowserOperationResult::SelectPage(value)
        | BrowserOperationResult::ClosePage(value)
        | BrowserOperationResult::NavigatePage(value)
        | BrowserOperationResult::ReloadPage(value)
        | BrowserOperationResult::GoBack(value)
        | BrowserOperationResult::GoForward(value) => project_page_operation(*value, response),
        BrowserOperationResult::SetViewport(value) => {
            let mut projection = project_page_operation(value.operation, response)?;
            let mut warnings = Vec::new();
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
            projection.degrade_with(warnings);
            Ok(projection)
        }
        BrowserOperationResult::WriteClipboard(value) => {
            let bytes = value.utf8_bytes;
            let mut projection = project_page_operation(value.operation, response)?;
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
            let (observation, warnings, image) =
                project_live_observation(value.observation, ImageRole::PostAction, None, response)?;
            let mut projection = Projection::success(json!({
                "record": value.record,
                "observation": observation,
            }));
            projection.interaction = Some(anchor);
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
        BrowserOperationResult::Batch(value) => project_batch(*value, response),
    }
}

fn project_page_operation(
    value: PageOperationResult,
    response: ResponseRequest,
) -> Result<Projection, ResponseInvariantError> {
    let interaction = value.interaction.clone();
    let (observation, warnings, image) =
        project_live_observation_part(value.observation, ImageRole::PostAction, response)?;
    let outcome = serde_json::to_value(&value.outcome).map_err(|_| ResponseInvariantError)?;
    let mut projection = Projection::success(json!({
        "interaction": interaction,
        "outcome": outcome,
        "observation": observation,
    }));
    projection.interaction = Some(interaction);
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
) -> Result<Projection, ResponseInvariantError> {
    let mut images = Vec::new();
    let mut step_values = Vec::with_capacity(value.steps.len());
    let mut first_step_error = None;
    let mut step_failure_seen = false;
    for step in value.steps {
        let result = step
            .result
            .map(|result| project_batch_step(result, response))
            .transpose()?
            .flatten();
        if step.status != krometrail_core::BatchStepStatus::Succeeded {
            step_failure_seen = true;
        }
        if first_step_error.is_none() {
            first_step_error = step.error.clone();
        }
        let screenshot = match step.screenshot {
            ObservationPart::Available(screenshot) => {
                let metadata = screenshot.metadata().clone();
                images.push(EncodedMcpImage::Screenshot {
                    role: ImageRole::BatchStep,
                    step_index: Some(step.index),
                    screenshot,
                });
                json!({"available": metadata})
            }
            ObservationPart::Unavailable(error) => json!({"unavailable": error}),
        };
        step_values.push(json!({
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
            "screenshot": screenshot,
        }));
    }

    let (final_observation, final_warnings, final_image) =
        project_live_observation_part(value.final_observation, ImageRole::BatchFinal, response)?;
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
    projection.degrade_with(final_warnings);
    match outcome {
        BatchOutcome::Completed => {}
        // The domain uses CompletedWithFailures for both failed steps and incomplete final live
        // evidence. If every step succeeded, preserve the already-applied mutations and expose the
        // missing evidence as degradation instead of encouraging callers to replay the batch.
        BatchOutcome::CompletedWithFailures if !step_failure_seen => {}
        _ => projection.fail_with(first_step_error.unwrap_or_else(|| batch_outcome_error(outcome))),
    }
    Ok(projection)
}

fn project_batch_step(
    result: BrowserOperationResult,
    response: ResponseRequest,
) -> Result<Option<Value>, ResponseInvariantError> {
    let mut value = project_operation(result, response)?.result;
    let Some(object) = value.as_object_mut() else {
        return Ok(Some(value));
    };
    object.remove("observation");
    Ok(Some(value))
}

fn project_live_observation_part(
    value: ObservationPart<LiveObservation>,
    role: ImageRole,
    response: ResponseRequest,
) -> Result<(Value, Vec<KrometrailError>, Option<EncodedMcpImage>), ResponseInvariantError> {
    match value {
        ObservationPart::Available(observation) => {
            let (value, warnings, image) =
                project_live_observation(observation, role, None, response)?;
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
) -> Result<(Value, Vec<KrometrailError>, Option<EncodedMcpImage>), ResponseInvariantError> {
    let mut warnings = Vec::new();
    let mut page = project_serializable_part(value.page, &mut warnings)?;
    project_page_state_part(&mut page, response.detail)?;
    let mut snapshot = project_serializable_part(value.snapshot, &mut warnings)?;
    project_snapshot_part(&mut snapshot, response.detail)?;
    let (screenshot, image) = match value.screenshot {
        ObservationPart::Available(screenshot) => (
            json!({"available": screenshot.metadata()}),
            Some(screenshot),
        ),
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
}

#[derive(Clone, Copy, Debug, Serialize)]
struct ExpandedSnapshotOmissions {
    source_nodes: u32,
    presentation_targets: u32,
    presentation_context_nodes: u32,
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
struct ExpandedSnapshot {
    context: krometrail_core::ObservationContext,
    generation: krometrail_core::SnapshotGeneration,
    targets: Vec<ExactTarget>,
    semantic_context: Vec<SemanticContextEntry>,
    omissions: ExpandedSnapshotOmissions,
}

fn exact_target(
    node: &krometrail_core::SnapshotNode,
) -> Result<ExactTarget, ResponseInvariantError> {
    Ok(ExactTarget {
        reference: node.reference.ok_or(ResponseInvariantError)?,
        role: node.role.clone(),
        name: node.name.clone(),
        value: node.value.clone(),
        states: node.properties.clone(),
    })
}

fn bounded_targets(snapshot: &PageSnapshot) -> Result<Vec<ExactTarget>, ResponseInvariantError> {
    let mut actions = snapshot
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| node.actionable)
        .collect::<Vec<_>>();
    actions.sort_by_key(|(index, node)| (snapshot_action_rank(node), *index));
    let mut targets = Vec::new();
    let mut bytes = 2usize;
    for (_, node) in actions {
        if targets.len() == MAX_CONCISE_TARGETS {
            break;
        }
        let target = exact_target(node)?;
        let entry_bytes = serde_json::to_vec(&target)
            .map_err(|_| ResponseInvariantError)?
            .len();
        let next = bytes
            .checked_add(usize::from(!targets.is_empty()))
            .and_then(|value| value.checked_add(entry_bytes))
            .ok_or(ResponseInvariantError)?;
        if next > MAX_CONCISE_TARGET_JSON_BYTES {
            continue;
        }
        bytes = next;
        targets.push(target);
    }
    Ok(targets)
}

fn concise_snapshot(snapshot: &PageSnapshot) -> Result<ExactTargetIndex, ResponseInvariantError> {
    let actionable = snapshot.nodes.iter().filter(|node| node.actionable).count();
    let targets = bounded_targets(snapshot)?;
    Ok(ExactTargetIndex {
        context: snapshot.context.clone(),
        generation: snapshot.generation,
        omissions: TargetOmissions {
            source_nodes: snapshot.omitted_node_count,
            presentation_targets: u32::try_from(actionable - targets.len())
                .map_err(|_| ResponseInvariantError)?,
        },
        targets,
    })
}

fn expanded_snapshot(snapshot: &PageSnapshot) -> Result<ExpandedSnapshot, ResponseInvariantError> {
    let targets = bounded_targets(snapshot)?;
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
        if semantic_context.len() == MAX_EXPANDED_CONTEXT_NODES {
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
        let candidate = ExpandedSnapshot {
            context: snapshot.context.clone(),
            generation: snapshot.generation,
            targets: targets.clone(),
            semantic_context: semantic_context.clone(),
            omissions: ExpandedSnapshotOmissions {
                source_nodes: snapshot.omitted_node_count,
                presentation_targets: u32::try_from(actionable - targets.len())
                    .map_err(|_| ResponseInvariantError)?,
                presentation_context_nodes: u32::try_from(context_count - semantic_context.len())
                    .map_err(|_| ResponseInvariantError)?,
            },
        };
        if serde_json::to_vec(&candidate)
            .map_err(|_| ResponseInvariantError)?
            .len()
            > MAX_EXPANDED_SNAPSHOT_JSON_BYTES
        {
            semantic_context.pop();
            continue;
        }
    }
    Ok(ExpandedSnapshot {
        context: snapshot.context.clone(),
        generation: snapshot.generation,
        omissions: ExpandedSnapshotOmissions {
            source_nodes: snapshot.omitted_node_count,
            presentation_targets: u32::try_from(actionable - targets.len())
                .map_err(|_| ResponseInvariantError)?,
            presentation_context_nodes: u32::try_from(context_count - semantic_context.len())
                .map_err(|_| ResponseInvariantError)?,
        },
        targets,
        semantic_context,
    })
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
    } else if node.name.is_some() || node.value.is_some() || node.description.is_some() {
        1
    } else {
        2
    }
}

fn project_response(
    tool: &str,
    projection: &mut Projection,
    response: ResponseRequest,
) -> Result<(), ResponseInvariantError> {
    if !response.inline_images {
        projection.images.clear();
    }
    if tool == "snapshot_page" {
        project_root_snapshot(&mut projection.result, response.detail)?;
    } else if tool == "inspect_page" {
        project_root_page_state(&mut projection.result, response.detail)?;
    }
    Ok(())
}

fn project_root_snapshot(
    value: &mut Value,
    detail: ResponseDetail,
) -> Result<(), ResponseInvariantError> {
    match detail {
        ResponseDetail::Full => {}
        ResponseDetail::Concise => {
            *value = serde_json::to_value(concise_snapshot(
                &serde_json::from_value(value.clone()).map_err(|_| ResponseInvariantError)?,
            )?)
            .map_err(|_| ResponseInvariantError)?
        }
        ResponseDetail::Expanded => {
            *value = serde_json::to_value(expanded_snapshot(
                &serde_json::from_value(value.clone()).map_err(|_| ResponseInvariantError)?,
            )?)
            .map_err(|_| ResponseInvariantError)?
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
) -> Result<(), ResponseInvariantError> {
    let Some(value) = part.get_mut("available") else {
        return Ok(());
    };
    project_root_snapshot(value, detail)
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
                "version": effective.get("version"),
                "artifact_anchor": effective.get("artifact_anchor"),
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
        "capture_quality": capture.map(|capture| json!({
            "requested_range": capture.get("requested_range"),
            "retained_range": capture.get("retained_range"),
            "frame_count": capture.get("frame_count"),
            "cadence": capture.get("cadence"),
            "gap_summary": capture.get("gap_summary"),
            "retention_warnings": capture.get("retention_warnings"),
            "warnings": capture.get("warnings"),
        })),
        "browser_events": events.map(|events| json!({
            "effective_range": events.get("effective_range"),
            "matched_count": events.get("matched_count"),
            "returned_count": events.get("returned_count"),
            "next_cursor": events.get("next_cursor"),
            "collection_gap_count": events.get("collection_gaps").and_then(Value::as_array).map_or(0, Vec::len),
            "unavailable_range_count": events.get("unavailable_ranges").and_then(Value::as_array).map_or(0, Vec::len),
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
    Ok(json!({
        "range": object.get("range"),
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
        ResponseDetail::Full => {
            serde_json::to_value(&bundle).map_err(|_| ResponseInvariantError)?
        }
        ResponseDetail::Concise | ResponseDetail::Expanded => compact_bundle_value(&bundle, scope)?,
    };
    let mut projection = Projection::success(result);
    let mut candidate: Option<(u8, u32, u32, ArtifactHandle)> = None;
    if let krometrail_core::BundleArtifactEvidence::Available(generation) = &bundle.artifacts {
        add_artifact_generation_resources(&mut projection, generation, scope, true)?;
        for outcome in &generation.outcomes {
            let ArtifactOutcome::Available {
                epoch_index,
                generator_index,
                artifact,
            } = outcome
            else {
                continue;
            };
            let rank = artifact_kind_rank(artifact.manifest.artifact_kind());
            if rank >= 3 {
                continue;
            }
            let key = (rank, *epoch_index, *generator_index, artifact.artifact_id);
            if candidate.as_ref().is_none_or(
                |(old_rank, old_epoch, old_generator, old_artifact)| {
                    key < (
                        *old_rank,
                        *old_epoch,
                        *old_generator,
                        old_artifact.artifact_id,
                    )
                },
            ) {
                candidate = Some((rank, *epoch_index, *generator_index, artifact.clone()));
            }
        }
    }
    if response.inline_images
        && let Some((_, _, _, artifact)) = candidate
    {
        match read_inline_artifact(
            scope,
            artifact.artifact_id,
            progressive,
            deadline,
            &cancellation,
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
    }
    project_temporal_value(&mut projection.result, response.detail)?;
    project_response(tool, &mut projection, response)?;
    Ok(mapped(tool, projection, format!("{tool} succeeded")))
}

pub(crate) fn map_progressive_result(
    tool: &str,
    result: ProgressiveEvidenceResult,
    response: ResponseRequest,
) -> Result<MappedResult, ResponseInvariantError> {
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
            let frames = if response.detail == ResponseDetail::Concise {
                list.frames
                    .iter()
                    .take(MAX_CONCISE_TARGETS)
                    .cloned()
                    .collect()
            } else {
                list.frames.clone()
            };
            let omitted_frame_count = list.frames.len().saturating_sub(frames.len());
            let mut projection = Projection::success(json!({
                "range": list.range,
                "frames": frames,
                "omitted_frame_count": omitted_frame_count,
            }));
            for frame in &list.frames {
                add_source_frame_resource(&mut projection, frame)?;
            }
            projection
        }
        ProgressiveEvidenceResult::FetchSourceFrames(batch) => {
            project_source_frame_batch(*batch, response.inline_images)?
        }
        ProgressiveEvidenceResult::GenerateArtifacts(generation) => {
            let generation = *generation;
            let scope = artifact_scope(&generation.range)?;
            let mut projection = if response.detail == ResponseDetail::Concise {
                Projection::success(compact_generation_value(&generation, scope)?)
            } else {
                serializable(generation.clone())?
            };
            add_artifact_generation_resources(&mut projection, &generation, scope, false)?;
            projection
        }
        ProgressiveEvidenceResult::GenerateRegionFilmstrip(evidence) => {
            let evidence = *evidence;
            let region = evidence.region;
            let generation = evidence.generation;
            let scope = artifact_scope(&generation.range)?;
            let generation_for_links = generation.clone();
            let generation_value = if response.detail == ResponseDetail::Concise {
                compact_generation_value(&generation, scope)?
            } else {
                serde_json::to_value(&generation).map_err(|_| ResponseInvariantError)?
            };
            let mut projection = Projection::success(json!({
                "region": region,
                "generation": generation_value,
            }));
            add_artifact_generation_resources(
                &mut projection,
                &generation_for_links,
                scope,
                false,
            )?;
            projection
        }
        ProgressiveEvidenceResult::PinResolvedRange(change)
        | ProgressiveEvidenceResult::UnpinResolvedRange(change) => serializable(*change)?,
        ProgressiveEvidenceResult::QueryPinState(state) => serializable(*state)?,
    };
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

fn compact_bundle_value(
    bundle: &TemporalDebugBundle,
    scope: krometrail_core::EvidenceScope,
) -> Result<Value, ResponseInvariantError> {
    let mut value = serde_json::to_value(bundle).map_err(|_| ResponseInvariantError)?;
    let Some(outcomes) = value
        .get_mut("artifacts")
        .and_then(|artifacts| artifacts.get_mut("outcomes"))
        .and_then(Value::as_array_mut)
    else {
        if matches!(
            bundle.artifacts,
            krometrail_core::BundleArtifactEvidence::Unavailable { .. }
        ) {
            return Ok(value);
        }
        return Err(ResponseInvariantError);
    };
    let krometrail_core::BundleArtifactEvidence::Available(generation) = &bundle.artifacts else {
        return Err(ResponseInvariantError);
    };
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
        *artifact_value = serde_json::to_value(compact_artifact_handle(scope, artifact)?)
            .map_err(|_| ResponseInvariantError)?;
    }
    Ok(value)
}

fn compact_generation_value(
    generation: &ArtifactGenerationResult,
    scope: krometrail_core::EvidenceScope,
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
                    "artifact": compact_artifact_handle(scope, artifact)?,
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
    Ok(json!({
        "range": generation.range,
        "outcomes": outcomes,
    }))
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
        selected_frame_count: u32::try_from(manifest.selected_frame_ids().len())
            .map_err(|_| ResponseInvariantError)?,
        omitted_frame_count: u32::try_from(manifest.omitted_frame_count())
            .map_err(|_| ResponseInvariantError)?,
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
    inline_images: bool,
) -> Result<Projection, ResponseInvariantError> {
    let mut projection = Projection::success(json!({
        "range": batch.range,
        "frames": batch.frames.iter().map(|frame| &frame.handle).collect::<Vec<_>>(),
    }));
    let mut inline_bytes = 0_u64;
    for (index, frame) in batch.frames.into_iter().enumerate() {
        add_source_frame_resource(&mut projection, &frame.handle)?;
        if !inline_images {
            continue;
        }
        let frame_bytes = frame.encoded_bytes();
        let length = frame_bytes.len() as u64;
        if index >= 4
            || length > 4 * 1024 * 1024
            || inline_bytes.saturating_add(length) > 16 * 1024 * 1024
        {
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
        DeviceScaleFactor, EveryNthFrame, FrameId, ImageFormat, InteractionId, InteractionTiming,
        NodeReference, ObservationContext, ObservedTime, PageChange, PageOperationResult,
        PageSelection, PageSnapshot, PixelDimensions, PresentationRange, PresentationTime,
        RangeResolutionOptions, ResolvedRange, ScreenshotTarget, SessionId, SessionRange,
        SessionTime, Sha256Digest, SnapshotGeneration, SnapshotNode, SnapshotNodeId,
        SourceFrameRead, TargetCaptureStatus, TargetId, TemporalRangeAnchorKind,
        TemporalVideoGenerationClip, TemporalVideoManifest, VideoArtifactEvidenceHandle,
        VideoEncodedClip, VideoEncoderIdentity, VideoEncodingProfile, VideoOutputGeometry,
        VideoPresentationPlan, VideoPresentationSegment, VideoSegmentSource, VideoTimingBasis,
        VisualEpoch, WaitCondition, WaitProbe, WaitRequest, WaitResult,
    };
    use std::time::Duration;

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
        let result = into_call_tool_result(mapped).unwrap();
        let links = result
            .content
            .iter()
            .filter(|content| serde_json::to_value(content).unwrap()["type"] == "resource_link")
            .count();
        assert_eq!(links, 4);
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
        });
        PageSnapshot::new(context(), generation, nodes, 7).unwrap()
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

    fn source_frame_batch(frame_count: u32) -> SourceFrameBatch {
        let range = video_result().range;
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
                inline_images: true,
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
        assert!(response.inline_images);

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
            ],
            actionable: true,
            reference: Some(NodeReference {
                target_id: target_id(),
                generation,
                node_id: focused_id,
            }),
        });
        let snapshot = PageSnapshot::new(context(), generation, nodes, 9).unwrap();
        let concise = concise_snapshot(&snapshot).unwrap();
        assert!(concise.targets.len() <= MAX_CONCISE_TARGETS);
        assert!(
            serde_json::to_vec(&concise.targets).unwrap().len() <= MAX_CONCISE_TARGET_JSON_BYTES
        );
        assert_eq!(concise.targets[0].reference.node_id, focused_id);
        assert_eq!(concise.omissions.source_nodes, 9);
        assert_eq!(concise.omissions.presentation_targets, 32);
    }

    #[test]
    fn expanded_snapshot_complete_json_stays_within_its_budget() {
        let expanded = expanded_snapshot(&complex_snapshot()).unwrap();
        let encoded = serde_json::to_vec(&expanded).unwrap();
        assert!(encoded.len() <= MAX_EXPANDED_SNAPSHOT_JSON_BYTES);
        assert!(expanded.omissions.presentation_context_nodes > 0);
    }

    #[test]
    fn source_frame_inline_limits_apply_only_when_pixels_are_requested() {
        let without = map_progressive_result(
            "fetch_source_frames",
            ProgressiveEvidenceResult::FetchSourceFrames(Box::new(source_frame_batch(5))),
            ResponseRequest::default(),
        )
        .unwrap();
        assert_eq!(without.response.status, ToolResponseStatus::Succeeded);
        assert!(without.response.warnings.is_empty());
        assert!(without.images.is_empty());
        assert_eq!(without.response.resources.len(), 5);

        let with = map_progressive_result(
            "fetch_source_frames",
            ProgressiveEvidenceResult::FetchSourceFrames(Box::new(source_frame_batch(5))),
            ResponseRequest {
                inline_images: true,
                ..ResponseRequest::default()
            },
        )
        .unwrap();
        assert_eq!(with.response.status, ToolResponseStatus::Degraded);
        assert_eq!(
            with.response.warnings[0].code,
            ErrorCode::ResourceLimitExceeded
        );
        assert_eq!(with.images.len(), 4);
        assert_eq!(with.response.resources.len(), 5);
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
                inline_images: false,
            },
        )
        .unwrap();
        let full = map_operation_result_with_capture(
            "snapshot_page",
            operation(),
            &[],
            ResponseRequest {
                detail: ResponseDetail::Full,
                inline_images: false,
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
        assert!(full.response.result.get("nodes").is_some());
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
                inline_images: true,
            },
        )
        .unwrap();
        assert_eq!(without.response.result, with.response.result);
        assert!(without.images.is_empty());
        assert_eq!(with.images.len(), 1);
    }

    #[test]
    fn temporal_detail_defaults_to_concise_and_full_preserves_rows() {
        let value = json!({
            "range": {"scope": "fixture"},
            "capture_quality": {"status": "available", "cadence": {}, "gap_summary": {}, "retention_warnings": [], "warnings": []},
            "browser_events": {"status": "available", "effective_range": {}, "matched_count": 50, "returned_count": 2, "events": [{}, {}], "collection_gaps": [], "unavailable_ranges": [], "warnings": []}
        });
        let mut concise = value.clone();
        project_temporal_value(&mut concise, ResponseDetail::Concise).unwrap();
        assert!(concise["browser_events"].get("events").is_none());
        let mut expanded = value.clone();
        project_temporal_value(&mut expanded, ResponseDetail::Expanded).unwrap();
        assert_eq!(expanded, value);
        let mut full = value.clone();
        project_temporal_value(&mut full, ResponseDetail::Full).unwrap();
        assert_eq!(full, value);
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
                    inline_images: true,
                    ..ResponseRequest::default()
                },
            )
            .unwrap();
            let result = into_call_tool_result(mapped).unwrap();
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
        assert_eq!(page.response.interaction, Some(anchor));
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
            ObservationPart::Unavailable(error(ErrorCode::ScreenshotFailed, "not requested")),
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
            ObservationPart::Unavailable(error(ErrorCode::ScreenshotFailed, "not requested")),
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
        assert_eq!(batch.response.result["steps"].as_array().unwrap().len(), 2);

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
            ObservationPart::Unavailable(error(ErrorCode::ScreenshotFailed, "not requested")),
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
            ObservationPart::Unavailable(error(ErrorCode::ScreenshotFailed, "not requested")),
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
