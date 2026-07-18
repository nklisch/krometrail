//! Canonical temporal-evidence resource identities and reads.
//!
//! This module is deliberately independent of the rmcp server lifecycle.  The
//! server layer can call `read_resource` without gaining a second storage path;
//! every successful read still goes through the progressive evidence port.

use std::{str::FromStr, sync::Arc, time::Instant};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use krometrail_core::{
    ArtifactId, CancellationSignal, ErrorCode, EvidenceScope, FrameId, KrometrailError,
    NonEmptyText, ProgressiveEvidence, ProgressiveEvidenceContext, ProgressiveEvidenceRequest,
    ProgressiveEvidenceResult, Result, RetrieveArtifactRequest, RetrieveSourceFrameRequest,
    SessionId, TargetId,
};
use rmcp::model::{
    Annotated, RawResourceTemplate, ReadResourceResult, ResourceContents, ResourceTemplate,
};
use serde_json::json;

const ARTIFACT_READ_LIMIT: u64 = 64 * 1024 * 1024;
const SOURCE_FRAME_READ_LIMIT: u64 = 32 * 1024 * 1024;

const ARTIFACT_URI_TEMPLATE: &str = "krometrail://evidence/{session}/{target}/artifacts/{id}";
const ARTIFACT_MANIFEST_URI_TEMPLATE: &str =
    "krometrail://evidence/{session}/{target}/artifact-manifests/{id}";
const SOURCE_FRAME_URI_TEMPLATE: &str = "krometrail://evidence/{session}/{target}/frames/{id}";

/// The only resource templates exposed by this adapter. Concrete retained
/// resources remain intentionally unlisted because storage is dynamic and may
/// contain a large number of weak evidence handles.
pub(crate) fn resource_templates() -> Vec<ResourceTemplate> {
    vec![
        Annotated::new(
            RawResourceTemplate {
                uri_template: ARTIFACT_URI_TEMPLATE.to_owned(),
                name: "temporal-artifact".to_owned(),
                title: Some("Temporal artifact evidence".to_owned()),
                description: Some(
                    "Read one retained generated artifact by canonical evidence URI.".to_owned(),
                ),
                mime_type: Some("image/png".to_owned()),
            },
            None,
        ),
        Annotated::new(
            RawResourceTemplate {
                uri_template: ARTIFACT_MANIFEST_URI_TEMPLATE.to_owned(),
                name: "temporal-artifact-manifest".to_owned(),
                title: Some("Temporal artifact provenance manifest".to_owned()),
                description: Some(
                    "Read one retained artifact's complete provenance as canonical JSON."
                        .to_owned(),
                ),
                mime_type: Some("application/json".to_owned()),
            },
            None,
        ),
        Annotated::new(
            RawResourceTemplate {
                uri_template: SOURCE_FRAME_URI_TEMPLATE.to_owned(),
                name: "temporal-source-frame".to_owned(),
                title: Some("Temporal source-frame evidence".to_owned()),
                description: Some(
                    "Read one retained source frame by canonical evidence URI.".to_owned(),
                ),
                mime_type: None,
            },
            None,
        ),
    ]
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ResourceKind {
    Artifact,
    ArtifactManifest,
    SourceFrame,
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

    pub(crate) fn canonical_uri(self) -> String {
        let kind = match self.kind {
            ResourceKind::Artifact => "artifacts",
            ResourceKind::ArtifactManifest => "artifact-manifests",
            ResourceKind::SourceFrame => "frames",
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
    progressive: &dyn ProgressiveEvidence,
    deadline: Instant,
    cancellation: Arc<dyn CancellationSignal>,
) -> std::result::Result<ReadResourceResult, rmcp::ErrorData> {
    let parsed = EvidenceResourceUri::parse(uri).map_err(|_| {
        rmcp::ErrorData::invalid_params("resource URI is not a canonical evidence URI", None)
    })?;
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
    Ok(ReadResourceResult {
        contents: vec![contents],
    })
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
        CaptureOrdinal, CapturedFrame, DeviceScaleFactor, ImageFormat, PixelDimensions, PortFuture,
        SourceFrameHandle, SourceFrameRead,
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

    struct ErrorSpy {
        error: KrometrailError,
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
            &ErrorSpy { error },
            Instant::now() + Duration::from_secs(1),
            Arc::new(NeverCancelled),
        )
        .await
        .unwrap_err();
        assert_eq!(result.code, rmcp::model::ErrorCode::RESOURCE_NOT_FOUND);
        assert!(result.data.unwrap()["krometrail_error"].is_object());
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
                    spy.as_ref(),
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
            &spy,
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
