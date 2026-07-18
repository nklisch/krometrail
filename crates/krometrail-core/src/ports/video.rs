use std::{sync::Arc, time::Instant};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};

use crate::{
    ErrorCode, ImageFormat, KrometrailError, NonEmptyText, PixelDimensions, Result,
    VideoOutputGeometry, VideoPresentationPlan, VideoSegmentSource,
    ports::{CancellationSignal, PortFuture},
    validation::{delegate_json_schema, deserialize_validated},
};

pub const MAX_VIDEO_ENCODER_LABEL_BYTES: usize = 256;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct VideoEncoderIdentity {
    implementation_version: NonEmptyText,
    #[serde(serialize_with = "serialize_sha256")]
    build_report_sha256: [u8; 32],
    encoder_name: NonEmptyText,
    adapter_version: NonEmptyText,
    argument_policy_version: NonEmptyText,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct VideoEncoderIdentityWire {
    #[schemars(
        length(min = 1, max = 256),
        regex(pattern = r"^[^/\\\u0000-\u001F\u007F]+$")
    )]
    implementation_version: String,
    #[schemars(length(min = 64, max = 64), regex(pattern = "^[0-9a-f]{64}$"))]
    build_report_sha256: String,
    #[schemars(
        length(min = 1, max = 256),
        regex(pattern = r"^[^/\\\u0000-\u001F\u007F]+$")
    )]
    encoder_name: String,
    #[schemars(
        length(min = 1, max = 256),
        regex(pattern = r"^[^/\\\u0000-\u001F\u007F]+$")
    )]
    adapter_version: String,
    #[schemars(
        length(min = 1, max = 256),
        regex(pattern = r"^[^/\\\u0000-\u001F\u007F]+$")
    )]
    argument_policy_version: String,
}

impl VideoEncoderIdentity {
    pub fn new(
        implementation_version: impl Into<String>,
        build_report_sha256: [u8; 32],
        encoder_name: impl Into<String>,
        adapter_version: impl Into<String>,
        argument_policy_version: impl Into<String>,
    ) -> Result<Self> {
        let value = Self {
            implementation_version: NonEmptyText::new(implementation_version.into())
                .map_err(|_| invalid_video("video encoder implementation version is empty"))?,
            build_report_sha256,
            encoder_name: NonEmptyText::new(encoder_name.into())
                .map_err(|_| invalid_video("video encoder name is empty"))?,
            adapter_version: NonEmptyText::new(adapter_version.into())
                .map_err(|_| invalid_video("video encoder adapter version is empty"))?,
            argument_policy_version: NonEmptyText::new(argument_policy_version.into())
                .map_err(|_| invalid_video("video encoder argument policy version is empty"))?,
        };
        for (label, text) in [
            (
                "implementation version",
                value.implementation_version.as_str(),
            ),
            ("encoder name", value.encoder_name.as_str()),
            ("adapter version", value.adapter_version.as_str()),
            (
                "argument policy version",
                value.argument_policy_version.as_str(),
            ),
        ] {
            validate_identity_label(label, text)?;
        }
        Ok(value)
    }

    pub fn implementation_version(&self) -> &str {
        self.implementation_version.as_str()
    }

    pub const fn build_report_sha256(&self) -> &[u8; 32] {
        &self.build_report_sha256
    }

    pub fn encoder_name(&self) -> &str {
        self.encoder_name.as_str()
    }

    pub fn adapter_version(&self) -> &str {
        self.adapter_version.as_str()
    }

    pub fn argument_policy_version(&self) -> &str {
        self.argument_policy_version.as_str()
    }
}

impl<'de> Deserialize<'de> for VideoEncoderIdentity {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let wire = VideoEncoderIdentityWire::deserialize(deserializer)?;
        Self::new(
            wire.implementation_version,
            parse_sha256(&wire.build_report_sha256).map_err(serde::de::Error::custom)?,
            wire.encoder_name,
            wire.adapter_version,
            wire.argument_policy_version,
        )
        .map_err(serde::de::Error::custom)
    }
}

delegate_json_schema!(VideoEncoderIdentity => VideoEncoderIdentityWire);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct VideoEncodingProfile {
    geometry: VideoOutputGeometry,
    max_encoded_bytes: u64,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct VideoEncodingProfileWire {
    geometry: VideoOutputGeometry,
    #[schemars(range(min = 1_u64, max = 67_108_864_u64))]
    max_encoded_bytes: u64,
}

impl VideoEncodingProfile {
    pub fn new(geometry: VideoOutputGeometry, max_encoded_bytes: u64) -> Result<Self> {
        validate_output_size(max_encoded_bytes)?;
        Ok(Self {
            geometry,
            max_encoded_bytes,
        })
    }

    pub const fn geometry(self) -> VideoOutputGeometry {
        self.geometry
    }

    pub const fn max_encoded_bytes(self) -> u64 {
        self.max_encoded_bytes
    }
}

impl<'de> Deserialize<'de> for VideoEncodingProfile {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        deserialize_validated(deserializer, |wire: VideoEncodingProfileWire| {
            Self::new(wire.geometry, wire.max_encoded_bytes)
        })
    }
}

delegate_json_schema!(VideoEncodingProfile => VideoEncodingProfileWire);

#[derive(Clone, Debug, PartialEq)]
pub struct VideoEncodeFrame {
    segment_index: u32,
    source: VideoSegmentSource,
    format: ImageFormat,
    dimensions: PixelDimensions,
    bytes: Arc<[u8]>,
}

impl VideoEncodeFrame {
    pub fn new(
        segment_index: u32,
        source: VideoSegmentSource,
        format: ImageFormat,
        dimensions: PixelDimensions,
        bytes: impl Into<Arc<[u8]>>,
    ) -> Result<Self> {
        let bytes = bytes.into();
        if bytes.is_empty() {
            return Err(invalid_video(
                "video encoder input frames must contain encoded image bytes",
            ));
        }
        Ok(Self {
            segment_index,
            source,
            format,
            dimensions,
            bytes,
        })
    }

    pub const fn segment_index(&self) -> u32 {
        self.segment_index
    }

    pub const fn source(&self) -> &VideoSegmentSource {
        &self.source
    }

    pub const fn format(&self) -> ImageFormat {
        self.format
    }

    pub const fn dimensions(&self) -> PixelDimensions {
        self.dimensions
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn encoded_bytes(&self) -> Arc<[u8]> {
        Arc::clone(&self.bytes)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct VideoEncodeRequest {
    plan: VideoPresentationPlan,
    frames: Vec<VideoEncodeFrame>,
    profile: VideoEncodingProfile,
}

impl VideoEncodeRequest {
    pub fn new(
        plan: VideoPresentationPlan,
        frames: Vec<VideoEncodeFrame>,
        profile: VideoEncodingProfile,
    ) -> Result<Self> {
        if profile.geometry() != plan.output() {
            return Err(invalid_video(
                "video encoding profile geometry must exactly match the presentation plan",
            ));
        }
        if frames.len() != plan.segments().len() {
            return Err(invalid_video(
                "video encoding requires one encoded image for every presentation segment",
            ));
        }
        validate_input_size(frames.iter().map(|frame| frame.bytes.len() as u64))?;
        for (segment, frame) in plan.segments().iter().zip(&frames) {
            if frame.segment_index != segment.index() {
                return Err(invalid_video(
                    "video encoded images must preserve exact presentation segment order",
                ));
            }
            if frame.source != *segment.source() {
                return Err(invalid_video(
                    "video encoded images must match the exact source identity of their presentation segment",
                ));
            }
            let expected_dimensions = match segment.source() {
                VideoSegmentSource::SourceFrame { .. } => plan.output().source(),
                VideoSegmentSource::GapSlate { .. } => {
                    if frame.format != ImageFormat::Png {
                        return Err(invalid_video("video gap slates must be encoded as PNG"));
                    }
                    plan.output().canvas()
                }
            };
            if frame.dimensions != expected_dimensions {
                return Err(invalid_video(
                    "video encoded image dimensions contradict their presentation segment",
                ));
            }
        }
        Ok(Self {
            plan,
            frames,
            profile,
        })
    }

    pub const fn plan(&self) -> &VideoPresentationPlan {
        &self.plan
    }

    pub fn frames(&self) -> &[VideoEncodeFrame] {
        &self.frames
    }

    pub const fn profile(&self) -> VideoEncodingProfile {
        self.profile
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct VideoEncodedClip {
    identity: VideoEncoderIdentity,
    profile: VideoEncodingProfile,
    output_hash: temporal_vision::OutputHash,
    encoded_bytes: Arc<[u8]>,
}

impl VideoEncodedClip {
    pub fn new(
        identity: VideoEncoderIdentity,
        profile: VideoEncodingProfile,
        output_hash: temporal_vision::OutputHash,
        encoded_bytes: impl Into<Arc<[u8]>>,
    ) -> Result<Self> {
        let encoded_bytes = encoded_bytes.into();
        validate_output_size(encoded_bytes.len() as u64)?;
        if encoded_bytes.len() as u64 > profile.max_encoded_bytes {
            return Err(limit_error(
                "encoded video exceeds the request output byte limit",
            ));
        }
        let actual: [u8; 32] = Sha256::digest(&encoded_bytes).into();
        if output_hash.as_bytes() != &actual {
            return Err(invalid_video(
                "encoded video output hash must match the exact returned bytes",
            ));
        }
        Ok(Self {
            identity,
            profile,
            output_hash,
            encoded_bytes,
        })
    }

    pub const fn identity(&self) -> &VideoEncoderIdentity {
        &self.identity
    }

    pub const fn profile(&self) -> VideoEncodingProfile {
        self.profile
    }

    pub const fn output_hash(&self) -> temporal_vision::OutputHash {
        self.output_hash
    }

    pub fn encoded_bytes(&self) -> &[u8] {
        &self.encoded_bytes
    }

    pub fn owned_encoded_bytes(&self) -> Arc<[u8]> {
        Arc::clone(&self.encoded_bytes)
    }
}

pub struct VideoEncodingContext {
    pub deadline: Instant,
    pub cancellation: Arc<dyn CancellationSignal>,
}

pub trait TemporalVideoEncoder: Send + Sync {
    fn identity(&self) -> &VideoEncoderIdentity;

    fn encode(
        &self,
        request: VideoEncodeRequest,
        context: VideoEncodingContext,
    ) -> PortFuture<'_, Result<VideoEncodedClip>>;
}

fn validate_identity_label(label: &str, value: &str) -> Result<()> {
    if value.len() > MAX_VIDEO_ENCODER_LABEL_BYTES {
        return Err(limit_error(
            "video encoder identity label exceeds the 256 byte server limit",
        ));
    }
    if value.chars().any(char::is_control) || value.contains('/') || value.contains('\\') {
        return Err(invalid_video(format!(
            "video encoder {label} must not contain control characters or path separators"
        )));
    }
    Ok(())
}

fn validate_input_size(lengths: impl IntoIterator<Item = u64>) -> Result<u64> {
    let mut total = 0_u64;
    for length in lengths {
        total = total
            .checked_add(length)
            .ok_or_else(|| limit_error("video encoder input byte accounting overflowed"))?;
        if total > crate::MAX_VIDEO_ENCODED_INPUT_BYTES {
            return Err(limit_error(
                "video encoder input exceeds the 512 MiB server limit",
            ));
        }
    }
    Ok(total)
}

fn validate_output_size(size: u64) -> Result<()> {
    if size == 0 {
        return Err(invalid_video("encoded video output must not be empty"));
    }
    if size > crate::MAX_VIDEO_ENCODED_OUTPUT_BYTES {
        return Err(limit_error("encoded video exceeds the 64 MiB server limit"));
    }
    Ok(())
}

fn serialize_sha256<S: Serializer>(
    value: &[u8; 32],
    serializer: S,
) -> std::result::Result<S::Ok, S::Error> {
    serializer.collect_str(&Sha256Display(value))
}

struct Sha256Display<'a>(&'a [u8; 32]);

impl std::fmt::Display for Sha256Display<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

fn parse_sha256(value: &str) -> Result<[u8; 32]> {
    if value.len() != 64
        || value
            .bytes()
            .any(|byte| !byte.is_ascii_digit() && !(b'a'..=b'f').contains(&byte))
    {
        return Err(invalid_video(
            "video encoder build hash must be 64 lowercase hexadecimal characters",
        ));
    }
    let mut bytes = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let nibble = |byte| match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            _ => 0,
        };
        bytes[index] = (nibble(pair[0]) << 4) | nibble(pair[1]);
    }
    Ok(bytes)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        DeviceScaleFactor, FrameId, SessionRange, SessionTime, VideoPresentationPolicy,
        VideoPresentationSegment, VideoTimingBasis, VisualEpoch,
    };
    use std::{
        pin::Pin,
        task::{Context, Poll},
    };
    use uuid::Uuid;

    fn geometry() -> VideoOutputGeometry {
        let dimensions = PixelDimensions::new(4, 4).unwrap();
        VideoOutputGeometry::new(dimensions, dimensions, dimensions).unwrap()
    }

    fn plan() -> VideoPresentationPlan {
        let frame_id = FrameId::from_uuid(Uuid::from_u128(3));
        let source =
            VideoSegmentSource::source_frame(frame_id, SessionTime::from_nanos(2)).unwrap();
        let segment = VideoPresentationSegment::new(
            0,
            source,
            crate::PresentationRange::new(
                crate::PresentationTime::ZERO,
                crate::PresentationTime::from_nanos(250_000_000).unwrap(),
            )
            .unwrap(),
            VideoTimingBasis::TerminalHold,
        )
        .unwrap();
        VideoPresentationPlan::new(
            VideoPresentationPolicy::RealTime,
            SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(5)).unwrap(),
            SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(5)).unwrap(),
            SessionRange::new(SessionTime::from_nanos(2), SessionTime::from_nanos(2)).unwrap(),
            VisualEpoch {
                index: 0,
                frame_ids: vec![frame_id],
                image: geometry().source(),
                viewport: geometry().source(),
                device_scale_factor: DeviceScaleFactor::new(1.0).unwrap(),
            },
            vec![frame_id],
            vec![SessionTime::from_nanos(2)],
            vec![],
            vec![segment],
            geometry(),
        )
        .unwrap()
    }

    fn two_frame_plan() -> VideoPresentationPlan {
        let first = FrameId::from_uuid(Uuid::from_u128(3));
        let second = FrameId::from_uuid(Uuid::from_u128(4));
        let segments = vec![
            VideoPresentationSegment::new(
                0,
                VideoSegmentSource::source_frame(first, SessionTime::from_nanos(2)).unwrap(),
                crate::PresentationRange::new(
                    crate::PresentationTime::ZERO,
                    crate::PresentationTime::from_nanos(2).unwrap(),
                )
                .unwrap(),
                VideoTimingBasis::RecordedDelta,
            )
            .unwrap(),
            VideoPresentationSegment::new(
                1,
                VideoSegmentSource::source_frame(second, SessionTime::from_nanos(4)).unwrap(),
                crate::PresentationRange::new(
                    crate::PresentationTime::from_nanos(2).unwrap(),
                    crate::PresentationTime::from_nanos(250_000_002).unwrap(),
                )
                .unwrap(),
                VideoTimingBasis::TerminalHold,
            )
            .unwrap(),
        ];
        VideoPresentationPlan::new(
            VideoPresentationPolicy::RealTime,
            SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(5)).unwrap(),
            SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(5)).unwrap(),
            SessionRange::new(SessionTime::from_nanos(2), SessionTime::from_nanos(4)).unwrap(),
            VisualEpoch {
                index: 0,
                frame_ids: vec![first, second],
                image: geometry().source(),
                viewport: geometry().source(),
                device_scale_factor: DeviceScaleFactor::new(1.0).unwrap(),
            },
            vec![first, second],
            vec![SessionTime::from_nanos(2), SessionTime::from_nanos(4)],
            vec![],
            segments,
            geometry(),
        )
        .unwrap()
    }

    fn identity() -> VideoEncoderIdentity {
        VideoEncoderIdentity::new("ffmpeg 7", [7; 32], "libx264", "adapter-v1", "args-v1").unwrap()
    }

    fn profile() -> VideoEncodingProfile {
        VideoEncodingProfile::new(geometry(), 1024).unwrap()
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

    struct FakeEncoder {
        identity: VideoEncoderIdentity,
    }

    impl TemporalVideoEncoder for FakeEncoder {
        fn identity(&self) -> &VideoEncoderIdentity {
            &self.identity
        }

        fn encode(
            &self,
            request: VideoEncodeRequest,
            _context: VideoEncodingContext,
        ) -> PortFuture<'_, Result<VideoEncodedClip>> {
            let identity = self.identity.clone();
            Box::pin(async move {
                let bytes: Arc<[u8]> = Arc::from([1_u8, 2, 3]);
                let hash = temporal_vision::OutputHash::from_bytes(Sha256::digest(&bytes).into());
                VideoEncodedClip::new(identity, request.profile(), hash, bytes)
            })
        }
    }

    fn block_on<T>(future: PortFuture<'_, T>) -> T {
        let mut context = Context::from_waker(std::task::Waker::noop());
        let mut future = future;
        loop {
            match Pin::new(&mut future).poll(&mut context) {
                Poll::Ready(value) => return value,
                Poll::Pending => std::thread::yield_now(),
            }
        }
    }

    #[test]
    fn encoder_identity_round_trips_without_path_material() {
        let identity =
            VideoEncoderIdentity::new("ffmpeg 7.1", [0xab; 32], "libx264", "adapter-v1", "args-v1")
                .unwrap();
        let json = serde_json::to_string(&identity).unwrap();
        assert!(json.contains(&"ab".repeat(32)));
        assert!(!json.contains("path"));
        assert_eq!(
            serde_json::from_str::<VideoEncoderIdentity>(&json).unwrap(),
            identity
        );
        assert!(VideoEncoderIdentity::new("../ffmpeg", [0; 32], "h264", "v1", "v1").is_err());
        assert!(VideoEncoderIdentity::new("v1\n", [0; 32], "h264", "v1", "v1").is_err());
    }

    #[test]
    fn byte_limit_helpers_accept_exact_boundaries_and_reject_next_units() {
        assert_eq!(
            validate_input_size([crate::MAX_VIDEO_ENCODED_INPUT_BYTES]).unwrap(),
            crate::MAX_VIDEO_ENCODED_INPUT_BYTES
        );
        assert_eq!(
            validate_input_size([crate::MAX_VIDEO_ENCODED_INPUT_BYTES + 1])
                .unwrap_err()
                .code,
            ErrorCode::ResourceLimitExceeded
        );
        assert!(validate_output_size(crate::MAX_VIDEO_ENCODED_OUTPUT_BYTES).is_ok());
        assert_eq!(
            validate_output_size(crate::MAX_VIDEO_ENCODED_OUTPUT_BYTES + 1)
                .unwrap_err()
                .code,
            ErrorCode::ResourceLimitExceeded
        );
    }

    #[test]
    fn request_matches_every_segment_and_object_safe_fake_encodes() {
        let request = VideoEncodeRequest::new(
            plan(),
            vec![
                VideoEncodeFrame::new(
                    0,
                    plan().segments()[0].source().clone(),
                    ImageFormat::Jpeg,
                    geometry().source(),
                    vec![9],
                )
                .unwrap(),
            ],
            profile(),
        )
        .unwrap();
        let encoder: Arc<dyn TemporalVideoEncoder> = Arc::new(FakeEncoder {
            identity: identity(),
        });
        let encoded = block_on(encoder.encode(
            request,
            VideoEncodingContext {
                deadline: Instant::now(),
                cancellation: Arc::new(NeverCancelled),
            },
        ))
        .unwrap();
        assert_eq!(encoded.identity(), encoder.identity());
        assert_eq!(encoded.encoded_bytes(), &[1, 2, 3]);
    }

    #[test]
    fn request_rejects_missing_reordered_and_geometry_drift() {
        assert!(VideoEncodeRequest::new(plan(), vec![], profile()).is_err());
        assert!(
            VideoEncodeRequest::new(
                plan(),
                vec![
                    VideoEncodeFrame::new(
                        1,
                        plan().segments()[0].source().clone(),
                        ImageFormat::Png,
                        geometry().source(),
                        vec![1],
                    )
                    .unwrap()
                ],
                profile(),
            )
            .is_err()
        );
        assert!(
            VideoEncodeRequest::new(
                plan(),
                vec![
                    VideoEncodeFrame::new(
                        0,
                        plan().segments()[0].source().clone(),
                        ImageFormat::Png,
                        PixelDimensions::new(2, 2).unwrap(),
                        vec![1],
                    )
                    .unwrap()
                ],
                profile(),
            )
            .is_err()
        );
    }

    #[test]
    fn request_rejects_swapped_same_geometry_source_frames() {
        let plan = two_frame_plan();
        let frames = vec![
            VideoEncodeFrame::new(
                0,
                plan.segments()[1].source().clone(),
                ImageFormat::Png,
                geometry().source(),
                vec![1],
            )
            .unwrap(),
            VideoEncodeFrame::new(
                1,
                plan.segments()[0].source().clone(),
                ImageFormat::Png,
                geometry().source(),
                vec![2],
            )
            .unwrap(),
        ];
        assert!(VideoEncodeRequest::new(plan, frames, profile()).is_err());
    }

    #[test]
    fn encoded_clip_rejects_hash_and_profile_limits() {
        assert!(
            VideoEncodedClip::new(
                identity(),
                profile(),
                temporal_vision::OutputHash::from_bytes([0; 32]),
                vec![1, 2, 3],
            )
            .is_err()
        );
        let bytes = vec![1_u8; 4];
        let hash = temporal_vision::OutputHash::from_bytes(Sha256::digest(&bytes).into());
        let tiny_profile = VideoEncodingProfile::new(geometry(), 3).unwrap();
        assert_eq!(
            VideoEncodedClip::new(identity(), tiny_profile, hash, bytes)
                .unwrap_err()
                .code,
            ErrorCode::ResourceLimitExceeded
        );
    }

    #[test]
    fn video_errors_have_stable_retry_defaults_and_recovery() {
        assert_eq!(
            ErrorCode::VideoEncoderUnavailable.default_retry(),
            crate::RetryAdvice::AfterRecovery
        );
        assert_eq!(
            ErrorCode::VideoEncodingFailed.default_retry(),
            crate::RetryAdvice::Safe
        );
        assert!(
            ErrorCode::VideoEncoderUnavailable
                .default_recovery()
                .unwrap()
                .contains("FFmpeg")
        );
        for code in [
            ErrorCode::VideoEncoderUnavailable,
            ErrorCode::VideoEncodingFailed,
        ] {
            let json = serde_json::to_string(&code).unwrap();
            assert_eq!(serde_json::from_str::<ErrorCode>(&json).unwrap(), code);
        }
    }
}
