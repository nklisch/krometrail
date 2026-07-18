use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Instant,
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use krometrail_core::{
    ArtifactGenerationResult, ArtifactHandle, ArtifactId, ArtifactOutcome, BatchOutcome,
    BatchResult, BrowserOperationResult, EncodedScreenshot, ErrorCode, InteractionAnchor,
    KrometrailError, LiveObservation, NonEmptyText, ObservationPart, PageOperationOutcome,
    PageOperationResult, PageSnapshot, ProgressiveEvidence, ProgressiveEvidenceContext,
    ProgressiveEvidenceRequest, ProgressiveEvidenceResult, RetrieveArtifactRequest,
    ScreenshotMetadata, SourceFrameBatch, SourceFrameHandle, TemporalDebugBundle, WaitOutcome,
};
use rmcp::model::{CallToolResult, Content, RawResource};
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::{Value, json};
use temporal_vision::ArtifactKind;

use crate::resources::{ResourceKind, ResourceProjection};

const MAX_AUTOMATIC_SNAPSHOT_NODES: usize = 96;
const MAX_AUTOMATIC_SNAPSHOT_JSON_BYTES: usize = 32 * 1024;

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
    SourceFrame,
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
        for warning in &warnings {
            tracing::warn!(
                event = "mcp.response.degraded",
                failure_stage,
                error_code = warning.code.as_str(),
                "mcp.response.degraded"
            );
        }
        self.warnings.extend(warnings);
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
    let mut projection = project_operation(result)?;
    add_capture_warnings(&mut projection, capture_statuses);
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
    projection.fail_with(error);
    add_capture_warnings(&mut projection, capture_statuses);
    into_call_tool_result(mapped(tool, projection, summary))
        .expect("stable error envelopes always serialize")
}

fn add_capture_warnings(
    projection: &mut Projection,
    capture_statuses: &[krometrail_core::TargetCaptureStatus],
) {
    for status in capture_statuses
        .iter()
        .filter(|status| status.state() == krometrail_core::CaptureStreamState::Failed)
    {
        let stage = status
            .failure_stage()
            .expect("failed capture status is validated with a failure stage");
        projection.degrade_with_stage(vec![capture_failed_warning(status)], stage.as_str());
    }
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

fn project_operation(result: BrowserOperationResult) -> Result<Projection, ResponseInvariantError> {
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
                project_live_observation(*value, ImageRole::LiveObservation, None)?;
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
        | BrowserOperationResult::GoForward(value) => project_page_operation(*value),
        BrowserOperationResult::SetViewport(value) => {
            let mut projection = project_page_operation(value.operation)?;
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
            let (observation, warnings, image) =
                project_live_observation(value.observation, ImageRole::PostAction, None)?;
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
        BrowserOperationResult::Batch(value) => project_batch(*value),
    }
}

fn project_page_operation(
    value: PageOperationResult,
) -> Result<Projection, ResponseInvariantError> {
    let interaction = value.interaction.clone();
    let (observation, warnings, image) =
        project_live_observation_part(value.observation, ImageRole::PostAction)?;
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

fn project_batch(value: BatchResult) -> Result<Projection, ResponseInvariantError> {
    let mut images = Vec::new();
    let mut step_values = Vec::with_capacity(value.steps.len());
    let mut first_step_error = None;
    let mut step_failure_seen = false;
    for step in value.steps {
        let result = step
            .result
            .map(project_operation)
            .transpose()?
            .map(|projection| projection.result);
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
        project_live_observation_part(value.final_observation, ImageRole::BatchFinal)?;
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

fn project_live_observation_part(
    value: ObservationPart<LiveObservation>,
    role: ImageRole,
) -> Result<(Value, Vec<KrometrailError>, Option<EncodedMcpImage>), ResponseInvariantError> {
    match value {
        ObservationPart::Available(observation) => {
            let (value, warnings, image) = project_live_observation(observation, role, None)?;
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
) -> Result<(Value, Vec<KrometrailError>, Option<EncodedMcpImage>), ResponseInvariantError> {
    let mut warnings = Vec::new();
    let page = project_serializable_part(value.page, &mut warnings)?;
    let snapshot = match value.snapshot {
        ObservationPart::Available(snapshot)
            if matches!(&role, ImageRole::PostAction | ImageRole::BatchFinal) =>
        {
            ObservationPart::Available(compact_automatic_snapshot(snapshot)?)
        }
        snapshot => snapshot,
    };
    let snapshot = project_serializable_part(snapshot, &mut warnings)?;
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

fn compact_automatic_snapshot(
    snapshot: PageSnapshot,
) -> Result<PageSnapshot, ResponseInvariantError> {
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
    let mut projection = serializable(bundle.clone())?;
    let scope = artifact_scope(&bundle.range)?;
    let mut candidate: Option<(u8, u32, u32, ArtifactHandle)> = None;
    if let krometrail_core::BundleArtifactEvidence::Available(generation) = &bundle.artifacts {
        add_artifact_generation_resources(&mut projection, generation, scope)?;
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
            add_artifact_generation_resources(&mut projection, &generation, scope)?;
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
            add_artifact_generation_resources(&mut projection, &generation_for_links, scope)?;
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
) -> Result<(), ResponseInvariantError> {
    for outcome in &generation.outcomes {
        if let ArtifactOutcome::Available { artifact, .. } = outcome {
            add_resource(projection, artifact_resource(scope, artifact)?)?;
        }
    }
    Ok(())
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
        ResourceKind::SourceFrame => ResourceRole::SourceFrame,
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
        CssRect, CssSize, DeviceScaleFactor, EveryNthFrame, ImageFormat, InteractionId,
        InteractionTiming, NodeReference, ObservationContext, PageSelection, PageSnapshot,
        PixelDimensions, ScreenshotTarget, SessionId, SessionTime, SnapshotGeneration,
        SnapshotNode, SnapshotNodeId, TargetCaptureStatus, TargetId, WaitCondition, WaitProbe,
        WaitRequest, WaitResult,
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

    fn failed_capture() -> TargetCaptureStatus {
        TargetCaptureStatus::new_with_failure_stage(
            target_id(),
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

    #[test]
    fn failed_capture_degrades_success_without_removing_current_image() {
        let mapped = map_operation_result_with_capture(
            "take_screenshot",
            BrowserOperationResult::TakeScreenshot(Box::new(screenshot(ImageFormat::Png))),
            &[failed_capture()],
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
    fn automatic_live_observations_bound_complex_snapshots_with_exact_omissions() {
        let full = complex_snapshot();
        for role in [ImageRole::PostAction, ImageRole::BatchFinal] {
            let (value, _, _) =
                project_live_observation(live_with_snapshot(full.clone()), role, None).unwrap();
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
        let after = serde_json::to_vec(&compact_automatic_snapshot(small).unwrap()).unwrap();
        assert_eq!(after, before);
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
        assert_eq!(degraded.response.warnings.len(), 3);

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
