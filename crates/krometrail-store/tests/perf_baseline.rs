//! Explicit wall-clock probes for the store-ingestion performance feature.
//!
//! These are intentionally ignored: they are directional measurements, not a
//! CI gate. Run with:
//!
//! ```text
//! cargo test -p krometrail-store --release --test perf_baseline -- --ignored --nocapture
//! ```

use std::{
    fs,
    sync::{Arc, Barrier},
    time::{Duration, Instant},
};

use krometrail_core::{
    CaptureOrdinal, CapturedFrame, DeviceScaleFactor, DiskBudgetBytes, EncodedFrame, FrameId,
    FrameSource, ImageFormat, ObservationKind, ObservedTime, PixelDimensions, RecordingSink,
    RetentionStore, SessionId, SessionRange, SessionTime, TargetId, TimelineStore,
};
use krometrail_store::{
    IndexStoreConfig, RecordingStore, RotationConfig, SegmentStoreConfig, SegmentWriter,
    SqliteIndex, recover,
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
    open_store(directory, rotation, None)
}

fn open_store(
    directory: &TempDir,
    rotation: RotationConfig,
    budget: Option<DiskBudgetBytes>,
) -> Arc<RecordingStore> {
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
    let store = match budget {
        Some(budget) => RecordingStore::with_budget(writer, index, budget, test_clock()),
        None => RecordingStore::new(writer, index, test_clock()),
    }
    .unwrap();
    Arc::new(store)
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
    let status = store.status().await.unwrap();
    let budget = DiskBudgetBytes::new(
        status
            .usage
            .total_bytes()
            .unwrap()
            .saturating_sub(status.usage.segment_bytes / 2),
    )
    .unwrap();
    drop(store);

    let segments = directory.path().join("segments");
    let index = Arc::new(
        SqliteIndex::open(IndexStoreConfig {
            database_path: directory.path().join("index.sqlite3"),
            segments_directory: segments.clone(),
            busy_timeout: Duration::from_secs(1),
        })
        .unwrap(),
    );
    recover(index.as_ref()).unwrap();
    let writer = Arc::new(
        SegmentWriter::open(SegmentStoreConfig {
            directory: segments,
            rotation,
        })
        .unwrap(),
    );
    let store = RecordingStore::with_budget(writer, index, budget, test_clock()).unwrap();
    let start = Instant::now();
    let _ = store.enforce_budget().await.unwrap();
    println!("evict_segment_ms: elapsed={:?}", start.elapsed());
}

#[tokio::test]
#[ignore = "directional wall-clock probe"]
async fn read_one_frame_under_ingest() {
    let directory = TempDir::new().unwrap();
    let store = store(&directory, RotationConfig::suggested());
    store.append_frame(frame(1, 256)).await.unwrap();
    let started = Arc::new(Barrier::new(2));
    let ingestion_store = Arc::clone(&store);
    let ingestion_started = Arc::clone(&started);
    let ingestion = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async move {
            ingestion_started.wait();
            for ordinal in 2..=513 {
                ingestion_store
                    .append_frame(frame(ordinal, 256))
                    .await
                    .unwrap();
            }
        });
    });
    started.wait();
    let mut samples = Vec::with_capacity(128);
    for _ in 0..128 {
        let start = Instant::now();
        store
            .frames_by_id(vec![FrameId::from_uuid(Uuid::from_u128(101))])
            .await
            .unwrap();
        samples.push(start.elapsed());
    }
    ingestion.join().unwrap();
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

fn copy_regular_files(source: &std::path::Path, destination: &std::path::Path) {
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_file() {
            fs::copy(entry.path(), destination.join(entry.file_name())).unwrap();
        }
    }
}

fn wal_pages(database: &std::path::Path, page_size: u64) -> u64 {
    let wal = database.with_file_name(format!(
        "{}-wal",
        database.file_name().unwrap().to_string_lossy()
    ));
    let Ok(length) = fs::metadata(wal).map(|metadata| metadata.len()) else {
        return 0;
    };
    let frame_bytes = page_size + 24;
    length.saturating_sub(32) / frame_bytes
}

#[tokio::test]
async fn recovery_rebuilds_a_lost_index_tail_from_segments() {
    let source = TempDir::new().unwrap();
    let store = store(&source, RotationConfig::suggested());
    let frames: Vec<_> = (1..=8).map(|ordinal| frame(ordinal, 256)).collect();
    for value in &frames[..2] {
        store.append_frame(value.clone()).await.unwrap();
    }
    store.flush(SESSION).await.unwrap();
    for value in &frames[2..] {
        store.append_frame(value.clone()).await.unwrap();
    }

    // The first two frames and their usage are checkpointed into the main DB.
    // Copying only that DB file while the later index writes remain in the WAL
    // simulates a lost power-loss tail; the segment files contain all records.
    let recovered = TempDir::new().unwrap();
    fs::copy(
        source.path().join("index.sqlite3"),
        recovered.path().join("index.sqlite3"),
    )
    .unwrap();
    copy_regular_files(
        &source.path().join("segments"),
        &recovered.path().join("segments"),
    );
    drop(store);

    let index = Arc::new(
        SqliteIndex::open(IndexStoreConfig {
            database_path: recovered.path().join("index.sqlite3"),
            segments_directory: recovered.path().join("segments"),
            busy_timeout: Duration::from_secs(1),
        })
        .unwrap(),
    );
    let report = recover(index.as_ref()).unwrap();
    assert!(report.frames_recovered >= 6);
    let ids = frames.iter().map(|value| value.metadata().id()).collect();
    assert_eq!(index.frames_by_id(ids).await.unwrap().len(), frames.len());

    let observations = TimelineStore::range(
        index.as_ref(),
        SESSION,
        TARGET,
        SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(20)).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(
        observations
            .iter()
            .filter(|observation| observation.kind() == ObservationKind::Frame)
            .count(),
        frames.len()
    );

    let connection = Connection::open(recovered.path().join("index.sqlite3")).unwrap();
    let (segment_rows, usage_rows): (i64, i64) = connection
        .query_row(
            "SELECT (SELECT count(*) FROM segments), \
                    (SELECT count(*) FROM usage WHERE class='segment')",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(usage_rows, segment_rows);
    assert!(usage_rows > 0);
}

#[tokio::test]
async fn sustained_mutations_keep_the_wal_within_the_checkpoint_policy() {
    const POLICY_PAGE_LIMIT: u64 = 2_000;

    let directory = TempDir::new().unwrap();
    let database = directory.path().join("index.sqlite3");
    let store = store(&directory, RotationConfig::suggested());
    let page_size: u64 = Connection::open(&database)
        .unwrap()
        .pragma_query_value(None, "page_size", |row| row.get(0))
        .unwrap();
    let mut maximum = 0_u64;
    for ordinal in 1..=4_500 {
        store.append_frame(frame(ordinal, 256)).await.unwrap();
        maximum = maximum.max(wal_pages(&database, page_size));
    }
    let final_pages = wal_pages(&database, page_size);
    assert!(
        maximum <= POLICY_PAGE_LIMIT + 64,
        "WAL grew beyond the checkpoint policy: maximum {maximum} pages"
    );
    assert!(
        final_pages <= POLICY_PAGE_LIMIT,
        "WAL tail exceeded the checkpoint policy: final {final_pages} pages"
    );
}
