//! Canonical temporal-evidence resource identities and reads.
//!
#![allow(dead_code)]

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
use rmcp::model::{ReadResourceResult, ResourceContents};
use serde_json::json;

const ARTIFACT_READ_LIMIT: u64 = 64 * 1024 * 1024;
const SOURCE_FRAME_READ_LIMIT: u64 = 32 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ResourceKind {
    Artifact,
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

    pub(crate) fn canonical_uri(self) -> String {
        let kind = match self.kind {
            ResourceKind::Artifact => "artifacts",
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
            if read.handle.artifact_id != expected_id
                || read.handle.scope != parsed.scope
                || EvidenceResourceUri::artifact(read.handle.scope, read.handle.artifact_id)
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
    fn scope() -> EvidenceScope {
        EvidenceScope::new(
            "00000000-0000-0000-0000-000000000001".parse().unwrap(),
            "00000000-0000-0000-0000-000000000002".parse().unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn canonical_uris_round_trip_and_reject_alternate_forms() {
        let uri = EvidenceResourceUri::artifact(
            scope(),
            "00000000-0000-0000-0000-000000000003".parse().unwrap(),
        );
        let text = uri.canonical_uri();
        assert_eq!(EvidenceResourceUri::parse(&text).unwrap(), uri);
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
}
