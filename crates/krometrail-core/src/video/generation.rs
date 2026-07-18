use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize};

use crate::{
    ArtifactCacheDisposition, ArtifactGenerationContext, ArtifactId, ErrorCode, EvidenceScope,
    KrometrailError, MAX_VIDEO_ENCODED_OUTPUT_BYTES, MAX_VIDEO_HEIGHT, MAX_VIDEO_SOURCE_DURATION,
    MAX_VIDEO_SOURCE_FRAMES, MAX_VIDEO_WIDTH, NonEmptyText, OutputLimitsRequest, PortFuture,
    ResolvedRange, Result, Sha256Digest, TemporalVideoManifest, VideoPresentationPolicy,
    validation::{delegate_json_schema, deserialize_validated},
};

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TemporalVideoGenerationRequest {
    range: ResolvedRange,
    policy: VideoPresentationPolicy,
    output: OutputLimitsRequest,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct TemporalVideoGenerationRequestWire {
    range: ResolvedRange,
    policy: VideoPresentationPolicy,
    output: OutputLimitsRequest,
}

impl TemporalVideoGenerationRequest {
    pub fn new(
        range: ResolvedRange,
        policy: VideoPresentationPolicy,
        output: OutputLimitsRequest,
    ) -> Result<Self> {
        range.validate()?;
        let duration = range
            .resolved_range
            .end()
            .as_nanos()
            .saturating_sub(range.resolved_range.start().as_nanos());
        if duration > MAX_VIDEO_SOURCE_DURATION.as_nanos() as u64
            || range.frame_ids.len() > MAX_VIDEO_SOURCE_FRAMES
            || output.max_width() > MAX_VIDEO_WIDTH
            || output.max_height() > MAX_VIDEO_HEIGHT
            || output.max_width() < 2
            || output.max_height() < 2
            || output.max_encoded_bytes() > MAX_VIDEO_ENCODED_OUTPUT_BYTES
        {
            return Err(limit_error(
                "temporal video request exceeds the fixed duration, frame, geometry, or output limit",
            ));
        }
        Ok(Self {
            range,
            policy,
            output,
        })
    }

    pub const fn range(&self) -> &ResolvedRange {
        &self.range
    }

    pub const fn policy(&self) -> VideoPresentationPolicy {
        self.policy
    }

    pub const fn output(&self) -> OutputLimitsRequest {
        self.output
    }
}

impl<'de> Deserialize<'de> for TemporalVideoGenerationRequest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        deserialize_validated(deserializer, |wire: TemporalVideoGenerationRequestWire| {
            Self::new(wire.range, wire.policy, wire.output)
        })
    }
}

delegate_json_schema!(TemporalVideoGenerationRequest => TemporalVideoGenerationRequestWire);

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct VideoArtifactEvidenceHandle {
    pub artifact_id: ArtifactId,
    pub scope: EvidenceScope,
    pub media_type: NonEmptyText,
    pub content_sha256: Sha256Digest,
    pub encoded_byte_len: u64,
    pub provenance: TemporalVideoManifest,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct VideoArtifactEvidenceHandleWire {
    artifact_id: ArtifactId,
    scope: EvidenceScope,
    media_type: NonEmptyText,
    content_sha256: Sha256Digest,
    encoded_byte_len: u64,
    provenance: TemporalVideoManifest,
}

impl VideoArtifactEvidenceHandle {
    pub fn new(
        artifact_id: ArtifactId,
        scope: EvidenceScope,
        media_type: NonEmptyText,
        content_sha256: Sha256Digest,
        encoded_byte_len: u64,
        provenance: TemporalVideoManifest,
    ) -> Result<Self> {
        if artifact_id.as_uuid().is_nil()
            || artifact_id != provenance.artifact_id()
            || scope.session_id != provenance.session_id()
            || scope.target_id != provenance.target_id()
            || media_type.as_str() != "video/mp4"
            || content_sha256.as_bytes() != provenance.output_hash().as_bytes()
            || encoded_byte_len == 0
            || encoded_byte_len != provenance.encoded_byte_len()
        {
            return Err(invalid(
                "temporal video handle must exactly match its scope, media, digest, length, and provenance",
            ));
        }
        Ok(Self {
            artifact_id,
            scope,
            media_type,
            content_sha256,
            encoded_byte_len,
            provenance,
        })
    }
}

impl<'de> Deserialize<'de> for VideoArtifactEvidenceHandle {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        deserialize_validated(deserializer, |wire: VideoArtifactEvidenceHandleWire| {
            Self::new(
                wire.artifact_id,
                wire.scope,
                wire.media_type,
                wire.content_sha256,
                wire.encoded_byte_len,
                wire.provenance,
            )
        })
    }
}

delegate_json_schema!(VideoArtifactEvidenceHandle => VideoArtifactEvidenceHandleWire);

#[derive(Clone, Debug, PartialEq)]
pub struct VideoArtifactRead {
    pub handle: VideoArtifactEvidenceHandle,
    encoded_bytes: Arc<[u8]>,
}

impl VideoArtifactRead {
    pub fn new(
        handle: VideoArtifactEvidenceHandle,
        encoded_bytes: impl Into<Arc<[u8]>>,
    ) -> Result<Self> {
        let encoded_bytes = encoded_bytes.into();
        if encoded_bytes.len() as u64 != handle.encoded_byte_len
            || Sha256Digest::digest(&encoded_bytes) != handle.content_sha256
        {
            return Err(invalid(
                "temporal video payload must match its handle length and SHA-256",
            ));
        }
        Ok(Self {
            handle,
            encoded_bytes,
        })
    }

    pub fn encoded_bytes(&self) -> &[u8] {
        &self.encoded_bytes
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, schemars::JsonSchema)]
pub struct TemporalVideoGenerationClip {
    pub epoch_index: u32,
    pub cache: ArtifactCacheDisposition,
    pub artifact: VideoArtifactEvidenceHandle,
}

#[derive(Clone, Debug, PartialEq, Serialize, schemars::JsonSchema)]
pub struct TemporalVideoGenerationResult {
    pub range: ResolvedRange,
    pub clips: Vec<TemporalVideoGenerationClip>,
}

pub trait TemporalVideoGeneration: Send + Sync {
    fn generate_video(
        &self,
        request: TemporalVideoGenerationRequest,
        context: ArtifactGenerationContext,
    ) -> PortFuture<'_, Result<TemporalVideoGenerationResult>>;
}

fn invalid(message: &'static str) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::InvalidInput,
        NonEmptyText::new(message).expect("video validation message is non-empty"),
    )
}

fn limit_error(message: &'static str) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::ResourceLimitExceeded,
        NonEmptyText::new(message).expect("video limit message is non-empty"),
    )
}
