use std::{sync::Arc, time::Duration};

use krometrail_core::{
    BrowserOperationKind, ElementLocator, InteractionAnchor, InteractionAnchorSource,
    InteractionEvidenceSink, InteractionId, InteractionLocator, InteractionOutcome,
    InteractionPostcondition, InteractionRecord, InteractionRecordSource, InteractionTiming,
    LocatorSummary, NavigationId, NodeReference, NodeStateFacts, ObservationContext,
    ObservationKind, ObservedTime, SanitizedParameters, SessionId, SessionRange, SessionTime,
    SnapshotGeneration, SnapshotNodeId, TargetId, TargetNodeOutcome, TimelineStore,
};
use krometrail_store::{
    IndexStoreConfig, RecordingStore, RotationConfig, SegmentStoreConfig, SegmentWriter,
    SqliteIndex,
};
use tempfile::TempDir;
use uuid::Uuid;

/// A fully-populated side-channel postcondition block: the store round-trip
/// must decode every current fact shape at the current schema version.
fn populated_postcondition() -> InteractionPostcondition {
    let mut postcondition = InteractionPostcondition::from_facts(
        Some(&NodeStateFacts {
            connected: true,
            checked: Some(false),
            value_length: Some(0),
            ..NodeStateFacts::default()
        }),
        Some(&NodeStateFacts {
            connected: true,
            checked: Some(false),
            value_length: Some(0),
            ..NodeStateFacts::default()
        }),
        Some(false),
        true,
        Some(true),
        krometrail_core::SideChannelSignals {
            window_open_attempts: Some(1),
            download_requests: Some(0),
        },
    );
    postcondition.attach_new_pages(krometrail_core::NewPagePostcondition::from_observed(
        krometrail_core::PageSequence::new(2).unwrap(),
        vec![krometrail_core::NewPageFact {
            target_id: TargetId::from_uuid(Uuid::from_u128(77)),
            sequence: krometrail_core::PageSequence::new(3).unwrap(),
            opener_matched: true,
        }],
    ));
    postcondition.attach_downloads(krometrail_core::DownloadPostcondition::from_observed(
        krometrail_core::DownloadSequence::new(1).unwrap(),
        vec![krometrail_core::DownloadFact {
            download_id: krometrail_core::DownloadId::from_uuid(Uuid::from_u128(78)),
            sequence: krometrail_core::DownloadSequence::new(2).unwrap(),
            state: krometrail_core::DownloadState::InProgress,
        }],
    ));
    postcondition
}

struct Fixture {
    _directory: TempDir,
    database_path: std::path::PathBuf,
    index: Arc<SqliteIndex>,
    store: Arc<RecordingStore>,
    session: SessionId,
    target: TargetId,
}

impl Fixture {
    fn new() -> Self {
        let directory = TempDir::new().unwrap();
        let database_path = directory.path().join("index.sqlite3");
        let segments_directory = directory.path().join("segments");
        let index = Arc::new(
            SqliteIndex::open(IndexStoreConfig {
                database_path: database_path.clone(),
                segments_directory: segments_directory.clone(),
                busy_timeout: Duration::from_secs(1),
            })
            .unwrap(),
        );
        let writer = Arc::new(
            SegmentWriter::open(SegmentStoreConfig {
                directory: segments_directory,
                rotation: RotationConfig::suggested(),
            })
            .unwrap(),
        );
        let store =
            Arc::new(RecordingStore::new(writer, Arc::clone(&index), store_test_clock()).unwrap());
        Self {
            _directory: directory,
            database_path,
            index,
            store,
            session: SessionId::from_uuid(Uuid::from_u128(1)),
            target: TargetId::from_uuid(Uuid::from_u128(2)),
        }
    }

    fn page_anchor(&self, id: u128, operation: BrowserOperationKind) -> InteractionAnchor {
        InteractionAnchor::new(
            InteractionId::from_uuid(Uuid::from_u128(id)),
            self.session,
            self.target,
            operation,
            InteractionTiming::new(
                SessionTime::from_nanos(10),
                SessionTime::from_nanos(20),
                SessionTime::from_nanos(30),
                Some(SessionTime::from_nanos(40)),
            )
            .unwrap(),
        )
        .unwrap()
    }

    fn action_record(&self, id: u128) -> InteractionRecord {
        let reference = NodeReference {
            target_id: self.target,
            generation: SnapshotGeneration::new(1).unwrap(),
            node_id: SnapshotNodeId::new(1).unwrap(),
        };
        let locator = InteractionLocator::Element(ElementLocator::Reference(reference));
        InteractionRecord::new(
            InteractionId::from_uuid(Uuid::from_u128(id)),
            ObservationContext::new(
                self.session,
                self.target,
                1,
                SessionTime::from_nanos(10),
                SessionTime::from_nanos(40),
            )
            .unwrap(),
            SessionTime::from_nanos(20),
            SessionTime::from_nanos(30),
            BrowserOperationKind::Click,
            SanitizedParameters::new(serde_json::json!({
                "button": "left",
                "locator": {"kind": "reference"}
            }))
            .unwrap(),
            LocatorSummary::from_locator(Some(&locator)),
            Some(krometrail_core::ExpectationTargetRole::Checkbox),
            InteractionOutcome::Dispatched,
            populated_postcondition(),
            Some(InteractionId::from_uuid(Uuid::from_u128(99))),
        )
        .unwrap()
    }
}

#[tokio::test]
async fn exact_anchor_and_optional_action_record_round_trip_idempotently() {
    let fixture = Fixture::new();
    let page = fixture.page_anchor(10, BrowserOperationKind::NavigatePage);
    let navigation = NavigationId::from_uuid(Uuid::from_u128(11));
    fixture
        .store
        .append_operation_evidence(
            page.clone(),
            None,
            ObservedTime::from_nanos(50),
            Some(navigation),
        )
        .await
        .unwrap();
    fixture
        .store
        .append_operation_evidence(
            page.clone(),
            None,
            ObservedTime::from_nanos(50),
            Some(navigation),
        )
        .await
        .unwrap();
    assert_eq!(
        fixture
            .index
            .interaction_anchor(page.interaction_id)
            .await
            .unwrap(),
        Some(page.clone())
    );
    assert_eq!(
        fixture
            .index
            .interaction_record(page.interaction_id)
            .await
            .unwrap(),
        None
    );
    let observations = fixture
        .index
        .range(
            fixture.session,
            fixture.target,
            SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(50)).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        observations
            .iter()
            .filter(|item| item.kind() == ObservationKind::InteractionBoundary)
            .count(),
        4
    );
    assert_eq!(
        observations
            .iter()
            .filter(|item| item.kind() == ObservationKind::Navigation)
            .count(),
        1
    );

    let record = fixture.action_record(12);
    let anchor = record.anchor().unwrap();
    fixture
        .store
        .append_operation_evidence(
            anchor.clone(),
            Some(record.clone()),
            ObservedTime::from_nanos(50),
            None,
        )
        .await
        .unwrap();
    let decoded = fixture
        .index
        .interaction_record(record.id)
        .await
        .unwrap()
        .expect("stored interaction record decodes");
    assert_eq!(decoded, record);
    // The populated postcondition block survives the opaque record_json round trip.
    assert_eq!(
        decoded.postcondition.target.node,
        TargetNodeOutcome::Present
    );
    assert_eq!(decoded.postcondition.target.checked.changed, Some(false));
    assert_eq!(
        decoded.target_role,
        Some(krometrail_core::ExpectationTargetRole::Checkbox)
    );
    assert_eq!(
        decoded.expectation_note,
        Some(krometrail_core::ExpectationNote::CheckedStateUnchanged)
    );
    assert_eq!(
        decoded.postcondition.target.value_length_changed,
        Some(false)
    );
    assert_eq!(decoded.postcondition.page.url_changed, Some(false));
    assert!(decoded.postcondition.page.navigation_lifecycle_observed);
    assert_eq!(
        decoded.postcondition.page.main_frame_navigation_observed,
        Some(true)
    );
    assert_eq!(decoded.postcondition.signals.window_open_attempts, Some(1));
    let new_pages = decoded.postcondition.new_pages.as_ref().unwrap();
    assert_eq!(new_pages.pages.len(), 1);
    assert!(new_pages.pages[0].opener_matched);
    let downloads = decoded.postcondition.downloads.as_ref().unwrap();
    assert_eq!(downloads.downloads.len(), 1);
    assert_eq!(
        downloads.downloads[0].state,
        krometrail_core::DownloadState::InProgress
    );
    assert_eq!(
        fixture
            .index
            .interaction_anchor(anchor.interaction_id)
            .await
            .unwrap(),
        Some(anchor)
    );
}

#[tokio::test]
async fn conflicts_corruption_and_latest_ties_fail_or_order_source_safely() {
    let fixture = Fixture::new();
    let low = fixture.page_anchor(20, BrowserOperationKind::ReloadPage);
    let high = fixture.page_anchor(21, BrowserOperationKind::ReloadPage);
    for anchor in [low.clone(), high.clone()] {
        fixture
            .store
            .append_operation_evidence(anchor, None, ObservedTime::from_nanos(50), None)
            .await
            .unwrap();
    }
    assert_eq!(
        fixture
            .index
            .latest_interaction_anchor(fixture.session, fixture.target)
            .await
            .unwrap(),
        Some(high.clone())
    );

    let conflicting = fixture.page_anchor(21, BrowserOperationKind::GoBack);
    let error = fixture
        .store
        .append_operation_evidence(conflicting, None, ObservedTime::from_nanos(50), None)
        .await
        .unwrap_err();
    assert_eq!(error.code, krometrail_core::ErrorCode::PersistenceFailed);
    assert!(!error.message.as_str().contains("reload"));

    let record = fixture.action_record(22);
    fixture
        .store
        .append_operation_evidence(
            record.anchor().unwrap(),
            Some(record.clone()),
            ObservedTime::from_nanos(50),
            None,
        )
        .await
        .unwrap();
    let external = rusqlite::Connection::open(&fixture.database_path).unwrap();
    external
        .execute(
            "UPDATE interactions SET record_json='not-json' WHERE interaction_id=?1",
            [record.id.as_uuid().as_bytes().as_slice()],
        )
        .unwrap();
    let error = fixture
        .index
        .interaction_record(record.id)
        .await
        .unwrap_err();
    assert_eq!(error.code, krometrail_core::ErrorCode::PersistenceFailed);
    assert!(!error.message.as_str().contains("not-json"));
}

fn store_test_clock() -> std::sync::Arc<dyn krometrail_core::MonotonicClock> {
    struct Fixed;
    impl krometrail_core::MonotonicClock for Fixed {
        fn now(&self) -> krometrail_core::ObservedTime {
            krometrail_core::ObservedTime::from_nanos(0)
        }
    }
    std::sync::Arc::new(Fixed)
}
