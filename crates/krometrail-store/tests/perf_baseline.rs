//! Explicit wall-clock probes for the store-ingestion performance feature.
//!
//! These are intentionally ignored: they are directional measurements, not a
//! CI gate. Run with:
//!
//! ```text
//! cargo test -p krometrail-store --release --test perf_baseline -- --ignored --nocapture
//! ```

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use krometrail_core::{
    CaptureOrdinal, CapturedFrame, DeviceScaleFactor, EncodedFrame, FrameId, FrameSource,
    ImageFormat, ObservedTime, PixelDimensions, RecordingSink, RetentionStore, SessionId,
    SessionTime, TargetId,
};
use krometrail_store::{
    IndexStoreConfig, RecordingStore, RotationConfig, SegmentStoreConfig, SegmentWriter,
    SqliteIndex,
};
use rusqlite::Connection;
use tempfile::TempDir;
use uuid::Uuid;

const SESSION: SessionId = SessionId::from_uuid(Uuid::from_u128(1));
const TARGET: TargetId = TargetId::from_uuid(Uuid::from_u128(2));

fn frame(ordinal: u64, payload_bytes: usize) -> EncodedFrame {
    EncodedFrame::new(
        CapturedFrame::new(
            FrameId::from_uuid(Uuid::from_u128(u128::from(ordinal) + 100)),
            SESSION,
            TARGET,
            CaptureOrdinal::new(ordinal).unwrap(),
            None,
            ObservedTime::from_nanos(ordinal),
            SessionTime::from_nanos(ordinal),
            ImageFormat::Jpeg,
            PixelDimensions::new(1, 1).unwrap(),
            PixelDimensions::new(1, 1).unwrap(),
            DeviceScaleFactor::new(1.0).unwrap(),
            vec![],
        )
        .unwrap(),
        vec![7; payload_bytes],
    )
    .unwrap()
}

fn store(directory: &TempDir, rotation: RotationConfig) -> Arc<RecordingStore> {
    let segments = directory.path().join("segments");
    let index = Arc::new(
        SqliteIndex::open(IndexStoreConfig {
            database_path: directory.path().join("index.sqlite3"),
            segments_directory: segments.clone(),
            busy_timeout: Duration::from_secs(1),
        })
        .unwrap(),
    );
    let writer = Arc::new(
        SegmentWriter::open(SegmentStoreConfig {
            directory: segments,
            rotation,
        })
        .unwrap(),
    );
    Arc::new(RecordingStore::new(writer, index, test_clock()).unwrap())
}

fn test_clock() -> Arc<dyn krometrail_core::MonotonicClock> {
    struct Clock;
    impl krometrail_core::MonotonicClock for Clock {
        fn now(&self) -> ObservedTime {
            ObservedTime::from_nanos(0)
        }
    }
    Arc::new(Clock)
}

fn summarize(label: &str, samples: &mut [Duration]) {
    samples.sort_unstable();
    let total: Duration = samples.iter().copied().sum();
    let mean = total / u32::try_from(samples.len()).unwrap();
    let p99 = samples[(samples.len() * 99).div_ceil(100).saturating_sub(1)];
    println!(
        "{label}: mean={mean:?} p99={p99:?} samples={}",
        samples.len()
    );
}

#[tokio::test]
#[ignore = "directional wall-clock probe"]
async fn append_flat_vs_size() {
    for retained in [1_000_u64, 5_000, 20_000] {
        let directory = TempDir::new().unwrap();
        let store = store(&directory, RotationConfig::suggested());
        for ordinal in 1..=retained {
            store.append_frame(frame(ordinal, 256)).await.unwrap();
        }
        let mut samples = Vec::with_capacity(32);
        for ordinal in retained + 1..=retained + 32 {
            let start = Instant::now();
            store.append_frame(frame(ordinal, 256)).await.unwrap();
            samples.push(start.elapsed());
        }
        summarize(
            &format!("append_flat_vs_size retained={retained}"),
            &mut samples,
        );
    }
}

#[tokio::test]
#[ignore = "directional wall-clock probe"]
async fn append_btrfs_steady() {
    let directory = TempDir::new().unwrap();
    let store = store(&directory, RotationConfig::suggested());
    for ordinal in 1..=5_000 {
        store.append_frame(frame(ordinal, 20 * 1024)).await.unwrap();
    }
    let mut samples = Vec::with_capacity(128);
    for ordinal in 5_001..=5_128 {
        let start = Instant::now();
        store.append_frame(frame(ordinal, 20 * 1024)).await.unwrap();
        samples.push(start.elapsed());
    }
    summarize("append_btrfs_steady", &mut samples);
}

#[tokio::test]
#[ignore = "directional wall-clock probe"]
async fn evict_segment_ms() {
    let directory = TempDir::new().unwrap();
    let rotation = RotationConfig {
        max_duration: Duration::from_secs(60),
        max_size: 190 * 512,
    };
    let store = store(&directory, rotation);
    for ordinal in 1..=380 {
        store.append_frame(frame(ordinal, 256)).await.unwrap();
    }
    store.flush(SESSION).await.unwrap();
    let start = Instant::now();
    let _ = store.enforce_budget().await.unwrap();
    println!("evict_segment_ms: elapsed={:?}", start.elapsed());
}

#[tokio::test]
#[ignore = "directional wall-clock probe"]
async fn read_one_frame_under_ingest() {
    let directory = TempDir::new().unwrap();
    let store = store(&directory, RotationConfig::suggested());
    let mut samples = Vec::with_capacity(128);
    for ordinal in 1..=512 {
        store.append_frame(frame(ordinal, 256)).await.unwrap();
        let start = Instant::now();
        store
            .frames_by_id(vec![FrameId::from_uuid(Uuid::from_u128(
                u128::from(ordinal) + 100,
            ))])
            .await
            .unwrap();
        samples.push(start.elapsed());
    }
    summarize("read_one_frame_under_ingest", &mut samples);
}

#[test]
#[ignore = "directional query-plan probe"]
fn retained_bounds_plan() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("index.sqlite3");
    let segments = directory.path().join("segments");
    drop(
        SqliteIndex::open(IndexStoreConfig {
            database_path: path.clone(),
            segments_directory: segments,
            busy_timeout: Duration::from_secs(1),
        })
        .unwrap(),
    );
    let connection = Connection::open(path).unwrap();
    for sql in [
        "EXPLAIN QUERY PLAN SELECT segment_id, session_id, target_id FROM segments WHERE created_unix_ms = (SELECT min(created_unix_ms) FROM segments)",
        "EXPLAIN QUERY PLAN SELECT frame_id, session_time_be FROM frames WHERE session_id=X'00000000000000000000000000000000' AND target_id=X'00000000000000000000000000000000' AND segment_id=X'00000000000000000000000000000000' ORDER BY session_time_be ASC, capture_ordinal_be ASC LIMIT 1",
        "EXPLAIN QUERY PLAN SELECT segment_id, session_id, target_id FROM segments WHERE created_unix_ms = (SELECT max(created_unix_ms) FROM segments)",
        "EXPLAIN QUERY PLAN SELECT frame_id, session_time_be FROM frames WHERE session_id=X'00000000000000000000000000000000' AND target_id=X'00000000000000000000000000000000' AND segment_id=X'00000000000000000000000000000000' ORDER BY session_time_be DESC, capture_ordinal_be DESC LIMIT 1",
        "EXPLAIN QUERY PLAN SELECT f.frame_id FROM frames f JOIN segments s USING(segment_id) WHERE f.session_id=X'00000000000000000000000000000000' AND f.target_id=X'00000000000000000000000000000000' AND f.session_time_be>=X'0000000000000000' AND f.session_time_be<=X'FFFFFFFFFFFFFFFF' ORDER BY f.session_time_be ASC, f.capture_ordinal_be ASC",
        "EXPLAIN QUERY PLAN SELECT session_time_be FROM frames WHERE session_id=X'00000000000000000000000000000000' AND target_id=X'00000000000000000000000000000000' ORDER BY session_time_be ASC LIMIT 1",
        "EXPLAIN QUERY PLAN SELECT session_time_be FROM frames WHERE session_id=X'00000000000000000000000000000000' AND target_id=X'00000000000000000000000000000000' ORDER BY session_time_be DESC LIMIT 1",
        "EXPLAIN QUERY PLAN DELETE FROM timeline_observations WHERE kind='frame' AND payload_sort_key IN (X'00000000000000000000000000000000')",
    ] {
        let plan = connection
            .prepare(sql)
            .unwrap()
            .query_map([], |row| row.get::<_, String>(3))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        println!("retained_bounds_plan: {plan:?}");
    }
}

#[tokio::test]
#[ignore = "directional wall-clock probe"]
async fn frame_availability_ms() {
    let directory = TempDir::new().unwrap();
    let store = store(&directory, RotationConfig::suggested());
    for ordinal in 1..=5_000 {
        store.append_frame(frame(ordinal, 256)).await.unwrap();
    }
    let mut samples = Vec::with_capacity(128);
    for _ in 0..128 {
        let start = Instant::now();
        store.frame_availability(SESSION, TARGET).await.unwrap();
        samples.push(start.elapsed());
    }
    summarize("frame_availability_ms", &mut samples);
}
