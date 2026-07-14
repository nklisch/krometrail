use std::{sync::Arc, time::Duration};

use krometrail_core::{
    BrowserEvent, BrowserEventBatch, BrowserEventClass, BrowserEventCursor, BrowserEventId,
    BrowserEventOrdinal, BrowserEventPayload, BrowserEventSelector, BrowserEventSeverity,
    BrowserEventSink, BrowserEventSource, ConsoleArgumentType, ConsoleEvent, ConsoleEventSource,
    ConsoleLevel, ConsoleMethod, EventCandidateLimit, EventPageLimit, EventRedactor,
    NetworkFailureKind, NetworkRequestFailed, NetworkRequestId, ObservationKind, ObservedTime,
    RetentionStore, SessionId, SessionRange, SessionTime, TargetId, TargetLifecycle,
    TargetLifecycleEvent, TimelineStore,
};
use krometrail_store::{
    IndexStoreConfig, RecordingStore, RotationConfig, SegmentStoreConfig, SegmentWriter,
    SqliteIndex,
};
use rusqlite::Connection;
use tempfile::TempDir;
use uuid::Uuid;

struct Fixture {
    _directory: TempDir,
    database: std::path::PathBuf,
    store: RecordingStore,
}

fn fixture() -> Fixture {
    let directory = TempDir::new().unwrap();
    let database = directory.path().join("index.sqlite3");
    let segments = directory.path().join("segments");
    let index = Arc::new(
        SqliteIndex::open(IndexStoreConfig {
            database_path: database.clone(),
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
    let store = RecordingStore::new(writer, Arc::clone(&index)).unwrap();
    Fixture {
        _directory: directory,
        database,
        store,
    }
}

fn target_event(id: u128, session: u128, target: u128, ordinal: u64, time: u64) -> BrowserEvent {
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

fn console_event(
    id: u128,
    session: u128,
    target: u128,
    ordinal: u64,
    time: u64,
    level: ConsoleLevel,
    text: &str,
) -> BrowserEvent {
    let severity = match level {
        ConsoleLevel::Debug => BrowserEventSeverity::Debug,
        ConsoleLevel::Info => BrowserEventSeverity::Info,
        ConsoleLevel::Warning => BrowserEventSeverity::Warning,
        ConsoleLevel::Error => BrowserEventSeverity::Error,
    };
    BrowserEvent::new(
        BrowserEventId::from_uuid(Uuid::from_u128(id)),
        SessionId::from_uuid(Uuid::from_u128(session)),
        TargetId::from_uuid(Uuid::from_u128(target)),
        1,
        BrowserEventOrdinal::new(ordinal).unwrap(),
        SessionTime::from_nanos(time),
        None,
        ObservedTime::from_nanos(time + 10),
        severity,
        BrowserEventPayload::ConsoleMessage(ConsoleEvent::new(
            ConsoleEventSource::Runtime,
            level,
            ConsoleMethod::Log,
            vec![ConsoleArgumentType::String],
            Some(EventRedactor.text(text)),
            vec![],
        )),
    )
    .unwrap()
}

fn failed_request(id: u128, session: u128, target: u128, ordinal: u64, time: u64) -> BrowserEvent {
    BrowserEvent::new(
        BrowserEventId::from_uuid(Uuid::from_u128(id)),
        SessionId::from_uuid(Uuid::from_u128(session)),
        TargetId::from_uuid(Uuid::from_u128(target)),
        1,
        BrowserEventOrdinal::new(ordinal).unwrap(),
        SessionTime::from_nanos(time),
        None,
        ObservedTime::from_nanos(time + 10),
        BrowserEventSeverity::Error,
        BrowserEventPayload::NetworkRequestFailed(
            NetworkRequestFailed::new(
                NetworkRequestId::from_uuid(Uuid::from_u128(id + 10_000)),
                None,
                None,
                None,
                NetworkFailureKind::Connection,
            )
            .unwrap(),
        ),
    )
    .unwrap()
}

fn selector(
    session: u128,
    target: u128,
    classes: Vec<BrowserEventClass>,
    severity: BrowserEventSeverity,
) -> BrowserEventSelector {
    BrowserEventSelector::new(
        SessionId::from_uuid(Uuid::from_u128(session)),
        TargetId::from_uuid(Uuid::from_u128(target)),
        SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(100)).unwrap(),
        classes,
        severity,
    )
    .unwrap()
}

#[tokio::test]
async fn batch_append_is_atomic_idempotent_and_owns_timeline_usage() {
    let fixture = fixture();
    let first = target_event(1, 10, 20, 1, 5);
    let second = console_event(2, 10, 20, 2, 5, ConsoleLevel::Info, "hello");
    let batch = BrowserEventBatch::new(10_u128.into_session(), vec![first.clone(), second.clone()])
        .unwrap();
    fixture
        .store
        .append_event_batch(batch.clone())
        .await
        .unwrap();
    fixture.store.append_event_batch(batch).await.unwrap();

    let selected = selector(10, 20, vec![], BrowserEventSeverity::Debug);
    assert_eq!(
        fixture.store.count_events(selected.clone()).await.unwrap(),
        2
    );
    let events = fixture
        .store
        .chronological_events(selected, None, EventPageLimit::new(10).unwrap())
        .await
        .unwrap();
    assert_eq!(events, [first.clone(), second.clone()]);
    let timeline = fixture
        .store
        .range(
            first.session_id(),
            first.target_id(),
            SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(10)).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        timeline
            .iter()
            .filter(|row| row.kind() == ObservationKind::BrowserEvent)
            .count(),
        2
    );
    let status = fixture.store.status().await.unwrap();
    assert!(status.usage.browser_event_bytes > 0);
    let accounting = Connection::open(&fixture.database).unwrap();
    let page_count: u64 = accounting
        .pragma_query_value(None, "page_count", |row| row.get(0))
        .unwrap();
    let page_size: u64 = accounting
        .pragma_query_value(None, "page_size", |row| row.get(0))
        .unwrap();
    let freelist_count: u64 = accounting
        .pragma_query_value(None, "freelist_count", |row| row.get(0))
        .unwrap();
    assert_eq!(
        status.usage.index_bytes + status.usage.browser_event_bytes,
        (page_count - freelist_count) * page_size
    );
    assert_eq!(
        status.usage.accounting_slack_bytes,
        freelist_count * page_size
    );
    drop(accounting);

    let new_before_conflict = target_event(30, 10, 20, 3, 6);
    let conflicting_late = target_event(1, 10, 20, 4, 7);
    assert!(
        fixture
            .store
            .append_event_batch(
                BrowserEventBatch::new(
                    first.session_id(),
                    vec![new_before_conflict, conflicting_late],
                )
                .unwrap(),
            )
            .await
            .is_err()
    );
    assert_eq!(
        fixture
            .store
            .count_events(selector(10, 20, vec![], BrowserEventSeverity::Debug))
            .await
            .unwrap(),
        2
    );

    let conflicting_id = target_event(1, 10, 20, 3, 6);
    let error = fixture
        .store
        .append_event_batch(
            BrowserEventBatch::new(first.session_id(), vec![conflicting_id]).unwrap(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, krometrail_core::ErrorCode::PersistenceFailed);
    assert!(!error.message.as_str().contains("hello"));

    let conflicting_ordinal = target_event(3, 10, 20, 2, 6);
    assert!(
        fixture
            .store
            .append_event_batch(
                BrowserEventBatch::new(first.session_id(), vec![conflicting_ordinal]).unwrap()
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn semantic_reads_preserve_ties_filters_priority_nearest_and_cursor_scope() {
    let fixture = fixture();
    let events = vec![
        target_event(11, 100, 200, 1, 10),
        console_event(12, 100, 200, 2, 10, ConsoleLevel::Warning, "warning"),
        failed_request(13, 100, 200, 3, 11),
        console_event(14, 100, 200, 4, 20, ConsoleLevel::Debug, "debug"),
    ];
    fixture
        .store
        .append_event_batch(BrowserEventBatch::new(events[0].session_id(), events.clone()).unwrap())
        .await
        .unwrap();

    let all = selector(100, 200, vec![], BrowserEventSeverity::Debug);
    let first_page = fixture
        .store
        .chronological_events(all.clone(), None, EventPageLimit::new(2).unwrap())
        .await
        .unwrap();
    assert_eq!(first_page, events[..2]);
    let last = first_page.last().unwrap();
    let cursor =
        BrowserEventCursor::new(all.clone(), last.session_time(), last.ordinal(), last.id())
            .unwrap();
    let second_page = fixture
        .store
        .chronological_events(all.clone(), Some(cursor), EventPageLimit::new(10).unwrap())
        .await
        .unwrap();
    assert_eq!(second_page, events[2..]);

    let warning_console = selector(
        100,
        200,
        vec![BrowserEventClass::Console],
        BrowserEventSeverity::Warning,
    );
    assert_eq!(
        fixture
            .store
            .chronological_events(warning_console, None, EventPageLimit::new(10).unwrap())
            .await
            .unwrap(),
        [events[1].clone()]
    );
    assert_eq!(
        fixture
            .store
            .priority_candidates(all.clone(), EventCandidateLimit::new(2).unwrap())
            .await
            .unwrap()[0],
        events[2]
    );
    assert_eq!(
        fixture
            .store
            .nearest_candidates(all.clone(), vec![SessionTime::from_nanos(10)], 1)
            .await
            .unwrap(),
        [events[0].clone(), events[1].clone()]
    );

    let other_filter = selector(
        100,
        200,
        vec![BrowserEventClass::Network],
        BrowserEventSeverity::Debug,
    );
    let mismatched = BrowserEventCursor::new(
        other_filter,
        events[2].session_time(),
        events[2].ordinal(),
        events[2].id(),
    )
    .unwrap();
    assert_eq!(
        fixture
            .store
            .chronological_events(all, Some(mismatched), EventPageLimit::new(2).unwrap())
            .await
            .unwrap_err()
            .code,
        krometrail_core::ErrorCode::InvalidInput
    );
}

#[tokio::test]
async fn stored_payload_and_corruption_errors_remain_source_safe() {
    let fixture = fixture();
    let event = console_event(
        21,
        210,
        220,
        1,
        10,
        ConsoleLevel::Error,
        "token=private-token /home/private/project",
    );
    fixture
        .store
        .append_event_batch(
            BrowserEventBatch::new(event.session_id(), vec![event.clone()]).unwrap(),
        )
        .await
        .unwrap();
    let connection = Connection::open(&fixture.database).unwrap();
    let payload: String = connection
        .query_row("SELECT payload_json FROM browser_events", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert!(!payload.contains("private-token"));
    assert!(!payload.contains("/home/private/project"));
    connection
        .execute("UPDATE browser_events SET kind='unknown_event'", [])
        .unwrap();
    drop(connection);

    let error = fixture
        .store
        .chronological_events(
            selector(210, 220, vec![], BrowserEventSeverity::Debug),
            None,
            EventPageLimit::new(10).unwrap(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, krometrail_core::ErrorCode::PersistenceFailed);
    assert!(!error.message.as_str().contains("unknown_event"));
    assert!(!error.message.as_str().contains("private"));
}

trait SessionFromU128 {
    fn into_session(self) -> SessionId;
}

impl SessionFromU128 for u128 {
    fn into_session(self) -> SessionId {
        SessionId::from_uuid(Uuid::from_u128(self))
    }
}
