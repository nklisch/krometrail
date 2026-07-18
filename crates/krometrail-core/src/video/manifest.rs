use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{
    ArtifactId, ErrorCode, KrometrailError, NonEmptyText, ResolvedRange, Result, SessionId,
    SessionRange, TargetId, VideoEncodedClip, VideoEncoderIdentity, VideoEncodingProfile,
    VideoPresentationPlan, VideoSegmentSource, validation::delegate_json_schema,
};

pub const TEMPORAL_VIDEO_MANIFEST_SCHEMA_VERSION: u32 = 1;
const VIDEO_MEDIA_TYPE: &str = "video/mp4";
const VIDEO_CODEC: &str = "h264";
const VIDEO_PIXEL_FORMAT: &str = "yuv420p";

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TemporalVideoManifest {
    schema_version: u32,
    artifact_id: ArtifactId,
    session_id: SessionId,
    target_id: TargetId,
    requested_range: SessionRange,
    resolved_range: SessionRange,
    plan: VideoPresentationPlan,
    encoder: VideoEncoderIdentity,
    profile: VideoEncodingProfile,
    media_type: NonEmptyText,
    codec: NonEmptyText,
    pixel_format: NonEmptyText,
    has_audio: bool,
    encoded_byte_len: u64,
    #[serde(serialize_with = "serialize_output_hash")]
    output_hash: temporal_vision::OutputHash,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct TemporalVideoManifestWire {
    schema_version: u32,
    artifact_id: ArtifactId,
    session_id: SessionId,
    target_id: TargetId,
    requested_range: SessionRange,
    resolved_range: SessionRange,
    plan: VideoPresentationPlan,
    encoder: VideoEncoderIdentity,
    profile: VideoEncodingProfile,
    media_type: NonEmptyText,
    codec: NonEmptyText,
    pixel_format: NonEmptyText,
    has_audio: bool,
    encoded_byte_len: u64,
    output_hash: String,
}

impl TemporalVideoManifest {
    pub fn new(
        artifact_id: ArtifactId,
        scope: &ResolvedRange,
        plan: VideoPresentationPlan,
        encoded: &VideoEncodedClip,
    ) -> Result<Self> {
        scope.validate()?;
        if artifact_id.as_uuid().is_nil() {
            return Err(invalid_video("temporal video artifact id must be non-nil"));
        }
        if plan.requested_range() != scope.requested_range
            || plan.resolved_range() != scope.resolved_range
        {
            return Err(invalid_video(
                "temporal video plan ranges must exactly match the resolved scope",
            ));
        }
        let Some(start) = scope
            .frame_ids
            .iter()
            .position(|id| *id == plan.input_frame_ids()[0])
        else {
            return Err(invalid_video(
                "temporal video plan frames must belong to the resolved scope",
            ));
        };
        if scope
            .frame_ids
            .get(start..start + plan.input_frame_ids().len())
            != Some(plan.input_frame_ids())
        {
            return Err(invalid_video(
                "temporal video plan frames must preserve one contiguous resolved-scope epoch",
            ));
        }
        validate_plan_gaps(scope, &plan)?;
        if encoded.profile().geometry() != plan.output() {
            return Err(invalid_video(
                "encoded video profile must exactly match the presentation plan",
            ));
        }
        Self::from_parts(TemporalVideoManifestParts {
            schema_version: TEMPORAL_VIDEO_MANIFEST_SCHEMA_VERSION,
            artifact_id,
            session_id: scope.session_id,
            target_id: scope.target_id,
            requested_range: scope.requested_range,
            resolved_range: scope.resolved_range,
            plan,
            encoder: encoded.identity().clone(),
            profile: encoded.profile(),
            media_type: text(VIDEO_MEDIA_TYPE),
            codec: text(VIDEO_CODEC),
            pixel_format: text(VIDEO_PIXEL_FORMAT),
            has_audio: false,
            encoded_byte_len: encoded.encoded_bytes().len() as u64,
            output_hash: encoded.output_hash(),
        })
    }

    fn from_parts(parts: TemporalVideoManifestParts) -> Result<Self> {
        if parts.schema_version != TEMPORAL_VIDEO_MANIFEST_SCHEMA_VERSION {
            return Err(invalid_video(
                "unsupported temporal video manifest schema version",
            ));
        }
        if parts.artifact_id.as_uuid().is_nil()
            || parts.session_id.as_uuid().is_nil()
            || parts.target_id.as_uuid().is_nil()
        {
            return Err(invalid_video(
                "temporal video manifest identifiers must be non-nil",
            ));
        }
        if parts.resolved_range.start() < parts.requested_range.start()
            || parts.resolved_range.end() > parts.requested_range.end()
            || parts.plan.requested_range() != parts.requested_range
            || parts.plan.resolved_range() != parts.resolved_range
        {
            return Err(invalid_video(
                "temporal video manifest scope must match its embedded plan",
            ));
        }
        if parts.profile.geometry() != parts.plan.output() {
            return Err(invalid_video(
                "temporal video manifest profile must match its embedded plan",
            ));
        }
        if parts.media_type.as_str() != VIDEO_MEDIA_TYPE
            || parts.codec.as_str() != VIDEO_CODEC
            || parts.pixel_format.as_str() != VIDEO_PIXEL_FORMAT
            || parts.has_audio
        {
            return Err(invalid_video(
                "temporal video manifests require silent video/mp4, h264, and yuv420p",
            ));
        }
        if parts.encoded_byte_len == 0 {
            return Err(invalid_video(
                "temporal video manifest encoded length must be non-zero",
            ));
        }
        if parts.encoded_byte_len > parts.profile.max_encoded_bytes()
            || parts.encoded_byte_len > crate::MAX_VIDEO_ENCODED_OUTPUT_BYTES
        {
            return Err(limit_error(
                "temporal video manifest encoded length exceeds its output limit",
            ));
        }
        Ok(Self {
            schema_version: parts.schema_version,
            artifact_id: parts.artifact_id,
            session_id: parts.session_id,
            target_id: parts.target_id,
            requested_range: parts.requested_range,
            resolved_range: parts.resolved_range,
            plan: parts.plan,
            encoder: parts.encoder,
            profile: parts.profile,
            media_type: parts.media_type,
            codec: parts.codec,
            pixel_format: parts.pixel_format,
            has_audio: parts.has_audio,
            encoded_byte_len: parts.encoded_byte_len,
            output_hash: parts.output_hash,
        })
    }

    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub const fn artifact_id(&self) -> ArtifactId {
        self.artifact_id
    }

    pub const fn session_id(&self) -> SessionId {
        self.session_id
    }

    pub const fn target_id(&self) -> TargetId {
        self.target_id
    }

    pub const fn requested_range(&self) -> SessionRange {
        self.requested_range
    }

    pub const fn resolved_range(&self) -> SessionRange {
        self.resolved_range
    }

    pub const fn plan(&self) -> &VideoPresentationPlan {
        &self.plan
    }

    pub const fn encoder(&self) -> &VideoEncoderIdentity {
        &self.encoder
    }

    pub const fn profile(&self) -> VideoEncodingProfile {
        self.profile
    }

    pub fn media_type(&self) -> &str {
        self.media_type.as_str()
    }

    pub fn codec(&self) -> &str {
        self.codec.as_str()
    }

    pub fn pixel_format(&self) -> &str {
        self.pixel_format.as_str()
    }

    pub const fn has_audio(&self) -> bool {
        self.has_audio
    }

    pub const fn encoded_byte_len(&self) -> u64 {
        self.encoded_byte_len
    }

    pub const fn output_hash(&self) -> temporal_vision::OutputHash {
        self.output_hash
    }
}

impl<'de> Deserialize<'de> for TemporalVideoManifest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let wire = TemporalVideoManifestWire::deserialize(deserializer)?;
        deserialize_validated_parts(wire).map_err(serde::de::Error::custom)
    }
}

delegate_json_schema!(TemporalVideoManifest => TemporalVideoManifestWire);

pub fn canonical_video_cache_parameters(
    plan: &VideoPresentationPlan,
    identity: &VideoEncoderIdentity,
    profile: &VideoEncodingProfile,
) -> Result<Arc<[u8]>> {
    if profile.geometry() != plan.output() {
        return Err(invalid_video(
            "video cache profile must exactly match the presentation plan",
        ));
    }
    #[derive(Serialize)]
    struct Limits {
        max_source_duration_nanos: u64,
        max_presentation_duration_nanos: u64,
        max_source_frames: usize,
        max_meaningful_frames: usize,
        max_presentation_segments: usize,
        max_width: u32,
        max_height: u32,
        max_encoded_input_bytes: u64,
        max_encoded_output_bytes: u64,
    }
    #[derive(Serialize)]
    struct Transcript<'a> {
        schema_version: u32,
        plan: &'a VideoPresentationPlan,
        encoder: &'a VideoEncoderIdentity,
        profile: &'a VideoEncodingProfile,
        media_type: &'static str,
        codec: &'static str,
        pixel_format: &'static str,
        has_audio: bool,
        limits: Limits,
    }
    let transcript = Transcript {
        schema_version: TEMPORAL_VIDEO_MANIFEST_SCHEMA_VERSION,
        plan,
        encoder: identity,
        profile,
        media_type: VIDEO_MEDIA_TYPE,
        codec: VIDEO_CODEC,
        pixel_format: VIDEO_PIXEL_FORMAT,
        has_audio: false,
        limits: Limits {
            max_source_duration_nanos: crate::MAX_VIDEO_SOURCE_DURATION.as_nanos() as u64,
            max_presentation_duration_nanos: crate::MAX_VIDEO_PRESENTATION_DURATION.as_nanos()
                as u64,
            max_source_frames: crate::MAX_VIDEO_SOURCE_FRAMES,
            max_meaningful_frames: crate::MAX_VIDEO_MEANINGFUL_FRAMES,
            max_presentation_segments: crate::MAX_VIDEO_PRESENTATION_SEGMENTS,
            max_width: crate::MAX_VIDEO_WIDTH,
            max_height: crate::MAX_VIDEO_HEIGHT,
            max_encoded_input_bytes: crate::MAX_VIDEO_ENCODED_INPUT_BYTES,
            max_encoded_output_bytes: crate::MAX_VIDEO_ENCODED_OUTPUT_BYTES,
        },
    };
    serde_json::to_vec(&transcript)
        .map(Arc::from)
        .map_err(|_| internal_error("could not serialize canonical video cache parameters"))
}

struct TemporalVideoManifestParts {
    schema_version: u32,
    artifact_id: ArtifactId,
    session_id: SessionId,
    target_id: TargetId,
    requested_range: SessionRange,
    resolved_range: SessionRange,
    plan: VideoPresentationPlan,
    encoder: VideoEncoderIdentity,
    profile: VideoEncodingProfile,
    media_type: NonEmptyText,
    codec: NonEmptyText,
    pixel_format: NonEmptyText,
    has_audio: bool,
    encoded_byte_len: u64,
    output_hash: temporal_vision::OutputHash,
}

fn deserialize_validated_parts(wire: TemporalVideoManifestWire) -> Result<TemporalVideoManifest> {
    TemporalVideoManifest::from_parts(TemporalVideoManifestParts {
        schema_version: wire.schema_version,
        artifact_id: wire.artifact_id,
        session_id: wire.session_id,
        target_id: wire.target_id,
        requested_range: wire.requested_range,
        resolved_range: wire.resolved_range,
        plan: wire.plan,
        encoder: wire.encoder,
        profile: wire.profile,
        media_type: wire.media_type,
        codec: wire.codec,
        pixel_format: wire.pixel_format,
        has_audio: wire.has_audio,
        encoded_byte_len: wire.encoded_byte_len,
        output_hash: wire
            .output_hash
            .parse()
            .map_err(|_| invalid_video("temporal video output hash is invalid"))?,
    })
}

fn validate_plan_gaps(scope: &ResolvedRange, plan: &VideoPresentationPlan) -> Result<()> {
    for segment in plan.segments() {
        let VideoSegmentSource::GapSlate {
            gap_ids,
            source_range,
        } = segment.source()
        else {
            continue;
        };
        let mut ranges: Vec<_> = gap_ids
            .iter()
            .map(|id| {
                scope
                    .gaps
                    .iter()
                    .find(|gap| gap.id() == *id)
                    .map(|gap| gap.range())
                    .ok_or_else(|| {
                        invalid_video(
                            "temporal video gap slate references a gap outside the resolved scope",
                        )
                    })
            })
            .collect::<Result<_>>()?;
        ranges.sort_unstable_by_key(|range| (range.start(), range.end()));
        let mut covered_to = source_range.start();
        for range in ranges {
            let start = range.start().max(source_range.start());
            let end = range.end().min(source_range.end());
            if end <= covered_to {
                continue;
            }
            if start > covered_to {
                return Err(invalid_video(
                    "temporal video gap slate range is not covered by its contributing gaps",
                ));
            }
            covered_to = end;
        }
        if covered_to < source_range.end() {
            return Err(invalid_video(
                "temporal video gap slate range is not covered by its contributing gaps",
            ));
        }
    }
    Ok(())
}

fn serialize_output_hash<S: Serializer>(
    value: &temporal_vision::OutputHash,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error> {
    serializer.collect_str(value)
}

fn text(value: &'static str) -> NonEmptyText {
    NonEmptyText::new(value).expect("fixed temporal video media values are non-empty")
}

fn invalid_video(message: impl Into<String>) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::InvalidInput,
        NonEmptyText::new(message.into()).expect("video validation messages are non-empty"),
    )
}

fn limit_error(message: &'static str) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::ResourceLimitExceeded,
        NonEmptyText::new(message).expect("video limit messages are non-empty"),
    )
}

fn internal_error(message: &'static str) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::Internal,
        NonEmptyText::new(message).expect("video internal messages are non-empty"),
    )
}
