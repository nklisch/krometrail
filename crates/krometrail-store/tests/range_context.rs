use std::{num::NonZeroU64, sync::Arc, time::Duration};

use krometrail_core::{
    BrowserEvent, BrowserEventBatch, BrowserEventClass, BrowserEventCollectionGap,
    BrowserEventFilter, BrowserEventGapReason, BrowserEventId, BrowserEventOrdinal,
    BrowserEventPayload, BrowserEventSelection, BrowserEventSeverity, BrowserEventSink, CaptureGap,
    CaptureGapPolicy, CaptureGapReason, CaptureOrdinal, CaptureQualityWarning, CaptureStatistics,
    CaptureStreamState, CaptureTimingSummary, CaptureWarning, CapturedFrame, ConsoleArgumentType,
    ConsoleEvent, ConsoleEventSource, ConsoleLevel, ConsoleMethod, DeviceScaleFactor,
    DiskBudgetBytes, EncodedFrame, ErrorCode, EventQueryWarning, EventRedactor, FrameId,
    ImageFormat, ObservedTime, RangeResolutionOptions, RecordingSink, ResolvedRange,
    RetentionPolicy, RetentionRange, RetentionStore, RetentionWarning, SessionId, SessionRange,
    SessionTime, TargetCaptureStatus, TargetId, TemporalContextQuery, TemporalContextRequest,
    TemporalRangeAnchorKind,
};
use krometrail_store::{
    IndexStoreConfig, RecordingStore, RotationConfig, SegmentStoreConfig, SegmentWriter,
    SqliteIndex,
};
use rusqlite::{Connection, params};
use tempfile::TempDir;
use uuid::Uuid;

struct Fixture {
    directory: TempDir,
    index: Arc<SqliteIndex>,
    writer: Arc<SegmentWriter>,
    store: Arc<RecordingStore>,
    session: SessionId,
    target: TargetId,
    frames: Vec<EncodedFrame>,
}

impl Fixture {
    async fn new(times: &[u64]) -> Self {
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
        let store = Arc::new(
            RecordingStore::new(Arc::clone(&writer), Arc::clone(&index), store_test_clock())
                .unwrap(),
        );
        let session = SessionId::from_uuid(Uuid::from_u128(1));
        let target = TargetId::from_uuid(Uuid::from_u128(2));
        let mut frames = Vec::new();
        for (position, time) in times.iter().copied().enumerate() {
            let ordinal = u64::try_from(position + 1).unwrap();
            let warnings = match position {
                0 => vec![CaptureWarning::MissingSourceTime],
                1 => vec![
                    CaptureWarning::MissingSourceTime,
                    CaptureWarning::SourceTimestampRounded,
                ],
                _ => vec![],
            };
            let frame = EncodedFrame::new(
                CapturedFrame::new(
                    FrameId::from_uuid(Uuid::from_u128(100 + u128::from(ordinal))),
                    session,
                    target,
                    CaptureOrdinal::new(ordinal).unwrap(),
                    None,
                    ObservedTime::from_nanos(time),
                    SessionTime::from_nanos(time),
                    ImageFormat::Jpeg,
                    krometrail_core::PixelDimensions::new(2, 2).unwrap(),
                    krometrail_core::PixelDimensions::new(2, 2).unwrap(),
                    DeviceScaleFactor::new(1.0).unwrap(),
                    warnings,
                )
                .unwrap(),
                vec![position as u8 + 1],
            )
            .unwrap();
            store.append_frame(frame.clone()).await.unwrap();
            frames.push(frame);
        }
        Self {
            directory,
            index,
            writer,
            store,
            session,
            target,
            frames,
        }
    }

    fn resolved(&self, gaps: Vec<CaptureGap>) -> ResolvedRange {
        ResolvedRange::new(
            self.session,
            self.target,
            TemporalRangeAnchorKind::SessionTime,
            SessionRange::new(
                self.frames.first().unwrap().metadata().session_time(),
                self.frames.last().unwrap().metadata().session_time(),
            )
            .unwrap(),
            SessionRange::new(
                self.frames.first().unwrap().metadata().session_time(),
                self.frames.last().unwrap().metadata().session_time(),
            )
            .unwrap(),
            self.frames
                .iter()
                .map(|frame| frame.metadata().id())
                .collect(),
            vec![],
            vec![],
            vec![],
            gaps,
            vec![],
            RangeResolutionOptions {
                retention: RetentionPolicy::RequireComplete,
                capture_gaps: CaptureGapPolicy::Include,
                ..RangeResolutionOptions::DEFAULT
            },
        )
        .unwrap()
    }

    async fn append_events(&self, events: Vec<BrowserEvent>) {
        for chunk in events.chunks(128) {
            self.store
                .append_event_batch(BrowserEventBatch::new(self.session, chunk.to_vec()).unwrap())
                .await
                .unwrap();
        }
    }
}

fn console_event(
    fixture: &Fixture,
    id: u128,
    ordinal: u64,
    time: u64,
    level: ConsoleLevel,
) -> BrowserEvent {
    let severity = match level {
        ConsoleLevel::Debug => BrowserEventSeverity::Debug,
        ConsoleLevel::Info => BrowserEventSeverity::Info,
        ConsoleLevel::Warning => BrowserEventSeverity::Warning,
        ConsoleLevel::Error => BrowserEventSeverity::Error,
    };
    BrowserEvent::new(
        BrowserEventId::from_uuid(Uuid::from_u128(id)),
        fixture.session,
        fixture.target,
        1,
        BrowserEventOrdinal::new(ordinal).unwrap(),
        SessionTime::from_nanos(time),
        None,
        ObservedTime::from_nanos(time),
        severity,
        BrowserEventPayload::ConsoleMessage(ConsoleEvent::new(
            ConsoleEventSource::Runtime,
            level,
            ConsoleMethod::Log,
            vec![ConsoleArgumentType::String],
            Some(EventRedactor.text("safe context")),
            vec![],
        )),
    )
    .unwrap()
}

fn status_event(
    fixture: &Fixture,
    id: u128,
    ordinal: u64,
    time: u64,
    generation: u64,
    state: CaptureStreamState,
) -> BrowserEvent {
    let severity = match state {
        CaptureStreamState::Starting | CaptureStreamState::Capturing => BrowserEventSeverity::Debug,
        CaptureStreamState::PausedBudget | CaptureStreamState::Suspended => {
            BrowserEventSeverity::Warning
        }
        CaptureStreamState::Failed => BrowserEventSeverity::Error,
        _ => BrowserEventSeverity::Info,
    };
    let status = TargetCaptureStatus::new(
        fixture.target,
        generation,
        state,
        CaptureStatistics::default(),
        1,
        0,
        None,
        CaptureTimingSummary::empty(),
        CaptureTimingSummary::empty(),
        krometrail_core::EveryNthFrame::default(),
        None,
    )
    .unwrap();
    BrowserEvent::new(
        BrowserEventId::from_uuid(Uuid::from_u128(id)),
        fixture.session,
        fixture.target,
        generation,
        BrowserEventOrdinal::new(ordinal).unwrap(),
        SessionTime::from_nanos(time),
        None,
        ObservedTime::from_nanos(time),
        severity,
        BrowserEventPayload::CaptureStatusChanged(status),
    )
    .unwrap()
}

fn collection_gap_event(fixture: &Fixture, ordinal: u64, time: u64) -> BrowserEvent {
    let gap = BrowserEventCollectionGap::new(
        BrowserEventGapReason::QueueSaturated,
        Some(BrowserEventClass::Console),
        SessionRange::new(SessionTime::from_nanos(time), SessionTime::from_nanos(time)).unwrap(),
        BrowserEventOrdinal::new(ordinal).unwrap(),
        BrowserEventOrdinal::new(ordinal).unwrap(),
        NonZeroU64::new(1).unwrap(),
        false,
    )
    .unwrap();
    BrowserEvent::new(
        BrowserEventId::from_uuid(Uuid::from_u128(10_000 + u128::from(ordinal))),
        fixture.session,
        fixture.target,
        1,
        BrowserEventOrdinal::new(ordinal).unwrap(),
        SessionTime::from_nanos(time),
        None,
        ObservedTime::from_nanos(time),
        BrowserEventSeverity::Warning,
        BrowserEventPayload::CollectionGap(gap),
    )
    .unwrap()
}

fn gap(fixture: &Fixture, id: u128, start: u64, end: u64, estimate: Option<u64>) -> CaptureGap {
    CaptureGap::new(
        krometrail_core::GapId::from_uuid(Uuid::from_u128(id)),
        fixture.session,
        fixture.target,
        SessionRange::new(SessionTime::from_nanos(start), SessionTime::from_nanos(end)).unwrap(),
        ObservedTime::from_nanos(200),
        CaptureGapReason::CaptureStopped,
        estimate.and_then(NonZeroU64::new),
        None,
    )
    .unwrap()
}

#[tokio::test]
async fn context_derives_exact_capture_quality_gaps_warnings_and_status() {
    let fixture = Fixture::new(&[0, 0, 10, 30, 130]).await;
    fixture
        .append_events(vec![
            status_event(&fixture, 1_000, 1, 0, 1, CaptureStreamState::Capturing),
            status_event(&fixture, 1_001, 2, 50, 2, CaptureStreamState::Suspended),
        ])
        .await;
    let gaps = vec![
        gap(&fixture, 201, 5, 15, Some(2)),
        gap(&fixture, 202, 10, 20, None),
        gap(&fixture, 203, 25, 25, Some(1)),
    ];
    let range = fixture.resolved(gaps.clone());
    let request = TemporalContextRequest::new(
        range.clone(),
        None,
        BrowserEventFilter::new(
            vec![BrowserEventClass::Console],
            BrowserEventSeverity::Error,
        )
        .unwrap(),
        BrowserEventSelection::compact_default(),
        vec![],
    )
    .unwrap();
    let context = fixture.store.context(request).await.unwrap();

    assert_eq!(context.range, range);
    let quality = context.capture_quality;
    assert_eq!(quality.requested_range, range.requested_range);
    assert_eq!(quality.retained_range, range.resolved_range);
    assert_eq!(quality.frame_count, 5);
    assert_eq!(
        quality.first_frame.frame_id,
        fixture.frames[0].metadata().id()
    );
    assert_eq!(
        quality.last_frame.frame_id,
        fixture.frames[4].metadata().id()
    );
    let cadence = quality.cadence.unwrap();
    assert_eq!(
        (
            cadence.interval_count,
            cadence.min_nanos,
            cadence.median_nanos,
            cadence.p95_nanos,
            cadence.max_nanos,
        ),
        (4, 0, 10, 100, 100)
    );
    assert_eq!(quality.frame_warnings.len(), 2);
    assert_eq!(quality.frame_warnings[0].frame_count, 2);
    assert_eq!(quality.frame_warnings[1].frame_count, 1);
    assert_eq!(quality.gaps, gaps);
    assert_eq!(quality.gap_summary.gap_count, 3);
    assert_eq!(quality.gap_summary.covered_duration_nanos, 15);
    assert_eq!(quality.gap_summary.known_missing_frames, 3);
    assert!(quality.gap_summary.has_unknown_missing_estimate);
    assert!(quality.warnings.is_empty());
    assert_eq!(
        quality.capture_status.at_range_start.unwrap().state,
        CaptureStreamState::Capturing
    );
    assert_eq!(
        quality.capture_status.at_range_end.unwrap().state,
        CaptureStreamState::Suspended
    );
    assert_eq!(quality.capture_status.transitions.len(), 2);
}

#[tokio::test]
async fn compact_selection_ranks_focus_ties_deduplicates_and_presents_chronologically() {
    let fixture = Fixture::new(&[0, 100]).await;
    fixture
        .append_events(vec![
            console_event(&fixture, 301, 1, 20, ConsoleLevel::Info),
            console_event(&fixture, 302, 2, 40, ConsoleLevel::Info),
            console_event(&fixture, 303, 3, 60, ConsoleLevel::Info),
            console_event(&fixture, 304, 4, 80, ConsoleLevel::Info),
        ])
        .await;
    let request = TemporalContextRequest::new(
        fixture.resolved(vec![]),
        None,
        BrowserEventFilter::new(vec![BrowserEventClass::Console], BrowserEventSeverity::Info)
            .unwrap(),
        BrowserEventSelection::compact(2).unwrap(),
        vec![SessionTime::from_nanos(50)],
    )
    .unwrap();
    let context = fixture.store.context(request).await.unwrap();
    assert_eq!(
        serde_json::from_str::<krometrail_core::TemporalContext>(
            &serde_json::to_string(&context).unwrap()
        )
        .unwrap(),
        context
    );
    assert_eq!(context.browser_events.matched_count, 4);
    assert_eq!(context.browser_events.events.len(), 2);
    assert_eq!(
        context
            .browser_events
            .events
            .iter()
            .map(|selected| selected.event.session_time().as_nanos())
            .collect::<Vec<_>>(),
        [40, 60]
    );
    for selected in &context.browser_events.events {
        match selected.reason {
            krometrail_core::BrowserEventSelectionReason::CompactCorrelation {
                nearest_focus_distance_nanos,
                ..
            } => assert_eq!(nearest_focus_distance_nanos, Some(10)),
            _ => panic!("compact result must carry a correlation reason"),
        }
    }
    assert!(context.browser_events.warnings.iter().any(|warning| {
        matches!(
            warning,
            EventQueryWarning::Truncated {
                matched_count: 4,
                returned_count: 2
            }
        )
    }));
}

#[tokio::test]
async fn chronological_cursor_pages_preserve_equal_time_events_and_filter_scope() {
    let fixture = Fixture::new(&[0, 100]).await;
    fixture
        .append_events(
            (1..=4)
                .map(|ordinal| {
                    console_event(
                        &fixture,
                        400 + u128::from(ordinal),
                        ordinal,
                        50,
                        ConsoleLevel::Info,
                    )
                })
                .collect(),
        )
        .await;
    let range = fixture.resolved(vec![]);
    let filter =
        BrowserEventFilter::new(vec![BrowserEventClass::Console], BrowserEventSeverity::Info)
            .unwrap();
    let first = fixture
        .store
        .context(
            TemporalContextRequest::new(
                range.clone(),
                None,
                filter.clone(),
                BrowserEventSelection::chronological(2, None).unwrap(),
                vec![],
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let cursor = first.browser_events.next_cursor.clone().unwrap();
    assert_eq!(
        first
            .browser_events
            .events
            .iter()
            .map(|item| item.event.ordinal().get())
            .collect::<Vec<_>>(),
        [1, 2]
    );
    let second = fixture
        .store
        .context(
            TemporalContextRequest::new(
                range,
                None,
                filter,
                BrowserEventSelection::chronological(2, Some(cursor)).unwrap(),
                vec![],
            )
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        second
            .browser_events
            .events
            .iter()
            .map(|item| item.event.ordinal().get())
            .collect::<Vec<_>>(),
        [3, 4]
    );
    assert!(second.browser_events.next_cursor.is_none());
}

#[tokio::test]
async fn collection_loss_and_unavailable_ranges_ignore_filter_and_report_bounds() {
    let fixture = Fixture::new(&[0, 2_000]).await;
    fixture
        .append_events(
            (1..=1_001)
                .map(|ordinal| collection_gap_event(&fixture, ordinal, ordinal))
                .collect(),
        )
        .await;

    let connection = Connection::open(fixture.directory.path().join("index.sqlite3")).unwrap();
    let transaction = connection.unchecked_transaction().unwrap();
    for value in 0_u64..1_000 {
        transaction
            .execute(
                "INSERT INTO browser_event_unavailable_ranges(\
                    session_id,target_id,start_time_be,end_time_be,first_ordinal_be,last_ordinal_be,event_count_be,reason\
                 ) VALUES (?1,?2,?3,?3,NULL,NULL,?4,'retention_evicted')",
                params![
                    fixture.session.as_uuid().as_bytes().to_vec(),
                    fixture.target.as_uuid().as_bytes().to_vec(),
                    value.to_be_bytes().to_vec(),
                    1_u64.to_be_bytes().to_vec(),
                ],
            )
            .unwrap();
    }
    transaction.commit().unwrap();
    drop(connection);

    let request = TemporalContextRequest::new(
        fixture.resolved(vec![]),
        None,
        BrowserEventFilter::new(
            vec![BrowserEventClass::Console],
            BrowserEventSeverity::Error,
        )
        .unwrap(),
        BrowserEventSelection::compact_default(),
        vec![],
    )
    .unwrap();
    let context = fixture.store.context(request).await.unwrap();
    assert_eq!(context.browser_events.matched_count, 0);
    assert_eq!(context.browser_events.collection_gaps.len(), 1_000);
    assert_eq!(context.browser_events.unavailable_ranges.len(), 1_000);
    assert!(context.browser_events.warnings.iter().any(|warning| {
        matches!(
            warning,
            EventQueryWarning::CollectionEvidenceTruncated { .. }
        )
    }));
    assert!(
        context
            .browser_events
            .warnings
            .iter()
            .any(|warning| { matches!(warning, EventQueryWarning::UnavailableRangesTruncated) })
    );
    assert!(
        context
            .capture_quality
            .warnings
            .contains(&CaptureQualityWarning::CaptureStatusUnavailable)
    );
}

#[tokio::test]
async fn retained_frame_pin_survives_event_eviction_and_context_reports_unavailability() {
    let fixture = Fixture::new(&[0]).await;
    let range = fixture.resolved(vec![]);
    fixture
        .append_events(vec![console_event(
            &fixture,
            601,
            1,
            0,
            ConsoleLevel::Error,
        )])
        .await;
    fixture.store.flush(fixture.session).await.unwrap();
    fixture
        .store
        .pin_range(RetentionRange {
            session_id: fixture.session,
            target_id: fixture.target,
            range: range.resolved_range,
        })
        .await
        .unwrap();
    let status = fixture.store.status().await.unwrap();
    let budget = DiskBudgetBytes::new(status.usage.total_bytes().unwrap() - 1).unwrap();
    let Fixture {
        directory: _directory,
        index,
        writer,
        store,
        ..
    } = fixture;
    drop(store);
    let store = RecordingStore::with_budget(writer, index, budget, store_test_clock()).unwrap();
    let _ = store.enforce_budget().await.unwrap();
    let context = store
        .context(TemporalContextRequest::compact(range, vec![]).unwrap())
        .await
        .unwrap();
    assert_eq!(context.capture_quality.frame_count, 1);
    assert!(context.browser_events.events.is_empty());
    assert_eq!(context.browser_events.unavailable_ranges.len(), 1);
    assert!(
        context
            .capture_quality
            .warnings
            .contains(&CaptureQualityWarning::CaptureStatusUnavailable)
    );
}

#[tokio::test]
async fn missing_status_warns_and_metadata_corruption_fails_source_safely() {
    let fixture = Fixture::new(&[0, 10]).await;
    let range = fixture.resolved(vec![]);
    let context = fixture
        .store
        .context(TemporalContextRequest::compact(range.clone(), vec![]).unwrap())
        .await
        .unwrap();
    assert!(
        context
            .capture_quality
            .warnings
            .contains(&CaptureQualityWarning::CaptureStatusMissing)
    );

    let connection = Connection::open(fixture.directory.path().join("index.sqlite3")).unwrap();
    connection
        .execute(
            "UPDATE frames SET warnings_json='\"private/path/token\"' WHERE frame_id=?1",
            params![
                fixture.frames[0]
                    .metadata()
                    .id()
                    .as_uuid()
                    .as_bytes()
                    .to_vec()
            ],
        )
        .unwrap();
    drop(connection);
    let error = fixture
        .store
        .context(TemporalContextRequest::compact(range, vec![]).unwrap())
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::PersistenceFailed);
    assert!(error.context.session_id.is_some());
    assert!(error.context.target_id.is_some());
    assert!(error.context.range.is_some());
    assert!(error.recovery.is_some());
    assert!(!error.message.as_str().contains("private"));
    assert!(!error.message.as_str().contains("frames"));
}

#[tokio::test]
async fn compact_priority_precedes_focus_distance_and_event_corruption_is_source_safe() {
    let fixture = Fixture::new(&[0, 100]).await;
    let error_event = console_event(&fixture, 701, 1, 0, ConsoleLevel::Error);
    fixture
        .append_events(vec![
            error_event.clone(),
            console_event(&fixture, 702, 2, 50, ConsoleLevel::Info),
        ])
        .await;
    let request = TemporalContextRequest::new(
        fixture.resolved(vec![]),
        None,
        BrowserEventFilter::new(vec![BrowserEventClass::Console], BrowserEventSeverity::Info)
            .unwrap(),
        BrowserEventSelection::compact(1).unwrap(),
        vec![SessionTime::from_nanos(50)],
    )
    .unwrap();
    let context = fixture.store.context(request.clone()).await.unwrap();
    assert_eq!(
        context.browser_events.events[0].event.id(),
        error_event.id()
    );

    let connection = Connection::open(fixture.directory.path().join("index.sqlite3")).unwrap();
    connection
        .execute(
            "UPDATE browser_events SET kind='private-corrupt-kind' WHERE event_id=?1",
            params![error_event.id().as_uuid().as_bytes().to_vec()],
        )
        .unwrap();
    drop(connection);
    let error = fixture.store.context(request).await.unwrap_err();
    assert_eq!(error.code, ErrorCode::PersistenceFailed);
    assert!(error.context.range.is_some());
    assert!(error.recovery.is_some());
    assert!(!error.message.as_str().contains("private"));
    assert!(!error.message.as_str().contains("browser_events"));
}

#[tokio::test]
async fn status_transition_cap_and_partial_retention_are_explicit() {
    let fixture = Fixture::new(&[10, 190]).await;
    fixture
        .append_events(
            (1..=128)
                .map(|ordinal| {
                    status_event(
                        &fixture,
                        8_000 + u128::from(ordinal),
                        ordinal,
                        ordinal + 9,
                        ordinal,
                        CaptureStreamState::Capturing,
                    )
                })
                .collect(),
        )
        .await;
    let requested = SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(200)).unwrap();
    let retained =
        SessionRange::new(SessionTime::from_nanos(10), SessionTime::from_nanos(190)).unwrap();
    let retention_warning = RetentionWarning::PartiallyEvicted {
        requested,
        retained,
    };
    let range = ResolvedRange::new(
        fixture.session,
        fixture.target,
        TemporalRangeAnchorKind::SessionTime,
        requested,
        retained,
        fixture
            .frames
            .iter()
            .map(|frame| frame.metadata().id())
            .collect(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![retention_warning.clone()],
        RangeResolutionOptions {
            retention: RetentionPolicy::AllowPartial,
            capture_gaps: CaptureGapPolicy::Include,
            ..RangeResolutionOptions::DEFAULT
        },
    )
    .unwrap();
    let context = fixture
        .store
        .context(TemporalContextRequest::compact(range, vec![]).unwrap())
        .await
        .unwrap();
    assert_eq!(
        context.capture_quality.retention_warnings,
        [retention_warning]
    );
    assert_eq!(
        context.capture_quality.capture_status.transitions.len(),
        128
    );
    assert!(
        context
            .capture_quality
            .warnings
            .contains(&CaptureQualityWarning::CaptureStatusTruncated)
    );
}

#[tokio::test]
async fn frame_order_projection_mismatch_is_rejected() {
    let fixture = Fixture::new(&[0, 10]).await;
    let range = fixture.resolved(vec![]);
    let connection = Connection::open(fixture.directory.path().join("index.sqlite3")).unwrap();
    connection
        .execute(
            "UPDATE frames SET capture_ordinal_be=?1 WHERE frame_id=?2",
            params![
                99_u64.to_be_bytes().to_vec(),
                fixture.frames[0]
                    .metadata()
                    .id()
                    .as_uuid()
                    .as_bytes()
                    .to_vec()
            ],
        )
        .unwrap();
    drop(connection);
    let error = fixture
        .store
        .context(TemporalContextRequest::compact(range, vec![]).unwrap())
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::PersistenceFailed);
    assert_eq!(
        error.message.as_str(),
        "resolved source frame metadata is inconsistent"
    );
}

#[tokio::test]
async fn concurrent_session_deletion_cannot_return_partially_invalid_context() {
    let fixture = Fixture::new(&(0..200).collect::<Vec<_>>()).await;
    let request = TemporalContextRequest::compact(fixture.resolved(vec![]), vec![]).unwrap();
    let query_request = request.clone();
    let query_store = Arc::clone(&fixture.store);
    let query = tokio::spawn(async move { query_store.context(query_request).await });
    tokio::task::yield_now().await;
    let delete_store = Arc::clone(&fixture.store);
    let session = fixture.session;
    let deletion = tokio::spawn(async move { delete_store.delete_session(session).await });
    let query_result = query.await.unwrap();
    deletion.await.unwrap().unwrap();
    match query_result {
        Ok(context) => {
            assert_eq!(context.capture_quality.frame_count, 200);
            assert_eq!(context.range.frame_ids.len(), 200);
        }
        Err(error) => assert_eq!(error.code, ErrorCode::NotFound),
    }
    let deleted_error = fixture.store.context(request).await.unwrap_err();
    assert_eq!(deleted_error.code, ErrorCode::NotFound);
    assert!(deleted_error.context.session_id.is_some());
    assert!(deleted_error.context.target_id.is_some());
    assert!(deleted_error.context.range.is_some());
    assert!(deleted_error.recovery.is_some());
}

#[tokio::test]
async fn store_serves_capture_quality_without_loading_browser_events() {
    let fixture = Fixture::new(&[0, 10, 30]).await;
    let range = fixture.resolved(vec![]);
    let quality = fixture
        .store
        .capture_quality(range.clone())
        .await
        .expect("production store must serve capture-quality-only queries");
    assert_eq!(quality.requested_range, range.requested_range);
    assert_eq!(quality.retained_range, range.resolved_range);
    assert_eq!(quality.frame_count, 3);
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
