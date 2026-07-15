//! Ignored, browser-free release benchmark for opaque temporal normalization.
//!
//! The scaffold intentionally uses the production storyboard + orientation + difference-map
//! generator policy without opening a browser or contacting any external service. It reports one
//! JSON record per repetition. Artifact digests are SHA-256 over the serialized manifest bytes
//! followed by the PNG bytes; normalized digests are SHA-256 over dimensions, frame metadata, and
//! big-endian normalized RGB16 values.
//!
//! Example (Rust 1.85 release build):
//!
//! ```text
//! PERF_TEMPORAL_FRAMES=120 PERF_TEMPORAL_REPETITIONS=5 \
//!   rustup run 1.85.0 cargo test -p temporal-vision --release --locked \
//!   --test temporal_normalize_perf -- --ignored --exact production_policy_release_profile --nocapture
//! ```

use std::{env, num::NonZeroU8, time::Instant};

use serde::Serialize;
use sha2::{Digest, Sha256};
use temporal_vision::{
    ArtifactLabels, DifferenceMapParameters, Frame, FrameSequence, FrequencyMode, IntegerScale,
    Marker, MeasurementParameters, NormalizationParameters, PixelDimensions, PixelFormat,
    ProcessingLimits, RenderLimits, Rgb8, StoryboardParameters, StoryboardTileLimit, TimePalette,
    Timestamp, generate_storyboard, normalize_sequence, render_difference_map,
};

type Source = FrameSequence<u32, u32, u32, Box<[u8]>>;
type Normalized = temporal_vision::NormalizedSequence<u32>;

const POLICY_NOISE_FLOOR: u16 = 512;

#[derive(Clone, Copy, Debug)]
struct Config {
    frames: usize,
    repetitions: usize,
    dimensions: PixelDimensions,
    scale: IntegerScale,
}

impl Config {
    fn from_environment() -> Self {
        let frames = env_usize("PERF_TEMPORAL_FRAMES", 30);
        let repetitions = env_usize("PERF_TEMPORAL_REPETITIONS", 1);
        assert!(
            (2..=120).contains(&frames),
            "frames must be between 2 and 120"
        );
        assert!(
            (1..=20).contains(&repetitions),
            "repetitions must be between 1 and 20"
        );
        let width = env_usize("PERF_TEMPORAL_WIDTH", 1_920) as u32;
        let height = env_usize("PERF_TEMPORAL_HEIGHT", 1_080) as u32;
        let dimensions = PixelDimensions::new(width, height).unwrap();
        let scale = match env::var("PERF_TEMPORAL_SCALE").as_deref() {
            Ok("identity") | Ok("1") | Err(_) => IntegerScale::IDENTITY,
            Ok("down2") | Ok("2") => IntegerScale::down(NonZeroU8::new(2).unwrap()).unwrap(),
            Ok("down4") | Ok("4") => IntegerScale::down(NonZeroU8::new(4).unwrap()).unwrap(),
            Ok("down8") | Ok("8") => IntegerScale::down(NonZeroU8::new(8).unwrap()).unwrap(),
            Ok(value) => panic!("unsupported PERF_TEMPORAL_SCALE={value}"),
        };
        Self {
            frames,
            repetitions,
            dimensions,
            scale,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
struct RepetitionReport {
    repetition: usize,
    normalization_us: u128,
    e2e_us: u128,
    normalized_digest: String,
    artifact_digests: Vec<String>,
    output_digest: String,
    rss_before_kib: u64,
    rss_after_kib: u64,
    hwm_before_kib: u64,
    hwm_after_kib: u64,
    hwm_delta_kib: u64,
}

#[derive(Clone, Debug, Serialize)]
struct SummaryReport {
    frames: usize,
    repetitions: usize,
    width: u32,
    height: u32,
    scale: &'static str,
    normalized_digest: String,
    artifact_digests: Vec<String>,
    output_digest: String,
    normalization_us: Vec<u128>,
    e2e_us: Vec<u128>,
    hwm_delta_kib: Vec<u64>,
}

#[test]
#[ignore = "manual Rust 1.85 release benchmark; never part of ordinary tests"]
fn production_policy_release_profile() {
    let config = Config::from_environment();
    let source = fixture(config.frames, config.dimensions);
    let mut repetitions = Vec::with_capacity(config.repetitions);

    for repetition in 1..=config.repetitions {
        let rss_before_kib = current_rss_kib();
        let hwm_before_kib = peak_rss_kib();
        let e2e_started = Instant::now();
        let normalization_started = Instant::now();
        let normalized = normalize_sequence(
            &source,
            NormalizationParameters::new(
                Rgb8::new(0, 0, 0),
                None,
                config.scale,
                ProcessingLimits::default(),
            ),
        )
        .unwrap();
        let normalization_us = normalization_started.elapsed().as_micros();
        let normalized_digest = normalized_digest(&normalized);
        let artifacts = production_policy_artifacts(&source, &normalized);
        let artifact_digests = artifacts.iter().map(artifact_digest).collect::<Vec<_>>();
        let output_digest = digest_strings(&artifact_digests);
        let e2e_us = e2e_started.elapsed().as_micros();
        drop(artifacts);
        drop(normalized);
        let rss_after_kib = current_rss_kib();
        let hwm_after_kib = peak_rss_kib();
        let report = RepetitionReport {
            repetition,
            normalization_us,
            e2e_us,
            normalized_digest,
            artifact_digests,
            output_digest,
            rss_before_kib,
            rss_after_kib,
            hwm_before_kib,
            hwm_after_kib,
            hwm_delta_kib: hwm_after_kib.saturating_sub(hwm_before_kib),
        };
        println!(
            "TEMPORAL_NORMALIZE_REPORT {}",
            serde_json::to_string(&report).unwrap()
        );
        repetitions.push(report);
    }

    let first = &repetitions[0];
    assert!(repetitions.iter().all(|report| {
        report.normalized_digest == first.normalized_digest
            && report.artifact_digests == first.artifact_digests
            && report.output_digest == first.output_digest
    }));
    let summary = SummaryReport {
        frames: config.frames,
        repetitions: config.repetitions,
        width: config.dimensions.width(),
        height: config.dimensions.height(),
        scale: scale_name(config.scale),
        normalized_digest: first.normalized_digest.clone(),
        artifact_digests: first.artifact_digests.clone(),
        output_digest: first.output_digest.clone(),
        normalization_us: repetitions
            .iter()
            .map(|report| report.normalization_us)
            .collect(),
        e2e_us: repetitions.iter().map(|report| report.e2e_us).collect(),
        hwm_delta_kib: repetitions
            .iter()
            .map(|report| report.hwm_delta_kib)
            .collect(),
    };
    println!(
        "TEMPORAL_NORMALIZE_SUMMARY {}",
        serde_json::to_string(&summary).unwrap()
    );
}

fn fixture(frames: usize, dimensions: PixelDimensions) -> Source {
    let source_frames = (0..frames)
        .map(|frame_index| {
            let mut pixels = vec![0_u8; dimensions.rgba8_byte_len().unwrap()];
            for y in 0..dimensions.height() {
                for x in 0..dimensions.width() {
                    let offset = ((y * dimensions.width() + x) * 4) as usize;
                    let moving_patch = x >= (frame_index as u32 * 17) % dimensions.width()
                        && x < ((frame_index as u32 * 17) % dimensions.width()) + 64
                        && y >= dimensions.height() / 3
                        && y < dimensions.height() / 3 + 64;
                    pixels[offset..offset + 4].copy_from_slice(&[
                        (x * 13 + y * 7 + frame_index as u32 * 19) as u8,
                        (x * 5 + y * 17 + frame_index as u32 * 29 + x * y) as u8,
                        if moving_patch {
                            240
                        } else {
                            (x * 23 + y * 3 + frame_index as u32 * 11 + x * y * 2) as u8
                        },
                        u8::MAX,
                    ]);
                }
            }
            Frame::new(
                frame_index as u32,
                Timestamp::from_nanos(frame_index as u64 * 10_000_000),
                dimensions,
                PixelFormat::Rgba8SrgbStraight,
                pixels.into_boxed_slice(),
            )
            .unwrap()
        })
        .collect();
    FrameSequence::new(
        source_frames,
        Vec::<Marker<u32>>::new(),
        Vec::new(),
        None,
        None,
    )
    .unwrap()
}

fn production_policy_artifacts(
    source: &Source,
    normalized: &Normalized,
) -> Vec<temporal_vision::GeneratedArtifact<u32, u32, u32, u32>> {
    let labels =
        ArtifactLabels::new("TEMPORAL STORYBOARD", "KROMETRAIL RETAINED SOURCE FRAMES").unwrap();
    let storyboard = generate_storyboard(
        1,
        Some(2),
        source,
        normalized,
        StoryboardParameters::new(
            source.frames()[source.frames().len() / 3].timestamp(),
            StoryboardTileLimit::default(),
            MeasurementParameters::new(POLICY_NOISE_FLOOR),
            labels,
            RenderLimits::default(),
        ),
    )
    .unwrap();
    let difference = render_difference_map(
        3,
        source,
        normalized,
        DifferenceMapParameters::new(
            0,
            FrequencyMode::NormalizedFrequency,
            TimePalette::Spectral,
            None,
            MeasurementParameters::new(POLICY_NOISE_FLOOR),
            Rgb8::new(0, 0, 0),
            temporal_vision::DifferenceMapLimits::default(),
        ),
    )
    .unwrap();
    vec![
        storyboard.storyboard().clone(),
        storyboard.orientation().unwrap().clone(),
        difference,
    ]
}

fn normalized_digest(normalized: &Normalized) -> String {
    let mut digest = Sha256::new();
    digest.update(normalized.dimensions().width().to_be_bytes());
    digest.update(normalized.dimensions().height().to_be_bytes());
    for frame in normalized.frames() {
        digest.update(frame.id().to_be_bytes());
        digest.update(frame.timestamp().as_nanos().to_be_bytes());
        for value in frame.linear_rgb16() {
            digest.update(value.to_be_bytes());
        }
    }
    format!("{:x}", digest.finalize())
}

fn artifact_digest<A, F, M, G>(artifact: &temporal_vision::GeneratedArtifact<A, F, M, G>) -> String
where
    A: Serialize,
    F: Serialize,
    M: Serialize,
    G: Serialize,
{
    let mut digest = Sha256::new();
    digest.update(serde_json::to_vec(artifact.manifest()).unwrap());
    digest.update(artifact.image().bytes());
    format!("{:x}", digest.finalize())
}

fn digest_strings(values: &[String]) -> String {
    let mut digest = Sha256::new();
    for value in values {
        digest.update(value.as_bytes());
        digest.update([0]);
    }
    format!("{:x}", digest.finalize())
}

fn scale_name(scale: IntegerScale) -> &'static str {
    match scale.factor() {
        1 => "identity",
        2 => "down2",
        4 => "down4",
        8 => "down8",
        _ => unreachable!("scale validation keeps the benchmark factors bounded"),
    }
}

fn env_usize(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .map(|value| {
            value
                .parse()
                .unwrap_or_else(|_| panic!("{name} must be an integer"))
        })
        .unwrap_or(default)
}

#[cfg(target_os = "linux")]
fn current_rss_kib() -> u64 {
    proc_status_kib("VmRSS:")
}

#[cfg(not(target_os = "linux"))]
fn current_rss_kib() -> u64 {
    0
}

#[cfg(target_os = "linux")]
fn peak_rss_kib() -> u64 {
    proc_status_kib("VmHWM:")
}

#[cfg(not(target_os = "linux"))]
fn peak_rss_kib() -> u64 {
    0
}

#[cfg(target_os = "linux")]
fn proc_status_kib(prefix: &str) -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find(|line| line.starts_with(prefix))
                .and_then(|line| line.split_whitespace().nth(1))
                .and_then(|value| value.parse().ok())
        })
        .unwrap_or(0)
}
