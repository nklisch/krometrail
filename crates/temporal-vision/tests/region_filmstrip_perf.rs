//! Ignored, browser-free release benchmark for selected-frame filmstrip normalization.
//!
//! Run explicitly with Rust 1.85 and `--release`. The default fixture is the 120-frame,
//! 1224x958 retained-evidence case that previously exceeded the normalized-byte ceiling when
//! every source frame was normalized before tile selection.

use std::{
    num::{NonZeroU32, NonZeroUsize},
    time::Instant,
};

use serde::Serialize;
use sha2::{Digest, Sha256};
use temporal_vision::{
    DeclaredGap, FilmstripTileLimit, Frame, FrameSequence, IntegerScale, Marker,
    NormalizationParameters, PixelDimensions, PixelFormat, PixelRect, ProcessingLimits,
    RegionDefinition, RegionFilmstripLabels, RegionFilmstripParameters,
    RegionFilmstripRenderLimits, Rgb8, SignedPixelRect, Timestamp, generate_region_filmstrip,
    normalize_sequence,
};

const FRAME_INTERVAL_NS: u64 = 10_000_000;

#[derive(Serialize)]
struct Report {
    benchmark: &'static str,
    frame_count: usize,
    width: u32,
    height: u32,
    tile_count: usize,
    normalized_retained_bytes: usize,
    wall_us: u128,
    image_sha256: String,
    success: bool,
}

#[test]
#[ignore = "manual Rust 1.85 release benchmark; never part of ordinary tests"]
fn region_filmstrip_release_profile() {
    let dimensions = PixelDimensions::new(1_224, 958).unwrap();
    let frame_count = 120;
    let source = fixture(frame_count, dimensions);
    let region = SignedPixelRect::new(
        384,
        320,
        NonZeroU32::new(256).unwrap(),
        NonZeroU32::new(256).unwrap(),
    )
    .unwrap();
    let parameters = parameters(region, frame_count);
    let started = Instant::now();
    let artifact = generate_region_filmstrip(7_u32, &source, parameters).unwrap();
    let wall_us = started.elapsed().as_micros();
    let area = usize::try_from(region.width()).unwrap() * usize::try_from(region.height()).unwrap();
    let normalized_retained_bytes = (artifact.plan().tiles().len() + 1) * area * 6;
    let tile_source = FrameSequence::new(
        artifact
            .plan()
            .tiles()
            .iter()
            .map(|tile| source.frames()[tile.frame_index()].clone())
            .collect(),
        Vec::<Marker<u32>>::new(),
        Vec::<DeclaredGap<u32>>::new(),
        None,
        None,
    )
    .unwrap();
    let full_limits = ProcessingLimits::new(
        NonZeroUsize::new(frame_count).unwrap(),
        NonZeroUsize::new(dimensions.pixel_count().unwrap()).unwrap(),
        NonZeroUsize::new(512 * 1024 * 1024).unwrap(),
    );
    let crop = PixelRect::new(
        u32::try_from(region.x()).unwrap(),
        u32::try_from(region.y()).unwrap(),
        region.width(),
        region.height(),
    )
    .unwrap();
    let full = normalize_sequence(
        &source,
        NormalizationParameters::new(
            Rgb8::new(0, 0, 0),
            Some(crop),
            IntegerScale::IDENTITY,
            full_limits,
        ),
    )
    .unwrap();
    let selected = normalize_sequence(
        &tile_source,
        NormalizationParameters::new(
            Rgb8::new(0, 0, 0),
            Some(crop),
            IntegerScale::IDENTITY,
            full_limits,
        ),
    )
    .unwrap();
    let mut selected_digest = Sha256::new();
    let mut full_digest = Sha256::new();
    for (position, tile) in artifact.plan().tiles().iter().enumerate() {
        let selected_pixels = selected.frames()[position].linear_rgb16();
        let full_pixels = full.frames()[tile.frame_index()].linear_rgb16();
        assert_eq!(selected_pixels, full_pixels);
        for value in selected_pixels {
            selected_digest.update(value.to_be_bytes());
        }
        for value in full_pixels {
            full_digest.update(value.to_be_bytes());
        }
    }
    assert_eq!(selected_digest.finalize(), full_digest.finalize());
    let report = Report {
        benchmark: "temporal_vision.region_filmstrip_selected_normalization.v1",
        frame_count,
        width: dimensions.width(),
        height: dimensions.height(),
        tile_count: artifact.plan().tiles().len(),
        normalized_retained_bytes,
        wall_us,
        image_sha256: format!("{:x}", Sha256::digest(artifact.image().bytes())),
        success: true,
    };
    println!(
        "REGION_FILMSTRIP_REPORT {}",
        serde_json::to_string(&report).unwrap()
    );
}

fn parameters(region: SignedPixelRect, frame_count: usize) -> RegionFilmstripParameters {
    RegionFilmstripParameters::new(
        RegionDefinition::FixedSourceImage { rect: region },
        Timestamp::from_nanos(u64::try_from(frame_count / 3).unwrap() * FRAME_INTERVAL_NS),
        FilmstripTileLimit::DEFAULT,
        Rgb8::new(0, 0, 0),
        Rgb8::new(255, 0, 255),
        IntegerScale::IDENTITY,
        RegionFilmstripLabels::new("REGION FILMSTRIP", "KROMETRAIL RETAINED SOURCE FRAMES")
            .unwrap(),
        RegionFilmstripRenderLimits::default()
            .with_max_source_frames(NonZeroUsize::new(frame_count).unwrap()),
    )
}

fn fixture(
    frame_count: usize,
    dimensions: PixelDimensions,
) -> FrameSequence<u32, u32, u32, Box<[u8]>> {
    let frames = (0..frame_count)
        .map(|frame_index| {
            let mut pixels = vec![0_u8; dimensions.rgba8_byte_len().unwrap()];
            for y in 0..dimensions.height() {
                for x in 0..dimensions.width() {
                    let pixel = usize::try_from(y * dimensions.width() + x).unwrap();
                    let offset = pixel * 4;
                    let frame = u32::try_from(frame_index).unwrap();
                    pixels[offset..offset + 4].copy_from_slice(&[
                        (x.wrapping_mul(3).wrapping_add(frame * 5) & 0xff) as u8,
                        (y.wrapping_mul(7).wrapping_add(frame * 3) & 0xff) as u8,
                        ((x + y).wrapping_add(frame * 11) & 0xff) as u8,
                        u8::MAX,
                    ]);
                }
            }
            Frame::new(
                u32::try_from(frame_index).unwrap(),
                Timestamp::from_nanos(u64::try_from(frame_index).unwrap() * FRAME_INTERVAL_NS),
                dimensions,
                PixelFormat::Rgba8SrgbStraight,
                pixels.into_boxed_slice(),
            )
            .unwrap()
        })
        .collect();
    FrameSequence::new(frames, Vec::new(), Vec::new(), None, None).unwrap()
}
