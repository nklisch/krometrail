use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{
    ArtifactId, ErrorCode, GapId, KrometrailError, NonEmptyText, ResolvedRange, Result, SessionId,
    SessionRange, TargetId, VideoEncodedClip, VideoEncoderIdentity, VideoEncodingProfile,
    VideoPresentationPlan, VideoPresentationPolicy, VideoSegmentSource,
    validation::{delegate_json_schema, deserialize_validated},
};

pub const TEMPORAL_VIDEO_MANIFEST_SCHEMA_VERSION: u32 = 1;
pub const VIDEO_MEANINGFUL_SELECTOR_NAME: &str = "temporal-video-meaningful-selection";
pub const VIDEO_MEANINGFUL_SELECTOR_VERSION: &str = "v1";
const VIDEO_MEDIA_TYPE: &str = "video/mp4";
const VIDEO_CODEC: &str = "h264";
const VIDEO_PIXEL_FORMAT: &str = "yuv420p";

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct VideoGapEvidence {
    gap_id: GapId,
    source_range: SessionRange,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct VideoGapEvidenceWire {
    gap_id: GapId,
    source_range: SessionRange,
}

impl VideoGapEvidence {
    pub fn new(gap_id: GapId, source_range: SessionRange) -> Result<Self> {
        if gap_id.as_uuid().is_nil() || source_range.start() >= source_range.end() {
            return Err(invalid_video(
                "temporal video gap evidence requires a non-nil id and non-empty range",
            ));
        }
        Ok(Self {
            gap_id,
            source_range,
        })
    }

    pub const fn gap_id(&self) -> GapId {
        self.gap_id
    }

    pub const fn source_range(&self) -> SessionRange {
        self.source_range
    }
}

impl<'de> Deserialize<'de> for VideoGapEvidence {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        deserialize_validated(deserializer, |wire: VideoGapEvidenceWire| {
            Self::new(wire.gap_id, wire.source_range)
        })
    }
}

delegate_json_schema!(VideoGapEvidence => VideoGapEvidenceWire);

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct VideoSelectionIdentity {
    name: NonEmptyText,
    version: NonEmptyText,
    #[serde(serialize_with = "serialize_sha256")]
    parameters_sha256: [u8; 32],
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct VideoSelectionIdentityWire {
    #[schemars(length(min = 1, max = 256))]
    name: String,
    #[schemars(length(min = 1, max = 256))]
    version: String,
    #[schemars(length(min = 64, max = 64), regex(pattern = "^[0-9a-f]{64}$"))]
    parameters_sha256: String,
}

impl VideoSelectionIdentity {
    pub fn new(
        name: impl Into<String>,
        version: impl Into<String>,
        parameters_sha256: [u8; 32],
    ) -> Result<Self> {
        let name = bounded_label(name.into(), "selector name")?;
        let version = bounded_label(version.into(), "selector version")?;
        Ok(Self {
            name,
            version,
            parameters_sha256,
        })
    }

    pub fn meaningful_v1(parameters_sha256: [u8; 32]) -> Self {
        Self::new(
            VIDEO_MEANINGFUL_SELECTOR_NAME,
            VIDEO_MEANINGFUL_SELECTOR_VERSION,
            parameters_sha256,
        )
        .expect("static video selector identity is valid")
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn version(&self) -> &str {
        self.version.as_str()
    }

    pub const fn parameters_sha256(&self) -> &[u8; 32] {
        &self.parameters_sha256
    }
}

impl<'de> Deserialize<'de> for VideoSelectionIdentity {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let wire = VideoSelectionIdentityWire::deserialize(deserializer)?;
        Self::new(
            wire.name,
            wire.version,
            decode_sha256(&wire.parameters_sha256).map_err(serde::de::Error::custom)?,
        )
        .map_err(serde::de::Error::custom)
    }
}

delegate_json_schema!(VideoSelectionIdentity => VideoSelectionIdentityWire);

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TemporalVideoManifest {
    schema_version: u32,
    artifact_id: ArtifactId,
    session_id: SessionId,
    target_id: TargetId,
    requested_range: SessionRange,
    resolved_range: SessionRange,
    gap_evidence: Vec<VideoGapEvidence>,
    selection: Option<VideoSelectionIdentity>,
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
    #[schemars(range(min = 1_u32, max = 1_u32))]
    schema_version: u32,
    artifact_id: ArtifactId,
    session_id: SessionId,
    target_id: TargetId,
    requested_range: SessionRange,
    resolved_range: SessionRange,
    gap_evidence: Vec<VideoGapEvidence>,
    selection: Option<VideoSelectionIdentity>,
    plan: VideoPresentationPlan,
    encoder: VideoEncoderIdentity,
    profile: VideoEncodingProfile,
    #[schemars(regex(pattern = "^video/mp4$"))]
    media_type: String,
    #[schemars(regex(pattern = "^h264$"))]
    codec: String,
    #[schemars(regex(pattern = "^yuv420p$"))]
    pixel_format: String,
    #[schemars(extend("const" = false))]
    has_audio: bool,
    #[schemars(range(min = 1_u64, max = 67_108_864_u64))]
    encoded_byte_len: u64,
    #[schemars(length(min = 64, max = 64), regex(pattern = "^[0-9a-f]{64}$"))]
    output_hash: String,
}

impl TemporalVideoManifest {
    pub fn new(
        artifact_id: ArtifactId,
        scope: &ResolvedRange,
        plan: VideoPresentationPlan,
        selection: Option<VideoSelectionIdentity>,
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
        let gap_evidence = canonical_gap_evidence(scope, plan.presented_source_range())?;
        validate_plan_gaps(&gap_evidence, &plan)?;
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
            gap_evidence,
            selection,
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
        validate_gap_evidence(&parts.gap_evidence, parts.plan.presented_source_range())?;
        validate_plan_gaps(&parts.gap_evidence, &parts.plan)?;
        validate_selection(parts.selection.as_ref(), &parts.plan)?;
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
            gap_evidence: parts.gap_evidence,
            selection: parts.selection,
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

    pub fn gap_evidence(&self) -> &[VideoGapEvidence] {
        &self.gap_evidence
    }

    pub const fn selection(&self) -> Option<&VideoSelectionIdentity> {
        self.selection.as_ref()
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
    selection: Option<&VideoSelectionIdentity>,
) -> Result<Arc<[u8]>> {
    if profile.geometry() != plan.output() {
        return Err(invalid_video(
            "video cache profile must exactly match the presentation plan",
        ));
    }
    validate_selection(selection, plan)?;
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
        selection: Option<&'a VideoSelectionIdentity>,
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
        selection,
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
    gap_evidence: Vec<VideoGapEvidence>,
    selection: Option<VideoSelectionIdentity>,
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
        gap_evidence: wire.gap_evidence,
        selection: wire.selection,
        plan: wire.plan,
        encoder: wire.encoder,
        profile: wire.profile,
        media_type: NonEmptyText::new(wire.media_type)
            .map_err(|_| invalid_video("temporal video media type is empty"))?,
        codec: NonEmptyText::new(wire.codec)
            .map_err(|_| invalid_video("temporal video codec is empty"))?,
        pixel_format: NonEmptyText::new(wire.pixel_format)
            .map_err(|_| invalid_video("temporal video pixel format is empty"))?,
        has_audio: wire.has_audio,
        encoded_byte_len: wire.encoded_byte_len,
        output_hash: wire
            .output_hash
            .parse()
            .map_err(|_| invalid_video("temporal video output hash is invalid"))?,
    })
}

fn canonical_gap_evidence(
    scope: &ResolvedRange,
    presented_source_range: SessionRange,
) -> Result<Vec<VideoGapEvidence>> {
    let mut evidence: Vec<_> = scope
        .gaps
        .iter()
        .filter_map(|gap| {
            let start = gap.range().start().max(presented_source_range.start());
            let end = gap.range().end().min(presented_source_range.end());
            (start < end).then_some((start, end, gap.id()))
        })
        .map(|(start, end, id)| {
            VideoGapEvidence::new(
                id,
                SessionRange::new(start, end).expect("clipped gap is ordered"),
            )
        })
        .collect::<Result<_>>()?;
    evidence
        .sort_unstable_by_key(|gap| (gap.source_range.start(), gap.source_range.end(), gap.gap_id));
    validate_gap_evidence(&evidence, presented_source_range)?;
    Ok(evidence)
}

fn validate_gap_evidence(
    evidence: &[VideoGapEvidence],
    presented_source_range: SessionRange,
) -> Result<()> {
    let mut prior_key = None;
    let mut ids = std::collections::HashSet::with_capacity(evidence.len());
    for gap in evidence {
        let key = (gap.source_range.start(), gap.source_range.end(), gap.gap_id);
        if gap.gap_id.as_uuid().is_nil()
            || !ids.insert(gap.gap_id)
            || gap.source_range.start() < presented_source_range.start()
            || gap.source_range.end() > presented_source_range.end()
            || prior_key.is_some_and(|prior| prior >= key)
        {
            return Err(invalid_video(
                "temporal video gap evidence must be unique, clipped, and canonically ordered",
            ));
        }
        prior_key = Some(key);
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CanonicalGapComponent {
    gap_ids: Vec<GapId>,
    source_range: SessionRange,
}

fn canonical_gap_components(evidence: &[VideoGapEvidence]) -> Result<Vec<CanonicalGapComponent>> {
    let mut components: Vec<CanonicalGapComponent> = Vec::new();
    for gap in evidence {
        if let Some(last) = components.last_mut()
            && gap.source_range.start() <= last.source_range.end()
        {
            last.source_range = SessionRange::new(
                last.source_range.start(),
                last.source_range.end().max(gap.source_range.end()),
            )?;
            last.gap_ids.push(gap.gap_id);
            last.gap_ids.sort_unstable();
            continue;
        }
        components.push(CanonicalGapComponent {
            gap_ids: vec![gap.gap_id],
            source_range: gap.source_range,
        });
    }
    Ok(components)
}

fn validate_plan_gaps(evidence: &[VideoGapEvidence], plan: &VideoPresentationPlan) -> Result<()> {
    let components = canonical_gap_components(evidence)?;
    let plan_gaps: Vec<_> = plan
        .segments()
        .iter()
        .filter_map(|segment| match segment.source() {
            VideoSegmentSource::GapSlate {
                gap_ids,
                source_range,
            } => Some((gap_ids.as_slice(), *source_range)),
            VideoSegmentSource::SourceFrame { .. } => None,
        })
        .collect();
    if plan_gaps.len() != components.len()
        || plan_gaps
            .iter()
            .zip(&components)
            .any(|((ids, range), component)| {
                *ids != component.gap_ids.as_slice() || *range != component.source_range
            })
    {
        return Err(invalid_video(
            "temporal video gap slates must exactly match canonical gap contributor evidence",
        ));
    }
    Ok(())
}

fn validate_selection(
    selection: Option<&VideoSelectionIdentity>,
    plan: &VideoPresentationPlan,
) -> Result<()> {
    match (
        plan.policy(),
        selection,
        plan.meaningful_frame_ids().is_empty(),
    ) {
        (VideoPresentationPolicy::RealTime, None, true)
        | (VideoPresentationPolicy::ModelOptimized, Some(_), false) => Ok(()),
        _ => Err(invalid_video(
            "real-time video forbids selector provenance and model-optimized video requires selector provenance and meaningful frame ids",
        )),
    }
}

fn bounded_label(value: String, label: &'static str) -> Result<NonEmptyText> {
    if value.len() > crate::MAX_VIDEO_ENCODER_LABEL_BYTES
        || value.bytes().any(|byte| byte.is_ascii_control())
        || value.contains(['/', '\\'])
    {
        return Err(invalid_video(format!(
            "temporal video {label} must be bounded and path-free"
        )));
    }
    NonEmptyText::new(value)
        .map_err(|_| invalid_video(format!("temporal video {label} must be non-empty")))
}

fn serialize_sha256<S: Serializer>(
    value: &[u8; 32],
    serializer: S,
) -> std::result::Result<S::Ok, S::Error> {
    serializer.collect_str(&hex_sha256(value))
}

fn hex_sha256(value: &[u8; 32]) -> String {
    use std::fmt::Write as _;
    let mut output = String::with_capacity(64);
    for byte in value {
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

fn decode_sha256(value: &str) -> Result<[u8; 32]> {
    if value.len() != 64
        || value
            .bytes()
            .any(|byte| !byte.is_ascii_digit() && !(b'a'..=b'f').contains(&byte))
    {
        return Err(invalid_video(
            "temporal video selector hash must be lowercase SHA-256",
        ));
    }
    let mut bytes = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let nibble = |byte| match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            _ => unreachable!("validated lowercase hexadecimal"),
        };
        bytes[index] = (nibble(pair[0]) << 4) | nibble(pair[1]);
    }
    Ok(bytes)
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
