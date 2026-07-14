use std::{sync::Arc, time::Duration};

use krometrail_core::{
    BrowserEvent, BrowserEventBatch, BrowserEventClass, BrowserEventId, BrowserEventOrdinal,
    BrowserEventPayload, BrowserEventSelector, BrowserEventSeverity, BrowserEventSink,
    BrowserEventSource, BrowserEventUnavailableReason, EventPageLimit, ObservedTime,
    RetentionStore, SessionId, SessionRange, SessionTime, TargetId, TargetLifecycle,
    TargetLifecycleEvent, TimelineStore,
};
use krometrail_store::{
    IndexStoreConfig, RecordingStore, RotationConfig, SegmentStoreConfig, SegmentWriter,
    SqliteIndex,
};
use rusqlite::{Connection, params};
use tempfile::TempDir;
use uuid::Uuid;

fn event(id: u128, session: u128, target: u128, ordinal: u64, time: u64) -> BrowserEvent {
    BrowserEvent::new(
        BrowserEventId::from_uuid(Uuid::from_u128(id)),
        SessionId::from_uuid(Uuid::from_u128(session)),
        TargetId::from_uuid(Uuid::from_u128(target)),
        1,
        BrowserEventOrdinal::new(ordinal).unwrap(),
        SessionTime::from_nanos(time),
        None,
        ObservedTime::from_nanos(time + 10),
        BrowserEventSeverity::Info,
        BrowserEventPayload::TargetLifecycle(TargetLifecycleEvent::new(TargetLifecycle::Attached)),
    )
    .unwrap()
}

fn open(directory: &std::path::Path) -> (Arc<SqliteIndex>, Arc<SegmentWriter>, RecordingStore) {
    let segments = directory.join("segments");
    let index = Arc::new(
        SqliteIndex::open(IndexStoreConfig {
            database_path: directory.join("index.sqlite3"),
            segments_directory: segments.clone(),
            busy_timeout: Duration::from_secs(1),
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
    let store = RecordingStore::new(Arc::clone(&writer), Arc::clone(&index)).unwrap();
    (index, writer, store)
}

fn selector(session: u128, target: u128) -> BrowserEventSelector {
    BrowserEventSelector::new(
        SessionId::from_uuid(Uuid::from_u128(session)),
        TargetId::from_uuid(Uuid::from_u128(target)),
        SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(100)).unwrap(),
        Vec::<BrowserEventClass>::new(),
        BrowserEventSeverity::Debug,
    )
    .unwrap()
}

#[tokio::test]
async fn reopen_repairs_dependents_discards_recoverable_corruption_and_is_idempotent() {
    let directory = TempDir::new().unwrap();
    let (index, writer, store) = open(directory.path());
    let valid = event(1, 10, 20, 1, 1);
    let corrupt = event(2, 10, 20, 2, 2);
    let orphan = event(3, 10, 20, 3, 3);
    let mismatched_dependents = event(5, 10, 20, 4, 4);
    let bad_retention_projection = event(6, 10, 20, 5, 5);
    let other = event(4, 11, 21, 1, 1);
    store
        .append_event_batch(
            BrowserEventBatch::new(
                valid.session_id(),
                vec![
                    valid.clone(),
                    corrupt.clone(),
                    orphan.clone(),
                    mismatched_dependents.clone(),
                    bad_retention_projection.clone(),
                ],
            )
            .unwrap(),
        )
        .await
        .unwrap();
    store
        .append_event_batch(
            BrowserEventBatch::new(other.session_id(), vec![other.clone()]).unwrap(),
        )
        .await
        .unwrap();
    drop(store);
    drop(index);
    drop(writer);

    let connection = Connection::open(directory.path().join("index.sqlite3")).unwrap();
    connection
        .execute_batch("PRAGMA ignore_check_constraints=ON;")
        .unwrap();
    connection
        .execute(
            "DELETE FROM timeline_observations WHERE kind='browser_event' AND payload_sort_key=?1",
            params![valid.id().as_uuid().as_bytes().to_vec()],
        )
        .unwrap();
    connection
        .execute(
            "DELETE FROM usage WHERE class='browser_event' AND object_key=?1",
            params![valid.id().as_uuid().as_bytes().to_vec()],
        )
        .unwrap();
    connection
        .execute(
            "UPDATE browser_events SET payload_json='{broken' WHERE event_id=?1",
            params![corrupt.id().as_uuid().as_bytes().to_vec()],
        )
        .unwrap();
    connection
        .execute(
            "DELETE FROM browser_events WHERE event_id=?1",
            params![orphan.id().as_uuid().as_bytes().to_vec()],
        )
        .unwrap();
    connection
        .execute(
            "UPDATE timeline_observations SET payload_json='{broken' \
             WHERE kind='browser_event' AND payload_sort_key=?1",
            params![orphan.id().as_uuid().as_bytes().to_vec()],
        )
        .unwrap();
    connection
        .execute(
            "UPDATE timeline_observations SET observed_time_be=?1 \
             WHERE kind='browser_event' AND payload_sort_key=?2",
            params![
                99_u64.to_be_bytes().to_vec(),
                mismatched_dependents.id().as_uuid().as_bytes().to_vec()
            ],
        )
        .unwrap();
    connection
        .execute(
            "UPDATE usage SET byte_len_be=?1 WHERE class='browser_event' AND object_key=?2",
            params![
                1_u64.to_be_bytes().to_vec(),
                mismatched_dependents.id().as_uuid().as_bytes().to_vec()
            ],
        )
        .unwrap();
    connection
        .execute(
            "UPDATE browser_events SET retention_sequence=-1 WHERE event_id=?1",
            params![bad_retention_projection.id().as_uuid().as_bytes().to_vec()],
        )
        .unwrap();
    drop(connection);

    let (index, writer, store) = open(directory.path());
    assert_eq!(store.count_events(selector(10, 20)).await.unwrap(), 2);
    assert_eq!(
        store
            .chronological_events(selector(10, 20), None, EventPageLimit::new(10).unwrap())
            .await
            .unwrap(),
        [valid.clone(), mismatched_dependents.clone()]
    );
    assert_eq!(store.count_events(selector(11, 21)).await.unwrap(), 1);
    let unavailable = store
        .unavailable_ranges(
            valid.session_id(),
            valid.target_id(),
            SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(10)).unwrap(),
            10,
        )
        .await
        .unwrap();
    assert_eq!(unavailable.len(), 3);
    assert!(
        unavailable
            .iter()
            .all(|range| range.reason() == BrowserEventUnavailableReason::CorruptDiscarded)
    );
    let timeline = store
        .range(
            valid.session_id(),
            valid.target_id(),
            SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(10)).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(timeline.len(), 2);

    let counts_before: (u64, u64, u64) = {
        let connection = Connection::open(directory.path().join("index.sqlite3")).unwrap();
        connection
            .query_row(
                "SELECT (SELECT count(*) FROM browser_events),\
                        (SELECT count(*) FROM browser_event_unavailable_ranges),\
                        (SELECT count(*) FROM usage WHERE class='browser_event')",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap()
    };
    drop(store);
    drop(index);
    drop(writer);
    let (_index, _writer, reopened) = open(directory.path());
    let counts_after: (u64, u64, u64) = {
        let connection = Connection::open(directory.path().join("index.sqlite3")).unwrap();
        connection
            .query_row(
                "SELECT (SELECT count(*) FROM browser_events),\
                        (SELECT count(*) FROM browser_event_unavailable_ranges),\
                        (SELECT count(*) FROM usage WHERE class='browser_event')",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap()
    };
    assert_eq!(counts_after, counts_before);

    reopened.delete_session(valid.session_id()).await.unwrap();
    assert_eq!(reopened.count_events(selector(10, 20)).await.unwrap(), 0);
    assert_eq!(reopened.count_events(selector(11, 21)).await.unwrap(), 1);
    assert!(
        reopened
            .unavailable_ranges(
                valid.session_id(),
                valid.target_id(),
                SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(10)).unwrap(),
                10,
            )
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        reopened
            .append_event_batch(
                BrowserEventBatch::new(valid.session_id(), vec![valid.clone()]).unwrap()
            )
            .await
            .unwrap_err()
            .code,
        krometrail_core::ErrorCode::NotFound
    );
    let deleted_counts: (u64, u64, u64, u64) = {
        let connection = Connection::open(directory.path().join("index.sqlite3")).unwrap();
        connection
            .query_row(
                "SELECT (SELECT count(*) FROM browser_events WHERE session_id=?1),\
                        (SELECT count(*) FROM browser_event_unavailable_ranges WHERE session_id=?1),\
                        (SELECT count(*) FROM timeline_observations WHERE session_id=?1),\
                        (SELECT count(*) FROM usage WHERE session_id=?1)",
                params![valid.session_id().as_uuid().as_bytes().to_vec()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap()
    };
    assert_eq!(deleted_counts, (0, 0, 0, 0));
    drop(reopened);
}

#[tokio::test]
async fn reopen_fails_source_safely_on_unbounded_identity_or_time_corruption() {
    let directory = TempDir::new().unwrap();
    let (index, writer, store) = open(directory.path());
    let item = event(30, 31, 32, 1, 1);
    store
        .append_event_batch(BrowserEventBatch::new(item.session_id(), vec![item]).unwrap())
        .await
        .unwrap();
    drop(store);
    drop(index);
    drop(writer);

    let connection = Connection::open(directory.path().join("index.sqlite3")).unwrap();
    connection
        .execute_batch("PRAGMA ignore_check_constraints=ON;")
        .unwrap();
    connection
        .execute("UPDATE browser_events SET session_time_be=x'01'", [])
        .unwrap();
    drop(connection);

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
            rotation: RotationConfig::suggested(),
        })
        .unwrap(),
    );
    let error = RecordingStore::new(writer, index).err().unwrap();
    assert_eq!(error.code, krometrail_core::ErrorCode::PersistenceFailed);
    assert!(!error.message.as_str().contains("01"));
    assert!(!error.message.as_str().contains("browser_events"));
}
