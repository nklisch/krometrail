use std::{
    num::NonZeroU64,
    time::{Duration, SystemTime},
};

use krometrail_core::{
    BrowserProduct, BrowserProductVersion, BrowserVersion, CapabilityId, CaptureGap,
    CaptureGapReason, CaptureGapStore, DiskBudgetBytes, GapId, InteractionId, MarkerId,
    ObservationKind, ObservationPayloadRef, ObservedTime, PageTarget, ProfileIdentity, ProfileRef,
    RecordingCatalog, RecordingSession, SessionId, SessionRange, SessionTime, TargetId,
    TimelineObservation, TimelineStore,
};
use krometrail_store::{IndexStoreConfig, SqliteIndex};
use rusqlite::{Connection, params};
use tempfile::TempDir;
use uuid::Uuid;

struct Fixture {
    _directory: TempDir,
    path: std::path::PathBuf,
    index: SqliteIndex,
    session: SessionId,
    target: TargetId,
}

impl Fixture {
    fn new() -> Self {
        let directory = TempDir::new().unwrap();
        let path = directory.path().join("index.sqlite3");
        let index = SqliteIndex::open(IndexStoreConfig {
            database_path: path.clone(),
            segments_directory: directory.path().join("segments"),
            busy_timeout: Duration::from_secs(1),
        })
        .unwrap();
        Self {
            _directory: directory,
            path,
            index,
            session: SessionId::from_uuid(Uuid::from_u128(1)),
            target: TargetId::from_uuid(Uuid::from_u128(2)),
        }
    }

    fn observation(
        &self,
        at: u64,
        observed: u64,
        kind: ObservationKind,
        payload: ObservationPayloadRef,
    ) -> TimelineObservation {
        TimelineObservation::new(
            self.session,
            self.target,
            SessionTime::from_nanos(at),
            None,
            ObservedTime::from_nanos(observed),
            kind,
            payload,
        )
        .unwrap()
    }
}

#[tokio::test]
async fn placeholders_are_completed_by_lossless_catalog_upserts() {
    let fixture = Fixture::new();
    let marker = fixture.observation(
        1,
        2,
        ObservationKind::Marker,
        ObservationPayloadRef::Marker(MarkerId::from_uuid(Uuid::from_u128(3))),
    );
    fixture.index.append(marker).await.unwrap();
    let connection = Connection::open(&fixture.path).unwrap();
    let records: (Option<String>, Option<String>) = connection
        .query_row(
            "SELECT s.record_json, t.record_json FROM sessions s JOIN targets t USING(session_id)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(records, (None, None));
    drop(connection);

    let session = RecordingSession::new(
        fixture.session,
        ObservedTime::from_nanos(0),
        SystemTime::UNIX_EPOCH,
        BrowserVersion::new(
            BrowserProduct::Chrome,
            BrowserProductVersion::new("128").unwrap(),
            "revision",
            "1.3",
            "Chrome/128",
            "12",
        )
        .unwrap(),
        ProfileRef::managed(ProfileIdentity::new("profile").unwrap()),
        DiskBudgetBytes::new(1024).unwrap(),
        vec![CapabilityId::Control],
    )
    .unwrap();
    let target = PageTarget::new(
        fixture.target,
        "opaque-target",
        "https://example.test",
        "Example",
    )
    .unwrap();
    fixture.index.put_session(session.clone()).await.unwrap();
    fixture
        .index
        .put_target(fixture.session, target.clone())
        .await
        .unwrap();

    let connection = Connection::open(&fixture.path).unwrap();
    let records: (String, String) = connection
        .query_row(
            "SELECT s.record_json, t.record_json FROM sessions s JOIN targets t USING(session_id)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<RecordingSession>(&records.0).unwrap(),
        session
    );
    assert_eq!(
        serde_json::from_str::<PageTarget>(&records.1).unwrap(),
        target
    );
}

#[tokio::test]
async fn generic_timeline_round_trips_registry_names_in_deterministic_order() {
    let fixture = Fixture::new();
    let entries = [
        fixture.observation(
            5,
            9,
            ObservationKind::Marker,
            ObservationPayloadRef::Marker(MarkerId::from_uuid(Uuid::from_u128(9))),
        ),
        fixture.observation(
            5,
            8,
            ObservationKind::InteractionBoundary,
            ObservationPayloadRef::Interaction(InteractionId::from_uuid(Uuid::from_u128(8))),
        ),
        fixture.observation(
            5,
            9,
            ObservationKind::ConsoleMessage,
            ObservationPayloadRef::External("console-ref".into()),
        ),
    ];
    for entry in entries.iter().cloned() {
        fixture.index.append(entry).await.unwrap();
    }
    let result = fixture
        .index
        .range(
            fixture.session,
            fixture.target,
            SessionRange::new(SessionTime::from_nanos(5), SessionTime::from_nanos(5)).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        result,
        [entries[1].clone(), entries[2].clone(), entries[0].clone()]
    );

    for kind in ObservationKind::ALL {
        assert_eq!(
            ObservationKind::from_stable_name(kind.as_str()),
            Some(*kind)
        );
    }

    let frame = fixture.observation(
        6,
        7,
        ObservationKind::Frame,
        ObservationPayloadRef::Frame(krometrail_core::FrameId::from_uuid(Uuid::from_u128(10))),
    );
    assert_eq!(
        fixture.index.append(frame).await.unwrap_err().code,
        krometrail_core::ErrorCode::InvalidInput
    );
}

#[tokio::test]
async fn capture_gap_and_timeline_row_are_atomic_and_overlap_queries_are_lossless() {
    let fixture = Fixture::new();
    let gap = CaptureGap::new(
        GapId::from_uuid(Uuid::from_u128(20)),
        fixture.session,
        fixture.target,
        SessionRange::new(SessionTime::from_nanos(2), SessionTime::from_nanos(8)).unwrap(),
        ObservedTime::from_nanos(9),
        CaptureGapReason::IngestionQueueSaturated,
        NonZeroU64::new(3),
        Some("bounded handoff rejected frames".into()),
    )
    .unwrap();
    fixture.index.append_gap(gap.clone()).await.unwrap();
    assert_eq!(
        fixture
            .index
            .gaps(
                fixture.session,
                fixture.target,
                SessionRange::new(SessionTime::from_nanos(5), SessionTime::from_nanos(6)).unwrap(),
            )
            .await
            .unwrap(),
        std::slice::from_ref(&gap)
    );
    let timeline = fixture
        .index
        .range(
            fixture.session,
            fixture.target,
            SessionRange::new(SessionTime::from_nanos(2), SessionTime::from_nanos(2)).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(timeline.len(), 1);
    assert_eq!(timeline[0].payload(), &ObservationPayloadRef::Gap(gap.id()));

    assert!(fixture.index.append_gap(gap).await.is_err());
    let connection = Connection::open(&fixture.path).unwrap();
    let counts: (u32, u32) = connection
        .query_row(
            "SELECT (SELECT count(*) FROM capture_gaps), \
                    (SELECT count(*) FROM timeline_observations)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(counts, (1, 1));
}

#[tokio::test]
async fn unknown_database_names_fail_with_source_safe_errors() {
    let fixture = Fixture::new();
    fixture
        .index
        .append(fixture.observation(
            1,
            1,
            ObservationKind::TargetLifecycle,
            ObservationPayloadRef::External("target-event-ref".into()),
        ))
        .await
        .unwrap();
    let connection = Connection::open(&fixture.path).unwrap();
    connection
        .execute(
            "UPDATE timeline_observations SET kind=?1",
            params!["future_kind"],
        )
        .unwrap();
    drop(connection);
    let error = fixture
        .index
        .range(
            fixture.session,
            fixture.target,
            SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(2)).unwrap(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, krometrail_core::ErrorCode::PersistenceFailed);
    assert!(!error.message.as_str().contains("future_kind"));
    assert!(!error.message.as_str().contains("SELECT"));
}

#[tokio::test]
async fn selected_range_filters_by_kind_reports_exact_count_and_truncates() {
    use krometrail_core::{
        InteractionId, MarkerId, NavigationId, TimelineRangeQuery, TimelineStore,
    };
    use std::num::NonZeroU16;

    let fixture = Fixture::new();
    // Three marker-kind observations at distinct times. Browser-event and frame
    // rows are never inserted through the generic timeline path; the kind IN
    // (...) filter excludes them by design when not requested.
    let interaction = fixture.observation(
        1,
        2,
        ObservationKind::InteractionBoundary,
        ObservationPayloadRef::Interaction(InteractionId::from_uuid(Uuid::from_u128(10))),
    );
    let navigation = fixture.observation(
        2,
        3,
        ObservationKind::Navigation,
        ObservationPayloadRef::Navigation(NavigationId::from_uuid(Uuid::from_u128(11))),
    );
    let marker = fixture.observation(
        3,
        4,
        ObservationKind::Marker,
        ObservationPayloadRef::Marker(MarkerId::from_uuid(Uuid::from_u128(12))),
    );
    for entry in [interaction, navigation, marker] {
        fixture.index.append(entry).await.unwrap();
    }

    // Limit below the matched count: exact count, bounded rows, truncated flag.
    let query = TimelineRangeQuery::new(
        fixture.session,
        fixture.target,
        SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(10)).unwrap(),
        vec![
            ObservationKind::InteractionBoundary,
            ObservationKind::Navigation,
            ObservationKind::Marker,
        ],
        NonZeroU16::new(2).unwrap(),
    )
    .unwrap();
    let slice = TimelineStore::selected_range(&fixture.index, query)
        .await
        .unwrap();
    assert_eq!(slice.matched_count, 3);
    assert_eq!(slice.observations.len(), 2);
    assert!(slice.truncated);
    assert_eq!(
        slice.observations[0].kind(),
        ObservationKind::InteractionBoundary
    );
    assert_eq!(slice.observations[1].kind(), ObservationKind::Navigation);

    // Limit above the matched count: no truncation.
    let query = TimelineRangeQuery::new(
        fixture.session,
        fixture.target,
        SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(10)).unwrap(),
        vec![ObservationKind::Marker],
        NonZeroU16::new(64).unwrap(),
    )
    .unwrap();
    let slice = TimelineStore::selected_range(&fixture.index, query)
        .await
        .unwrap();
    assert_eq!(slice.matched_count, 1);
    assert_eq!(slice.observations.len(), 1);
    assert!(!slice.truncated);
    assert_eq!(slice.observations[0].kind(), ObservationKind::Marker);

    // A kind not present in the timeline returns zero matched.
    let query = TimelineRangeQuery::new(
        fixture.session,
        fixture.target,
        SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(10)).unwrap(),
        vec![ObservationKind::ConsoleMessage],
        NonZeroU16::new(64).unwrap(),
    )
    .unwrap();
    let slice = TimelineStore::selected_range(&fixture.index, query)
        .await
        .unwrap();
    assert_eq!(slice.matched_count, 0);
    assert!(slice.observations.is_empty());
    assert!(!slice.truncated);

    // Query constructor validates unique non-empty kinds and the 4096-row cap.
    assert!(
        TimelineRangeQuery::new(
            fixture.session,
            fixture.target,
            SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(1)).unwrap(),
            Vec::new(),
            NonZeroU16::new(1).unwrap(),
        )
        .is_err()
    );
    assert!(
        TimelineRangeQuery::new(
            fixture.session,
            fixture.target,
            SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(1)).unwrap(),
            vec![ObservationKind::Marker, ObservationKind::Marker],
            NonZeroU16::new(1).unwrap(),
        )
        .is_err()
    );
    assert!(
        TimelineRangeQuery::new(
            fixture.session,
            fixture.target,
            SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(1)).unwrap(),
            vec![ObservationKind::Marker],
            NonZeroU16::new(4097).unwrap(),
        )
        .is_err()
    );
    assert_eq!(krometrail_core::MAX_TIMELINE_RANGE_ROWS, 4096);
}
