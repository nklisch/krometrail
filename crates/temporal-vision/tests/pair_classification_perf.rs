//! Ignored, browser-free release benchmark for adjacent-pair classification accounting.
//!
//! This is deliberately a measurement scaffold, not a production optimization. It exercises the
//! current public storyboard, orientation, difference-map, and optional motion-history generators
//! over one deterministic 1080p moving-patch sequence. Run it explicitly with Rust 1.85 and
//! `--release`; ordinary workspace tests never execute this file because the test is ignored.
//!
//! Hardware counters are intentionally collected outside this process with `perf stat`. The JSON
//! report records the external counter status supplied through `PERF_PAIR_COUNTER_STATUS` rather
//! than manufacturing values from process CPU time.

use std::{
    alloc::{GlobalAlloc, Layout, System},
    env,
    num::{NonZeroU8, NonZeroU64, NonZeroUsize},
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

use libc::{RUSAGE_SELF, getrusage, rusage};
use serde::Serialize;
use sha2::{Digest, Sha256};
use temporal_vision::{
    ArtifactLabels, BinaryMask, DeclaredGap, DifferenceMapLimits, DifferenceMapParameters, Frame,
    FrameSequence, FrequencyMode, IntegerScale, Marker, MeasurementParameters,
    NormalizationParameters, NormalizedSequence, PixelDimensions, PixelFormat, ProcessingLimits,
    RenderLimits, Rgb8, StoryboardParameters, StoryboardTileLimit, TimePalette, TimeRange,
    Timestamp, generate_motion_history, generate_storyboard, normalize_sequence,
    render_difference_map,
};

type Source = FrameSequence<u32, u32, u32, Box<[u8]>>;
type Normalized = NormalizedSequence<u32>;
type Artifact = temporal_vision::GeneratedArtifact<u32, u32, u32, u32>;

const MEASUREMENT_NOISE_FLOOR: u16 = 512;
const FRAME_INTERVAL_NS: u64 = 10_000_000;
const MAX_RETAINED_BYTES: usize = 3_000_000_000;
const PERF_EVENTS: &str = "task-clock,cycles,instructions,cache-misses,branch-misses";

struct CountingAllocator;

static ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);

// This allocator is test-only accounting. Production crates retain their normal allocator and
// no algorithm or scheduler code is instrumented for the benchmark.
#[global_allocator]
static GLOBAL_ALLOCATOR: CountingAllocator = CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarding the caller's valid layout to the system allocator.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            ALLOCATED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarding the caller's valid layout to the system allocator.
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            ALLOCATED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: the pointer and layout were returned by this allocator.
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the pointer, old layout, and new size follow GlobalAlloc's realloc contract.
        let replacement = unsafe { System.realloc(pointer, layout, new_size) };
        if !replacement.is_null() {
            ALLOCATED_BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
        }
        replacement
    }
}

#[derive(Clone, Copy, Debug)]
struct Config {
    frames: usize,
    dimensions: PixelDimensions,
    scale: IntegerScale,
    evidence: Evidence,
    generators: Generators,
    repetitions: usize,
}

impl Config {
    fn from_environment() -> Self {
        let frames = env_usize("PERF_PAIR_FRAMES", 30);
        assert!(
            matches!(frames, 8 | 30 | 60 | 120),
            "PERF_PAIR_FRAMES must be one of 8, 30, 60, or 120"
        );
        let width = env_usize("PERF_PAIR_WIDTH", 1_920);
        let height = env_usize("PERF_PAIR_HEIGHT", 1_080);
        let dimensions = PixelDimensions::new(
            u32::try_from(width).expect("PERF_PAIR_WIDTH exceeds u32"),
            u32::try_from(height).expect("PERF_PAIR_HEIGHT exceeds u32"),
        )
        .unwrap();
        let scale = match env::var("PERF_PAIR_SCALE").as_deref() {
            Ok("identity") | Ok("1") | Err(_) => IntegerScale::IDENTITY,
            Ok("down2") | Ok("2") => IntegerScale::down(NonZeroU8::new(2).unwrap()).unwrap(),
            Ok(value) => panic!("unsupported PERF_PAIR_SCALE={value}"),
        };
        let evidence = match env::var("PERF_PAIR_EVIDENCE").as_deref() {
            Ok("clean") | Err(_) => Evidence::Clean,
            Ok("masked") => Evidence::Masked,
            Ok("gapped") => Evidence::Gapped,
            Ok(value) => panic!("unsupported PERF_PAIR_EVIDENCE={value}"),
        };
        let generators = match env::var("PERF_PAIR_GENERATORS").as_deref() {
            Ok("storyboard-difference") | Err(_) => Generators::StoryboardDifference,
            Ok("storyboard-difference-motion") => Generators::StoryboardDifferenceMotion,
            Ok(value) => panic!("unsupported PERF_PAIR_GENERATORS={value}"),
        };
        let repetitions = env_usize("PERF_PAIR_REPETITIONS", 1);
        assert!(
            (1..=20).contains(&repetitions),
            "PERF_PAIR_REPETITIONS must be between 1 and 20"
        );
        assert!(
            width % usize::from(scale.factor()) == 0 && height % usize::from(scale.factor()) == 0,
            "benchmark dimensions must be divisible by the selected scale"
        );
        Self {
            frames,
            dimensions,
            scale,
            evidence,
            generators,
            repetitions,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Evidence {
    Clean,
    Masked,
    Gapped,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum Generators {
    StoryboardDifference,
    StoryboardDifferenceMotion,
}

impl Generators {
    const fn includes_motion(self) -> bool {
        matches!(self, Self::StoryboardDifferenceMotion)
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
struct ConsumerAccounting {
    measure_adjacent_calls: u64,
    direct_pair_pixel_passes: u64,
    classified_pixel_passes: u64,
    classifier_pixel_calls: u64,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
struct Accounting {
    frame_count: u64,
    adjacent_pairs: u64,
    measurable_adjacent_pairs: u64,
    gap_pairs: u64,
    included_analysis_pixels: u64,
    storyboard_baseline_pair_calls: u64,
    storyboard_baseline_classified_pair_calls: u64,
    storyboard: ConsumerAccounting,
    difference: ConsumerAccounting,
    motion: Option<ConsumerAccounting>,
    total_measure_adjacent_calls: u64,
    total_direct_pair_pixel_passes: u64,
    expected_classified_pass_formula: String,
    expected_classified_pixel_passes: u64,
    expected_classifier_pixel_calls: u64,
    predicted_context_classified_pass_formula: String,
    predicted_context_classified_pixel_passes: u64,
    predicted_context_classifier_pixel_calls: u64,
    predicted_adjacent_pixel_pass_reduction: u64,
    predicted_classifier_call_reduction: u64,
    predicted_trace_bytes: u64,
    prediction_scope: &'static str,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
struct ArtifactDigest {
    artifact_digest: String,
    manifest_digest: String,
    image_digest: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
struct DigestReport {
    normalized_digest: String,
    artifact_digests: Vec<ArtifactDigest>,
    output_digest: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
struct CounterEvidence {
    source: &'static str,
    events: &'static str,
    status: String,
    note: String,
}

#[derive(Clone, Debug, Serialize)]
struct RepetitionReport {
    repetition: usize,
    cold_policy: &'static str,
    normalization_us: u128,
    generator_us: u128,
    wall_us: u128,
    process_cpu_us: u128,
    task_clock_us: Option<u128>,
    allocation_bytes: u64,
    rss_before_kib: u64,
    rss_after_kib: u64,
    peak_rss_kib: u64,
    accounting: Accounting,
    digests: DigestReport,
    duplicate_run_equal: bool,
}

#[derive(Clone, Debug, Serialize)]
struct BenchmarkReport {
    benchmark: &'static str,
    rust_policy: &'static str,
    cache_policy: &'static str,
    frame_count: usize,
    width: u32,
    height: u32,
    scale: &'static str,
    evidence: Evidence,
    generators: Generators,
    repetitions: usize,
    counter_evidence: CounterEvidence,
    runs: Vec<RepetitionReport>,
}

struct PipelineOutput {
    normalized: Normalized,
    artifacts: Vec<Artifact>,
    accounting: Accounting,
    digests: DigestReport,
}

struct TimedPipeline {
    output: PipelineOutput,
    normalization_us: u128,
    generator_us: u128,
    wall_us: u128,
    process_cpu_us: u128,
}

#[test]
#[ignore = "manual Rust 1.85 release benchmark; never part of ordinary tests"]
fn baseline_pair_classification_profile() {
    let config = Config::from_environment();
    let source = fixture(config);
    let mut runs = Vec::with_capacity(config.repetitions);

    for repetition in 1..=config.repetitions {
        let rss_before_kib = current_rss_kib();
        let peak_before_kib = peak_rss_kib();
        let allocated_before = allocated_bytes();
        let timed = timed_pipeline(&source, config);
        let allocated_after = allocated_bytes();
        let rss_after_kib = current_rss_kib();
        let peak_after_kib = peak_rss_kib();

        // Keep the measured output alive while a second, equal run checks normalized buffers and
        // all public artifact bytes/manifests. The duplicate is deliberately not included in the
        // timing or allocation deltas reported for the cold measured run.
        let duplicate = execute_pipeline(&source, config);
        assert_equivalent(&timed.output, &duplicate);
        drop(duplicate);

        runs.push(RepetitionReport {
            repetition,
            cold_policy: "new in-memory fixture; no artifact/source cache",
            normalization_us: timed.normalization_us,
            generator_us: timed.generator_us,
            wall_us: timed.wall_us,
            process_cpu_us: timed.process_cpu_us,
            task_clock_us: None,
            allocation_bytes: allocated_after.saturating_sub(allocated_before),
            rss_before_kib,
            rss_after_kib,
            peak_rss_kib: peak_after_kib.max(peak_before_kib),
            accounting: timed.output.accounting.clone(),
            digests: timed.output.digests.clone(),
            duplicate_run_equal: true,
        });
        drop(timed.output);
    }

    let counter_evidence = CounterEvidence {
        source: "external perf stat",
        events: PERF_EVENTS,
        status: env::var("PERF_PAIR_COUNTER_STATUS")
            .unwrap_or_else(|_| "not_provided".to_owned()),
        note: env::var("PERF_PAIR_COUNTER_NOTE").unwrap_or_else(|_| {
            "No counter values are synthesized; run the command under perf stat and record permission denial verbatim."
                .to_owned()
        }),
    };
    let report = BenchmarkReport {
        benchmark: "temporal_vision.pair_classification_baseline.v1",
        rust_policy: "rustup run 1.85.0 cargo test --release --locked",
        cache_policy: "none; each measured run normalizes and generates from the in-memory source",
        frame_count: config.frames,
        width: config.dimensions.width(),
        height: config.dimensions.height(),
        scale: scale_name(config.scale),
        evidence: config.evidence,
        generators: config.generators,
        repetitions: config.repetitions,
        counter_evidence,
        runs,
    };
    println!(
        "PAIR_CLASSIFICATION_REPORT {}",
        serde_json::to_string(&report).unwrap()
    );
}

fn timed_pipeline(source: &Source, config: Config) -> TimedPipeline {
    let started = Instant::now();
    let cpu_started = process_cpu_us();
    let normalization_started = Instant::now();
    let normalized = normalized(source, config);
    let normalization_us = normalization_started.elapsed().as_micros();
    let generator_started = Instant::now();
    let output = build_output(source, normalized, config);
    let generator_us = generator_started.elapsed().as_micros();
    TimedPipeline {
        output,
        normalization_us,
        generator_us,
        wall_us: started.elapsed().as_micros(),
        process_cpu_us: process_cpu_us().saturating_sub(cpu_started),
    }
}

fn execute_pipeline(source: &Source, config: Config) -> PipelineOutput {
    let normalized = normalized(source, config);
    build_output(source, normalized, config)
}

fn normalized(source: &Source, config: Config) -> Normalized {
    let limits = ProcessingLimits::new(
        NonZeroUsize::new(config.frames).unwrap(),
        NonZeroUsize::new(config.dimensions.pixel_count().unwrap()).unwrap(),
        NonZeroUsize::new(MAX_RETAINED_BYTES).unwrap(),
    );
    normalize_sequence(
        source,
        NormalizationParameters::new(Rgb8::new(0, 0, 0), None, config.scale, limits),
    )
    .unwrap()
}

fn build_output(source: &Source, normalized: Normalized, config: Config) -> PipelineOutput {
    let accounting = accounting(source, &normalized, config);
    assert_accounting(&accounting, config.generators.includes_motion());
    let labels =
        ArtifactLabels::new("TEMPORAL STORYBOARD", "KROMETRAIL RETAINED SOURCE FRAMES").unwrap();
    let anchor = source.frames()[source.frames().len() / 3].timestamp();
    let storyboard = generate_storyboard(
        1,
        Some(2),
        source,
        &normalized,
        StoryboardParameters::new(
            anchor,
            StoryboardTileLimit::default(),
            MeasurementParameters::new(MEASUREMENT_NOISE_FLOOR),
            labels,
            RenderLimits::default(),
        ),
    )
    .unwrap();
    let difference = render_difference_map(
        3,
        source,
        &normalized,
        DifferenceMapParameters::new(
            0,
            FrequencyMode::NormalizedFrequency,
            TimePalette::Spectral,
            None,
            MeasurementParameters::new(MEASUREMENT_NOISE_FLOOR),
            Rgb8::new(0, 0, 0),
            DifferenceMapLimits::default(),
        ),
    )
    .unwrap();
    let mut artifacts = vec![
        storyboard.storyboard().clone(),
        storyboard.orientation().unwrap().clone(),
        difference,
    ];
    if config.generators.includes_motion() {
        let motion_labels = ArtifactLabels::new(
            "TEMPORAL MOTION HISTORY",
            "KROMETRAIL RETAINED SOURCE FRAMES",
        )
        .unwrap();
        artifacts.push(
            generate_motion_history(
                4,
                source,
                &normalized,
                temporal_vision::MotionHistoryParameters::new(
                    0,
                    MeasurementParameters::new(MEASUREMENT_NOISE_FLOOR),
                    temporal_vision::MotionDecay::default(),
                    64,
                    Rgb8::new(255, 176, 0),
                    Rgb8::new(255, 255, 255),
                    motion_labels,
                    RenderLimits::default(),
                ),
            )
            .unwrap(),
        );
    }
    let digests = digest_report(&normalized, &artifacts);
    PipelineOutput {
        normalized,
        artifacts,
        accounting,
        digests,
    }
}

fn accounting(source: &Source, normalized: &Normalized, config: Config) -> Accounting {
    let adjacent_pairs = source.frames().len().saturating_sub(1);
    let gap_pairs = (1..source.frames().len())
        .filter(|later| {
            intersects_gap(
                source,
                source.frames()[later - 1].timestamp(),
                source.frames()[*later].timestamp(),
            )
        })
        .count();
    let measurable_adjacent_pairs = adjacent_pairs.saturating_sub(gap_pairs);
    let anchor = source.frames()[source.frames().len() / 3].timestamp();
    let baseline = source
        .frames()
        .iter()
        .rposition(|frame| frame.timestamp() < anchor)
        .unwrap_or(0);
    let mut storyboard_baseline_pair_calls = 0_usize;
    let mut storyboard_baseline_classified_pair_calls = 0_usize;
    for later in baseline.saturating_add(1)..source.frames().len() {
        storyboard_baseline_pair_calls += 1;
        if intersects_gap(
            source,
            source.frames()[baseline].timestamp(),
            source.frames()[later].timestamp(),
        ) {
            // The current selector calls measure_pair once at the continuity boundary to obtain
            // its gap metadata, then stops. That call performs no pixel classification.
            break;
        }
        storyboard_baseline_classified_pair_calls += 1;
    }

    let included = normalized.analysis_pixel_count();
    let adjacent = u64::try_from(adjacent_pairs).unwrap();
    let measurable = u64::try_from(measurable_adjacent_pairs).unwrap();
    let gap = u64::try_from(gap_pairs).unwrap();
    let baseline_calls = u64::try_from(storyboard_baseline_pair_calls).unwrap();
    let baseline_classified = u64::try_from(storyboard_baseline_classified_pair_calls).unwrap();
    let storyboard_passes = measurable + baseline_classified;
    let difference_passes = measurable;
    let motion_passes = measurable * 2;
    let storyboard = ConsumerAccounting {
        measure_adjacent_calls: adjacent,
        direct_pair_pixel_passes: 0,
        classified_pixel_passes: storyboard_passes,
        classifier_pixel_calls: included * storyboard_passes,
    };
    let difference = ConsumerAccounting {
        measure_adjacent_calls: 0,
        direct_pair_pixel_passes: difference_passes,
        classified_pixel_passes: difference_passes,
        classifier_pixel_calls: included * difference_passes,
    };
    let motion = config
        .generators
        .includes_motion()
        .then(|| ConsumerAccounting {
            measure_adjacent_calls: adjacent,
            direct_pair_pixel_passes: measurable,
            classified_pixel_passes: motion_passes,
            classifier_pixel_calls: included * motion_passes,
        });
    let total_measure_adjacent_calls = adjacent
        + if config.generators.includes_motion() {
            adjacent
        } else {
            0
        };
    let total_direct_pair_pixel_passes = difference_passes
        + if config.generators.includes_motion() {
            measurable
        } else {
            0
        };
    let expected_passes = storyboard_passes
        + difference_passes
        + if config.generators.includes_motion() {
            motion_passes
        } else {
            0
        };
    let expected_formula = if config.generators.includes_motion() {
        "4M+B".to_owned()
    } else {
        "2M+B".to_owned()
    };
    // Pure-kernel prediction for one request-local context: adjacent measurement,
    // difference, and optional motion consumers share M; selector baseline work B
    // remains explicit and non-adjacent. This is not an end-to-end service claim.
    let predicted_context_passes = measurable + baseline_classified;
    let predicted_context_formula = "M+B".to_owned();
    let predicted_trace_bytes = adjacent
        .checked_mul(80)
        .and_then(|bytes| bytes.checked_add(64))
        .unwrap();
    Accounting {
        frame_count: u64::try_from(source.frames().len()).unwrap(),
        adjacent_pairs: adjacent,
        measurable_adjacent_pairs: measurable,
        gap_pairs: gap,
        included_analysis_pixels: included,
        storyboard_baseline_pair_calls: baseline_calls,
        storyboard_baseline_classified_pair_calls: baseline_classified,
        storyboard,
        difference,
        motion,
        total_measure_adjacent_calls,
        total_direct_pair_pixel_passes,
        expected_classified_pass_formula: expected_formula,
        expected_classified_pixel_passes: expected_passes,
        expected_classifier_pixel_calls: included * expected_passes,
        predicted_context_classified_pass_formula: predicted_context_formula,
        predicted_context_classified_pixel_passes: predicted_context_passes,
        predicted_context_classifier_pixel_calls: included * predicted_context_passes,
        predicted_adjacent_pixel_pass_reduction: expected_passes - predicted_context_passes,
        predicted_classifier_call_reduction: included
            * (expected_passes - predicted_context_passes),
        predicted_trace_bytes,
        prediction_scope: "pure temporal-vision kernel; no service or scheduler integration",
    }
}

fn assert_accounting(accounting: &Accounting, includes_motion: bool) {
    assert_eq!(
        accounting.adjacent_pairs,
        accounting.measurable_adjacent_pairs + accounting.gap_pairs
    );
    let expected = if includes_motion {
        4 * accounting.measurable_adjacent_pairs
            + accounting.storyboard_baseline_classified_pair_calls
    } else {
        2 * accounting.measurable_adjacent_pairs
            + accounting.storyboard_baseline_classified_pair_calls
    };
    assert_eq!(accounting.expected_classified_pixel_passes, expected);
    assert_eq!(
        accounting.expected_classifier_pixel_calls,
        accounting.included_analysis_pixels * expected
    );
    let predicted =
        accounting.measurable_adjacent_pairs + accounting.storyboard_baseline_classified_pair_calls;
    assert_eq!(accounting.predicted_context_classified_pass_formula, "M+B");
    assert_eq!(
        accounting.predicted_context_classified_pixel_passes,
        predicted
    );
    assert_eq!(
        accounting.predicted_context_classifier_pixel_calls,
        accounting.included_analysis_pixels * predicted
    );
    assert_eq!(
        accounting.predicted_adjacent_pixel_pass_reduction,
        expected - predicted
    );
    assert_eq!(
        accounting.predicted_classifier_call_reduction,
        accounting.included_analysis_pixels * (expected - predicted)
    );
    assert!(accounting.predicted_trace_bytes <= accounting.adjacent_pairs * 80 + 64);
    assert_eq!(
        accounting.prediction_scope,
        "pure temporal-vision kernel; no service or scheduler integration"
    );
    assert_eq!(
        accounting.storyboard.classified_pixel_passes,
        accounting.measurable_adjacent_pairs + accounting.storyboard_baseline_classified_pair_calls
    );
    assert_eq!(
        accounting.difference.classified_pixel_passes,
        accounting.measurable_adjacent_pairs
    );
    if let Some(motion) = &accounting.motion {
        assert_eq!(
            motion.classified_pixel_passes,
            2 * accounting.measurable_adjacent_pairs
        );
    }
}

fn intersects_gap(source: &Source, earlier: Timestamp, later: Timestamp) -> bool {
    source
        .gaps()
        .iter()
        .any(|gap| gap.range().start() <= later && gap.range().end() >= earlier)
}

fn assert_equivalent(first: &PipelineOutput, second: &PipelineOutput) {
    assert_eq!(
        first.normalized, second.normalized,
        "normalized buffers differ"
    );
    assert_eq!(first.artifacts, second.artifacts, "artifact bytes differ");
    assert_eq!(first.accounting, second.accounting, "accounting differs");
    assert_eq!(first.digests, second.digests, "digest report differs");
}

fn digest_report(normalized: &Normalized, artifacts: &[Artifact]) -> DigestReport {
    let normalized_digest = normalized_digest(normalized);
    let artifact_digests = artifacts.iter().map(artifact_digest).collect::<Vec<_>>();
    let output_digest = digest_strings(
        &artifact_digests
            .iter()
            .map(|digest| digest.artifact_digest.clone())
            .collect::<Vec<_>>(),
    );
    DigestReport {
        normalized_digest,
        artifact_digests,
        output_digest,
    }
}

fn normalized_digest(normalized: &Normalized) -> String {
    let mut digest = Sha256::new();
    digest.update(normalized.source_dimensions().width().to_be_bytes());
    digest.update(normalized.source_dimensions().height().to_be_bytes());
    digest.update(normalized.dimensions().width().to_be_bytes());
    digest.update(normalized.dimensions().height().to_be_bytes());
    digest.update(normalized.analysis_pixel_count().to_be_bytes());
    for range in normalized.gap_ranges() {
        digest.update(range.start().as_nanos().to_be_bytes());
        digest.update(range.end().as_nanos().to_be_bytes());
    }
    if let Some(mask) = normalized.analysis_mask() {
        digest.update([1]);
        digest.update(mask.bits());
    } else {
        digest.update([0]);
    }
    digest.update(serde_json::to_vec(normalized.normalization_steps()).unwrap());
    for frame in normalized.frames() {
        digest.update(frame.id().to_be_bytes());
        digest.update(frame.timestamp().as_nanos().to_be_bytes());
        digest.update(frame.dimensions().width().to_be_bytes());
        digest.update(frame.dimensions().height().to_be_bytes());
        for value in frame.linear_rgb16() {
            digest.update(value.to_be_bytes());
        }
    }
    format!("{:x}", digest.finalize())
}

fn artifact_digest(artifact: &Artifact) -> ArtifactDigest {
    let manifest_bytes = serde_json::to_vec(artifact.manifest()).unwrap();
    let manifest_digest = sha256_hex(&manifest_bytes);
    let image_digest = sha256_hex(artifact.image().bytes());
    let mut combined = Sha256::new();
    combined.update(&manifest_bytes);
    combined.update(artifact.image().bytes());
    ArtifactDigest {
        artifact_digest: format!("{:x}", combined.finalize()),
        manifest_digest,
        image_digest,
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn digest_strings(values: &[String]) -> String {
    let mut digest = Sha256::new();
    for value in values {
        digest.update(value.as_bytes());
        digest.update([0]);
    }
    format!("{:x}", digest.finalize())
}

fn fixture(config: Config) -> Source {
    let dimensions = config.dimensions;
    let mut background = vec![0_u8; dimensions.rgba8_byte_len().unwrap()];
    for y in 0..dimensions.height() {
        for x in 0..dimensions.width() {
            let pixel = usize::try_from(y * dimensions.width() + x).unwrap();
            let offset = pixel * 4;
            background[offset..offset + 4].copy_from_slice(&[
                (x.wrapping_mul(3).wrapping_add(y.wrapping_mul(5)) & 0xff) as u8,
                (x.wrapping_mul(7).wrapping_add(y.wrapping_mul(2)) & 0xff) as u8,
                (x.wrapping_mul(11).wrapping_add(y.wrapping_mul(13)) & 0xff) as u8,
                u8::MAX,
            ]);
        }
    }
    let scale = u32::from(config.scale.factor());
    let patch_side = 128_u32
        .min(dimensions.width() / 8)
        .min(dimensions.height() / 8);
    let inset_x = ((dimensions.width() / 8) / scale).max(1) * scale;
    let inset_y = ((dimensions.height() / 8) / scale).max(1) * scale;
    let right = dimensions.width().saturating_sub(inset_x);
    let bottom = dimensions.height().saturating_sub(inset_y);
    let travel = right.saturating_sub(inset_x + patch_side).max(scale);
    let patch_y = ((dimensions.height() / 2).saturating_sub(patch_side / 2) / scale) * scale;
    let frames = (0..config.frames)
        .map(|frame_index| {
            let mut pixels = background.clone();
            let patch_x = inset_x + (u32::try_from(frame_index).unwrap().wrapping_mul(17) % travel);
            for y in patch_y..patch_y.saturating_add(patch_side).min(bottom) {
                for x in patch_x..patch_x.saturating_add(patch_side).min(right) {
                    let pixel = usize::try_from(y * dimensions.width() + x).unwrap();
                    let offset = pixel * 4;
                    let checker = ((x + y + u32::try_from(frame_index).unwrap()) % 2) as u8;
                    pixels[offset..offset + 4].copy_from_slice(&[
                        240_u8.saturating_sub(checker * 24),
                        72_u8.saturating_add(checker * 24),
                        24_u8.saturating_add(checker * 16),
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
        .collect::<Vec<_>>();
    let mask = (config.evidence == Evidence::Masked).then(|| rectangular_mask(dimensions, scale));
    let gaps = (config.evidence == Evidence::Gapped)
        .then(|| {
            let later = config.frames / 2;
            let earlier_time = u64::try_from(later - 1).unwrap() * FRAME_INTERVAL_NS;
            DeclaredGap::new(
                1_u32,
                TimeRange::new(
                    Timestamp::from_nanos(earlier_time + FRAME_INTERVAL_NS / 2),
                    Timestamp::from_nanos(earlier_time + FRAME_INTERVAL_NS / 2),
                )
                .unwrap(),
                "deterministic benchmark capture gap",
                NonZeroU64::new(1),
            )
            .unwrap()
        })
        .into_iter()
        .collect();
    FrameSequence::new(frames, Vec::<Marker<u32>>::new(), gaps, None, mask).unwrap()
}

fn rectangular_mask(dimensions: PixelDimensions, scale: u32) -> BinaryMask {
    let pixel_count = dimensions.pixel_count().unwrap();
    let mut bits = vec![0_u8; pixel_count.div_ceil(8)];
    let inset_x = ((dimensions.width() / 8) / scale).max(1) * scale;
    let inset_y = ((dimensions.height() / 8) / scale).max(1) * scale;
    let right = dimensions.width().saturating_sub(inset_x);
    let bottom = dimensions.height().saturating_sub(inset_y);
    for y in inset_y..bottom {
        for x in inset_x..right {
            let index = usize::try_from(y * dimensions.width() + x).unwrap();
            bits[index / 8] |= 0x80 >> (index % 8);
        }
    }
    BinaryMask::new(dimensions, bits.into_boxed_slice()).unwrap()
}

fn scale_name(scale: IntegerScale) -> &'static str {
    match scale.factor() {
        1 => "identity",
        2 => "down2",
        _ => unreachable!("benchmark only accepts identity and down2"),
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

fn allocated_bytes() -> u64 {
    ALLOCATED_BYTES.load(Ordering::Relaxed)
}

fn process_cpu_us() -> u128 {
    let usage = unsafe {
        let mut usage = std::mem::MaybeUninit::<rusage>::zeroed();
        assert_eq!(getrusage(RUSAGE_SELF, usage.as_mut_ptr()), 0);
        usage.assume_init()
    };
    u128::from(timeval_us(usage.ru_utime)) + u128::from(timeval_us(usage.ru_stime))
}

fn timeval_us(value: libc::timeval) -> u64 {
    let seconds = u64::try_from(value.tv_sec).unwrap_or(0);
    let micros = u64::try_from(value.tv_usec).unwrap_or(0);
    seconds.saturating_mul(1_000_000).saturating_add(micros)
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
    let usage = unsafe {
        let mut usage = std::mem::MaybeUninit::<rusage>::zeroed();
        assert_eq!(getrusage(RUSAGE_SELF, usage.as_mut_ptr()), 0);
        usage.assume_init()
    };
    // macOS reports ru_maxrss in bytes; other Unix platforms are best-effort here.
    u64::try_from(usage.ru_maxrss).unwrap_or(0) / 1024
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
