use std::num::NonZeroUsize;

use png::{ColorType, Decoder};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use temporal_vision::{
    ArtifactKind, ArtifactManifest, BinaryMask, DeclaredGap, DifferenceMapLimits,
    DifferenceMapParameters, ErrorCode, EvidenceClass, Frame, FrameRegion, FrameSequence,
    FrequencyMode, IntegerScale, Marker, MeasurementParameters, NormalizationParameters,
    PixelDimensions, PixelFormat, PixelRect, ProcessingLimits, Rgb8, TimePalette, TimeRange,
    Timestamp, normalize_sequence, render_difference_map,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct FrameId(String);
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct MarkerId(String);
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct GapId(String);
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct ArtifactId(String);

type Source = FrameSequence<FrameId, MarkerId, GapId, Box<[u8]>>;

fn rgba(pixels: [[u8; 3]; 4]) -> Box<[u8]> {
    pixels
        .into_iter()
        .flat_map(|pixel| [pixel[0], pixel[1], pixel[2], 255])
        .collect::<Vec<_>>()
        .into_boxed_slice()
}

fn fixture() -> (Source, temporal_vision::NormalizedSequence<FrameId>) {
    let dimensions = PixelDimensions::new(4, 1).unwrap();
    let black = [0, 0, 0];
    let frames = [
        ("f0", 0, [black, black, black, black]),
        (
            "f1",
            10,
            [[255, 255, 255], [64, 64, 64], black, [255, 0, 0]],
        ),
        (
            "f2",
            20,
            [[255, 255, 255], [64, 64, 64], black, [0, 255, 0]],
        ),
        ("f3", 40, [black, [128, 128, 128], [0, 0, 255], [0, 0, 255]]),
    ]
    .into_iter()
    .map(|(id, time, pixels)| {
        Frame::new(
            FrameId(id.into()),
            Timestamp::from_nanos(time),
            dimensions,
            PixelFormat::Rgba8SrgbStraight,
            rgba(pixels),
        )
        .unwrap()
    })
    .collect();
    let marker = Marker::new(
        MarkerId("m0".into()),
        Timestamp::from_nanos(10),
        "action",
        "open",
    )
    .unwrap();
    let gap = DeclaredGap::new(
        GapId("g0".into()),
        TimeRange::new(Timestamp::from_nanos(15), Timestamp::from_nanos(15)).unwrap(),
        "capture loss",
        None,
    )
    .unwrap();
    let region = FrameRegion::new(PixelRect::new(0, 0, 4, 1).unwrap(), dimensions).unwrap();
    let mask = BinaryMask::new(dimensions, [0xe0]).unwrap();
    let source =
        FrameSequence::new(frames, vec![marker], vec![gap], Some(region), Some(mask)).unwrap();
    let normalized = normalize_sequence(
        &source,
        NormalizationParameters::new(
            Rgb8::new(7, 9, 11),
            None,
            IntegerScale::IDENTITY,
            ProcessingLimits::default(),
        ),
    )
    .unwrap();
    (source, normalized)
}

fn parameters(limits: DifferenceMapLimits) -> DifferenceMapParameters {
    DifferenceMapParameters::new(
        0,
        FrequencyMode::Count,
        TimePalette::Spectral,
        Some(Timestamp::from_nanos(20)),
        MeasurementParameters::new(0),
        Rgb8::new(7, 9, 11),
        limits,
    )
}

#[test]
fn browser_free_public_contract_is_traceable_bounded_and_deterministic() {
    let (source, normalized) = fixture();
    let first = render_difference_map(
        ArtifactId("difference-a".into()),
        &source,
        &normalized,
        parameters(DifferenceMapLimits::default()),
    )
    .unwrap();
    let second = render_difference_map(
        ArtifactId("difference-a".into()),
        &source,
        &normalized,
        parameters(DifferenceMapLimits::default()),
    )
    .unwrap();
    assert_eq!(first, second);

    let manifest = first.manifest();
    assert_eq!(manifest.artifact_kind(), ArtifactKind::DifferenceMap);
    assert_eq!(manifest.evidence_class(), EvidenceClass::SourceDerived);
    assert_eq!(manifest.algorithm().name(), "temporal-difference-map");
    assert_eq!(manifest.algorithm().version(), "v1");
    assert_eq!(manifest.source_frame_count(), 4);
    assert_eq!(manifest.omitted_frame_count(), 3);
    assert_eq!(manifest.selected_frame_ids(), &[FrameId("f0".into())]);
    assert_eq!(manifest.markers().len(), 1);
    assert_eq!(manifest.gaps().len(), 1);
    assert_eq!(manifest.region(), source.region());
    assert_eq!(manifest.mask(), source.mask());
    assert_eq!(
        manifest.output_dimensions(),
        PixelDimensions::new(76, 261).unwrap()
    );

    let image = first.image();
    assert_eq!(image.media_type(), "image/png");
    assert_eq!(image.dimensions(), manifest.output_dimensions());
    assert_eq!(&image.bytes()[..8], b"\x89PNG\r\n\x1a\n");
    let digest: [u8; 32] = Sha256::digest(image.bytes()).into();
    assert_eq!(manifest.output_hash().as_bytes(), &digest);

    let json = serde_json::to_vec(manifest).unwrap();
    let decoded: ArtifactManifest<ArtifactId, FrameId, MarkerId, GapId> =
        serde_json::from_slice(&json).unwrap();
    assert_eq!(&decoded, manifest);

    let (dimensions, pixels) = decode_rgb(image.bytes());
    assert_eq!(dimensions, manifest.output_dimensions());
    // Fixed panel origins are x=16/36/56 and y=112 for this 4x1 fixture.
    assert_eq!(rgb_at(&pixels, dimensions, 16, 112), [0, 0, 0]);
    assert_eq!(rgb_at(&pixels, dimensions, 36, 112), [255, 255, 255]);
    assert_eq!(rgb_at(&pixels, dimensions, 38, 112), [127, 127, 127]);
    assert_eq!(rgb_at(&pixels, dimensions, 39, 112), [62, 68, 79]);
    assert_eq!(rgb_at(&pixels, dimensions, 56, 112), [255, 78, 142]);
    assert_eq!(rgb_at(&pixels, dimensions, 58, 112), [250, 190, 48]);
    assert_eq!(rgb_at(&pixels, dimensions, 59, 112), [62, 68, 79]);
    assert!(region_has_color(
        &pixels,
        dimensions,
        16,
        223,
        44,
        18,
        [255, 196, 64]
    ));

    let tiny_output = DifferenceMapLimits::new(
        NonZeroUsize::new(1024).unwrap(),
        NonZeroUsize::new(1).unwrap(),
    );
    assert_eq!(
        render_difference_map(
            ArtifactId("too-large".into()),
            &source,
            &normalized,
            parameters(tiny_output),
        )
        .unwrap_err()
        .code,
        ErrorCode::ResourceLimitExceeded
    );
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
