use std::sync::Arc;
use std::time::{Duration, SystemTime};

use krometrail_core::{
    AnchorScope, BrowserProduct, BrowserProductVersion, BrowserVersion, CaptureGap,
    CaptureGapPolicy, CaptureGapReason, CaptureGapStore, CaptureOrdinal, CapturedFrame,
    DeviceScaleFactor, DiskBudgetBytes, EncodedFrame, ErrorCode, FrameId, ImageFormat,
    IntervalAnchorScope, MarkerId, ObservationKind, ObservationPayloadRef, ObservedTime,
    PageTarget, ProfileIdentity, ProfileRef, RangeResolutionOptions, RecordingCatalog,
    RecordingSession, RecordingSink, SessionId, SessionRange, SessionTime, TargetId,
    TemporalRangeAnchor, TemporalRangeResolver, TimelineObservation, TimelineStore,
};
use krometrail_store::{
    IndexStoreConfig, RecordingStore, RotationConfig, SegmentStoreConfig, SegmentWriter,
    SqliteIndex,
};
use tempfile::TempDir;
use uuid::Uuid;

type RangeResolver = TemporalRangeResolver<
    Arc<SqliteIndex>,
    Arc<SqliteIndex>,
    Arc<SqliteIndex>,
    Arc<SqliteIndex>,
    Arc<SqliteIndex>,
>;

struct Fixture {
    _directory: TempDir,
    index: Arc<SqliteIndex>,
    store: Arc<RecordingStore>,
    session: SessionId,
    target: TargetId,
    frames: Vec<EncodedFrame>,
}

impl Fixture {
    async fn new() -> Self {
        let directory = TempDir::new().unwrap();
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
        let store = Arc::new(RecordingStore::new(writer, Arc::clone(&index)).unwrap());
        let session = SessionId::from_uuid(Uuid::from_u128(1));
        let target = TargetId::from_uuid(Uuid::from_u128(2));
        let mut frames = Vec::new();
        for (id, ordinal, at) in [(10, 1, 1), (11, 2, 5), (12, 3, 5), (13, 4, 10)] {
            let frame = EncodedFrame::new(
                CapturedFrame::new(
                    FrameId::from_uuid(Uuid::from_u128(id)),
                    session,
                    target,
                    CaptureOrdinal::new(ordinal).unwrap(),
                    None,
                    ObservedTime::from_nanos(at),
                    SessionTime::from_nanos(at),
                    ImageFormat::Jpeg,
                    krometrail_core::PixelDimensions::new(2, 2).unwrap(),
                    krometrail_core::PixelDimensions::new(2, 2).unwrap(),
                    DeviceScaleFactor::new(1.0).unwrap(),
                    vec![],
                )
                .unwrap(),
                vec![id as u8, ordinal as u8],
            )
            .unwrap();
            store.append_frame(frame.clone()).await.unwrap();
            frames.push(frame);
        }
        let session_record = RecordingSession::new(
            session,
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
            ProfileRef::managed(ProfileIdentity::new("range-test").unwrap()),
            DiskBudgetBytes::new(1024).unwrap(),
            vec![krometrail_core::CapabilityId::Control],
            krometrail_core::EveryNthFrame::default(),
        )
        .unwrap();
        index.put_session(session_record).await.unwrap();
        index
            .put_target(
                session,
                PageTarget::new(target, "target", "https://example.test", "Example").unwrap(),
            )
            .await
            .unwrap();
        Self {
            _directory: directory,
            index,
            store,
            session,
            target,
            frames,
        }
    }

    fn resolver(&self) -> RangeResolver {
        TemporalRangeResolver::new(
            Arc::clone(&self.index),
            Arc::clone(&self.index),
            Arc::clone(&self.index),
            Arc::clone(&self.index),
            Arc::clone(&self.index),
        )
    }

    async fn observation(&self, at: u64, payload: ObservationPayloadRef, kind: ObservationKind) {
        self.index
            .append(
                TimelineObservation::new(
                    self.session,
                    self.target,
                    SessionTime::from_nanos(at),
                    None,
                    ObservedTime::from_nanos(at),
                    kind,
                    payload,
                )
                .unwrap(),
            )
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn explicit_session_and_wall_clock_ranges_share_frame_order() {
    let fixture = Fixture::new().await;
    let scope = IntervalAnchorScope::new(fixture.session, fixture.target);
    let session_range = TemporalRangeAnchor::SessionTime {
        scope,
        range: SessionRange::new(SessionTime::from_nanos(1), SessionTime::from_nanos(10)).unwrap(),
    };
    let wall_range = TemporalRangeAnchor::WallClock {
        scope,
        start: SystemTime::UNIX_EPOCH + Duration::from_nanos(1),
        end: SystemTime::UNIX_EPOCH + Duration::from_nanos(10),
    };
    let first = fixture
        .resolver()
        .resolve(session_range, RangeResolutionOptions::DEFAULT)
        .await
        .unwrap();
    let second = fixture
        .resolver()
        .resolve(wall_range, RangeResolutionOptions::DEFAULT)
        .await
        .unwrap();
    assert_eq!(first.frame_ids, second.frame_ids);
    assert_eq!(
        first.frame_ids,
        fixture
            .frames
            .iter()
            .map(|frame| frame.metadata().id())
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn source_frame_ranges_are_inclusive_and_capture_ordinal_ordered() {
    let fixture = Fixture::new().await;
    let anchor = TemporalRangeAnchor::SourceFrame {
        scope: AnchorScope::new(Some(fixture.session), Some(fixture.target)),
        start_frame_id: fixture.frames[1].metadata().id(),
        end_frame_id: fixture.frames[2].metadata().id(),
    };
    let resolved = fixture
        .resolver()
        .resolve(anchor, RangeResolutionOptions::DEFAULT)
        .await
        .unwrap();
    assert_eq!(
        resolved.frame_ids,
        vec![
            fixture.frames[1].metadata().id(),
            fixture.frames[2].metadata().id()
        ]
    );
    assert_eq!(resolved.resolved_range.start(), SessionTime::from_nanos(5));
}

#[tokio::test]
async fn marker_navigation_and_gap_policies_use_generic_timeline_rows() {
    let fixture = Fixture::new().await;
    let marker = MarkerId::from_uuid(Uuid::from_u128(30));
    fixture
        .observation(
            5,
            ObservationPayloadRef::Marker(marker),
            ObservationKind::Marker,
        )
        .await;
    let navigation = krometrail_core::NavigationId::from_uuid(Uuid::from_u128(31));
    fixture
        .observation(
            10,
            ObservationPayloadRef::Navigation(navigation),
            ObservationKind::Navigation,
        )
        .await;
    let gap = CaptureGap::new(
        krometrail_core::GapId::from_uuid(Uuid::from_u128(32)),
        fixture.session,
        fixture.target,
        SessionRange::new(SessionTime::from_nanos(5), SessionTime::from_nanos(5)).unwrap(),
        ObservedTime::from_nanos(6),
        CaptureGapReason::CaptureStopped,
        None,
        None,
    )
    .unwrap();
    fixture.index.append_gap(gap.clone()).await.unwrap();

    let marker_range = fixture
        .resolver()
        .resolve(
            TemporalRangeAnchor::Marker {
                scope: AnchorScope::new(Some(fixture.session), Some(fixture.target)),
                marker_id: marker,
                window: None,
            },
            RangeResolutionOptions::DEFAULT,
        )
        .await
        .unwrap();
    assert_eq!(marker_range.marker_ids, vec![marker]);
    assert_eq!(marker_range.gaps, vec![gap]);
    // Non-interaction anchors carry no applied interaction window.
    assert_eq!(marker_range.applied_interaction_window, None);
    let mut reject = RangeResolutionOptions::DEFAULT;
    reject.capture_gaps = CaptureGapPolicy::Reject;
    assert_eq!(
        fixture
            .resolver()
            .resolve(
                TemporalRangeAnchor::Marker {
                    scope: AnchorScope::new(Some(fixture.session), Some(fixture.target)),
                    marker_id: marker,
                    window: None,
                },
                reject
            )
            .await
            .unwrap_err()
            .code,
        ErrorCode::NotFound
    );
    let navigation_range = fixture
        .resolver()
        .resolve(
            TemporalRangeAnchor::Navigation {
                scope: AnchorScope::new(None, None),
                navigation_id: navigation,
                window: None,
            },
            RangeResolutionOptions::DEFAULT,
        )
        .await
        .unwrap();
    assert_eq!(navigation_range.navigation_ids, vec![navigation]);
}

#[tokio::test]
async fn durable_interaction_anchors_resolve_and_uncaptured_edges_are_not_partial_eviction() {
    use krometrail_core::{
        BrowserOperationKind, InteractionAnchor, InteractionEvidenceSink, InteractionTiming,
    };

    let fixture = Fixture::new().await;
    let interaction_id = krometrail_core::InteractionId::from_uuid(Uuid::from_u128(40));
    let anchor = InteractionAnchor::new(
        interaction_id,
        fixture.session,
        fixture.target,
        BrowserOperationKind::NavigatePage,
        InteractionTiming::new(
            SessionTime::from_nanos(5),
            SessionTime::from_nanos(5),
            SessionTime::from_nanos(5),
            Some(SessionTime::from_nanos(5)),
        )
        .unwrap(),
    )
    .unwrap();
    fixture
        .store
        .append_operation_evidence(anchor, None, ObservedTime::from_nanos(6), None)
        .await
        .unwrap();
    let zero = krometrail_core::InteractionWindow::new(Duration::ZERO, Duration::ZERO).unwrap();
    let resolved = fixture
        .resolver()
        .resolve(
            TemporalRangeAnchor::Interaction {
                scope: AnchorScope::new(Some(fixture.session), Some(fixture.target)),
                interaction_id,
                window: Some(zero),
            },
            RangeResolutionOptions::DEFAULT,
        )
        .await
        .unwrap();
    assert_eq!(resolved.interaction_ids, vec![interaction_id]);
    assert_eq!(resolved.frame_ids.len(), 2);
    // The explicit window governs and is echoed exactly.
    assert_eq!(resolved.applied_interaction_window, Some(zero));

    // An omitted window falls back to the implicit default, and the resolved
    // range echoes that governing default rather than leaving the echo to the
    // ambiguous `options.implicit_interaction_window` input.
    let mut allow_partial = RangeResolutionOptions::DEFAULT;
    allow_partial.retention = krometrail_core::RetentionPolicy::AllowPartial;
    let implicit = fixture
        .resolver()
        .resolve(
            TemporalRangeAnchor::Interaction {
                scope: AnchorScope::new(Some(fixture.session), Some(fixture.target)),
                interaction_id,
                window: None,
            },
            allow_partial,
        )
        .await
        .unwrap();
    assert_eq!(
        implicit.applied_interaction_window,
        Some(RangeResolutionOptions::DEFAULT.implicit_interaction_window)
    );

    let late_interaction_id = krometrail_core::InteractionId::from_uuid(Uuid::from_u128(41));
    let late_completion = SessionTime::from_nanos(26_000_010);
    fixture
        .store
        .append_operation_evidence(
            InteractionAnchor::new(
                late_interaction_id,
                fixture.session,
                fixture.target,
                BrowserOperationKind::NavigatePage,
                InteractionTiming::new(
                    SessionTime::from_nanos(5),
                    SessionTime::from_nanos(6),
                    late_completion,
                    Some(late_completion),
                )
                .unwrap(),
            )
            .unwrap(),
            None,
            ObservedTime::from_nanos(26_000_011),
            None,
        )
        .await
        .unwrap();

    let latest = TemporalRangeAnchor::LatestInteraction {
        session_id: fixture.session,
        target_id: fixture.target,
        window: Some(
            krometrail_core::InteractionWindow::new(Duration::from_millis(100), Duration::ZERO)
                .unwrap(),
        ),
    };
    let mut allow_natural_tail = RangeResolutionOptions::DEFAULT;
    allow_natural_tail.retention = krometrail_core::RetentionPolicy::AllowPartial;
    let partial_latest = fixture
        .resolver()
        .resolve(latest.clone(), allow_natural_tail)
        .await
        .unwrap();
    assert_eq!(
        partial_latest.requested_range,
        SessionRange::new(SessionTime::ZERO, late_completion).unwrap()
    );
    assert_eq!(
        partial_latest.resolved_range,
        SessionRange::new(SessionTime::from_nanos(1), SessionTime::from_nanos(10)).unwrap()
    );
    assert_eq!(
        partial_latest.resolved_anchor.reference,
        krometrail_core::ResolvedAnchorReference::Interaction {
            interaction_id: late_interaction_id
        }
    );
    assert!(
        partial_latest
            .retention_warnings
            .iter()
            .any(|warning| matches!(
                warning,
                krometrail_core::RetentionWarning::PartiallyCaptured { requested, retained }
                    if *requested == partial_latest.requested_range
                        && *retained == partial_latest.resolved_range
            ))
    );

    assert_eq!(
        fixture
            .resolver()
            .resolve(latest, RangeResolutionOptions::DEFAULT)
            .await
            .unwrap_err()
            .code,
        ErrorCode::NotFound
    );
    let disjoint_interaction_id = krometrail_core::InteractionId::from_uuid(Uuid::from_u128(42));
    fixture
        .store
        .append_operation_evidence(
            InteractionAnchor::new(
                disjoint_interaction_id,
                fixture.session,
                fixture.target,
                BrowserOperationKind::NavigatePage,
                InteractionTiming::new(
                    late_completion,
                    late_completion,
                    late_completion,
                    Some(late_completion),
                )
                .unwrap(),
            )
            .unwrap(),
            None,
            ObservedTime::from_nanos(26_000_012),
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        fixture
            .resolver()
            .resolve(
                TemporalRangeAnchor::LatestInteraction {
                    session_id: fixture.session,
                    target_id: fixture.target,
                    window: Some(
                        krometrail_core::InteractionWindow::new(Duration::ZERO, Duration::ZERO)
                            .unwrap(),
                    ),
                },
                allow_natural_tail,
            )
            .await
            .unwrap_err()
            .code,
        ErrorCode::NotFound
    );

    let mut partial = RangeResolutionOptions::DEFAULT;
    partial.retention = krometrail_core::RetentionPolicy::AllowPartial;
    let error = fixture
        .resolver()
        .resolve(
            TemporalRangeAnchor::SessionTime {
                scope: IntervalAnchorScope::new(fixture.session, fixture.target),
                range: SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(20)).unwrap(),
            },
            partial,
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::NotFound);
}

#[tokio::test]
async fn source_frame_scope_mismatch_is_invalid_input() {
    let fixture = Fixture::new().await;
    let wrong_target = TargetId::from_uuid(Uuid::from_u128(99));
    let error = fixture
        .resolver()
        .resolve(
            TemporalRangeAnchor::SourceFrame {
                scope: AnchorScope::new(Some(fixture.session), Some(wrong_target)),
                start_frame_id: fixture.frames[0].metadata().id(),
                end_frame_id: fixture.frames[0].metadata().id(),
            },
            RangeResolutionOptions::DEFAULT,
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidInput);
}

#[tokio::test]
async fn wall_clock_before_session_and_empty_target_are_not_found() {
    let fixture = Fixture::new().await;
    let scope = IntervalAnchorScope::new(fixture.session, fixture.target);
    let before = fixture
        .resolver()
        .resolve(
            TemporalRangeAnchor::WallClock {
                scope,
                start: SystemTime::UNIX_EPOCH - Duration::from_nanos(1),
                end: SystemTime::UNIX_EPOCH,
            },
            RangeResolutionOptions::DEFAULT,
        )
        .await
        .unwrap_err();
    assert_eq!(before.code, ErrorCode::NotFound);

    let empty_target = TargetId::from_uuid(Uuid::from_u128(88));
    let error = fixture
        .resolver()
        .resolve(
            TemporalRangeAnchor::SessionTime {
                scope: IntervalAnchorScope::new(fixture.session, empty_target),
                range: SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(10)).unwrap(),
            },
            RangeResolutionOptions::DEFAULT,
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::NotFound);
}

#[tokio::test]
async fn anchor_window_overflow_is_invalid_time_before_frame_lookup() {
    let fixture = Fixture::new().await;
    let marker = MarkerId::from_uuid(Uuid::from_u128(90));
    fixture
        .observation(
            u64::MAX,
            ObservationPayloadRef::Marker(marker),
            ObservationKind::Marker,
        )
        .await;
    let error = fixture
        .resolver()
        .resolve(
            TemporalRangeAnchor::Marker {
                scope: AnchorScope::new(Some(fixture.session), Some(fixture.target)),
                marker_id: marker,
                window: Some(
                    krometrail_core::InteractionWindow::new(
                        Duration::ZERO,
                        Duration::from_millis(1),
                    )
                    .unwrap(),
                ),
            },
            RangeResolutionOptions::DEFAULT,
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidTime);
}

#[tokio::test]
async fn marker_scope_mismatch_is_invalid_input_not_missing_data() {
    let fixture = Fixture::new().await;
    let marker = MarkerId::from_uuid(Uuid::from_u128(91));
    fixture
        .observation(
            5,
            ObservationPayloadRef::Marker(marker),
            ObservationKind::Marker,
        )
        .await;
    let error = fixture
        .resolver()
        .resolve(
            TemporalRangeAnchor::Marker {
                scope: AnchorScope::new(
                    Some(fixture.session),
                    Some(TargetId::from_uuid(Uuid::from_u128(92))),
                ),
                marker_id: marker,
                window: None,
            },
            RangeResolutionOptions::DEFAULT,
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidInput);
}
