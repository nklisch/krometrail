//! Canonical temporal-evidence resource identities and reads.
//!
//! This module is deliberately independent of the rmcp server lifecycle.  The
//! server layer can call `read_resource` without gaining a second storage path;
//! every successful read still goes through the progressive evidence port.

use std::{str::FromStr, sync::Arc, time::Instant};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use krometrail_core::{
    ArtifactId, CancellationSignal, CapabilityId, ErrorCode, EvidenceScope, FrameId,
    KrometrailError, NonEmptyText, ProgressiveEvidence, ProgressiveEvidenceContext,
    ProgressiveEvidenceRequest, ProgressiveEvidenceResult, Result, RetrieveArtifactRequest,
    RetrieveSourceFrameRequest, SessionId, TargetId, TemporalVideoGeneration,
};
use rmcp::model::{ReadResourceResult, ResourceContents, ResourceTemplate};
use serde_json::json;

use crate::config::McpConfig;
use crate::session::BrowserSessionOwner;

const ARTIFACT_READ_LIMIT: u64 = 64 * 1024 * 1024;
const SOURCE_FRAME_READ_LIMIT: u64 = 32 * 1024 * 1024;

const ARTIFACT_URI_TEMPLATE: &str = "krometrail://evidence/{session}/{target}/artifacts/{id}";
const ARTIFACT_MANIFEST_URI_TEMPLATE: &str =
    "krometrail://evidence/{session}/{target}/artifact-manifests/{id}";
const SOURCE_FRAME_URI_TEMPLATE: &str = "krometrail://evidence/{session}/{target}/frames/{id}";
const VIDEO_URI_TEMPLATE: &str = "krometrail://evidence/{session}/{target}/videos/{id}";
const VIDEO_MANIFEST_URI_TEMPLATE: &str =
    "krometrail://evidence/{session}/{target}/video-manifests/{id}";
const MANAGED_DOWNLOAD_URI_TEMPLATE: &str = "krometrail://local/{session}/downloads/{id}";

#[derive(Clone, Copy)]
struct ResourceDefinition {
    kind: ResourceKind,
    capability: CapabilityId,
    uri_template: &'static str,
    name: &'static str,
    title: &'static str,
    description: &'static str,
    mime_type: Option<&'static str>,
}

const RESOURCE_DEFINITIONS: &[ResourceDefinition] = &[
    ResourceDefinition {
        kind: ResourceKind::Artifact,
        capability: CapabilityId::TemporalVision,
        uri_template: ARTIFACT_URI_TEMPLATE,
        name: "temporal-artifact",
        title: "Temporal artifact evidence",
        description: "Read one retained generated artifact by canonical evidence URI.",
        mime_type: Some("image/png"),
    },
    ResourceDefinition {
        kind: ResourceKind::ArtifactManifest,
        capability: CapabilityId::TemporalVision,
        uri_template: ARTIFACT_MANIFEST_URI_TEMPLATE,
        name: "temporal-artifact-manifest",
        title: "Temporal artifact provenance manifest",
        description: "Read one retained artifact's complete provenance as canonical JSON.",
        mime_type: Some("application/json"),
    },
    ResourceDefinition {
        kind: ResourceKind::SourceFrame,
        capability: CapabilityId::TemporalVision,
        uri_template: SOURCE_FRAME_URI_TEMPLATE,
        name: "temporal-source-frame",
        title: "Temporal source-frame evidence",
        description: "Read one retained source frame by canonical evidence URI.",
        mime_type: None,
    },
    ResourceDefinition {
        kind: ResourceKind::Video,
        capability: CapabilityId::TemporalVideo,
        uri_template: VIDEO_URI_TEMPLATE,
        name: "temporal-video",
        title: "Temporal video evidence",
        description: "Read one retained bounded MP4/H.264 clip by canonical evidence URI.",
        mime_type: Some("video/mp4"),
    },
    ResourceDefinition {
        kind: ResourceKind::VideoManifest,
        capability: CapabilityId::TemporalVideo,
        uri_template: VIDEO_MANIFEST_URI_TEMPLATE,
        name: "temporal-video-manifest",
        title: "Temporal video provenance manifest",
        description: "Read one retained temporal video's complete provenance as canonical JSON.",
        mime_type: Some("application/json"),
    },
];

/// The only resource templates exposed by this adapter. Concrete retained
/// resources remain intentionally unlisted because storage is dynamic and may
/// contain a large number of weak evidence handles.
pub(crate) fn resource_templates(config: &McpConfig) -> Vec<ResourceTemplate> {
    let mut templates = RESOURCE_DEFINITIONS
        .iter()
        .filter(|definition| config.is_enabled(definition.capability))
        .map(|definition| {
            let mut template = ResourceTemplate::new(definition.uri_template, definition.name)
                .with_title(definition.title)
                .with_description(format!("{} Use URIs from tool results; retained evidence belongs to this process and may expire.", definition.description));
            template.mime_type = definition.mime_type.map(str::to_owned);
            template
        })
        .collect::<Vec<_>>();
    if config.is_enabled(CapabilityId::Control) {
        templates.push(ResourceTemplate::new(MANAGED_DOWNLOAD_URI_TEMPLATE, "managed-download")
            .with_title("Active managed-session download")
            .with_description("Read a completed download URI from tool results while its managed browser session remains active.")
            .with_mime_type("application/octet-stream"));
    }
    templates
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ManagedDownloadResourceUri {
    session_id: SessionId,
    download_id: krometrail_core::DownloadId,
}

impl ManagedDownloadResourceUri {
    fn parse(uri: &str) -> Result<Self> {
        if uri.is_empty() || uri.contains(['%', '?', '#', '\\']) {
            return Err(krometrail_core::KrometrailError::new(
                ErrorCode::InvalidInput,
                NonEmptyText::new("managed download resource URI is not canonical").unwrap(),
            ));
        }
        let parts = uri.split('/').collect::<Vec<_>>();
        if parts.len() != 6
            || parts[0] != "krometrail:"
            || !parts[1].is_empty()
            || parts[2] != "local"
            || parts[4] != "downloads"
        {
            return Err(krometrail_core::KrometrailError::new(
                ErrorCode::InvalidInput,
                NonEmptyText::new("managed download resource URI is not canonical").unwrap(),
            ));
        }
        let session_id = parts[3].parse::<SessionId>().map_err(|_| {
            krometrail_core::KrometrailError::new(
                ErrorCode::InvalidInput,
                NonEmptyText::new("managed download session ID is invalid").unwrap(),
            )
        })?;
        let download_id = parts[5]
            .parse::<krometrail_core::DownloadId>()
            .map_err(|_| {
                krometrail_core::KrometrailError::new(
                    ErrorCode::InvalidInput,
                    NonEmptyText::new("managed download ID is invalid").unwrap(),
                )
            })?;
        let parsed = Self {
            session_id,
            download_id,
        };
        if parsed.canonical_uri() != uri {
            return Err(krometrail_core::KrometrailError::new(
                ErrorCode::InvalidInput,
                NonEmptyText::new("managed download resource URI is not canonical").unwrap(),
            ));
        }
        Ok(parsed)
    }

    fn canonical_uri(self) -> String {
        format!(
            "krometrail://local/{}/downloads/{}",
            self.session_id, self.download_id
        )
    }
}

pub(crate) async fn read_resource_with_local(
    uri: &str,
    config: &McpConfig,
    sessions: &BrowserSessionOwner,
    progressive: &dyn ProgressiveEvidence,
    temporal_video: Option<&dyn TemporalVideoGeneration>,
    deadline: Instant,
    cancellation: Arc<dyn CancellationSignal>,
) -> std::result::Result<ReadResourceResult, rmcp::ErrorData> {
    if uri.starts_with("krometrail://local/") {
        if !config.is_enabled(CapabilityId::Control) {
            return Err(rmcp::ErrorData::resource_not_found(
                "local browser resource is not registered",
                None,
            ));
        }
        let parsed = ManagedDownloadResourceUri::parse(uri).map_err(|_| {
            rmcp::ErrorData::invalid_params(
                "resource URI is not a canonical managed-download URI",
                None,
            )
        })?;
        let read = sessions
            .read_managed_download(krometrail_core::ReadManagedDownloadRequest {
                session_id: parsed.session_id,
                download_id: parsed.download_id,
                max_bytes: 64 * 1024 * 1024,
            })
            .await
            .map_err(map_resource_domain_error)?;
        if read.session_id != parsed.session_id
            || read.download_id != parsed.download_id
            || parsed.canonical_uri() != uri
            || read.bytes.len() as u64 > 64 * 1024 * 1024
        {
            return Err(rmcp::ErrorData::internal_error(
                "managed download resource identity mismatch",
                None,
            ));
        }
        return Ok(ReadResourceResult::new(vec![
            ResourceContents::BlobResourceContents {
                uri: uri.to_owned(),
                mime_type: Some(read.media_type.as_str().to_owned()),
                blob: STANDARD.encode(read.bytes),
                meta: None,
            },
        ]));
    }
    read_resource(
        uri,
        config,
        progressive,
        temporal_video,
        deadline,
        cancellation,
    )
    .await
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ResourceKind {
    Artifact,
    ArtifactManifest,
    SourceFrame,
    Video,
    VideoManifest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct EvidenceResourceUri {
    pub(crate) kind: ResourceKind,
    pub(crate) scope: EvidenceScope,
    pub(crate) id: EvidenceResourceId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EvidenceResourceId {
    Artifact(ArtifactId),
    SourceFrame(FrameId),
}

impl EvidenceResourceUri {
    pub(crate) fn artifact(scope: EvidenceScope, artifact_id: ArtifactId) -> Self {
        Self {
            kind: ResourceKind::Artifact,
            scope,
            id: EvidenceResourceId::Artifact(artifact_id),
        }
    }

    pub(crate) fn source_frame(scope: EvidenceScope, frame_id: FrameId) -> Self {
        Self {
            kind: ResourceKind::SourceFrame,
            scope,
            id: EvidenceResourceId::SourceFrame(frame_id),
        }
    }

    pub(crate) fn artifact_manifest(scope: EvidenceScope, artifact_id: ArtifactId) -> Self {
        Self {
            kind: ResourceKind::ArtifactManifest,
            scope,
            id: EvidenceResourceId::Artifact(artifact_id),
        }
    }

    pub(crate) fn video(scope: EvidenceScope, artifact_id: ArtifactId) -> Self {
        Self {
            kind: ResourceKind::Video,
            scope,
            id: EvidenceResourceId::Artifact(artifact_id),
        }
    }

    pub(crate) fn video_manifest(scope: EvidenceScope, artifact_id: ArtifactId) -> Self {
        Self {
            kind: ResourceKind::VideoManifest,
            scope,
            id: EvidenceResourceId::Artifact(artifact_id),
        }
    }

    pub(crate) fn canonical_uri(self) -> String {
        let kind = match self.kind {
            ResourceKind::Artifact => "artifacts",
            ResourceKind::ArtifactManifest => "artifact-manifests",
            ResourceKind::SourceFrame => "frames",
            ResourceKind::Video => "videos",
            ResourceKind::VideoManifest => "video-manifests",
        };
        let id = match self.id {
            EvidenceResourceId::Artifact(id) => id.to_string(),
            EvidenceResourceId::SourceFrame(id) => id.to_string(),
        };
        format!(
            "krometrail://evidence/{}/{}/{}/{}",
            self.scope.session_id, self.scope.target_id, kind, id
        )
    }

    pub(crate) fn name(self) -> String {
        let kind = match self.kind {
            ResourceKind::Artifact => "artifact",
            ResourceKind::ArtifactManifest => "artifact-manifest",
            ResourceKind::SourceFrame => "source-frame",
            ResourceKind::Video => "video",
            ResourceKind::VideoManifest => "video-manifest",
        };
        format!(
            "{kind}-{}",
            match self.id {
                EvidenceResourceId::Artifact(id) => id.to_string(),
                EvidenceResourceId::SourceFrame(id) => id.to_string(),
            }
        )
    }

    pub(crate) fn parse(uri: &str) -> Result<Self> {
        // Manual parsing is intentional: URL parsers commonly normalize or
        // decode forms that are not part of this prepublic canonical grammar.
        if uri.is_empty()
            || uri.contains('%')
            || uri.contains('?')
            || uri.contains('#')
            || uri.contains('\\')
            || !uri.starts_with("krometrail://evidence/")
        {
            return Err(invalid_uri());
        }
        let segments: Vec<&str> = uri["krometrail://evidence/".len()..].split('/').collect();
        if segments.len() != 4 || segments.iter().any(|segment| segment.is_empty()) {
            return Err(invalid_uri());
        }
        let session_id = SessionId::from_str(segments[0]).map_err(|_| invalid_uri())?;
        let target_id = TargetId::from_str(segments[1]).map_err(|_| invalid_uri())?;
        if session_id.as_uuid().is_nil()
            || target_id.as_uuid().is_nil()
            || segments[0] != session_id.to_string()
            || segments[1] != target_id.to_string()
        {
            return Err(invalid_uri());
        }
        let scope = EvidenceScope::new(session_id, target_id)?;
        let parsed = match segments[2] {
            "artifacts" => {
                let id = ArtifactId::from_str(segments[3]).map_err(|_| invalid_uri())?;
                if id.as_uuid().is_nil() || segments[3] != id.to_string() {
                    return Err(invalid_uri());
                }
                Self::artifact(scope, id)
            }
            "artifact-manifests" => {
                let id = ArtifactId::from_str(segments[3]).map_err(|_| invalid_uri())?;
                if id.as_uuid().is_nil() || segments[3] != id.to_string() {
                    return Err(invalid_uri());
                }
                Self::artifact_manifest(scope, id)
            }
            "frames" => {
                let id = FrameId::from_str(segments[3]).map_err(|_| invalid_uri())?;
                if id.as_uuid().is_nil() || segments[3] != id.to_string() {
                    return Err(invalid_uri());
                }
                Self::source_frame(scope, id)
            }
            "videos" => {
                let id = ArtifactId::from_str(segments[3]).map_err(|_| invalid_uri())?;
                if id.as_uuid().is_nil() || segments[3] != id.to_string() {
                    return Err(invalid_uri());
                }
                Self::video(scope, id)
            }
            "video-manifests" => {
                let id = ArtifactId::from_str(segments[3]).map_err(|_| invalid_uri())?;
                if id.as_uuid().is_nil() || segments[3] != id.to_string() {
                    return Err(invalid_uri());
                }
                Self::video_manifest(scope, id)
            }
            _ => return Err(invalid_uri()),
        };
        if parsed.canonical_uri() != uri {
            return Err(invalid_uri());
        }
        Ok(parsed)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResourceProjection {
    pub(crate) role: ResourceKind,
    pub(crate) uri: String,
    pub(crate) name: String,
    pub(crate) mime_type: String,
    pub(crate) encoded_byte_len: u64,
}

impl ResourceProjection {
    pub(crate) fn from_artifact(
        scope: EvidenceScope,
        artifact_id: ArtifactId,
        mime_type: &str,
        encoded_byte_len: u64,
    ) -> Result<Self> {
        Self::new(
            EvidenceResourceUri::artifact(scope, artifact_id),
            mime_type,
            encoded_byte_len,
        )
    }

    pub(crate) fn from_source_frame(
        scope: EvidenceScope,
        frame_id: FrameId,
        mime_type: &str,
        encoded_byte_len: u64,
    ) -> Result<Self> {
        Self::new(
            EvidenceResourceUri::source_frame(scope, frame_id),
            mime_type,
            encoded_byte_len,
        )
    }

    pub(crate) fn from_artifact_manifest(
        scope: EvidenceScope,
        artifact_id: ArtifactId,
        encoded_byte_len: u64,
    ) -> Result<Self> {
        Self::new(
            EvidenceResourceUri::artifact_manifest(scope, artifact_id),
            "application/json",
            encoded_byte_len,
        )
    }

    pub(crate) fn from_video(
        scope: EvidenceScope,
        artifact_id: ArtifactId,
        encoded_byte_len: u64,
    ) -> Result<Self> {
        Self::new(
            EvidenceResourceUri::video(scope, artifact_id),
            "video/mp4",
            encoded_byte_len,
        )
    }

    pub(crate) fn from_video_manifest(
        scope: EvidenceScope,
        artifact_id: ArtifactId,
        encoded_byte_len: u64,
    ) -> Result<Self> {
        Self::new(
            EvidenceResourceUri::video_manifest(scope, artifact_id),
            "application/json",
            encoded_byte_len,
        )
    }

    fn new(uri: EvidenceResourceUri, mime_type: &str, encoded_byte_len: u64) -> Result<Self> {
        if encoded_byte_len == 0 || mime_type.trim().is_empty() {
            return Err(invalid_uri());
        }
        Ok(Self {
            role: uri.kind,
            uri: uri.canonical_uri(),
            name: uri.name(),
            mime_type: mime_type.to_owned(),
            encoded_byte_len,
        })
    }

    pub(crate) fn parsed_uri(&self) -> Result<EvidenceResourceUri> {
        EvidenceResourceUri::parse(&self.uri)
    }
}

pub(crate) async fn read_resource(
    uri: &str,
    config: &McpConfig,
    progressive: &dyn ProgressiveEvidence,
    temporal_video: Option<&dyn TemporalVideoGeneration>,
    deadline: Instant,
    cancellation: Arc<dyn CancellationSignal>,
) -> std::result::Result<ReadResourceResult, rmcp::ErrorData> {
    let parsed = EvidenceResourceUri::parse(uri).map_err(|_| {
        rmcp::ErrorData::invalid_params("resource URI is not a canonical evidence URI", None)
    })?;
    let definition = RESOURCE_DEFINITIONS
        .iter()
        .find(|definition| definition.kind == parsed.kind)
        .expect("resource registry contains every resource kind");
    if !config.is_enabled(definition.capability) {
        return Err(rmcp::ErrorData::resource_not_found(
            "evidence resource is not registered",
            None,
        ));
    }
    if matches!(
        parsed.kind,
        ResourceKind::Video | ResourceKind::VideoManifest
    ) {
        return read_video_resource(
            parsed,
            uri,
            temporal_video.ok_or_else(|| {
                rmcp::ErrorData::internal_error(
                    "temporal video resource service is unavailable",
                    None,
                )
            })?,
            deadline,
            cancellation,
        )
        .await;
    }
    let request = match parsed.id {
        EvidenceResourceId::Artifact(artifact_id) => ProgressiveEvidenceRequest::RetrieveArtifact(
            RetrieveArtifactRequest::new(parsed.scope, artifact_id, ARTIFACT_READ_LIMIT)
                .map_err(internal_resource_error)?,
        ),
        EvidenceResourceId::SourceFrame(frame_id) => {
            ProgressiveEvidenceRequest::RetrieveSourceFrame(
                RetrieveSourceFrameRequest::new(parsed.scope, frame_id, SOURCE_FRAME_READ_LIMIT)
                    .map_err(internal_resource_error)?,
            )
        }
    };
    let result = tokio::select! {
        result = progressive.execute(request, ProgressiveEvidenceContext {
            deadline: Some(deadline),
            cancellation: Some(Arc::clone(&cancellation)),
            current_reference_geometry: None,
        }) => result,
        () = cancellation.cancelled() => Err(cancelled_error()),
        () = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)) => Err(deadline_error()),
    };
    let result = result.map_err(map_resource_domain_error)?;
    let contents = match result {
        ProgressiveEvidenceResult::RetrieveArtifact(read) => {
            let EvidenceResourceId::Artifact(expected_id) = parsed.id else {
                return Err(rmcp::ErrorData::internal_error(
                    "resource result kind mismatch",
                    None,
                ));
            };
            let expected_uri = match parsed.kind {
                ResourceKind::Artifact => {
                    EvidenceResourceUri::artifact(read.handle.scope, read.handle.artifact_id)
                }
                ResourceKind::ArtifactManifest => EvidenceResourceUri::artifact_manifest(
                    read.handle.scope,
                    read.handle.artifact_id,
                ),
                ResourceKind::SourceFrame => {
                    return Err(rmcp::ErrorData::internal_error(
                        "resource result kind mismatch",
                        None,
                    ));
                }
                ResourceKind::Video | ResourceKind::VideoManifest => unreachable!(
                    "video resource kinds return through the retained video authority above"
                ),
            };
            if read.handle.artifact_id != expected_id
                || read.handle.scope != parsed.scope
                || expected_uri.canonical_uri() != uri
            {
                return Err(rmcp::ErrorData::internal_error(
                    "resource handle identity mismatch",
                    None,
                ));
            }
            match parsed.kind {
                ResourceKind::Artifact => ResourceContents::BlobResourceContents {
                    uri: uri.to_owned(),
                    mime_type: Some(read.handle.media_type.as_str().to_owned()),
                    blob: STANDARD.encode(read.encoded_bytes()),
                    meta: None,
                },
                ResourceKind::ArtifactManifest => ResourceContents::TextResourceContents {
                    uri: uri.to_owned(),
                    mime_type: Some("application/json".to_owned()),
                    text: serde_json::to_string(&read.handle.provenance).map_err(|_| {
                        rmcp::ErrorData::internal_error(
                            "artifact manifest could not be serialized",
                            None,
                        )
                    })?,
                    meta: None,
                },
                ResourceKind::SourceFrame => unreachable!("source-frame result rejected above"),
                ResourceKind::Video | ResourceKind::VideoManifest => unreachable!(
                    "video resource kinds return through the retained video authority above"
                ),
            }
        }
        ProgressiveEvidenceResult::RetrieveSourceFrame(read) => {
            let EvidenceResourceId::SourceFrame(expected_id) = parsed.id else {
                return Err(rmcp::ErrorData::internal_error(
                    "resource result kind mismatch",
                    None,
                ));
            };
            if read.handle.frame_id != expected_id
                || read.handle.scope != parsed.scope
                || EvidenceResourceUri::source_frame(read.handle.scope, read.handle.frame_id)
                    .canonical_uri()
                    != uri
            {
                return Err(rmcp::ErrorData::internal_error(
                    "resource handle identity mismatch",
                    None,
                ));
            }
            ResourceContents::BlobResourceContents {
                uri: uri.to_owned(),
                mime_type: Some(read.handle.media_type.as_str().to_owned()),
                blob: STANDARD.encode(read.encoded_bytes()),
                meta: None,
            }
        }
        _ => {
            return Err(rmcp::ErrorData::internal_error(
                "resource result kind mismatch",
                None,
            ));
        }
    };
    Ok(ReadResourceResult::new(vec![contents]))
}

async fn read_video_resource(
    parsed: EvidenceResourceUri,
    uri: &str,
    temporal_video: &dyn TemporalVideoGeneration,
    deadline: Instant,
    cancellation: Arc<dyn CancellationSignal>,
) -> std::result::Result<ReadResourceResult, rmcp::ErrorData> {
    let EvidenceResourceId::Artifact(artifact_id) = parsed.id else {
        return Err(rmcp::ErrorData::internal_error(
            "video resource identity kind mismatch",
            None,
        ));
    };
    let request = RetrieveArtifactRequest::new(parsed.scope, artifact_id, ARTIFACT_READ_LIMIT)
        .map_err(internal_resource_error)?;
    let read = tokio::select! {
        result = temporal_video.read_video_artifact(request) => result,
        () = cancellation.cancelled() => Err(cancelled_error()),
        () = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)) => Err(deadline_error()),
    }
    .map_err(map_resource_domain_error)?;
    let expected = match parsed.kind {
        ResourceKind::Video => {
            EvidenceResourceUri::video(read.handle.scope, read.handle.artifact_id)
        }
        ResourceKind::VideoManifest => {
            EvidenceResourceUri::video_manifest(read.handle.scope, read.handle.artifact_id)
        }
        _ => {
            return Err(rmcp::ErrorData::internal_error(
                "video resource result kind mismatch",
                None,
            ));
        }
    };
    if read.handle.scope != parsed.scope
        || read.handle.artifact_id != artifact_id
        || expected.canonical_uri() != uri
    {
        return Err(rmcp::ErrorData::internal_error(
            "video resource handle identity mismatch",
            None,
        ));
    }
    let contents = match parsed.kind {
        ResourceKind::Video => ResourceContents::BlobResourceContents {
            uri: uri.to_owned(),
            mime_type: Some("video/mp4".to_owned()),
            blob: STANDARD.encode(read.encoded_bytes()),
            meta: None,
        },
        ResourceKind::VideoManifest => ResourceContents::TextResourceContents {
            uri: uri.to_owned(),
            mime_type: Some("application/json".to_owned()),
            text: serde_json::to_string(&read.handle.provenance).map_err(|_| {
                rmcp::ErrorData::internal_error(
                    "temporal video manifest could not be serialized",
                    None,
                )
            })?,
            meta: None,
        },
        _ => unreachable!("video resource kind checked above"),
    };
    Ok(ReadResourceResult::new(vec![contents]))
}

fn invalid_uri() -> KrometrailError {
    KrometrailError::new(
        ErrorCode::InvalidInput,
        NonEmptyText::new("resource URI is not a canonical evidence URI")
            .expect("static URI error is non-empty"),
    )
}

fn internal_resource_error(error: KrometrailError) -> rmcp::ErrorData {
    rmcp::ErrorData::internal_error(
        "resource read request could not be constructed",
        Some(json!({ "krometrail_error": error })),
    )
}

fn map_resource_domain_error(error: KrometrailError) -> rmcp::ErrorData {
    let data = Some(json!({ "krometrail_error": error }));
    match error.code {
        ErrorCode::NotFound | ErrorCode::EvidenceInvalidated | ErrorCode::CaptureRejected => {
            rmcp::ErrorData::resource_not_found("evidence resource is no longer available", data)
        }
        ErrorCode::Cancelled => {
            rmcp::ErrorData::internal_error("evidence resource read cancelled", data)
        }
        _ => rmcp::ErrorData::internal_error("evidence resource read failed", data),
    }
}

fn cancelled_error() -> KrometrailError {
    KrometrailError::new(
        ErrorCode::Cancelled,
        NonEmptyText::new("MCP request was cancelled").expect("static cancellation error"),
    )
}

fn deadline_error() -> KrometrailError {
    KrometrailError::new(
        ErrorCode::Cancelled,
        NonEmptyText::new("MCP request deadline elapsed").expect("static deadline error"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use krometrail_core::{
        ArtifactGenerationContext, CapabilitySnapshot, CaptureOrdinal, CapturedFrame,
        DeviceScaleFactor, ImageFormat, PixelDimensions, PortFuture, SourceFrameHandle,
        SourceFrameRead, TemporalVideoGenerationRequest, TemporalVideoGenerationResult,
        VideoArtifactRead,
    };
    use std::{sync::Mutex, time::Duration};
    use tokio::sync::Notify;
    use tokio_util::sync::CancellationToken;

    fn scope() -> EvidenceScope {
        EvidenceScope::new(
            "00000000-0000-0000-0000-000000000001".parse().unwrap(),
            "00000000-0000-0000-0000-000000000002".parse().unwrap(),
        )
        .unwrap()
    }

    fn qualified_config() -> McpConfig {
        McpConfig::from_snapshot(
            CapabilitySnapshot::resolve_defaults(&[CapabilityId::TemporalVideo]).unwrap(),
        )
    }

    struct NeverCancelled;

    impl CancellationSignal for NeverCancelled {
        fn is_cancelled(&self) -> bool {
            false
        }

        fn cancelled(&self) -> PortFuture<'_, ()> {
            Box::pin(std::future::pending())
        }
    }

    struct ResourceSpy {
        result: ProgressiveEvidenceResult,
        request: Mutex<Option<ProgressiveEvidenceRequest>>,
    }

    impl ProgressiveEvidence for ResourceSpy {
        fn execute(
            &self,
            request: ProgressiveEvidenceRequest,
            _context: ProgressiveEvidenceContext,
        ) -> PortFuture<'_, Result<ProgressiveEvidenceResult>> {
            *self.request.lock().unwrap() = Some(request);
            let result = self.result.clone();
            Box::pin(std::future::ready(Ok(result)))
        }
    }

    #[test]
    fn canonical_uris_round_trip_and_reject_alternate_forms() {
        let artifact_id = "00000000-0000-0000-0000-000000000003".parse().unwrap();
        let uri = EvidenceResourceUri::artifact(scope(), artifact_id);
        let text = uri.canonical_uri();
        assert_eq!(EvidenceResourceUri::parse(&text).unwrap(), uri);
        let manifest = EvidenceResourceUri::artifact_manifest(scope(), artifact_id);
        assert_eq!(
            EvidenceResourceUri::parse(&manifest.canonical_uri()).unwrap(),
            manifest
        );
        let video = EvidenceResourceUri::video(scope(), artifact_id);
        assert_eq!(
            EvidenceResourceUri::parse(&video.canonical_uri()).unwrap(),
            video
        );
        let video_manifest = EvidenceResourceUri::video_manifest(scope(), artifact_id);
        assert_eq!(
            EvidenceResourceUri::parse(&video_manifest.canonical_uri()).unwrap(),
            video_manifest
        );
        for alternate in [
            text.to_uppercase(),
            text.replace("artifacts", "artifact"),
            format!("{text}?x=1"),
            format!("{text}/extra"),
            text.replace("krometrail", "file"),
            text.replace("artifacts", "%61rtifacts"),
        ] {
            assert!(
                EvidenceResourceUri::parse(&alternate).is_err(),
                "{alternate}"
            );
        }
    }

    #[test]
    fn one_capability_snapshot_filters_video_templates_as_one_registry() {
        let unavailable = resource_templates(&McpConfig::default());
        let qualified = resource_templates(&qualified_config());
        assert_eq!(unavailable.len(), 4);
        assert_eq!(qualified.len(), 6);
        let encoded = serde_json::to_value(qualified).unwrap();
        let text = encoded.to_string();
        assert!(text.contains(VIDEO_URI_TEMPLATE));
        assert!(text.contains(VIDEO_MANIFEST_URI_TEMPLATE));
        assert_eq!(text.matches("temporal-video\"").count(), 1);
        assert_eq!(text.matches("temporal-video-manifest\"").count(), 1);
        assert!(text.contains(MANAGED_DOWNLOAD_URI_TEMPLATE));
    }

    #[test]
    fn managed_download_uri_is_strict_and_canonical() {
        let uri = "krometrail://local/00000000-0000-0000-0000-000000000001/downloads/00000000-0000-0000-0000-000000000002";
        let parsed = ManagedDownloadResourceUri::parse(uri).unwrap();
        assert_eq!(parsed.canonical_uri(), uri);
        for alternate in [
            format!("{uri}?x=1"),
            format!("{uri}/extra"),
            uri.replace("downloads", "%64ownloads"),
            uri.to_uppercase(),
            uri.replace("krometrail", "file"),
        ] {
            assert!(
                ManagedDownloadResourceUri::parse(&alternate).is_err(),
                "{alternate}"
            );
        }
    }

    struct ErrorSpy {
        error: KrometrailError,
    }

    struct VideoResourceSpy {
        reads: Vec<VideoArtifactRead>,
        requests: Mutex<Vec<RetrieveArtifactRequest>>,
    }

    impl TemporalVideoGeneration for VideoResourceSpy {
        fn generate_video(
            &self,
            _request: TemporalVideoGenerationRequest,
            _context: ArtifactGenerationContext,
        ) -> PortFuture<'_, Result<TemporalVideoGenerationResult>> {
            panic!("resource test must not generate video")
        }

        fn read_video_artifact(
            &self,
            request: RetrieveArtifactRequest,
        ) -> PortFuture<'_, Result<VideoArtifactRead>> {
            self.requests.lock().unwrap().push(request.clone());
            let read = self
                .reads
                .iter()
                .find(|read| read.handle.artifact_id == request.artifact_id)
                .unwrap()
                .clone();
            Box::pin(std::future::ready(Ok(read)))
        }
    }

    impl ProgressiveEvidence for ErrorSpy {
        fn execute(
            &self,
            _request: ProgressiveEvidenceRequest,
            _context: ProgressiveEvidenceContext,
        ) -> PortFuture<'_, Result<ProgressiveEvidenceResult>> {
            Box::pin(std::future::ready(Err(self.error.clone())))
        }
    }

    struct TokenCancellation(CancellationToken);

    impl CancellationSignal for TokenCancellation {
        fn is_cancelled(&self) -> bool {
            self.0.is_cancelled()
        }

        fn cancelled(&self) -> PortFuture<'_, ()> {
            Box::pin(self.0.cancelled())
        }
    }

    struct BlockingSpy {
        started: Arc<Notify>,
    }

    impl ProgressiveEvidence for BlockingSpy {
        fn execute(
            &self,
            _request: ProgressiveEvidenceRequest,
            context: ProgressiveEvidenceContext,
        ) -> PortFuture<'_, Result<ProgressiveEvidenceResult>> {
            let started = Arc::clone(&self.started);
            let cancellation = context
                .cancellation
                .expect("resource reads provide cancellation");
            Box::pin(async move {
                started.notify_one();
                cancellation.cancelled().await;
                Err(KrometrailError::new(
                    ErrorCode::Cancelled,
                    NonEmptyText::new("resource read cancelled").unwrap(),
                ))
            })
        }
    }

    #[tokio::test]
    async fn unavailable_resource_returns_not_found_without_contents() {
        let error = KrometrailError::new(
            ErrorCode::NotFound,
            NonEmptyText::new("fixture evidence was evicted").unwrap(),
        );
        let uri = EvidenceResourceUri::source_frame(
            scope(),
            "00000000-0000-0000-0000-000000000003".parse().unwrap(),
        )
        .canonical_uri();
        let result = read_resource(
            &uri,
            &McpConfig::default(),
            &ErrorSpy { error },
            None,
            Instant::now() + Duration::from_secs(1),
            Arc::new(NeverCancelled),
        )
        .await
        .unwrap_err();
        assert_eq!(result.code, rmcp::model::ErrorCode::RESOURCE_NOT_FOUND);
        assert!(result.data.unwrap()["krometrail_error"].is_object());
    }

    #[tokio::test]
    async fn unavailable_video_resource_is_rejected_before_any_retained_read() {
        let uri = EvidenceResourceUri::video(
            scope(),
            "00000000-0000-0000-0000-000000000003".parse().unwrap(),
        )
        .canonical_uri();
        let result = read_resource(
            &uri,
            &McpConfig::default(),
            &ErrorSpy {
                error: KrometrailError::new(
                    ErrorCode::Internal,
                    NonEmptyText::new("must not be called").unwrap(),
                ),
            },
            None,
            Instant::now() + Duration::from_secs(1),
            Arc::new(NeverCancelled),
        )
        .await
        .unwrap_err();
        assert_eq!(result.code, rmcp::model::ErrorCode::RESOURCE_NOT_FOUND);
        assert_eq!(result.message, "evidence resource is not registered");
    }

    #[tokio::test]
    async fn retained_video_and_manifest_reads_preserve_identity_bytes_and_provenance() {
        let fixture = crate::test_fixture::video_fixture();
        let expected = fixture.reads[0].clone();
        let spy = VideoResourceSpy {
            reads: fixture.reads,
            requests: Mutex::new(Vec::new()),
        };
        let scope = expected.handle.scope;
        let artifact_id = expected.handle.artifact_id;
        let video_uri = EvidenceResourceUri::video(scope, artifact_id).canonical_uri();
        let manifest_uri = EvidenceResourceUri::video_manifest(scope, artifact_id).canonical_uri();
        let video = read_resource(
            &video_uri,
            &qualified_config(),
            &ErrorSpy {
                error: KrometrailError::new(
                    ErrorCode::Internal,
                    NonEmptyText::new("still authority must not be called").unwrap(),
                ),
            },
            Some(&spy),
            Instant::now() + Duration::from_secs(1),
            Arc::new(NeverCancelled),
        )
        .await
        .unwrap();
        let manifest = read_resource(
            &manifest_uri,
            &qualified_config(),
            &ErrorSpy {
                error: KrometrailError::new(
                    ErrorCode::Internal,
                    NonEmptyText::new("still authority must not be called").unwrap(),
                ),
            },
            Some(&spy),
            Instant::now() + Duration::from_secs(1),
            Arc::new(NeverCancelled),
        )
        .await
        .unwrap();
        let ResourceContents::BlobResourceContents {
            uri,
            mime_type,
            blob,
            ..
        } = &video.contents[0]
        else {
            panic!("video resource must be a blob")
        };
        assert_eq!(uri, &video_uri);
        assert_eq!(mime_type.as_deref(), Some("video/mp4"));
        assert_eq!(STANDARD.decode(blob).unwrap(), expected.encoded_bytes());
        let ResourceContents::TextResourceContents {
            uri,
            mime_type,
            text,
            ..
        } = &manifest.contents[0]
        else {
            panic!("video manifest resource must be text")
        };
        assert_eq!(uri, &manifest_uri);
        assert_eq!(mime_type.as_deref(), Some("application/json"));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(text).unwrap(),
            serde_json::to_value(&expected.handle.provenance).unwrap()
        );
        let requests = spy.requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(requests.iter().all(|request| {
            request.scope == scope
                && request.artifact_id == artifact_id
                && request.max_encoded_bytes() == ARTIFACT_READ_LIMIT
        }));
    }

    #[tokio::test]
    async fn cancelled_resource_read_returns_no_partial_contents() {
        let token = CancellationToken::new();
        let started = Arc::new(Notify::new());
        let spy = Arc::new(BlockingSpy {
            started: Arc::clone(&started),
        });
        let uri = EvidenceResourceUri::source_frame(
            scope(),
            "00000000-0000-0000-0000-000000000003".parse().unwrap(),
        )
        .canonical_uri();
        let task = tokio::spawn({
            let token = token.clone();
            async move {
                read_resource(
                    &uri,
                    &McpConfig::default(),
                    spy.as_ref(),
                    None,
                    Instant::now() + Duration::from_secs(1),
                    Arc::new(TokenCancellation(token)),
                )
                .await
            }
        });
        started.notified().await;
        token.cancel();
        let result = task.await.unwrap().unwrap_err();
        assert_eq!(result.code, rmcp::model::ErrorCode::INTERNAL_ERROR);
        assert!(result.data.unwrap()["krometrail_error"].is_object());
    }

    #[tokio::test]
    async fn retained_resource_reads_return_exact_blob_mime_and_uri() {
        let bytes: Arc<[u8]> = Arc::from(b"\x89PNG\r\n\x1a\npayload".as_slice());
        let frame_id: FrameId = "00000000-0000-0000-0000-000000000003".parse().unwrap();
        let frame = CapturedFrame::new(
            frame_id,
            scope().session_id,
            scope().target_id,
            CaptureOrdinal::new(1).unwrap(),
            None,
            krometrail_core::ObservedTime::from_nanos(1),
            krometrail_core::SessionTime::from_nanos(1),
            ImageFormat::Png,
            PixelDimensions::new(1, 1).unwrap(),
            PixelDimensions::new(1, 1).unwrap(),
            DeviceScaleFactor::new(1.0).unwrap(),
            Vec::new(),
        )
        .unwrap();
        let handle = SourceFrameHandle::new(
            frame_id,
            scope(),
            0,
            0,
            NonEmptyText::new("image/png").unwrap(),
            krometrail_core::Sha256Digest::digest(&bytes),
            bytes.len() as u64,
            frame,
        )
        .unwrap();
        let spy = ResourceSpy {
            result: ProgressiveEvidenceResult::RetrieveSourceFrame(Box::new(
                SourceFrameRead::new(handle, Arc::clone(&bytes)).unwrap(),
            )),
            request: Mutex::new(None),
        };
        let uri = EvidenceResourceUri::source_frame(scope(), frame_id).canonical_uri();
        let result = read_resource(
            &uri,
            &McpConfig::default(),
            &spy,
            None,
            Instant::now() + Duration::from_secs(1),
            Arc::new(NeverCancelled),
        )
        .await
        .unwrap();
        let ResourceContents::BlobResourceContents {
            uri: returned_uri,
            mime_type,
            blob,
            ..
        } = &result.contents[0]
        else {
            panic!("resource authority must return a blob");
        };
        assert_eq!(returned_uri, &uri);
        assert_eq!(mime_type.as_deref(), Some("image/png"));
        assert_eq!(STANDARD.decode(blob).unwrap().as_slice(), bytes.as_ref());
        assert!(matches!(
            spy.request.lock().unwrap().as_ref(),
            Some(ProgressiveEvidenceRequest::RetrieveSourceFrame(_))
        ));
    }
}
