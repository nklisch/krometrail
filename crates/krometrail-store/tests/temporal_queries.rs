use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};

use krometrail_core::{
    AnchorScope, BrowserActionRequest, BrowserOperationKind, BrowserProduct, BrowserProductVersion,
    BrowserVersion, CaptureGap, CaptureGapPolicy, CaptureGapReason, CaptureGapStore,
    CaptureOrdinal, CapturedFrame, DeviceScaleFactor, DialogAction, DiskBudgetBytes,
    ElementLocator, EncodedFrame, FillMode, FillRequest, FrameId, FrameSource, HandleDialogRequest,
    ImageFormat, InteractionAnchor, InteractionEvidenceSink, InteractionId, InteractionLocator,
    InteractionOutcome, InteractionRecord, InteractionTiming, LocatorSummary, MarkerId,
    NavigationId, NonEmptyText, ObservationContext, ObservationKind, ObservationPayloadRef,
    ObservedTime, PageSelection, PageTarget, PixelDimensions, ProfileIdentity, ProfileRef,
    RecordingCatalog, RecordingSink, RetentionPolicy, RetentionStore, SessionId, SessionRange,
    SessionTime, TargetId, TemporalQuery, TemporalQueryRequest, TemporalRangeAnchor,
    TemporalRangeAnchorKind, TimelineObservation, TimelineStore, UploadFilesRequest,
    ValidatedFilePath,
};
use krometrail_store::{
    IndexStoreConfig, RecordingStore, RotationConfig, SegmentStoreConfig, SegmentWriter,
    SqliteIndex,
};
use rusqlite::params;
use tempfile::TempDir;
use uuid::Uuid;

const MS: u64 = 1_000_000;

struct Fixture {
    _directory: TempDir,
    database_path: std::path::PathBuf,
    index: Arc<SqliteIndex>,
    store: Arc<RecordingStore>,
    session: SessionId,
    target: TargetId,
    other_target: TargetId,
    frames: Vec<EncodedFrame>,
    interaction_id: InteractionId,
    navigation_id: NavigationId,
    marker_ids: [MarkerId; 2],
}

impl Fixture {
    async fn new() -> Self {
        let directory = TempDir::new().unwrap();
        let database_path = directory.path().join("index.sqlite3");
        let segments = directory.path().join("segments");
        let index = Arc::new(
            SqliteIndex::open(IndexStoreConfig {
                database_path: database_path.clone(),
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
        let other_target = TargetId::from_uuid(Uuid::from_u128(3));
        index
            .put_session(
                krometrail_core::RecordingSession::new(
                    session,
                    ObservedTime::from_nanos(0),
                    SystemTime::UNIX_EPOCH,
                    BrowserVersion::new(
                        BrowserProduct::Chrome,
                        BrowserProductVersion::new("149").unwrap(),
                        "revision",
                        "1.3",
                        "Chrome/149",
                        "12",
                    )
                    .unwrap(),
                    ProfileRef::managed(ProfileIdentity::new("temporal-query").unwrap()),
                    DiskBudgetBytes::new(1024 * 1024).unwrap(),
                    vec![krometrail_core::CapabilityId::Control],
                    krometrail_core::EveryNthFrame::default(),
                )
                .unwrap(),
            )
            .await
            .unwrap();
        for (id, key) in [(target, "target-a"), (other_target, "target-b")] {
            index
                .put_target(
                    session,
                    PageTarget::new(id, key, "https://fixture.invalid", "Fixture").unwrap(),
                )
                .await
                .unwrap();
        }

        let mut frames = Vec::new();
        for (position, millis) in [
            0_u64, 50, 100, 150, 200, 250, 300, 350, 400, 400, 500, 600, 700, 800,
        ]
        .into_iter()
        .enumerate()
        {
            let frame = frame(
                100 + position as u128,
                session,
                target,
                u64::try_from(position + 1).unwrap(),
                millis,
            );
            store.append_frame(frame.clone()).await.unwrap();
            frames.push(frame);
        }
        store
            .append_frame(frame(200, session, other_target, 1, 400))
            .await
            .unwrap();

        let interaction_id = InteractionId::from_uuid(Uuid::from_u128(300));
        let action_record = InteractionRecord::new(
            interaction_id,
            ObservationContext::new(session, target, 1, at(200), at(350)).unwrap(),
            at(250),
            at(350),
            BrowserOperationKind::Click,
            krometrail_core::SanitizedParameters::new(serde_json::json!({"button":"left"}))
                .unwrap(),
            LocatorSummary::from_locator(None),
            InteractionOutcome::Dispatched,
            None,
        )
        .unwrap();
        store
            .append_operation_evidence(
                action_record.anchor().unwrap(),
                Some(action_record),
                ObservedTime::from_nanos(900 * MS),
                None,
            )
            .await
            .unwrap();

        let navigation_id = NavigationId::from_uuid(Uuid::from_u128(301));
        store
            .append_operation_evidence(
                InteractionAnchor::new(
                    InteractionId::from_uuid(Uuid::from_u128(302)),
                    session,
                    target,
                    BrowserOperationKind::NavigatePage,
                    InteractionTiming::new(at(500), at(500), at(500), Some(at(500))).unwrap(),
                )
                .unwrap(),
                None,
                ObservedTime::from_nanos(900 * MS),
                Some(navigation_id),
            )
            .await
            .unwrap();

        let marker_ids = [
            MarkerId::from_uuid(Uuid::from_u128(303)),
            MarkerId::from_uuid(Uuid::from_u128(304)),
        ];
        for marker_id in marker_ids {
            store
                .append(
                    TimelineObservation::new(
                        session,
                        target,
                        at(400),
                        None,
                        ObservedTime::from_nanos(900 * MS),
                        ObservationKind::Marker,
                        ObservationPayloadRef::Marker(marker_id),
                    )
                    .unwrap(),
                )
                .await
                .unwrap();
        }
        index
            .append_gap(
                CaptureGap::new(
                    krometrail_core::GapId::from_uuid(Uuid::from_u128(305)),
                    session,
                    target,
                    SessionRange::new(at(300), at(320)).unwrap(),
                    ObservedTime::from_nanos(900 * MS),
                    CaptureGapReason::IngestionQueueSaturated,
                    std::num::NonZeroU64::new(1),
                    Some("declared fixture gap".into()),
                )
                .unwrap(),
            )
            .await
            .unwrap();

        Self {
            _directory: directory,
            database_path,
            index,
            store,
            session,
            target,
            other_target,
            frames,
            interaction_id,
            navigation_id,
            marker_ids,
        }
    }

    fn scope(&self) -> AnchorScope {
        AnchorScope::new(Some(self.session), Some(self.target))
    }

    async fn resolve(&self, anchor: TemporalRangeAnchor) -> krometrail_core::ResolvedRange {
        self.store
            .resolve_range(TemporalQueryRequest::strict(anchor).unwrap())
            .await
            .unwrap()
    }

    fn simulate_eviction(&self, range: SessionRange) {
        let connection = rusqlite::Connection::open(&self.database_path).unwrap();
        connection
            .execute(
                "INSERT INTO evicted_frame_ranges(session_id,target_id,start_time_be,end_time_be) \
                 VALUES (?1,?2,?3,?4)",
                params![
                    self.session.as_uuid().as_bytes().as_slice(),
                    self.target.as_uuid().as_bytes().as_slice(),
                    range.start().as_nanos().to_be_bytes().as_slice(),
                    range.end().as_nanos().to_be_bytes().as_slice(),
                ],
            )
            .unwrap();
        connection
            .execute(
                "DELETE FROM frames WHERE session_id=?1 AND target_id=?2 \
                 AND session_time_be>=?3 AND session_time_be<=?4",
                params![
                    self.session.as_uuid().as_bytes().as_slice(),
                    self.target.as_uuid().as_bytes().as_slice(),
                    range.start().as_nanos().to_be_bytes().as_slice(),
                    range.end().as_nanos().to_be_bytes().as_slice(),
                ],
            )
            .unwrap();
    }
}

fn at(millis: u64) -> SessionTime {
    SessionTime::from_nanos(millis * MS)
}

fn frame(
    id: u128,
    session: SessionId,
    target: TargetId,
    ordinal: u64,
    millis: u64,
) -> EncodedFrame {
    EncodedFrame::new(
        CapturedFrame::new(
            FrameId::from_uuid(Uuid::from_u128(id)),
            session,
            target,
            CaptureOrdinal::new(ordinal).unwrap(),
            None,
            ObservedTime::from_nanos(millis * MS),
            at(millis),
            ImageFormat::Jpeg,
            PixelDimensions::new(1, 1).unwrap(),
            PixelDimensions::new(1, 1).unwrap(),
            DeviceScaleFactor::new(1.0).unwrap(),
            vec![],
        )
        .unwrap(),
        vec![ordinal as u8],
    )
    .unwrap()
}

#[tokio::test]
async fn all_anchor_forms_resolve_once_with_exact_implicit_window_and_ordering() {
    let fixture = Fixture::new().await;
    let anchors = [
        (
            TemporalRangeAnchor::SessionTime {
                scope: fixture.scope(),
                range: SessionRange::new(at(100), at(500)).unwrap(),
            },
            TemporalRangeAnchorKind::SessionTime,
        ),
        (
            TemporalRangeAnchor::WallClock {
                scope: fixture.scope(),
                start: SystemTime::UNIX_EPOCH + Duration::from_millis(100),
                end: SystemTime::UNIX_EPOCH + Duration::from_millis(500),
            },
            TemporalRangeAnchorKind::WallClock,
        ),
        (
            TemporalRangeAnchor::Interaction {
                scope: fixture.scope(),
                interaction_id: fixture.interaction_id,
                window: None,
            },
            TemporalRangeAnchorKind::Interaction,
        ),
        (
            TemporalRangeAnchor::LatestInteraction {
                session_id: fixture.session,
                target_id: fixture.target,
                window: None,
            },
            TemporalRangeAnchorKind::LatestInteraction,
        ),
        (
            TemporalRangeAnchor::Navigation {
                scope: fixture.scope(),
                navigation_id: fixture.navigation_id,
                window: Some(
                    krometrail_core::InteractionWindow::new(Duration::ZERO, Duration::ZERO)
                        .unwrap(),
                ),
            },
            TemporalRangeAnchorKind::Navigation,
        ),
        (
            TemporalRangeAnchor::Marker {
                scope: fixture.scope(),
                marker_id: fixture.marker_ids[0],
                window: Some(
                    krometrail_core::InteractionWindow::new(Duration::ZERO, Duration::ZERO)
                        .unwrap(),
                ),
            },
            TemporalRangeAnchorKind::Marker,
        ),
        (
            TemporalRangeAnchor::SourceFrame {
                scope: fixture.scope(),
                start_frame_id: fixture.frames[2].metadata().id(),
                end_frame_id: fixture.frames[6].metadata().id(),
            },
            TemporalRangeAnchorKind::SourceFrame,
        ),
    ];
    for (anchor, kind) in anchors {
        assert_eq!(fixture.resolve(anchor).await.anchor_kind, kind);
    }

    let interaction = fixture
        .resolve(TemporalRangeAnchor::Interaction {
            scope: fixture.scope(),
            interaction_id: fixture.interaction_id,
            window: None,
        })
        .await;
    assert_eq!(
        interaction.requested_range,
        SessionRange::new(at(50), at(600)).unwrap()
    );
    assert_eq!(interaction.resolved_range, interaction.requested_range);
    assert_eq!(
        interaction.options.implicit_interaction_window.before(),
        Duration::from_millis(150)
    );
    assert_eq!(
        interaction.options.implicit_interaction_window.after(),
        Duration::from_millis(250)
    );
    assert_eq!(interaction.gaps.len(), 1);

    let marker = fixture
        .resolve(TemporalRangeAnchor::Marker {
            scope: fixture.scope(),
            marker_id: fixture.marker_ids[0],
            window: Some(
                krometrail_core::InteractionWindow::new(Duration::ZERO, Duration::ZERO).unwrap(),
            ),
        })
        .await;
    assert_eq!(
        marker.frame_ids,
        vec![
            fixture.frames[8].metadata().id(),
            fixture.frames[9].metadata().id(),
        ]
    );
    assert_eq!(marker.marker_ids, fixture.marker_ids);

    let reject = TemporalQueryRequest::new(
        TemporalRangeAnchor::Interaction {
            scope: fixture.scope(),
            interaction_id: fixture.interaction_id,
            window: None,
        },
        RetentionPolicy::RequireComplete,
        CaptureGapPolicy::Reject,
    )
    .unwrap();
    assert_eq!(
        fixture.store.resolve_range(reject).await.unwrap_err().code,
        krometrail_core::ErrorCode::NotFound
    );
}

#[tokio::test]
async fn wrong_scope_and_retention_truth_are_explicit_and_contiguous_only() {
    let fixture = Fixture::new().await;
    for anchor in [
        TemporalRangeAnchor::Interaction {
            scope: AnchorScope::new(Some(fixture.session), Some(fixture.other_target)),
            interaction_id: fixture.interaction_id,
            window: None,
        },
        TemporalRangeAnchor::Navigation {
            scope: AnchorScope::new(Some(fixture.session), Some(fixture.other_target)),
            navigation_id: fixture.navigation_id,
            window: None,
        },
        TemporalRangeAnchor::Marker {
            scope: AnchorScope::new(Some(fixture.session), Some(fixture.other_target)),
            marker_id: fixture.marker_ids[0],
            window: None,
        },
        TemporalRangeAnchor::SourceFrame {
            scope: AnchorScope::new(Some(fixture.session), Some(fixture.other_target)),
            start_frame_id: fixture.frames[0].metadata().id(),
            end_frame_id: fixture.frames[1].metadata().id(),
        },
    ] {
        assert_eq!(
            fixture
                .store
                .resolve_range(TemporalQueryRequest::strict(anchor).unwrap())
                .await
                .unwrap_err()
                .code,
            krometrail_core::ErrorCode::InvalidInput
        );
    }

    fixture.simulate_eviction(SessionRange::new(at(0), at(200)).unwrap());
    let anchor = TemporalRangeAnchor::SessionTime {
        scope: fixture.scope(),
        range: SessionRange::new(at(0), at(500)).unwrap(),
    };
    assert_eq!(
        fixture
            .store
            .resolve_range(TemporalQueryRequest::strict(anchor.clone()).unwrap())
            .await
            .unwrap_err()
            .code,
        krometrail_core::ErrorCode::NotFound
    );
    let partial = fixture
        .store
        .resolve_range(
            TemporalQueryRequest::new(
                anchor,
                RetentionPolicy::AllowPartial,
                CaptureGapPolicy::Include,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        partial.resolved_range,
        SessionRange::new(at(250), at(500)).unwrap()
    );
    assert!(matches!(
        partial.retention_warnings.last(),
        Some(krometrail_core::RetentionWarning::EvictedRanges { .. })
    ));

    let mixed_natural = fixture
        .store
        .resolve_range(
            TemporalQueryRequest::new(
                TemporalRangeAnchor::Interaction {
                    scope: fixture.scope(),
                    interaction_id: fixture.interaction_id,
                    window: Some(
                        krometrail_core::InteractionWindow::new(
                            Duration::from_millis(300),
                            Duration::from_millis(600),
                        )
                        .unwrap(),
                    ),
                },
                RetentionPolicy::AllowPartial,
                CaptureGapPolicy::Include,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        mixed_natural.requested_range,
        SessionRange::new(at(0), at(950)).unwrap()
    );
    assert_eq!(
        mixed_natural.resolved_range,
        SessionRange::new(at(250), at(800)).unwrap()
    );
    assert!(
        mixed_natural
            .retention_warnings
            .iter()
            .any(|warning| matches!(
                warning,
                krometrail_core::RetentionWarning::PartiallyEvicted { .. }
            ))
    );
    assert!(
        mixed_natural
            .retention_warnings
            .iter()
            .any(|warning| matches!(
                warning,
                krometrail_core::RetentionWarning::PartiallyCaptured { .. }
            ))
    );
    assert_eq!(
        fixture
            .store
            .resolve_range(
                TemporalQueryRequest::strict(TemporalRangeAnchor::Interaction {
                    scope: fixture.scope(),
                    interaction_id: fixture.interaction_id,
                    window: Some(
                        krometrail_core::InteractionWindow::new(
                            Duration::from_millis(300),
                            Duration::from_millis(600),
                        )
                        .unwrap(),
                    ),
                })
                .unwrap(),
            )
            .await
            .unwrap_err()
            .code,
        krometrail_core::ErrorCode::NotFound
    );

    let evicted_only = fixture
        .store
        .resolve_range(
            TemporalQueryRequest::new(
                TemporalRangeAnchor::SessionTime {
                    scope: fixture.scope(),
                    range: SessionRange::new(at(0), at(200)).unwrap(),
                },
                RetentionPolicy::AllowPartial,
                CaptureGapPolicy::Include,
            )
            .unwrap(),
        )
        .await
        .unwrap_err();
    assert_eq!(evicted_only.code, krometrail_core::ErrorCode::NotFound);
    assert_eq!(evicted_only.retry, krometrail_core::RetryAdvice::Never);
    assert_eq!(evicted_only.recovery, None);

    let internal = Fixture::new().await;
    internal.simulate_eviction(SessionRange::new(at(300), at(400)).unwrap());
    assert_eq!(
        internal
            .store
            .resolve_range(
                TemporalQueryRequest::new(
                    TemporalRangeAnchor::SessionTime {
                        scope: internal.scope(),
                        range: SessionRange::new(at(0), at(700)).unwrap(),
                    },
                    RetentionPolicy::AllowPartial,
                    CaptureGapPolicy::Include,
                )
                .unwrap(),
            )
            .await
            .unwrap_err()
            .code,
        krometrail_core::ErrorCode::NotFound
    );

    let fully = Fixture::new().await;
    fully.simulate_eviction(SessionRange::new(at(0), at(800)).unwrap());
    assert_eq!(
        fully
            .store
            .resolve_range(
                TemporalQueryRequest::new(
                    TemporalRangeAnchor::SessionTime {
                        scope: fully.scope(),
                        range: SessionRange::new(at(0), at(800)).unwrap(),
                    },
                    RetentionPolicy::AllowPartial,
                    CaptureGapPolicy::Include,
                )
                .unwrap(),
            )
            .await
            .unwrap_err()
            .code,
        krometrail_core::ErrorCode::NotFound
    );

    let never = Fixture::new().await;
    let requested = SessionRange::new(at(700), at(1_000)).unwrap();
    let error = never
        .store
        .resolve_range(
            TemporalQueryRequest::new(
                TemporalRangeAnchor::SessionTime {
                    scope: never.scope(),
                    range: requested,
                },
                RetentionPolicy::AllowPartial,
                CaptureGapPolicy::Include,
            )
            .unwrap(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, krometrail_core::ErrorCode::NotFound);
    assert_eq!(error.context.session_id, Some(never.session));
    assert_eq!(error.context.target_id, Some(never.target));
    assert_eq!(error.context.range, Some(requested));
    assert_eq!(error.retry, krometrail_core::RetryAdvice::AfterRecovery);
    assert_eq!(
        error.recovery.unwrap().as_str(),
        "retry with a range contained by captured bounds: start_session_nanos=0, end_session_nanos=800000000"
    );
}

#[tokio::test]
async fn persisted_browser_sanitization_survives_storage_and_session_deletion_removes_anchors() {
    let fixture = Fixture::new().await;
    let locator = InteractionLocator::Element(ElementLocator::CssSelector(
        NonEmptyText::new("#private-field").unwrap(),
    ));
    let fill_secret = "tok_live_do_not_store";
    let fill = FillRequest::new(
        PageSelection::Target(fixture.target),
        locator.clone(),
        fill_secret,
        FillMode::Replace,
        false,
    )
    .unwrap();
    let dialog_secret = "prompt-secret";
    let dialog = HandleDialogRequest {
        target: PageSelection::Target(fixture.target),
        action: DialogAction::Accept {
            prompt_text: Some(NonEmptyText::new(dialog_secret).unwrap()),
        },
    };
    let upload = UploadFilesRequest::new(
        PageSelection::Target(fixture.target),
        locator,
        vec![ValidatedFilePath::new("/private/customer/secret/upload.txt").unwrap()],
    )
    .unwrap();

    let records = [
        InteractionRecord::new(
            InteractionId::from_uuid(Uuid::from_u128(400)),
            ObservationContext::new(fixture.session, fixture.target, 1, at(100), at(200)).unwrap(),
            at(120),
            at(180),
            BrowserOperationKind::Fill,
            fill.sanitize(),
            LocatorSummary::from_locator(fill.locator()),
            InteractionOutcome::Dispatched,
            Some(InteractionId::from_uuid(Uuid::from_u128(499))),
        )
        .unwrap(),
        InteractionRecord::new(
            InteractionId::from_uuid(Uuid::from_u128(401)),
            ObservationContext::new(fixture.session, fixture.target, 1, at(100), at(200)).unwrap(),
            at(120),
            at(180),
            BrowserOperationKind::HandleDialog,
            dialog.sanitize(),
            LocatorSummary::from_locator(dialog.locator()),
            InteractionOutcome::Dispatched,
            None,
        )
        .unwrap(),
        InteractionRecord::new(
            InteractionId::from_uuid(Uuid::from_u128(402)),
            ObservationContext::new(fixture.session, fixture.target, 1, at(100), at(200)).unwrap(),
            at(120),
            at(180),
            BrowserOperationKind::UploadFiles,
            upload.sanitize(),
            LocatorSummary::from_locator(upload.locator()),
            InteractionOutcome::Dispatched,
            None,
        )
        .unwrap(),
    ];
    for record in &records {
        fixture
            .store
            .append_operation_evidence(
                record.anchor().unwrap(),
                Some(record.clone()),
                ObservedTime::from_nanos(900 * MS),
                None,
            )
            .await
            .unwrap();
    }
    let connection = rusqlite::Connection::open(&fixture.database_path).unwrap();
    let persisted: String = connection
        .query_row(
            "SELECT group_concat(record_json, '') FROM interactions WHERE record_json IS NOT NULL",
            [],
            |row| row.get(0),
        )
        .unwrap();
    for secret in [
        fill_secret,
        dialog_secret,
        "/private",
        "customer",
        "secret/",
    ] {
        assert!(!persisted.contains(secret));
    }
    for permitted in [
        "value_length",
        "prompt_text_length",
        "upload.txt",
        "parent_batch",
    ] {
        assert!(persisted.contains(permitted));
    }
    drop(connection);

    fixture.store.delete_session(fixture.session).await.unwrap();
    assert_eq!(
        fixture
            .store
            .resolve_range(
                TemporalQueryRequest::strict(TemporalRangeAnchor::Interaction {
                    scope: fixture.scope(),
                    interaction_id: fixture.interaction_id,
                    window: None,
                })
                .unwrap(),
            )
            .await
            .unwrap_err()
            .code,
        krometrail_core::ErrorCode::NotFound
    );
    assert!(
        fixture
            .index
            .frame_availability(fixture.session, fixture.target)
            .await
            .unwrap()
            .evicted_ranges
            .is_empty()
    );
}
