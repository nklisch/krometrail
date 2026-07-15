//! Ignored, browser-free Rust 1.85 release benchmark for overlapping artifact requests.
//!
//! This is a measurement scaffold for `perf-temporal-overlap-frame-reuse`, not an optimization.
//! It exercises the production artifact service and recording store with adjacent windows that
//! share N-1 retained PNG frames, plus sequential one-frame sliding windows. Run one cell per
//! process so RSS/HWM and the counting allocator describe that cell without cross-test noise.
//!
//! Example:
//!
//! ```text
//! PERF_OVERLAP_FRAMES=120 PERF_OVERLAP_MODE=concurrent PERF_OVERLAP_REQUEST_PERMITS=2 \
//!   rustup run 1.85.0 cargo test --release --locked \
//!   overlap_and_sliding_release_profile -- --ignored --exact --nocapture
//! ```
//!
//! The production-policy request uses explicit down-2 analysis, matching the retained discovery
//! baseline and keeping all 30/60/120 cells below the artifact output ceiling. The benchmark
//! deliberately raises only the combined
//! request ceiling enough to make the two-permit cells observable; it does not change the
//! decoder, generator, or publication path. The identity-scale case is kept in the design's
//! memory budget because one 1920x1080 normalized RGB16 frame is 12,441,600 bytes before the
//! surrounding decoded and output reservations.

use std::{
    alloc::{GlobalAlloc, Layout, System},
    io::Cursor,
    num::NonZeroUsize,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

use image::{ColorType, ImageEncoder, codecs::png::PngEncoder};
use krometrail_core::{
    AnalysisScale, ArtifactFailurePolicy, ArtifactGeneration, ArtifactGenerationContext,
    ArtifactGenerationRequest, ArtifactGeneratorRequest, ArtifactLabelsRequest, ArtifactOutcome,
    ArtifactStore, CaptureOrdinal, CapturedFrame, DeviceScaleFactor, DifferenceMapRequest,
    EncodedFrame, FrameId, FrameSelector, FrameSource, IdSource, IdValue, ImageFormat,
    NonEmptyText, NormalizationRequest, ObservedTime, OutputLimitsRequest, PixelDimensions,
    RangeResolutionOptions, RecordingSink, ResolvedRange, SessionId, SessionRange, SessionTime,
    StoredArtifact, StoryboardRequest, TargetId, TemporalRangeAnchorKind,
};
use krometrail_store::{
    IndexStoreConfig, RecordingStore, RotationConfig, SegmentStoreConfig, SegmentWriter,
    SqliteIndex,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use temporal_vision::{FrequencyMode, Rgb8};
use uuid::Uuid;

use super::{
    ArtifactWorkLimits, TemporalVisionArtifactService,
    perf_counters::{self, Snapshot as CounterSnapshot},
};

const FRAME_INTERVAL_NS: u64 = 10_000_000;
const NORMALIZED_BYTES_LIMIT: usize = 512 * 1024 * 1024;
const BENCHMARK_COMBINED_BYTES: usize = 2_000_000_000;
const BENCHMARK_DECODED_BYTES: usize = 1_100_000_000;
const PERF_EVENTS: &str = "task-clock,cycles,instructions,cache-misses,branch-misses";

struct CountingAllocator;

static ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Mode {
    Concurrent,
    Sequential,
}

impl Mode {
    fn from_environment() -> Self {
        match std::env::var("PERF_OVERLAP_MODE")
            .unwrap_or_else(|_| "concurrent".to_owned())
            .as_str()
        {
            "concurrent" => Self::Concurrent,
            "sequential" => Self::Sequential,
            value => panic!("unsupported PERF_OVERLAP_MODE={value}"),
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Concurrent => "concurrent_adjacent",
            Self::Sequential => "sequential_sliding",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Config {
    frames: usize,
    request_permits: usize,
    sliding_windows: usize,
    repetitions: usize,
    mode: Mode,
}

impl Config {
    fn from_environment() -> Self {
        let frames = env_usize("PERF_OVERLAP_FRAMES", 60);
        assert!(matches!(frames, 30 | 60 | 120));
        let request_permits = env_usize("PERF_OVERLAP_REQUEST_PERMITS", 2);
        assert!(matches!(request_permits, 1 | 2));
        let sliding_windows = env_usize("PERF_OVERLAP_SLIDING_WINDOWS", 4);
        assert!((1..=8).contains(&sliding_windows));
        let repetitions = env_usize("PERF_OVERLAP_REPETITIONS", 1);
        assert!((1..=5).contains(&repetitions));
        Self {
            frames,
            request_permits,
            sliding_windows,
            repetitions,
            mode: Mode::from_environment(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
struct ArtifactEvidence {
    artifact_id: String,
    kind: String,
    cache: String,
    manifest_sha256: String,
    output_sha256: String,
    artifact_sha256: String,
}

#[derive(Clone, Debug, Serialize)]
struct RunReport {
    repetition: usize,
    mode: &'static str,
    frame_count: usize,
    shared_frames: usize,
    request_permits: usize,
    sliding_windows: usize,
    wall_us: u128,
    process_cpu_us: u128,
    allocation_bytes: u64,
    rss_before_kib: u64,
    rss_after_kib: u64,
    peak_rss_before_kib: u64,
    peak_rss_after_kib: u64,
    peak_rss_delta_kib: u64,
    counters: CounterSnapshot,
    expected_decode_calls: u64,
    expected_normalize_calls: u64,
    intermediate_decode_hits: u64,
    intermediate_normalize_hits: u64,
    expected_decode_hits_if_enabled: u64,
    expected_normalize_hits_if_enabled: u64,
    artifact_cache_hits: u64,
    artifact_generated: u64,
    artifact_evidence: Vec<ArtifactEvidence>,
    current_request_memory_reservation_bytes: usize,
    current_total_requested_memory_bytes: usize,
    unique_shared_intermediate_bytes_if_enabled: usize,
    scheduler_combined_budget_bytes: usize,
    scheduler_blocking_permits: usize,
    scheduler_generator_permits: usize,
    capture_headroom_proxy: &'static str,
    perf_events: &'static str,
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "manual Rust 1.85 overlap/reuse benchmark; never part of ordinary tests"]
async fn overlap_and_sliding_release_profile() {
    let config = Config::from_environment();
    for repetition in 1..=config.repetitions {
        let run = run_case(config, repetition).await;
        println!(
            "TEMPORAL_OVERLAP_REPORT {}",
            serde_json::to_string(&run).unwrap()
        );
    }
}

async fn run_case(config: Config, repetition: usize) -> RunReport {
    let root = std::env::temp_dir().join(format!(
        "krometrail-overlap-perf-{}-{}",
        std::process::id(),
        Uuid::new_v4()
    ));
    let segments = root.join("segments");
    let index = Arc::new(
        SqliteIndex::open(IndexStoreConfig {
            database_path: root.join("index.sqlite3"),
            segments_directory: segments.clone(),
            busy_timeout: std::time::Duration::from_secs(1),
        })
        .unwrap(),
    );
    let writer = Arc::new(
        SegmentWriter::open(SegmentStoreConfig {
            directory: segments,
            rotation: RotationConfig::suggested(),
        })
        .unwrap(),
    );
    let store = Arc::new(RecordingStore::new(writer, Arc::clone(&index)).unwrap());
    let session = SessionId::from_uuid(Uuid::from_u128(0x7000 + repetition as u128));
    let target = TargetId::from_uuid(Uuid::from_u128(0x8000 + repetition as u128));
    let dimensions = PixelDimensions::new(1_920, 1_080).unwrap();
    let source_count = config.frames
        + match config.mode {
            Mode::Concurrent => 1,
            Mode::Sequential => config.sliding_windows,
        };
    let frame_ids = append_source_frames(&store, session, target, source_count, dimensions).await;
    store.flush(session).await.unwrap();

    let limits = ArtifactWorkLimits {
        max_active_requests: NonZeroUsize::new(config.request_permits).unwrap(),
        max_parallel_generators_per_request: NonZeroUsize::new(1).unwrap(),
        max_decoded_bytes: NonZeroUsize::new(BENCHMARK_DECODED_BYTES).unwrap(),
        max_normalized_bytes: NonZeroUsize::new(NORMALIZED_BYTES_LIMIT).unwrap(),
        max_combined_request_bytes: NonZeroUsize::new(BENCHMARK_COMBINED_BYTES).unwrap(),
        ..ArtifactWorkLimits::default()
    };
    let service = TemporalVisionArtifactService::new(
        Arc::clone(&store) as Arc<dyn FrameSource>,
        Arc::clone(&store) as Arc<dyn ArtifactStore>,
        Arc::new(BenchmarkIds(AtomicU64::new(0))),
        limits,
    )
    .unwrap();
    let requests = match config.mode {
        Mode::Concurrent => vec![
            request(session, target, &frame_ids[0..config.frames], 0),
            request(session, target, &frame_ids[1..=config.frames], 1),
        ],
        Mode::Sequential => (0..config.sliding_windows)
            .map(|start| {
                request(
                    session,
                    target,
                    &frame_ids[start..start + config.frames],
                    start,
                )
            })
            .collect(),
    };

    perf_counters::reset();
    let allocation_before = ALLOCATED_BYTES.load(Ordering::SeqCst);
    let cpu_before = process_cpu_us();
    let rss_before = current_rss_kib();
    let peak_before = peak_rss_kib();
    let started = Instant::now();
    let results = match config.mode {
        Mode::Concurrent => {
            let (first, second) = tokio::join!(
                service.generate(requests[0].clone(), ArtifactGenerationContext::default()),
                service.generate(requests[1].clone(), ArtifactGenerationContext::default()),
            );
            vec![first.unwrap(), second.unwrap()]
        }
        Mode::Sequential => {
            let mut results = Vec::with_capacity(requests.len());
            for request in requests {
                results.push(
                    service
                        .generate(request, ArtifactGenerationContext::default())
                        .await
                        .unwrap(),
                );
            }
            results
        }
    };
    let wall_us = started.elapsed().as_micros();
    let process_cpu_us = process_cpu_us().saturating_sub(cpu_before);
    let allocation_bytes = ALLOCATED_BYTES
        .load(Ordering::SeqCst)
        .saturating_sub(allocation_before);
    let counters = perf_counters::snapshot();
    let rss_after = current_rss_kib();
    let peak_after = peak_rss_kib();
    let artifact_evidence = artifact_evidence(&store, &results).await;
    let artifact_cache_hits = artifact_evidence
        .iter()
        .filter(|artifact| artifact.cache == "Hit")
        .count() as u64;
    let artifact_generated = artifact_evidence
        .iter()
        .filter(|artifact| artifact.cache != "Hit")
        .count() as u64;
    let request_count = match config.mode {
        Mode::Concurrent => 2,
        Mode::Sequential => config.sliding_windows,
    };
    let expected_decode_calls = config.frames.saturating_mul(request_count) as u64;
    let expected_normalize_calls = config.frames.saturating_mul(request_count) as u64;
    let current_request_memory_reservation_bytes = request_memory_reservation(config.frames);
    let current_total_requested_memory_bytes =
        current_request_memory_reservation_bytes.saturating_mul(request_count);
    let unique_shared_intermediate_bytes_if_enabled = (config.frames + 1)
        .saturating_mul(decoded_bytes_per_frame() + normalized_bytes_per_frame());
    let report = RunReport {
        repetition,
        mode: config.mode.as_str(),
        frame_count: config.frames,
        shared_frames: config.frames.saturating_sub(1),
        request_permits: config.request_permits,
        sliding_windows: config.sliding_windows,
        wall_us,
        process_cpu_us,
        allocation_bytes,
        rss_before_kib: rss_before,
        rss_after_kib: rss_after,
        peak_rss_before_kib: peak_before,
        peak_rss_after_kib: peak_after,
        peak_rss_delta_kib: peak_after.saturating_sub(peak_before),
        counters,
        expected_decode_calls,
        expected_normalize_calls,
        // Current post-rollback service has no cross-request intermediate cache; these are the
        // observed hit counters. The expected fields define the candidate's equivalence target.
        intermediate_decode_hits: 0,
        intermediate_normalize_hits: 0,
        expected_decode_hits_if_enabled: if config.mode == Mode::Concurrent
            && config.request_permits == 2
        {
            config.frames.saturating_sub(1) as u64
        } else {
            0
        },
        expected_normalize_hits_if_enabled: if config.mode == Mode::Concurrent
            && config.request_permits == 2
        {
            config.frames.saturating_sub(1) as u64
        } else {
            0
        },
        artifact_cache_hits,
        artifact_generated,
        artifact_evidence,
        current_request_memory_reservation_bytes,
        current_total_requested_memory_bytes,
        unique_shared_intermediate_bytes_if_enabled,
        scheduler_combined_budget_bytes: BENCHMARK_COMBINED_BYTES,
        scheduler_blocking_permits: limits.max_blocking_jobs.get(),
        scheduler_generator_permits: limits.max_parallel_generators_per_request.get(),
        capture_headroom_proxy: "browser-free: compare request/cpu/memory permits; no CDP queue claim",
        perf_events: PERF_EVENTS,
    };

    drop(service);
    drop(store);
    drop(index);
    let _ = std::fs::remove_dir_all(root);
    report
}

struct BenchmarkIds(AtomicU64);

impl IdSource for BenchmarkIds {
    fn next(&self) -> IdValue {
        IdValue::from_uuid(Uuid::from_u128(
            0x9000 + self.0.fetch_add(1, Ordering::SeqCst) as u128,
        ))
    }
}

async fn append_source_frames(
    store: &RecordingStore,
    session: SessionId,
    target: TargetId,
    count: usize,
    dimensions: PixelDimensions,
) -> Vec<FrameId> {
    let mut ids = Vec::with_capacity(count);
    for position in 0..count {
        let id = FrameId::from_uuid(Uuid::from_u128(0xa000 + position as u128));
        let timestamp = position as u64 * FRAME_INTERVAL_NS;
        let frame = EncodedFrame::new(
            CapturedFrame::new(
                id,
                session,
                target,
                CaptureOrdinal::new(position as u64 + 1).unwrap(),
                None,
                ObservedTime::from_nanos(timestamp + 1),
                SessionTime::from_nanos(timestamp),
                ImageFormat::Png,
                dimensions,
                dimensions,
                DeviceScaleFactor::new(1.0).unwrap(),
                vec![],
            )
            .unwrap(),
            production_png(position, dimensions),
        )
        .unwrap();
        store.append_frame(frame).await.unwrap();
        ids.push(id);
    }
    ids
}

fn request(
    session: SessionId,
    target: TargetId,
    frame_ids: &[FrameId],
    start_position: usize,
) -> ArtifactGenerationRequest {
    let frame_count = frame_ids.len();
    let start = SessionTime::from_nanos(start_position as u64 * FRAME_INTERVAL_NS);
    let end = SessionTime::from_nanos(
        (start_position + frame_count.saturating_sub(1)) as u64 * FRAME_INTERVAL_NS,
    );
    let range = SessionRange::new(start, end).unwrap();
    let labels = ArtifactLabelsRequest::new(
        NonEmptyText::new("overlap storyboard").unwrap(),
        NonEmptyText::new("retained overlap benchmark").unwrap(),
    );
    let normalization = NormalizationRequest::new(
        None,
        Rgb8::new(0, 0, 0),
        // The retained discovery baseline used explicit down-2 analysis so every 30/60/120
        // cell is a valid production artifact request under the 64 MiB output ceiling. Identity
        // remains a separate memory-budget case in the design below.
        AnalysisScale::Down(2),
    )
    .unwrap();
    let output = OutputLimitsRequest::new(4_096, 4_096, 64 * 1024 * 1024).unwrap();
    let generators = vec![
        ArtifactGeneratorRequest::Storyboard(StoryboardRequest {
            anchor: SessionTime::from_nanos(
                (start_position + frame_count / 3) as u64 * FRAME_INTERVAL_NS,
            ),
            tile_limit: 8,
            noise_floor: 512,
            normalization,
            labels,
            include_orientation: true,
            output,
        }),
        ArtifactGeneratorRequest::DifferenceMap(DifferenceMapRequest {
            reference: FrameSelector::First,
            frequency_mode: FrequencyMode::NormalizedFrequency,
            repeated_change_separation_nanos: None,
            noise_floor: 512,
            normalization,
            canvas_background: Rgb8::new(0, 0, 0),
            output,
        }),
    ];
    ArtifactGenerationRequest::new(
        ResolvedRange::new(
            session,
            target,
            TemporalRangeAnchorKind::SessionTime,
            range,
            range,
            frame_ids.to_vec(),
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            RangeResolutionOptions::DEFAULT,
        )
        .unwrap(),
        vec![],
        generators,
        ArtifactFailurePolicy::RequireAll,
    )
    .unwrap()
}

async fn artifact_evidence(
    store: &RecordingStore,
    results: &[krometrail_core::ArtifactGenerationResult],
) -> Vec<ArtifactEvidence> {
    let mut evidence = Vec::new();
    for result in results {
        for outcome in &result.outcomes {
            let ArtifactOutcome::Available { artifact, .. } = outcome else {
                panic!("benchmark request did not produce every requested artifact")
            };
            let stored = store
                .artifact(artifact.artifact_id)
                .await
                .unwrap()
                .expect("published benchmark artifact remains readable");
            evidence.push(artifact_evidence_one(artifact, &stored));
        }
    }
    evidence
}

fn artifact_evidence_one(
    handle: &krometrail_core::ArtifactHandle,
    stored: &StoredArtifact,
) -> ArtifactEvidence {
    let manifest = serde_json::to_vec(&stored.manifest).unwrap();
    let manifest_sha256 = sha256_hex(&manifest);
    let output_sha256 = sha256_hex(&stored.encoded_bytes);
    let mut digest = Sha256::new();
    digest.update(&manifest);
    digest.update(&stored.encoded_bytes);
    ArtifactEvidence {
        artifact_id: handle.artifact_id.to_string(),
        kind: stored.manifest.artifact_kind().as_str().to_owned(),
        cache: format!("{:?}", handle.cache),
        manifest_sha256,
        output_sha256,
        artifact_sha256: format!("{:x}", digest.finalize()),
    }
}

fn decoded_bytes_per_frame() -> usize {
    1_920 * 1_080 * 4
}

fn normalized_bytes_per_frame() -> usize {
    960 * 540 * 6
}

fn request_memory_reservation(frames: usize) -> usize {
    frames
        .saturating_mul(decoded_bytes_per_frame() + normalized_bytes_per_frame())
        .saturating_add(3 * 64 * 1024 * 1024)
}

fn production_png(position: usize, dimensions: PixelDimensions) -> Vec<u8> {
    let width = dimensions.width() as usize;
    let height = dimensions.height() as usize;
    let mut pixels = vec![0_u8; width * height * 4];
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.copy_from_slice(&[18, 22, 32, u8::MAX]);
    }
    let patch_x = (position * 19) % (width - 192);
    let patch_y = (position * 11) % (height - 108);
    for y in patch_y..patch_y + 108 {
        for x in patch_x..patch_x + 192 {
            let pixel = &mut pixels[(y * width + x) * 4..(y * width + x) * 4 + 4];
            pixel.copy_from_slice(&[220, 70, 120, u8::MAX]);
        }
    }
    let mut bytes = Vec::new();
    PngEncoder::new(Cursor::new(&mut bytes))
        .write_image(
            &pixels,
            dimensions.width(),
            dimensions.height(),
            ColorType::Rgba8.into(),
        )
        .unwrap();
    bytes
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .map(|value| {
            value
                .parse()
                .unwrap_or_else(|_| panic!("{name} must be an integer"))
        })
        .unwrap_or(default)
}

fn process_cpu_us() -> u128 {
    let usage = unsafe {
        let mut usage = std::mem::MaybeUninit::<libc::rusage>::zeroed();
        assert_eq!(libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()), 0);
        usage.assume_init()
    };
    u128::from(timeval_us(usage.ru_utime)) + u128::from(timeval_us(usage.ru_stime))
}

fn timeval_us(value: libc::timeval) -> u64 {
    u64::try_from(value.tv_sec)
        .unwrap_or(0)
        .saturating_mul(1_000_000)
        .saturating_add(u64::try_from(value.tv_usec).unwrap_or(0))
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
