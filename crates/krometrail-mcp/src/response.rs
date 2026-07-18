use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Instant,
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use krometrail_core::{
    ArtifactCacheDisposition, ArtifactGenerationResult, ArtifactHandle, ArtifactId,
    ArtifactOutcome, BatchOutcome, BatchResult, BrowserOperationResult, EncodedScreenshot,
    ErrorCode, InteractionAnchor, KrometrailError, LiveObservation, NonEmptyText, ObservationPart,
    PageOperationOutcome, PageOperationResult, PageSnapshot, ProgressiveEvidence,
    ProgressiveEvidenceContext, ProgressiveEvidenceRequest, ProgressiveEvidenceResult,
    RetrieveArtifactRequest, ScreenshotMetadata, SourceFrameBatch, SourceFrameHandle, TargetId,
    TemporalDebugBundle, TemporalVideoGenerationResult, VideoPresentationPolicy, WaitOutcome,
};
use rmcp::model::JsonObject;
use rmcp::model::{CallToolResult, Content, RawResource};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use temporal_vision::{ArtifactKind, EvidenceClass, PixelDimensions};

use crate::resources::{ResourceKind, ResourceProjection};

const MAX_AUTOMATIC_SNAPSHOT_NODES: usize = 96;
const MAX_AUTOMATIC_SNAPSHOT_JSON_BYTES: usize = 32 * 1024;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum StructuredResponseDetail {
    #[default]
    Legacy,
    Full,
    Compact,
    Omit,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum InlineImageDetail {
    #[default]
    Inline,
    Omit,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DiagnosticDetail {
    #[default]
    Automatic,
    Omit,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ResponseProjectionRequest {
    #[serde(default)]
    pub inline_images: InlineImageDetail,
    #[serde(default)]
    pub snapshot: StructuredResponseDetail,
    #[serde(default)]
    pub page_state: StructuredResponseDetail,
    #[serde(default)]
    pub diagnostics: DiagnosticDetail,
}

pub(crate) fn split_response_projection(
    mut arguments: JsonObject,
) -> krometrail_core::Result<(JsonObject, ResponseProjectionRequest)> {
    let preference = match arguments.remove("response") {
        None => ResponseProjectionRequest::default(),
        Some(value) => decode_projection(value)?,
    };
    Ok((arguments, preference))
}

fn decode_projection(value: Value) -> krometrail_core::Result<ResponseProjectionRequest> {
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
    map_operation_result_with_capture(tool, result, &[])
}

pub(crate) fn map_operation_result_with_capture(
    tool: &str,
    result: BrowserOperationResult,
    capture_statuses: &[krometrail_core::TargetCaptureStatus],
) -> Result<MappedResult, ResponseInvariantError> {
    map_operation_result_with_capture_projected(
        tool,
        result,
        capture_statuses,
        ResponseProjectionRequest::default(),
    )
}

pub(crate) fn map_operation_result_with_capture_projected(
    tool: &str,
    result: BrowserOperationResult,
    capture_statuses: &[krometrail_core::TargetCaptureStatus],
    preference: ResponseProjectionRequest,
) -> Result<MappedResult, ResponseInvariantError> {
    let mut projection = project_operation(result, preference)?;
    let target_id = projection_target_id(&projection);
    add_capture_warnings(&mut projection, capture_statuses, target_id);
    apply_response_projection(tool, &mut projection, preference)?;
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
    let stage = status
        .failure_stage()
        .expect("failed capture status is validated with a failure stage");
    KrometrailError::from_browser_failure(
        krometrail_core::ErrorCode::CaptureFailed,
        krometrail_core::NonEmptyText::new(format!(
            "current-state control may have succeeded, but retained temporal frames are unavailable after {}",
            stage.as_str()
        ))
        .expect("capture failure warning is non-empty"),
    )
    .with_context(krometrail_core::ErrorContext {
        target_id: Some(status.target_id()),
        ..krometrail_core::ErrorContext::default()
    })
}

pub(crate) fn map_lifecycle_result<T: Serialize>(
    tool: &str,
    value: T,
) -> Result<MappedResult, ResponseInvariantError> {
    let value = serde_json::to_value(value).map_err(|_| ResponseInvariantError)?;
    Ok(mapped(
        tool,
        Projection::success(value),
        format!("{tool} succeeded"),
    ))
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
        let stage = status
            .failure_stage()
            .expect("failed capture status is validated with a failure stage");
        projection.degrade_with_stage(vec![capture_failed_warning(status)], stage.as_str());
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
    preference: ResponseProjectionRequest,
) -> Result<Projection, ResponseInvariantError> {
    match result {
        BrowserOperationResult::InspectPage(value) => serializable(*value),
        BrowserOperationResult::SnapshotPage(value) => serializable(*value),
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
                project_live_observation(*value, ImageRole::LiveObservation, None, preference)?;
            let mut projection = Projection::success(result);
            projection.degrade_with(warnings);
            projection.images.extend(image);
            Ok(projection)
        }
        BrowserOperationResult::ListPages(value) => serializable(*value),
        BrowserOperationResult::CreatePage(value)
        | BrowserOperationResult::SelectPage(value)
        | BrowserOperationResult::ClosePage(value)
        | BrowserOperationResult::NavigatePage(value)
        | BrowserOperationResult::ReloadPage(value)
        | BrowserOperationResult::GoBack(value)
        | BrowserOperationResult::GoForward(value) => project_page_operation(*value, preference),
        BrowserOperationResult::SetViewport(value) => {
            let mut projection = project_page_operation(value.operation, preference)?;
            let mut warnings = Vec::new();
            let effective = project_serializable_part(value.effective, &mut warnings)?;
            projection
                .result
                .as_object_mut()
                .ok_or(ResponseInvariantError)?
                .insert("effective".to_owned(), effective);
            projection.degrade_with(warnings);
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
                preference,
            )?;
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
        BrowserOperationResult::Batch(value) => project_batch(*value, preference),
    }
}

fn project_page_operation(
    value: PageOperationResult,
    preference: ResponseProjectionRequest,
) -> Result<Projection, ResponseInvariantError> {
    let interaction = value.interaction.clone();
    let (observation, warnings, image) =
        project_live_observation_part(value.observation, ImageRole::PostAction, preference)?;
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
    preference: ResponseProjectionRequest,
) -> Result<Projection, ResponseInvariantError> {
    let mut images = Vec::new();
    let mut step_values = Vec::with_capacity(value.steps.len());
    let mut first_step_error = None;
    let mut step_failure_seen = false;
    for step in value.steps {
        let result = step
            .result
            .map(|result| project_batch_step(result, preference))
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
        project_live_observation_part(value.final_observation, ImageRole::BatchFinal, preference)?;
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
    preference: ResponseProjectionRequest,
) -> Result<Option<Value>, ResponseInvariantError> {
    let mut value = project_operation(result, preference)?.result;
    let Some(object) = value.as_object_mut() else {
        return Ok(Some(value));
    };
    object.remove("observation");
    Ok(Some(value))
}

fn project_live_observation_part(
    value: ObservationPart<LiveObservation>,
    role: ImageRole,
    preference: ResponseProjectionRequest,
) -> Result<(Value, Vec<KrometrailError>, Option<EncodedMcpImage>), ResponseInvariantError> {
    match value {
        ObservationPart::Available(observation) => {
            let (value, warnings, image) =
                project_live_observation(observation, role, None, preference)?;
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
    preference: ResponseProjectionRequest,
) -> Result<(Value, Vec<KrometrailError>, Option<EncodedMcpImage>), ResponseInvariantError> {
    let mut warnings = Vec::new();
    let mut page = project_serializable_part(value.page, &mut warnings)?;
    apply_structured_part(&mut page, preference.page_state, compact_page_state)?;
    let snapshot = match value.snapshot {
        ObservationPart::Available(snapshot)
            if preference.snapshot == StructuredResponseDetail::Legacy
                && matches!(&role, ImageRole::PostAction | ImageRole::BatchFinal) =>
        {
            ObservationPart::Available(compact_snapshot(snapshot)?)
        }
        ObservationPart::Available(snapshot)
            if preference.snapshot == StructuredResponseDetail::Compact =>
        {
            ObservationPart::Available(compact_snapshot(snapshot)?)
        }
        snapshot => snapshot,
    };
    let mut snapshot = project_serializable_part(snapshot, &mut warnings)?;
    apply_structured_part(&mut snapshot, preference.snapshot, compact_snapshot_value)?;
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

fn compact_snapshot(snapshot: PageSnapshot) -> Result<PageSnapshot, ResponseInvariantError> {
    let full_json_bytes = serde_json::to_vec(&snapshot.nodes)
        .map_err(|_| ResponseInvariantError)?
        .len();
    if snapshot.nodes.len() <= MAX_AUTOMATIC_SNAPSHOT_NODES
        && full_json_bytes <= MAX_AUTOMATIC_SNAPSHOT_JSON_BYTES
    {
        return Ok(snapshot);
    }

    let parents: HashMap<_, _> = snapshot
        .nodes
        .iter()
        .map(|node| (node.id, node.parent))
        .collect();
    let mut priority = HashSet::new();
    for node in snapshot.nodes.iter().filter(|node| node.actionable) {
        let mut current = Some(node.id);
        while let Some(node_id) = current {
            if !priority.insert(node_id) {
                break;
            }
            current = parents.get(&node_id).copied().flatten();
        }
    }

    let mut selected = vec![false; snapshot.nodes.len()];
    let mut selected_ids = HashSet::new();
    let mut selected_count = 0;
    let mut serialized_bytes = 2; // JSON array brackets.
    for (index, node) in snapshot.nodes.iter().enumerate() {
        if priority.contains(&node.id)
            && node
                .parent
                .is_none_or(|parent| selected_ids.contains(&parent))
        {
            let Some(next_bytes) =
                automatic_snapshot_bytes_after(node, selected_count, serialized_bytes)?
            else {
                continue;
            };
            selected[index] = true;
            selected_ids.insert(node.id);
            selected_count += 1;
            serialized_bytes = next_bytes;
        }
    }
    for (index, node) in snapshot.nodes.iter().enumerate() {
        if selected[index]
            || !node
                .parent
                .is_none_or(|parent| selected_ids.contains(&parent))
        {
            continue;
        }
        let Some(next_bytes) =
            automatic_snapshot_bytes_after(node, selected_count, serialized_bytes)?
        else {
            continue;
        };
        selected[index] = true;
        selected_ids.insert(node.id);
        selected_count += 1;
        serialized_bytes = next_bytes;
    }

    let original_node_count = snapshot.nodes.len();
    let nodes = snapshot
        .nodes
        .into_iter()
        .zip(selected)
        .filter_map(|(node, selected)| selected.then_some(node))
        .collect::<Vec<_>>();
    let presentation_omissions =
        u32::try_from(original_node_count - nodes.len()).map_err(|_| ResponseInvariantError)?;
    let omitted_node_count = snapshot
        .omitted_node_count
        .checked_add(presentation_omissions)
        .ok_or(ResponseInvariantError)?;
    PageSnapshot::new(
        snapshot.context,
        snapshot.generation,
        nodes,
        omitted_node_count,
    )
    .map_err(|_| ResponseInvariantError)
}

fn apply_response_projection(
    tool: &str,
    projection: &mut Projection,
    preference: ResponseProjectionRequest,
) -> Result<(), ResponseInvariantError> {
    if preference.inline_images == InlineImageDetail::Omit {
        projection.images.clear();
    }

    if tool == "snapshot_page" {
        apply_root_structured_detail(
            &mut projection.result,
            preference.snapshot,
            compact_snapshot_value,
        )?;
    } else if tool == "inspect_page" {
        apply_root_structured_detail(
            &mut projection.result,
            preference.page_state,
            compact_page_state,
        )?;
    }

    Ok(())
}

fn apply_root_structured_detail(
    value: &mut Value,
    detail: StructuredResponseDetail,
    compact: fn(&Value) -> Result<Value, ResponseInvariantError>,
) -> Result<(), ResponseInvariantError> {
    match detail {
        StructuredResponseDetail::Legacy | StructuredResponseDetail::Full => Ok(()),
        StructuredResponseDetail::Compact => {
            *value = compact(value)?;
            Ok(())
        }
        StructuredResponseDetail::Omit => {
            *value = projection_omitted_part();
            Ok(())
        }
    }
}

fn apply_structured_part(
    part: &mut Value,
    detail: StructuredResponseDetail,
    compact: fn(&Value) -> Result<Value, ResponseInvariantError>,
) -> Result<(), ResponseInvariantError> {
    match detail {
        StructuredResponseDetail::Legacy | StructuredResponseDetail::Full => Ok(()),
        StructuredResponseDetail::Omit if part.get("available").is_some() => {
            *part = projection_omitted_part();
            Ok(())
        }
        StructuredResponseDetail::Omit => Ok(()),
        StructuredResponseDetail::Compact => {
            let Some(value) = part.get_mut("available") else {
                return Ok(());
            };
            *value = compact(value)?;
            Ok(())
        }
    }
}

fn projection_omitted_part() -> Value {
    json!({"omitted": {"reason": "response_projection"}})
}

fn compact_page_state(value: &Value) -> Result<Value, ResponseInvariantError> {
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
    let mut compact = serde_json::Map::new();
    for key in retained {
        if let Some(value) = object.get(key) {
            compact.insert(key.to_owned(), value.clone());
        }
    }
    if compact.is_empty() {
        Ok(value.clone())
    } else {
        Ok(Value::Object(compact))
    }
}

fn compact_snapshot_value(value: &Value) -> Result<Value, ResponseInvariantError> {
    let snapshot: PageSnapshot =
        serde_json::from_value(value.clone()).map_err(|_| ResponseInvariantError)?;
    serde_json::to_value(compact_snapshot(snapshot)?).map_err(|_| ResponseInvariantError)
}

fn compact_temporal_value(value: &Value) -> Result<Value, ResponseInvariantError> {
    let Some(bundle) = value.as_object() else {
        return Ok(value.clone());
    };
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

fn automatic_snapshot_bytes_after(
    node: &krometrail_core::SnapshotNode,
    selected_count: usize,
    serialized_bytes: usize,
) -> Result<Option<usize>, ResponseInvariantError> {
    if selected_count >= MAX_AUTOMATIC_SNAPSHOT_NODES {
        return Ok(None);
    }
    let node_bytes = serde_json::to_vec(node)
        .map_err(|_| ResponseInvariantError)?
        .len();
    let separator_bytes = usize::from(selected_count != 0);
    let next_bytes = serialized_bytes
        .checked_add(separator_bytes)
        .and_then(|bytes| bytes.checked_add(node_bytes))
        .ok_or(ResponseInvariantError)?;
    Ok((next_bytes <= MAX_AUTOMATIC_SNAPSHOT_JSON_BYTES).then_some(next_bytes))
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
) -> Result<MappedResult, ResponseInvariantError> {
    map_temporal_bundle_result_projected(
        tool,
        bundle,
        progressive,
        deadline,
        cancellation,
        ResponseProjectionRequest::default(),
    )
    .await
}

pub(crate) async fn map_temporal_bundle_result_projected(
    tool: &str,
    bundle: TemporalDebugBundle,
    progressive: &dyn ProgressiveEvidence,
    deadline: Instant,
    cancellation: Arc<dyn krometrail_core::CancellationSignal>,
    preference: ResponseProjectionRequest,
) -> Result<MappedResult, ResponseInvariantError> {
    let scope = artifact_scope(&bundle.range)?;
    let mut projection = Projection::success(compact_bundle_value(&bundle, scope)?);
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
    if let Some((_, _, _, artifact)) = candidate {
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
    if matches!(preference.snapshot, StructuredResponseDetail::Compact)
        || matches!(preference.page_state, StructuredResponseDetail::Compact)
    {
        projection.result = compact_temporal_value(&projection.result)?;
    }
    apply_response_projection(tool, &mut projection, preference)?;
    Ok(mapped(tool, projection, format!("{tool} succeeded")))
}

pub(crate) fn map_progressive_result(
    tool: &str,
    result: ProgressiveEvidenceResult,
) -> Result<MappedResult, ResponseInvariantError> {
    let projection = match result {
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
            let mut projection = Projection::success(json!({
                "range": list.range,
                "frames": list.frames,
            }));
            for frame in &list.frames {
                add_source_frame_resource(&mut projection, frame)?;
            }
            projection
        }
        ProgressiveEvidenceResult::FetchSourceFrames(batch) => project_source_frame_batch(*batch)?,
        ProgressiveEvidenceResult::GenerateArtifacts(generation) => {
            let generation = *generation;
            let scope = artifact_scope(&generation.range)?;
            let mut projection = serializable(generation.clone())?;
            add_artifact_generation_resources(&mut projection, &generation, scope, false)?;
            projection
        }
        ProgressiveEvidenceResult::GenerateRegionFilmstrip(evidence) => {
            let evidence = *evidence;
            let region = evidence.region;
            let generation = evidence.generation;
            let scope = artifact_scope(&generation.range)?;
            let generation_for_links = generation.clone();
            let mut projection = Projection::success(json!({
                "region": region,
                "generation": generation,
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
) -> Result<Projection, ResponseInvariantError> {
    let mut projection = Projection::success(json!({
        "range": batch.range,
        "frames": batch.frames.iter().map(|frame| &frame.handle).collect::<Vec<_>>(),
    }));
    let mut inline_bytes = 0_u64;
    for (index, frame) in batch.frames.into_iter().enumerate() {
        add_source_frame_resource(&mut projection, &frame.handle)?;
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
        BatchSkipReason, BatchStepResult, BatchStepStatus, BrowserOperationKind,
        CaptureFailureStage, CaptureStatistics, CaptureStreamState, CaptureTimingSummary, CssPoint,
        CssRect, CssSize, DeviceScaleFactor, ErrorContext, EveryNthFrame, FrameId, ImageFormat,
        InteractionId, InteractionTiming, NodeReference, ObservationContext, PageChange,
        PageSelection, PageSnapshot, PixelDimensions, PresentationRange, PresentationTime,
        RangeResolutionOptions, ResolvedRange, ScreenshotTarget, SessionId, SessionRange,
        SessionTime, Sha256Digest, SnapshotGeneration, SnapshotNode, SnapshotNodeId,
        TargetCaptureStatus, TargetId, TemporalRangeAnchorKind, TemporalVideoGenerationClip,
        TemporalVideoManifest, VideoArtifactEvidenceHandle, VideoEncodedClip, VideoEncoderIdentity,
        VideoEncodingProfile, VideoOutputGeometry, VideoPresentationPlan, VideoPresentationSegment,
        VideoSegmentSource, VideoTimingBasis, VisualEpoch, WaitCondition, WaitProbe, WaitRequest,
        WaitResult,
    };
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        time::Duration,
    };

    #[derive(Clone)]
    struct EventCounter(Arc<AtomicUsize>);

    impl tracing::Subscriber for EventCounter {
        fn enabled(&self, _metadata: &tracing::Metadata<'_>) -> bool {
            true
        }

        fn new_span(&self, _span: &tracing::span::Attributes<'_>) -> tracing::span::Id {
            tracing::span::Id::from_u64(1)
        }

        fn record(&self, _span: &tracing::span::Id, _values: &tracing::span::Record<'_>) {}

        fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {}

        fn event(&self, _event: &tracing::Event<'_>) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }

        fn enter(&self, _span: &tracing::span::Id) {}

        fn exit(&self, _span: &tracing::span::Id) {}
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

    fn failed_capture_for(target_id: TargetId) -> TargetCaptureStatus {
        TargetCaptureStatus::new_with_failure_stage(
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
            Some(CaptureFailureStage::FramePersistence),
        )
        .unwrap()
    }

    fn failed_capture() -> TargetCaptureStatus {
        failed_capture_for(target_id())
    }

    #[test]
    fn failed_capture_degrades_success_without_removing_current_image() {
        let mapped = map_operation_result_with_capture(
            "take_screenshot",
            BrowserOperationResult::TakeScreenshot(Box::new(screenshot(ImageFormat::Png))),
            &[failed_capture(), failed_capture()],
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
        assert!(mapped.response.error.is_none());
    }

    #[test]
    fn failed_capture_on_another_target_does_not_degrade_page_result() {
        let other = TargetId::from_uuid(uuid::Uuid::from_u128(99));
        let mapped = map_operation_result_with_capture(
            "take_screenshot",
            BrowserOperationResult::TakeScreenshot(Box::new(screenshot(ImageFormat::Png))),
            &[failed_capture_for(other)],
        )
        .unwrap();
        assert_eq!(mapped.response.status, ToolResponseStatus::Succeeded);
        assert!(mapped.response.warnings.is_empty());
    }

    #[test]
    fn equivalent_warnings_are_logged_and_retained_once_without_collapsing_same_code() {
        let first = error(
            ErrorCode::PageObservationFailed,
            "dialog blocked observation",
        );
        let distinct = first.clone().with_context(ErrorContext {
            target_id: Some(target_id()),
            ..ErrorContext::default()
        });
        let events = Arc::new(AtomicUsize::new(0));
        let counter = EventCounter(Arc::clone(&events));
        let mut projection = Projection::success(json!({}));
        tracing::subscriber::with_default(counter, || {
            projection.degrade_with_stage(
                vec![
                    first.clone(),
                    first.clone(),
                    distinct.clone(),
                    first.clone(),
                ],
                "live_observation",
            );
        });

        assert_eq!(projection.warnings, vec![first, distinct]);
        assert_eq!(events.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn automatic_live_observations_bound_complex_snapshots_with_exact_omissions() {
        let full = complex_snapshot();
        for role in [ImageRole::PostAction, ImageRole::BatchFinal] {
            let (value, _, _) = project_live_observation(
                live_with_snapshot(full.clone()),
                role,
                None,
                ResponseProjectionRequest::default(),
            )
            .unwrap();
            let compact: PageSnapshot =
                serde_json::from_value(value["snapshot"]["available"].clone()).unwrap();
            assert!(compact.nodes.len() <= MAX_AUTOMATIC_SNAPSHOT_NODES);
            assert!(
                serde_json::to_vec(&compact.nodes).unwrap().len()
                    <= MAX_AUTOMATIC_SNAPSHOT_JSON_BYTES
            );
            assert_eq!(
                compact.omitted_node_count,
                full.omitted_node_count
                    + u32::try_from(full.nodes.len() - compact.nodes.len()).unwrap()
            );
            let action = compact
                .nodes
                .iter()
                .find(|node| node.actionable)
                .expect("the late actionable node is prioritized");
            let group = compact
                .nodes
                .iter()
                .find(|node| Some(node.id) == action.parent)
                .expect("the action's parent is retained");
            assert!(
                compact
                    .nodes
                    .iter()
                    .any(|node| Some(node.id) == group.parent)
            );
            PageSnapshot::new(
                compact.context.clone(),
                compact.generation,
                compact.nodes.clone(),
                compact.omitted_node_count,
            )
            .unwrap();
        }
    }

    #[test]
    fn explicit_snapshot_and_live_observation_keep_full_snapshots() {
        let full = complex_snapshot();
        let snapshot = map_operation_result(
            "snapshot_page",
            BrowserOperationResult::SnapshotPage(Box::new(full.clone())),
        )
        .unwrap();
        assert_eq!(
            snapshot.response.result["nodes"].as_array().unwrap().len(),
            403
        );

        let observed = map_operation_result(
            "observe_live",
            BrowserOperationResult::ObserveLive(Box::new(live_with_snapshot(full))),
        )
        .unwrap();
        assert_eq!(
            observed.response.result["snapshot"]["available"]["nodes"]
                .as_array()
                .unwrap()
                .len(),
            403
        );
    }

    #[test]
    fn automatic_snapshot_below_limits_is_byte_equivalent() {
        let full = complex_snapshot();
        let small = PageSnapshot::new(
            full.context,
            full.generation,
            full.nodes.into_iter().take(3).collect(),
            full.omitted_node_count,
        )
        .unwrap();
        let before = serde_json::to_vec(&small).unwrap();
        let after = serde_json::to_vec(&compact_snapshot(small).unwrap()).unwrap();
        assert_eq!(after, before);
    }

    #[test]
    fn response_projection_validates_without_disclosing_supplied_values() {
        let (remaining, preference) = split_response_projection(
            serde_json::json!({
                "target": "kept",
                "response": {
                    "inline_images": "omit",
                    "snapshot": "compact",
                    "page_state": "full",
                    "diagnostics": "omit"
                }
            })
            .as_object()
            .unwrap()
            .clone(),
        )
        .unwrap();
        assert_eq!(remaining["target"], "kept");
        assert_eq!(preference.inline_images, InlineImageDetail::Omit);
        assert_eq!(preference.snapshot, StructuredResponseDetail::Compact);
        assert_eq!(preference.page_state, StructuredResponseDetail::Full);
        assert_eq!(preference.diagnostics, DiagnosticDetail::Omit);

        let secret = "must-not-be-echoed";
        let error = split_response_projection(
            serde_json::json!({"response": {"snapshot": secret}})
                .as_object()
                .unwrap()
                .clone(),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidInput);
        assert!(error.message.as_str().contains("response.snapshot"));
        assert!(!error.message.as_str().contains(secret));
    }

    #[test]
    fn explicit_snapshot_details_preserve_full_compact_and_omitted_truth() {
        let full = complex_snapshot();
        let observe = |detail| {
            project_live_observation(
                live_with_snapshot(full.clone()),
                ImageRole::PostAction,
                None,
                ResponseProjectionRequest {
                    snapshot: detail,
                    ..ResponseProjectionRequest::default()
                },
            )
            .unwrap()
            .0
        };
        assert_eq!(
            observe(StructuredResponseDetail::Full)["snapshot"]["available"]["nodes"]
                .as_array()
                .unwrap()
                .len(),
            403
        );
        let compact = observe(StructuredResponseDetail::Compact);
        assert!(
            compact["snapshot"]["available"]["nodes"]
                .as_array()
                .unwrap()
                .len()
                <= MAX_AUTOMATIC_SNAPSHOT_NODES
        );
        assert_eq!(
            observe(StructuredResponseDetail::Omit)["snapshot"],
            projection_omitted_part()
        );
    }

    #[test]
    fn omitted_inline_images_retain_screenshot_metadata_in_result() {
        let mapped = map_operation_result_with_capture_projected(
            "take_screenshot",
            BrowserOperationResult::TakeScreenshot(Box::new(screenshot(ImageFormat::Png))),
            &[],
            ResponseProjectionRequest {
                inline_images: InlineImageDetail::Omit,
                ..ResponseProjectionRequest::default()
            },
        )
        .unwrap();
        assert!(mapped.response.images.is_empty());
        assert!(mapped.images.is_empty());
        assert_eq!(mapped.response.result["image"]["width"], 10);
    }

    #[test]
    fn compact_temporal_projection_removes_repeated_rows_but_keeps_drill_down_facts() {
        let value = json!({
            "requested_query": {"verbose": "x".repeat(32_000)},
            "range": {"session_id": session_id(), "target_id": target_id(), "frame_ids": [FrameId::from_uuid(uuid::Uuid::from_u128(8))]},
            "effective": {"version": "v1", "artifact_anchor": 4, "artifact_generators": [{}, {}], "focus_times": [4]},
            "header": {"summary": "observed"},
            "markers": [{"id": 1}, {"id": 2}],
            "artifacts": {"status": "available", "range": {"repeated": true}, "outcomes": [{"artifact": {"manifest_uri": "krometrail://manifest"}}]},
            "context": {"status": "available", "range": {"repeated": true}, "capture_quality": {"requested_range": {"start": 0, "end": 5}, "retained_range": {"start": 0, "end": 5}, "frame_count": 1, "cadence": null, "gap_summary": {"gap_count": 0}, "retention_warnings": [], "warnings": [], "large": "x".repeat(32_000)}, "browser_events": {"effective_range": {"start": 0, "end": 5}, "matched_count": 50, "returned_count": 4, "next_cursor": null, "collection_gaps": [], "unavailable_ranges": [], "warnings": [], "events": ["x".repeat(32_000)]}},
            "warnings": [],
            "degradations": []
        });
        let compact = compact_temporal_value(&value).unwrap();
        assert_eq!(compact["marker_count"], 2);
        assert_eq!(compact["context"]["browser_events"]["matched_count"], 50);
        assert_eq!(
            compact["artifacts"]["outcomes"][0]["artifact"]["manifest_uri"],
            "krometrail://manifest"
        );
        assert!(compact["artifacts"].get("range").is_none());
        assert!(serde_json::to_vec(&compact).unwrap().len() < 4_096);
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
            let mapped = map_operation_result(
                "take_screenshot",
                BrowserOperationResult::TakeScreenshot(Box::new(screenshot(format))),
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
