//! Age-out, in-session trimming, and honest artifact lifetime.

use std::{sync::Arc, time::Duration};

use krometrail_core::{
    CaptureOrdinal, CapturedFrame, DeviceScaleFactor, DiskBudgetBytes, EncodedFrame, FrameId,
    FrameSource, ImageFormat, ObservedTime, PixelDimensions, RecordingSink, RetentionLifecycle,
    RetentionPinRequest, RetentionRange, RetentionStore, SessionId, SessionRange, SessionTime,
    TargetId,
};
use krometrail_store::{
    IndexStoreConfig, RecordingStore, RotationConfig, SegmentStoreConfig, SegmentWriter,
    SqliteIndex,
};
use rusqlite::Connection;
use tempfile::TempDir;
use uuid::Uuid;

fn frame(session: u128, target: u128, id: u128, ordinal: u64, bytes: usize) -> EncodedFrame {
    EncodedFrame::new(
        CapturedFrame::new(
            FrameId::from_uuid(Uuid::from_u128(id)),
            SessionId::from_uuid(Uuid::from_u128(session)),
            TargetId::from_uuid(Uuid::from_u128(target)),
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
        vec![7; bytes],
    )
    .unwrap()
}

fn fixture(directory: &TempDir) -> (Arc<SqliteIndex>, Arc<SegmentWriter>) {
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
            rotation: RotationConfig {
                max_duration: Duration::from_secs(60),
                max_size: 1,
            },
        })
        .unwrap(),
    );
    (index, writer)
}

/// Backdates every retained segment and artifact so an age policy measured in
/// real time can be exercised deterministically. Age-out reads the index's own
/// clock, so moving the stored stamps is the honest way to simulate elapsed time.
fn backdate_all(directory: &TempDir, millis: i64) {
    let connection = Connection::open(directory.path().join("index.sqlite3")).unwrap();
    connection
        .execute(
            "UPDATE segments SET created_unix_ms = created_unix_ms - ?1",
            [millis],
        )
        .unwrap();
    connection
        .execute(
            "UPDATE artifacts SET created_unix_ms = created_unix_ms - ?1",
            [millis],
        )
        .unwrap();
}

fn segment_count(directory: &TempDir) -> u64 {
    Connection::open(directory.path().join("index.sqlite3"))
        .unwrap()
        .query_row("SELECT count(*) FROM segments", [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap() as u64
}

/// Counts published segments only. A live append always leaves one `open`
/// segment behind, so reclaim assertions must look at sealed rows.
fn sealed_segment_count(directory: &TempDir) -> u64 {
    Connection::open(directory.path().join("index.sqlite3"))
        .unwrap()
        .query_row(
            "SELECT count(*) FROM segments WHERE state='sealed'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap() as u64
}

/// Evidence must expire on time, not only on size. Without this a store
/// accumulates until it reaches the budget wall and then sits there forever.
#[tokio::test]
async fn age_out_reclaims_expired_evidence_while_far_inside_budget() {
    let directory = TempDir::new().unwrap();
    let (index, writer) = fixture(&directory);
    let store = RecordingStore::with_retention(
        Arc::clone(&writer),
        Arc::clone(&index),
        RetentionLifecycle::new(
            DiskBudgetBytes::new(64 * 1024 * 1024).unwrap(),
            Some(Duration::from_secs(3_600)),
            85,
            Duration::ZERO,
        )
        .unwrap(),
        None,
    )
    .unwrap();

    let first = frame(1, 10, 100, 1, 4_000);
    store.append_frame(first.clone()).await.unwrap();
    store.flush(first.metadata().session_id()).await.unwrap();
    assert_eq!(segment_count(&directory), 1);

    // Two hours old against a one-hour policy, and nowhere near the budget.
    backdate_all(&directory, 2 * 3_600 * 1_000);
    let status = store.status().await.unwrap();
    assert!(
        status.usage.total_bytes().unwrap() < status.configured_budget.get() / 2,
        "the store must be well inside budget so only age can explain reclaim"
    );

    let second = frame(2, 20, 200, 1, 4_000);
    store.append_frame(second).await.unwrap();

    assert_eq!(
        sealed_segment_count(&directory),
        0,
        "the expired segment should have aged out even though the store fits"
    );
    assert!(
        index
            .frames_by_id(vec![first.metadata().id()])
            .await
            .is_err(),
        "aged-out frames must no longer be readable"
    );
}

/// Pins are load-bearing: pinned evidence must survive age-out exactly as it
/// survives budget pressure, or a pin means nothing once time passes.
#[tokio::test]
async fn pinned_evidence_survives_age_out() {
    let directory = TempDir::new().unwrap();
    let (index, writer) = fixture(&directory);
    let store = RecordingStore::with_retention(
        Arc::clone(&writer),
        Arc::clone(&index),
        RetentionLifecycle::new(
            DiskBudgetBytes::new(64 * 1024 * 1024).unwrap(),
            Some(Duration::from_secs(3_600)),
            85,
            Duration::ZERO,
        )
        .unwrap(),
        None,
    )
    .unwrap();

    let pinned = frame(1, 10, 100, 1, 4_000);
    store.append_frame(pinned.clone()).await.unwrap();
    store.flush(pinned.metadata().session_id()).await.unwrap();
    store
        .pin_resolved_range(
            RetentionPinRequest::new(
                RetentionRange {
                    session_id: SessionId::from_uuid(Uuid::from_u128(1)),
                    target_id: TargetId::from_uuid(Uuid::from_u128(10)),
                    range: SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(2))
                        .unwrap(),
                },
                vec![pinned.metadata().id()],
            )
            .unwrap(),
        )
        .await
        .unwrap();

    backdate_all(&directory, 30 * 24 * 3_600 * 1_000);
    store
        .append_frame(frame(2, 20, 200, 1, 4_000))
        .await
        .unwrap();

    assert!(
        index
            .frames_by_id(vec![pinned.metadata().id()])
            .await
            .is_ok(),
        "a pinned range must outlive the age policy"
    );
}

/// A long session should reclaim as it goes rather than climbing to the wall.
#[tokio::test]
async fn in_session_trimming_reclaims_before_the_budget_wall() {
    let directory = TempDir::new().unwrap();
    let (index, writer) = fixture(&directory);
    let probe = RecordingStore::new(Arc::clone(&writer), Arc::clone(&index)).unwrap();
    let baseline = probe.status().await.unwrap().usage.total_bytes().unwrap();
    drop(probe);

    // High-water at 50% of a budget sized to hold several frames, so trimming is
    // driven by the soft threshold and not by an append that fails to fit.
    let budget = DiskBudgetBytes::new(baseline + 200_000).unwrap();
    let store = RecordingStore::with_retention(
        Arc::clone(&writer),
        Arc::clone(&index),
        RetentionLifecycle::new(budget, None, 50, Duration::ZERO).unwrap(),
        None,
    )
    .unwrap();

    let mut peak = 0_u64;
    for ordinal in 1..=12_u64 {
        let session = u128::from(ordinal);
        let value = frame(session, session + 100, session + 200, 1, 20_000);
        store.append_frame(value.clone()).await.unwrap();
        store.flush(value.metadata().session_id()).await.unwrap();
        peak = peak.max(store.status().await.unwrap().usage.total_bytes().unwrap());
    }

    let status = store.status().await.unwrap();
    assert_eq!(
        status.budget_state,
        krometrail_core::RecordingBudgetState::Available,
        "trimming should keep the store available rather than pausing at the wall"
    );
    assert!(
        peak < budget.get(),
        "a trimming store must never reach the budget wall: peak {peak} budget {}",
        budget.get()
    );
    assert!(
        segment_count(&directory) < 12,
        "trimming should have reclaimed earlier segments as the session ran"
    );
}

/// Retained bounds must be ordered by a key that is valid across sessions.
///
/// Drives the shakedown observation directly: a later-recorded session whose
/// session-relative times are *lower* than an earlier session's. Ordered by
/// insertion order the endpoints inverted, reporting an `oldest_retained` whose
/// session time exceeded `newest_retained`. Ordered by the segment wall clock,
/// the oldest endpoint is the one actually recorded first.
#[tokio::test]
async fn retained_bounds_order_by_wall_clock_not_insertion_order() {
    let directory = TempDir::new().unwrap();
    let (index, writer) = fixture(&directory);
    let store = RecordingStore::new(Arc::clone(&writer), Arc::clone(&index)).unwrap();

    // Inserted first, session clock near zero.
    let first = frame(1, 10, 100, 1, 1_000);
    store.append_frame(first.clone()).await.unwrap();
    store.flush(first.metadata().session_id()).await.unwrap();

    // Inserted second, high session-relative time.
    let second = frame(2, 20, 200, 5_000, 1_000);
    store.append_frame(second.clone()).await.unwrap();
    store.flush(second.metadata().session_id()).await.unwrap();

    // Backdate the *second-inserted* session so wall-clock order and insertion
    // order genuinely disagree. Without this the two orderings coincide and the
    // test would pass under the old `rowid` query too.
    Connection::open(directory.path().join("index.sqlite3"))
        .unwrap()
        .execute(
            "UPDATE segments SET created_unix_ms = created_unix_ms - 60000 \
             WHERE session_id = ?1",
            [uuid::Uuid::from_u128(2).as_bytes().as_slice()],
        )
        .unwrap();

    let status = store.status().await.unwrap();
    let oldest = status.oldest_retained.expect("an oldest endpoint");
    let newest = status.newest_retained.expect("a newest endpoint");

    // Session 2 was inserted second but is wall-clock older, so it is the
    // oldest retained evidence. Insertion order would name session 1 here.
    assert_eq!(
        oldest.session_id,
        SessionId::from_uuid(uuid::Uuid::from_u128(2)),
        "the oldest endpoint must be the wall-clock oldest, not the first inserted"
    );
    assert_eq!(
        newest.session_id,
        SessionId::from_uuid(uuid::Uuid::from_u128(1)),
        "the newest endpoint must be the wall-clock newest, not the last inserted"
    );
    // The endpoints are correctly ordered even though the oldest one's
    // session-relative time is the larger number. That is not a contradiction:
    // session times from different sessions are not comparable, which is why the
    // ordering key must be the shared wall clock.
    assert!(oldest.session_time > newest.session_time);
    assert_ne!(oldest.session_id, newest.session_id);
}

/// Age-out must not reach browser events it has not proven expired: without a
/// bounding expired segment there is nothing establishing the events are old.
#[tokio::test]
async fn age_out_without_an_expired_segment_leaves_events_alone() {
    let directory = TempDir::new().unwrap();
    let (index, writer) = fixture(&directory);
    let store = RecordingStore::with_retention(
        Arc::clone(&writer),
        Arc::clone(&index),
        RetentionLifecycle::new(
            DiskBudgetBytes::new(64 * 1024 * 1024).unwrap(),
            Some(Duration::from_secs(3_600)),
            85,
            Duration::ZERO,
        )
        .unwrap(),
        None,
    )
    .unwrap();

    let value = frame(1, 10, 100, 1, 4_000);
    store.append_frame(value.clone()).await.unwrap();
    store.flush(value.metadata().session_id()).await.unwrap();

    // Nothing is expired, so an enforce pass must not reclaim anything.
    let before = segment_count(&directory);
    store.enforce_budget().await.unwrap();
    assert_eq!(segment_count(&directory), before);
}
