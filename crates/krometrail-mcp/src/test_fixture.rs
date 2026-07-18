use std::sync::Arc;

use krometrail_core::{
    ArtifactCacheDisposition, ArtifactId, DeviceScaleFactor, EvidenceScope, FrameId, NonEmptyText,
    OutputLimitsRequest, PixelDimensions, PresentationRange, PresentationTime,
    RangeResolutionOptions, ResolvedRange, SessionId, SessionRange, SessionTime, Sha256Digest,
    TargetId, TemporalRangeAnchorKind, TemporalVideoGenerationClip, TemporalVideoGenerationRequest,
    TemporalVideoGenerationResult, TemporalVideoManifest, VideoArtifactEvidenceHandle,
    VideoArtifactRead, VideoEncodedClip, VideoEncoderIdentity, VideoEncodingProfile,
    VideoOutputGeometry, VideoPresentationPlan, VideoPresentationPolicy, VideoPresentationSegment,
    VideoSegmentSource, VideoTimingBasis, VisualEpoch,
};
use uuid::Uuid;

pub(crate) struct VideoFixture {
    pub(crate) request: TemporalVideoGenerationRequest,
    pub(crate) result: TemporalVideoGenerationResult,
    pub(crate) reads: Vec<VideoArtifactRead>,
}

pub(crate) fn video_fixture() -> VideoFixture {
    let session_id = SessionId::from_uuid(Uuid::from_u128(1));
    let target_id = TargetId::from_uuid(Uuid::from_u128(2));
    let frames = [
        FrameId::from_uuid(Uuid::from_u128(30)),
        FrameId::from_uuid(Uuid::from_u128(31)),
    ];
    let range = SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(10)).unwrap();
    let resolved = ResolvedRange::new(
        session_id,
        target_id,
        TemporalRangeAnchorKind::SessionTime,
        range,
        range,
        frames.to_vec(),
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
    let mut reads = Vec::new();
    let clips = frames
        .into_iter()
        .enumerate()
        .map(|(index, frame_id)| {
            let epoch_index = index as u32;
            let frame_time = SessionTime::from_nanos(2 + index as u64 * 2);
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
                    [index as u8 + 1; 32],
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
                ArtifactId::from_uuid(Uuid::from_u128(50 + index as u128)),
                &resolved,
                plan,
                None,
                &encoded,
            )
            .unwrap();
            let artifact = VideoArtifactEvidenceHandle::new(
                manifest.artifact_id(),
                EvidenceScope::from_range(&resolved).unwrap(),
                NonEmptyText::new("video/mp4").unwrap(),
                Sha256Digest::from_bytes(*manifest.output_hash().as_bytes()),
                manifest.encoded_byte_len(),
                manifest,
            )
            .unwrap();
            reads.push(VideoArtifactRead::new(artifact.clone(), bytes).unwrap());
            TemporalVideoGenerationClip {
                epoch_index,
                cache: ArtifactCacheDisposition::Generated,
                artifact,
            }
        })
        .collect();
    let request = TemporalVideoGenerationRequest::new(
        resolved.clone(),
        VideoPresentationPolicy::RealTime,
        OutputLimitsRequest::new(4, 4, 1024).unwrap(),
    )
    .unwrap();
    VideoFixture {
        request,
        result: TemporalVideoGenerationResult {
            range: resolved,
            clips,
        },
        reads,
    }
}
