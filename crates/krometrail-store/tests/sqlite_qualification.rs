use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::{Duration, Instant},
};

use krometrail_core::{
    CaptureGap, CaptureGapReason, CaptureGapStore, ErrorCode, GapId, MarkerId, ObservationKind,
    ObservationPayloadRef, ObservedTime, SessionId, SessionRange, SessionTime, TargetId,
    TimelineObservation, TimelineStore,
};
use krometrail_store::{IndexStoreConfig, SqliteIndex};
use rusqlite::Connection;
use tempfile::TempDir;
use uuid::Uuid;

fn open(directory: &TempDir, timeout: Duration) -> (std::path::PathBuf, SqliteIndex) {
    let path = directory.path().join("index.sqlite3");
    let index = SqliteIndex::open(IndexStoreConfig {
        database_path: path.clone(),
        segments_directory: directory.path().join("segments"),
        busy_timeout: timeout,
    })
    .unwrap();
    (path, index)
}

fn observation(id: u128) -> TimelineObservation {
    TimelineObservation::new(
        SessionId::from_uuid(Uuid::from_u128(1)),
        TargetId::from_uuid(Uuid::from_u128(2)),
        SessionTime::from_nanos(1),
        None,
        ObservedTime::from_nanos(2),
        ObservationKind::Marker,
        ObservationPayloadRef::Marker(MarkerId::from_uuid(Uuid::from_u128(id))),
    )
    .unwrap()
}

#[tokio::test]
async fn external_writer_contention_stops_at_the_configured_busy_timeout() {
    let directory = TempDir::new().unwrap();
    let timeout = Duration::from_millis(40);
    let (path, index) = open(&directory, timeout);
    let blocker = Connection::open(path).unwrap();
    blocker.execute_batch("BEGIN IMMEDIATE").unwrap();

    let started = Instant::now();
    let error = index.append(observation(3)).await.unwrap_err();
    let elapsed = started.elapsed();
    assert_eq!(error.code, ErrorCode::PersistenceFailed);
    assert!(
        elapsed >= Duration::from_millis(20),
        "busy timeout returned too early: {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(2),
        "busy timeout was not bounded: {elapsed:?}"
    );
    blocker.execute_batch("ROLLBACK").unwrap();
}

#[test]
fn unpolled_operations_do_nothing_and_polled_transactions_finish_or_roll_back() {
    let directory = TempDir::new().unwrap();
    let (path, index) = open(&directory, Duration::from_secs(1));
    let cancelled = index.append(observation(4));
    drop(cancelled);
    let connection = Connection::open(&path).unwrap();
    let count: u32 = connection
        .query_row("SELECT count(*) FROM timeline_observations", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(count, 0);
    drop(connection);

    let mut context = Context::from_waker(std::task::Waker::noop());
    let mut completed = index.append(observation(5));
    assert!(matches!(
        Pin::new(&mut completed).poll(&mut context),
        Poll::Ready(Ok(()))
    ));

    let gap = CaptureGap::new(
        GapId::from_uuid(Uuid::from_u128(6)),
        SessionId::from_uuid(Uuid::from_u128(1)),
        TargetId::from_uuid(Uuid::from_u128(2)),
        SessionRange::new(SessionTime::from_nanos(1), SessionTime::from_nanos(1)).unwrap(),
        ObservedTime::from_nanos(2),
        CaptureGapReason::CaptureStopped,
        None,
        None,
    )
    .unwrap();
    let mut first = index.append_gap(gap.clone());
    assert!(matches!(
        Pin::new(&mut first).poll(&mut context),
        Poll::Ready(Ok(()))
    ));
    let mut duplicate = index.append_gap(gap);
    assert!(matches!(
        Pin::new(&mut duplicate).poll(&mut context),
        Poll::Ready(Err(_))
    ));

    let connection = Connection::open(path).unwrap();
    let counts: (u32, u32) = connection
        .query_row(
            "SELECT (SELECT count(*) FROM capture_gaps), \
                    (SELECT count(*) FROM timeline_observations WHERE kind='capture_gap')",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(counts, (1, 1));
}
