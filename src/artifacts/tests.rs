use image::ImageEncoder;
use krometrail_core::{
    ArtifactId, ArtifactMarker, ArtifactMarkerId, CaptureGap, CaptureGapReason, CaptureOrdinal,
    CapturedFrame, DeviceScaleFactor, EncodedFrame, FrameId, ImageFormat, NonEmptyText,
    ObservedTime, PixelDimensions, RangeResolutionOptions, ResolvedRange, SessionId, SessionRange,
    SessionTime, TargetId, TemporalRangeAnchorKind,
};
use std::num::NonZeroU64;
use uuid::Uuid;

use super::{
    decode::{DECODER_PROFILE, DecodeLimits, decode_frame},
    epoch::{
        ADAPTER_VERSION, AdaptationLimits, WorkCancellation, validate_and_partition,
        validate_and_plan,
    },
};
use temporal_vision::{
    ArtifactLabels, IntegerScale, MeasurementParameters, NormalizationParameters, RenderLimits,
    StoryboardParameters, StoryboardTileLimit, normalize_sequence,
};

const JPEG: &[u8] = include_bytes!("../../tests/fixtures/artifacts/chrome-rgb.jpg");
const PNG: &[u8] = include_bytes!("../../tests/fixtures/artifacts/chrome-rgba.png");
const MALFORMED: &[u8] = include_bytes!("../../tests/fixtures/artifacts/malformed.jpg");
const BOMB: &[u8] = include_bytes!("../../tests/fixtures/artifacts/bomb-header.png");

fn metadata(
    id: u128,
    ordinal: u64,
    time: u64,
    format: ImageFormat,
    image: (u32, u32),
    viewport: (u32, u32),
    scale: f64,
) -> CapturedFrame {
    CapturedFrame::new(
        FrameId::from_uuid(Uuid::from_u128(id)),
        SessionId::from_uuid(Uuid::from_u128(100)),
        TargetId::from_uuid(Uuid::from_u128(101)),
        CaptureOrdinal::new(ordinal).unwrap(),
        None,
        ObservedTime::from_nanos(time + 10),
        SessionTime::from_nanos(time),
        format,
        PixelDimensions::new(image.0, image.1).unwrap(),
        PixelDimensions::new(viewport.0, viewport.1).unwrap(),
        DeviceScaleFactor::new(scale).unwrap(),
        vec![],
    )
    .unwrap()
}

fn frame(metadata: CapturedFrame, bytes: &[u8]) -> EncodedFrame {
    EncodedFrame::new(metadata, bytes.to_vec()).unwrap()
}

fn decode_limits() -> DecodeLimits {
    DecodeLimits::new(8192, 16_777_216, 64 * 1024 * 1024, 64 * 1024 * 1024)
}

fn adaptation_limits() -> AdaptationLimits {
    AdaptationLimits {
        max_source_frames: 120,
        max_encoded_source_bytes: 512 * 1024 * 1024,
        max_dimension: 8192,
        max_pixels_per_frame: 16_777_216,
        max_decoded_bytes: 512 * 1024 * 1024,
        max_markers: 256,
    }
}

fn range(frames: &[EncodedFrame], gaps: Vec<CaptureGap>) -> ResolvedRange {
    let start = frames.first().unwrap().metadata().session_time();
    let end = frames.last().unwrap().metadata().session_time();
    let session_range = SessionRange::new(start, end).unwrap();
    ResolvedRange::new(
        frames[0].metadata().session_id(),
        frames[0].metadata().target_id(),
        TemporalRangeAnchorKind::SessionTime,
        session_range,
        session_range,
        frames.iter().map(|frame| frame.metadata().id()).collect(),
        vec![],
        vec![],
        vec![],
        gaps,
        vec![],
        RangeResolutionOptions::DEFAULT,
    )
    .unwrap()
}

#[test]
fn real_jpeg_and_png_decode_to_declared_straight_rgba8() {
    assert!(DECODER_PROFILE.contains("image-0.25.9"));
    assert_eq!(ADAPTER_VERSION, "krometrail-artifact-adapter-v3");
    let jpeg = frame(
        metadata(1, 1, 1, ImageFormat::Jpeg, (2, 2), (2, 2), 1.0),
        JPEG,
    );
    let jpeg = decode_frame(&jpeg, decode_limits()).unwrap();
    assert_eq!(
        (jpeg.dimensions().width(), jpeg.dimensions().height()),
        (2, 2)
    );
    assert_eq!(
        jpeg.pixels(),
        &[
            12, 21, 30, 255, 202, 143, 99, 255, 30, 216, 73, 255, 237, 249, 247, 255,
        ]
    );

    let png = frame(
        metadata(2, 2, 2, ImageFormat::Png, (2, 2), (2, 2), 1.0),
        PNG,
    );
    let png = decode_frame(&png, decode_limits()).unwrap();
    assert_eq!(
        png.pixels(),
        &[
            10, 20, 30, 40, 200, 150, 100, 50, 30, 220, 80, 128, 240, 240, 240, 255,
        ]
    );
}

#[test]
fn forced_format_malformed_precision_dimensions_and_bombs_fail_boundedly() {
    let wrong = frame(
        metadata(1, 1, 1, ImageFormat::Jpeg, (2, 2), (2, 2), 1.0),
        PNG,
    );
    assert!(decode_frame(&wrong, decode_limits()).is_err());
    let malformed = frame(
        metadata(1, 1, 1, ImageFormat::Jpeg, (2, 2), (2, 2), 1.0),
        MALFORMED,
    );
    assert!(decode_frame(&malformed, decode_limits()).is_err());
    let mismatch = frame(
        metadata(1, 1, 1, ImageFormat::Png, (3, 3), (3, 3), 1.0),
        PNG,
    );
    assert!(decode_frame(&mismatch, decode_limits()).is_err());
    let bomb = frame(
        metadata(1, 1, 1, ImageFormat::Png, (100_000, 100_000), (1, 1), 1.0),
        BOMB,
    );
    assert_eq!(
        decode_frame(&bomb, decode_limits()).unwrap_err().code,
        krometrail_core::ErrorCode::ResourceLimitExceeded
    );
    let overflow = frame(
        metadata(1, 1, 1, ImageFormat::Png, (u32::MAX, u32::MAX), (1, 1), 1.0),
        BOMB,
    );
    assert_eq!(
        decode_frame(&overflow, decode_limits()).unwrap_err().code,
        krometrail_core::ErrorCode::ResourceLimitExceeded
    );

    let mut sixteen_bit = Vec::new();
    image::codecs::png::PngEncoder::new(&mut sixteen_bit)
        .write_image(&[0, 1, 0, 2], 2, 1, image::ExtendedColorType::L16)
        .unwrap();
    let high_precision = frame(
        metadata(1, 1, 1, ImageFormat::Png, (2, 1), (2, 1), 1.0),
        &sixteen_bit,
    );
    assert!(decode_frame(&high_precision, decode_limits()).is_err());
}

#[test]
fn epochs_preserve_ties_formats_gaps_markers_and_exact_geometry_boundaries() {
    let frames = vec![
        frame(
            metadata(1, 1, 5, ImageFormat::Jpeg, (2, 2), (2, 2), 1.0),
            JPEG,
        ),
        frame(
            metadata(2, 2, 5, ImageFormat::Png, (2, 2), (2, 2), 1.0),
            PNG,
        ),
        frame(
            metadata(3, 3, 5, ImageFormat::Png, (2, 2), (3, 2), 1.0),
            PNG,
        ),
        frame(
            metadata(
                4,
                4,
                6,
                ImageFormat::Png,
                (2, 2),
                (3, 2),
                f64::from_bits(1.0_f64.to_bits() + 1),
            ),
            PNG,
        ),
    ];
    let gap = CaptureGap::new(
        krometrail_core::GapId::from_uuid(Uuid::from_u128(200)),
        frames[0].metadata().session_id(),
        frames[0].metadata().target_id(),
        SessionRange::new(SessionTime::from_nanos(5), SessionTime::from_nanos(6)).unwrap(),
        ObservedTime::from_nanos(6),
        CaptureGapReason::FrameRejected,
        NonZeroU64::new(2),
        None,
    )
    .unwrap();
    let resolved = range(&frames, vec![gap]);
    let markers = vec![
        ArtifactMarker::new(
            ArtifactMarkerId::Caller(NonEmptyText::new("second").unwrap()),
            SessionTime::from_nanos(5),
            NonEmptyText::new("event").unwrap(),
            NonEmptyText::new("second").unwrap(),
        ),
        ArtifactMarker::new(
            ArtifactMarkerId::Caller(NonEmptyText::new("first").unwrap()),
            SessionTime::from_nanos(5),
            NonEmptyText::new("event").unwrap(),
            NonEmptyText::new("first").unwrap(),
        ),
    ];
    let plans = validate_and_plan(
        &resolved,
        frames.clone(),
        &markers,
        adaptation_limits(),
        &WorkCancellation::default(),
    )
    .unwrap();
    let epochs = validate_and_partition(
        &resolved,
        frames,
        &markers,
        adaptation_limits(),
        &WorkCancellation::default(),
    )
    .unwrap();
    assert_eq!(epochs.len(), 3);
    assert_eq!(plans[0].source_fingerprints.len(), 2);
    assert_eq!(plans[0].cache_sources.len(), 2);
    assert_eq!(
        plans[0].source_fingerprints[0],
        plans[0].cache_sources[0].store_fingerprint()
    );
    assert_eq!(
        plans[0].descriptor.frame_ids,
        [
            FrameId::from_uuid(Uuid::from_u128(1)),
            FrameId::from_uuid(Uuid::from_u128(2))
        ]
    );
    assert_eq!(
        epochs[0].sequence.frames()[0].timestamp(),
        epochs[0].sequence.frames()[1].timestamp()
    );
    assert_eq!(epochs[0].sequence.markers()[0].label(), "second");
    assert_eq!(epochs[0].sequence.markers()[1].label(), "first");
    assert_eq!(epochs[0].sequence.gaps()[0].range().start().as_nanos(), 5);
    assert_eq!(epochs[0].sequence.gaps()[0].range().end().as_nanos(), 5);
    assert_eq!(
        epochs[0].sequence.gaps()[0]
            .estimated_missing_frames()
            .unwrap()
            .get(),
        2
    );
    assert_eq!(plans[1].descriptor.viewport.width(), 3);
    assert_ne!(
        plans[1].descriptor.device_scale_factor.get().to_bits(),
        plans[2].descriptor.device_scale_factor.get().to_bits(),
    );
    assert_eq!(plans.iter().filter(|plan| !plan.gaps.is_empty()).count(), 1);
    let manifest_gap_data: Vec<_> = epochs
        .iter()
        .enumerate()
        .map(|(index, epoch)| {
            let normalized = normalize_sequence(
                &epoch.sequence,
                NormalizationParameters::new(
                    temporal_vision::Rgb8::new(0, 0, 0),
                    None,
                    IntegerScale::IDENTITY,
                    temporal_vision::ProcessingLimits::default(),
                ),
            )
            .unwrap();
            let generated = temporal_vision::generate_storyboard(
                ArtifactId::from_uuid(Uuid::from_u128(300 + u128::try_from(index).unwrap())),
                None,
                &epoch.sequence,
                &normalized,
                StoryboardParameters::new(
                    epoch.sequence.range().start(),
                    StoryboardTileLimit::new(3).unwrap(),
                    MeasurementParameters::new(0),
                    ArtifactLabels::new("epoch", "test").unwrap(),
                    RenderLimits::default(),
                ),
            )
            .unwrap();
            let storyboard = generated.storyboard();
            let image = image::load_from_memory(storyboard.image().bytes())
                .unwrap()
                .to_rgb8();
            (
                storyboard.manifest().gaps().len(),
                image.pixels().any(|pixel| pixel.0 == [255, 196, 64]),
            )
        })
        .collect();
    assert_eq!(
        manifest_gap_data
            .iter()
            .map(|(count, _)| *count)
            .collect::<Vec<_>>(),
        [1, 0, 0]
    );
    assert!(!manifest_gap_data[1].1);
    assert!(!manifest_gap_data[2].1);
}

#[test]
fn source_order_disappearance_and_cancellation_are_explicit() {
    let frames = vec![
        frame(
            metadata(1, 1, 1, ImageFormat::Png, (2, 2), (2, 2), 1.0),
            PNG,
        ),
        frame(
            metadata(2, 2, 2, ImageFormat::Png, (2, 2), (2, 2), 1.0),
            PNG,
        ),
    ];
    let resolved = range(&frames, vec![]);
    assert!(
        validate_and_partition(
            &resolved,
            vec![frames[0].clone()],
            &[],
            adaptation_limits(),
            &WorkCancellation::default(),
        )
        .is_err()
    );
    let mut reversed = frames.clone();
    reversed.reverse();
    assert!(
        validate_and_partition(
            &resolved,
            reversed,
            &[],
            adaptation_limits(),
            &WorkCancellation::default(),
        )
        .is_err()
    );
    let cancellation = WorkCancellation::default();
    cancellation.cancel();
    assert_eq!(
        validate_and_partition(&resolved, frames, &[], adaptation_limits(), &cancellation,)
            .unwrap_err()
            .code,
        krometrail_core::ErrorCode::Cancelled
    );
}
