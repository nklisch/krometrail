use std::{
    fmt,
    num::{NonZeroU32, NonZeroUsize},
};

use png::{ColorType, Decoder};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use temporal_vision::{
    ArtifactKind, ArtifactLabels, ArtifactManifest, BinaryMask, DeclaredGap, ErrorCode,
    EvidenceClass, Frame, FrameSequence, IntegerScale, Marker, MeasurementParameters, MotionDecay,
    MotionHistoryParameters, NormalizationParameters, ParameterValue, PixelDimensions, PixelFormat,
    ProcessingLimits, RenderLimits, Rgb8, TimeRange, Timestamp, build_motion_history_plan,
    generate_motion_history, normalize_sequence,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct FrameId(String);

impl fmt::Display for FrameId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct MarkerId(String);
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct GapId(String);
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct ArtifactId(String);

type Source = FrameSequence<FrameId, MarkerId, GapId, Box<[u8]>>;

fn fixture() -> (Source, temporal_vision::NormalizedSequence<FrameId>) {
    let dimensions = PixelDimensions::new(96, 5).unwrap();
    let frames = [
        ("f0", 0, None),
        ("f1", 0, Some(10)),
        ("f2", 10, Some(11)),
        ("f3", 20, None),
        ("f4", 30, Some(10)),
        ("f5", 40, None),
        ("f6", 40, None),
    ]
    .into_iter()
    .map(|(id, timestamp, block_x)| {
        let mut pixels = vec![0_u8; dimensions.rgba8_byte_len().unwrap()];
        for pixel in pixels.chunks_exact_mut(4) {
            pixel[3] = 255;
        }
        if let Some(block_x) = block_x {
            for y in 1..4 {
                for x in block_x..block_x + 3 {
                    let index = (y * dimensions.width() as usize + x) * 4;
                    pixels[index..index + 3].fill(255);
                }
            }
        }
        // This changing pixel is outside the analysis mask and must remain absent.
        let excluded = (4 * dimensions.width() as usize + 95) * 4;
        pixels[excluded..excluded + 3].fill(if id == "f0" { 0 } else { 255 });
        Frame::new(
            FrameId(id.into()),
            Timestamp::from_nanos(timestamp),
            dimensions,
            PixelFormat::Rgba8SrgbStraight,
            pixels.into_boxed_slice(),
        )
        .unwrap()
    })
    .collect();
    let marker = Marker::new(
        MarkerId("m0".into()),
        Timestamp::from_nanos(10),
        "action",
        "translated block",
    )
    .unwrap();
    let gap = DeclaredGap::new(
        GapId("g0".into()),
        TimeRange::new(Timestamp::from_nanos(15), Timestamp::from_nanos(15)).unwrap(),
        "capture loss",
        None,
    )
    .unwrap();
    let mut mask = vec![0xff_u8; dimensions.pixel_count().unwrap() / 8];
    *mask.last_mut().unwrap() &= 0xfe;
    let source = FrameSequence::new(
        frames,
        vec![marker],
        vec![gap],
        None,
        Some(BinaryMask::new(dimensions, mask).unwrap()),
    )
    .unwrap();
    let normalized = normalize_sequence(
        &source,
        NormalizationParameters::new(
            Rgb8::new(0, 0, 0),
            None,
            IntegerScale::IDENTITY,
            ProcessingLimits::default(),
        ),
    )
    .unwrap();
    (source, normalized)
}

fn parameters(limits: RenderLimits) -> MotionHistoryParameters {
    MotionHistoryParameters::new(
        0,
        MeasurementParameters::new(0),
        MotionDecay::default(),
        64,
        Rgb8::new(255, 176, 0),
        Rgb8::new(255, 255, 255),
        ArtifactLabels::new("Motion history fixture", "typed browser-free source").unwrap(),
        limits,
    )
}

#[test]
fn public_plan_and_render_are_traceable_gap_aware_and_deterministic() {
    let (source, normalized) = fixture();
    let plan =
        build_motion_history_plan(&source, &normalized, &parameters(RenderLimits::default()))
            .unwrap();

    let pixel = |x: usize, y: usize| y * 96 + x;
    assert_eq!(plan.accumulation()[pixel(10, 2)], u16::MAX);
    assert_eq!(plan.accumulation()[pixel(11, 2)], 49_150);
    assert_eq!(plan.accumulation()[pixel(12, 2)], 49_150);
    assert_eq!(plan.accumulation()[pixel(13, 2)], u16::MAX);
    assert_eq!(plan.accumulation()[pixel(95, 4)], 0);
    assert_eq!(MotionDecay::default().weight_at(0), u16::MAX);
    assert_eq!(MotionDecay::default().weight_at(1), u16::MAX >> 1);
    assert_eq!(
        MotionDecay::default().weight_at(MotionDecay::default().live_window()),
        0
    );
    assert_eq!(plan.continuity_segment_count(), 2);
    assert_eq!(plan.measured_pair_count(), 5);
    assert_eq!(plan.gap_pair_count(), 1);
    assert_eq!(plan.changed_pixel_count(), 12);
    assert_eq!(plan.ever_changed().includes(11, 2), Some(true));
    assert_eq!(plan.outline().includes(11, 2), Some(false));
    assert_eq!(plan.outline().includes(10, 1), Some(true));
    assert_eq!(plan.ever_changed().includes(95, 4), Some(false));

    let first = generate_motion_history(
        ArtifactId("motion-a".into()),
        &source,
        &normalized,
        parameters(RenderLimits::default()),
    )
    .unwrap();
    let second = generate_motion_history(
        ArtifactId("motion-a".into()),
        &source,
        &normalized,
        parameters(RenderLimits::default()),
    )
    .unwrap();
    assert_eq!(first, second);

    let manifest = first.manifest();
    assert_eq!(manifest.artifact_kind(), ArtifactKind::MotionHistory);
    assert_eq!(manifest.evidence_class(), EvidenceClass::SourceDerived);
    assert_eq!(manifest.algorithm().name(), "motion-history");
    assert_eq!(manifest.algorithm().version(), "1.0.0");
    assert_eq!(manifest.source_frame_count(), 7);
    // All seven frames were analyzed; only the reference frame is referenced.
    assert_eq!(manifest.analyzed_frame_count(), 7);
    assert_eq!(manifest.omitted_frame_count(), 0);
    assert_eq!(manifest.selected_frame_ids(), &[FrameId("f0".into())]);
    assert_eq!(manifest.gaps().len(), 1);
    assert_eq!(manifest.mask(), source.mask());
    assert_eq!(
        manifest.parameters().get("direction_inference"),
        Some(&ParameterValue::Text("none".into()))
    );
    assert_eq!(
        manifest.parameters().get("disambiguation"),
        Some(&ParameterValue::Text(
            "storyboard_or_region_filmstrip".into()
        ))
    );
    assert_eq!(
        manifest.output_dimensions(),
        PixelDimensions::new(96, 137).unwrap()
    );

    let json = serde_json::to_vec(manifest).unwrap();
    let decoded: ArtifactManifest<ArtifactId, FrameId, MarkerId, GapId> =
        serde_json::from_slice(&json).unwrap();
    assert_eq!(&decoded, manifest);

    let image = first.image();
    assert_eq!(&image.bytes()[..8], b"\x89PNG\r\n\x1a\n");
    let digest: [u8; 32] = Sha256::digest(image.bytes()).into();
    assert_eq!(manifest.output_hash().as_bytes(), &digest);
    assert_eq!(
        manifest.output_hash().to_string(),
        "197ca7ca6534bea8672d390624d60b2d105356473533d30a417acb416af09137"
    );

    let (dimensions, pixels) = decode_rgb(image.bytes());
    assert_eq!(dimensions, manifest.output_dimensions());
    // Main raster starts below the 38-pixel annotation header.
    assert_eq!(rgb_at(&pixels, dimensions, 0, 40), [0, 0, 0]);
    assert_eq!(rgb_at(&pixels, dimensions, 11, 40), [191, 132, 0]);
    assert_eq!(rgb_at(&pixels, dimensions, 10, 39), [255, 255, 255]);
    // The narrow output suppresses unreadable annotation labels; the gap band remains visible.
    assert!(!region_has_non_background(
        &pixels,
        dimensions,
        0,
        0,
        96,
        38,
        [10, 12, 16]
    ));
    assert!(region_has_color(
        &pixels,
        dimensions,
        0,
        121,
        96,
        16,
        [255, 196, 64]
    ));
}

#[test]
fn rendering_rejects_each_tiny_output_limit() {
    let (source, normalized) = fixture();
    let limit = |width, height, canvas, encoded| {
        RenderLimits::new(
            NonZeroU32::new(width).unwrap(),
            NonZeroU32::new(height).unwrap(),
            NonZeroUsize::new(canvas).unwrap(),
            NonZeroUsize::new(encoded).unwrap(),
        )
    };
    for limits in [
        limit(95, 137, 1_000_000, 1_000_000),
        limit(96, 136, 1_000_000, 1_000_000),
        limit(96, 137, 1, 1_000_000),
        limit(96, 137, 1_000_000, 8),
    ] {
        assert_eq!(
            generate_motion_history(
                ArtifactId("bounded".into()),
                &source,
                &normalized,
                parameters(limits),
            )
            .unwrap_err()
            .code,
            ErrorCode::ResourceLimitExceeded
        );
    }
}

fn decode_rgb(bytes: &[u8]) -> (PixelDimensions, Vec<u8>) {
    let mut reader = Decoder::new(bytes).read_info().unwrap();
    let mut output = vec![0_u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut output).unwrap();
    assert_eq!(info.color_type, ColorType::Rgb);
    output.truncate(info.buffer_size());
    (
        PixelDimensions::new(info.width, info.height).unwrap(),
        output,
    )
}

fn rgb_at(pixels: &[u8], dimensions: PixelDimensions, x: u32, y: u32) -> [u8; 3] {
    let index = (y as usize * dimensions.width() as usize + x as usize) * 3;
    [pixels[index], pixels[index + 1], pixels[index + 2]]
}

#[allow(clippy::too_many_arguments)]
fn region_has_color(
    pixels: &[u8],
    dimensions: PixelDimensions,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    color: [u8; 3],
) -> bool {
    (y..y + height)
        .any(|row| (x..x + width).any(|column| rgb_at(pixels, dimensions, column, row) == color))
}

#[allow(clippy::too_many_arguments)]
fn region_has_non_background(
    pixels: &[u8],
    dimensions: PixelDimensions,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    background: [u8; 3],
) -> bool {
    (y..y + height).any(|row| {
        (x..x + width).any(|column| rgb_at(pixels, dimensions, column, row) != background)
    })
}
