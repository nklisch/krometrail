use std::{sync::Arc, time::Duration};

use krometrail_core::{
    CaptureOrdinal, CapturedFrame, DeviceScaleFactor, EncodedFrame, FrameId, FrameSource,
    ImageFormat, ObservedTime, PinProtectionScope, ProgressiveEvidenceStore,
    RangeResolutionOptions, RecordingSink, ResolvedRange, RetentionPinRequest, RetentionStore,
    SessionId, SessionRange, SessionTime, SourceFrameSelection, SourceFramesRequest,
    SourceReadLimitsRequest, TargetId, TemporalRangeAnchorKind,
};
use krometrail_store::{
    IndexStoreConfig, RecordingStore, RotationConfig, SegmentStoreConfig, SegmentWriter,
    SqliteIndex,
};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use tokio::sync::Barrier;
use uuid::Uuid;

struct Fixture {
    _directory: TempDir,
    store: Arc<RecordingStore>,
    session_id: SessionId,
    target_id: TargetId,
    frame_ids: Vec<FrameId>,
    bytes: Vec<Vec<u8>>,
}

async fn fixture(rotation_size: u64) -> Fixture {
    fixture_with_count(rotation_size, 3).await
}

async fn fixture_with_count(rotation_size: u64, count: usize) -> Fixture {
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
            rotation: RotationConfig {
                max_duration: Duration::from_secs(60),
                max_size: rotation_size,
            },
        })
        .unwrap(),
    );
    let store = Arc::new(RecordingStore::new(writer, index, store_test_clock()).unwrap());
    let session_id = SessionId::from_uuid(Uuid::from_u128(1));
    let target_id = TargetId::from_uuid(Uuid::from_u128(2));
    let frame_ids = (0..count)
        .map(|position| frame_id(3 + position as u128))
        .collect::<Vec<_>>();
    let bytes = (0..count)
        .map(|position| format!("source-frame-{position}").into_bytes())
        .collect::<Vec<_>>();
    for (position, (id, payload)) in frame_ids.iter().zip(&bytes).enumerate() {
        let ordinal = u64::try_from(position + 1).unwrap();
        store
            .append_frame(
                EncodedFrame::new(
                    CapturedFrame::new(
                        *id,
                        session_id,
                        target_id,
                        CaptureOrdinal::new(ordinal).unwrap(),
                        None,
                        ObservedTime::from_nanos(ordinal),
                        SessionTime::from_nanos(ordinal),
                        ImageFormat::Jpeg,
                        krometrail_core::PixelDimensions::new(1, 1).unwrap(),
                        krometrail_core::PixelDimensions::new(1, 1).unwrap(),
                        DeviceScaleFactor::new(1.0).unwrap(),
                        vec![],
                    )
                    .unwrap(),
                    payload.clone(),
                )
                .unwrap(),
            )
            .await
            .unwrap();
    }
    Fixture {
        _directory: directory,
        store,
        session_id,
        target_id,
        frame_ids,
        bytes,
    }
}

fn frame_id(value: u128) -> FrameId {
    FrameId::from_uuid(Uuid::from_u128(value))
}

fn resolved(fixture: &Fixture, ids: Vec<FrameId>, start: u64, end: u64) -> ResolvedRange {
    let range =
        SessionRange::new(SessionTime::from_nanos(start), SessionTime::from_nanos(end)).unwrap();
    ResolvedRange::new(
        fixture.session_id,
        fixture.target_id,
        TemporalRangeAnchorKind::SessionTime,
        range,
        range,
        ids,
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        RangeResolutionOptions::DEFAULT,
    )
    .unwrap()
}

fn source_request(
    range: ResolvedRange,
    selection: SourceFrameSelection,
    item_bytes: u64,
    total_bytes: u64,
) -> SourceFramesRequest {
    SourceFramesRequest::new(
        range,
        selection,
        SourceReadLimitsRequest::new(3, item_bytes, total_bytes).unwrap(),
    )
    .unwrap()
}

#[tokio::test]
async fn coherent_source_lists_and_fetches_preserve_exact_order_hashes_and_bounds() {
    let fixture = fixture(1).await;
    let composite: &dyn ProgressiveEvidenceStore = fixture.store.as_ref();
    let _ = composite;
    let range = resolved(&fixture, fixture.frame_ids.clone(), 1, 3);
    let listed = fixture
        .store
        .list_source_frames(source_request(
            range.clone(),
            SourceFrameSelection::ResolvedOrder,
            1024,
            4096,
        ))
        .await
        .unwrap();
    assert_eq!(
        listed
            .frames
            .iter()
            .map(|handle| handle.frame_id)
            .collect::<Vec<_>>(),
        fixture.frame_ids
    );
    for (position, (handle, bytes)) in listed.frames.iter().zip(&fixture.bytes).enumerate() {
        assert_eq!(handle.request_position, position as u32);
        assert_eq!(handle.resolved_position, position as u32);
        assert_eq!(handle.encoded_byte_len, bytes.len() as u64);
        assert_eq!(
            handle.content_sha256.as_bytes(),
            &<[u8; 32]>::from(Sha256::digest(bytes))
        );
        assert_eq!(handle.scope.session_id, fixture.session_id);
        assert_eq!(handle.scope.target_id, fixture.target_id);
    }

    let requested = vec![fixture.frame_ids[2], fixture.frame_ids[0]];
    let fetched = fixture
        .store
        .fetch_source_frames(source_request(
            range.clone(),
            SourceFrameSelection::Ids(requested.clone()),
            1024,
            4096,
        ))
        .await
        .unwrap();
    assert_eq!(
        fetched
            .frames
            .iter()
            .map(|read| read.handle.frame_id)
            .collect::<Vec<_>>(),
        requested
    );
    assert_eq!(fetched.frames[0].handle.resolved_position, 2);
    assert_eq!(fetched.frames[1].handle.resolved_position, 0);
    assert_eq!(fetched.frames[0].encoded_bytes(), fixture.bytes[2]);
    assert_eq!(fetched.frames[1].encoded_bytes(), fixture.bytes[0]);

    let mut wrong_scope = range.clone();
    wrong_scope.target_id = TargetId::from_uuid(Uuid::from_u128(999));
    assert_eq!(
        fixture
            .store
            .fetch_source_frames(source_request(
                wrong_scope,
                SourceFrameSelection::Ids(vec![fixture.frame_ids[0]]),
                1024,
                4096,
            ))
            .await
            .unwrap_err()
            .code,
        krometrail_core::ErrorCode::NotFound
    );

    let item_limit = u64::try_from(fixture.bytes[0].len() - 1).unwrap();
    assert_eq!(
        fixture
            .store
            .fetch_source_frames(source_request(
                range.clone(),
                SourceFrameSelection::Ids(vec![fixture.frame_ids[0]]),
                item_limit,
                4096,
            ))
            .await
            .unwrap_err()
            .code,
        krometrail_core::ErrorCode::ResourceLimitExceeded
    );
    let total_limit = u64::try_from(fixture.bytes[0].len() + fixture.bytes[1].len() - 1).unwrap();
    assert_eq!(
        fixture
            .store
            .list_source_frames(source_request(
                range,
                SourceFrameSelection::Ids(vec![fixture.frame_ids[0], fixture.frame_ids[1]]),
                fixture.bytes[1].len() as u64,
                total_limit,
            ))
            .await
            .unwrap_err()
            .code,
        krometrail_core::ErrorCode::ResourceLimitExceeded
    );
}

#[tokio::test]
async fn source_listing_pagination_reaches_tail_without_advertising_empty_pages() {
    let fixture = fixture_with_count(1, 367).await;
    let range = resolved(&fixture, fixture.frame_ids.clone(), 1, 367);
    let limits = SourceReadLimitsRequest::new(3, 1024, 4096).unwrap();
    let mut offset = 0;
    let mut listed_ids = Vec::new();
    loop {
        let page = fixture
            .store
            .list_source_frames(
                SourceFramesRequest::new_with_offset(
                    range.clone(),
                    SourceFrameSelection::ResolvedOrder,
                    offset,
                    limits,
                )
                .unwrap(),
            )
            .await
            .unwrap();
        listed_ids.extend(page.frames.iter().map(|frame| frame.frame_id));
        match page.next_offset {
            Some(next) => {
                assert_eq!(next, offset + 3);
                offset = next;
            }
            None => {
                assert_eq!(page.frames.len(), 1);
                break;
            }
        }
    }
    assert_eq!(listed_ids, fixture.frame_ids);

    let boundary = fixture_with_count(1, 6).await;
    let range = resolved(&boundary, boundary.frame_ids.clone(), 1, 6);
    let first = boundary
        .store
        .list_source_frames(
            SourceFramesRequest::new_with_offset(
                range.clone(),
                SourceFrameSelection::ResolvedOrder,
                0,
                limits,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.frames.len(), 3);
    assert_eq!(first.next_offset, Some(3));
    let last = boundary
        .store
        .list_source_frames(
            SourceFramesRequest::new_with_offset(
                range,
                SourceFrameSelection::ResolvedOrder,
                3,
                limits,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(last.frames.len(), 3);
    assert_eq!(last.next_offset, None);
}

#[tokio::test]
async fn resolved_pins_flush_open_segments_report_overreach_overlap_and_idempotence() {
    // One large rotation limit deliberately leaves the final segment open until pin.
    let fixture = fixture(1_000_000).await;
    let full =
        RetentionPinRequest::from_resolved(&resolved(&fixture, fixture.frame_ids.clone(), 1, 3))
            .unwrap();
    let first = fixture
        .store
        .pin_resolved_range(full.clone())
        .await
        .unwrap();
    assert!(first.changed);
    assert!(first.state.exact_pin_active);
    assert_eq!(
        first.state.protection_scope,
        PinProtectionScope::SourceSegmentsOnly
    );
    assert!(matches!(
        first.state.evidence,
        krometrail_core::RangeEvidenceAvailability::Complete
    ));
    assert!(!first.state.protected_segments.is_empty());
    assert!(
        first
            .state
            .protected_segments
            .iter()
            .all(|segment| segment.byte_len > 0)
    );
    assert!(first.state.coalesced_protected_ranges.iter().any(|range| {
        range.start() <= full.request.range.start() && range.end() >= full.request.range.end()
    }));
    assert_eq!(
        first.state.pinned_usage_bytes,
        first.state.retention.pinned_usage_bytes
    );

    let repeated = fixture
        .store
        .pin_resolved_range(full.clone())
        .await
        .unwrap();
    assert!(!repeated.changed);
    assert_eq!(
        repeated.state.protected_segments,
        first.state.protected_segments
    );

    let overlap = RetentionPinRequest::from_resolved(&resolved(
        &fixture,
        vec![fixture.frame_ids[1], fixture.frame_ids[2]],
        2,
        3,
    ))
    .unwrap();
    let overlapping = fixture
        .store
        .pin_resolved_range(overlap.clone())
        .await
        .unwrap();
    assert!(overlapping.changed);
    assert!(overlapping.state.protected_segments.iter().any(|segment| {
        segment.retained_range.start() < overlap.request.range.start()
            || segment.retained_range.end() > overlap.request.range.end()
    }));
    let removed = fixture
        .store
        .unpin_resolved_range(full.clone())
        .await
        .unwrap();
    assert!(removed.changed);
    assert!(!removed.state.exact_pin_active);
    assert!(!removed.state.protected_segments.is_empty());
    assert!(
        fixture
            .store
            .query_pin_state(overlap.clone())
            .await
            .unwrap()
            .exact_pin_active
    );

    let repeated_unpin = fixture.store.unpin_resolved_range(full).await.unwrap();
    assert!(!repeated_unpin.changed);
    assert!(!repeated_unpin.state.protected_segments.is_empty());
    fixture.store.unpin_resolved_range(overlap).await.unwrap();
}

#[tokio::test]
async fn resolved_pin_refuses_partial_or_deleted_source_truth() {
    let fixture = fixture(1).await;
    let request =
        RetentionPinRequest::from_resolved(&resolved(&fixture, fixture.frame_ids.clone(), 1, 3))
            .unwrap();
    let partial =
        RetentionPinRequest::new(request.request, vec![fixture.frame_ids[0], frame_id(999)])
            .unwrap();
    assert!(matches!(
        fixture
            .store
            .query_pin_state(partial.clone())
            .await
            .unwrap()
            .evidence,
        krometrail_core::RangeEvidenceAvailability::PartiallyUnavailable { .. }
    ));
    assert_eq!(
        fixture
            .store
            .pin_resolved_range(partial)
            .await
            .unwrap_err()
            .code,
        krometrail_core::ErrorCode::NotFound
    );
    let unavailable =
        RetentionPinRequest::new(request.request, vec![frame_id(998), frame_id(999)]).unwrap();
    assert!(matches!(
        fixture
            .store
            .query_pin_state(unavailable)
            .await
            .unwrap()
            .evidence,
        krometrail_core::RangeEvidenceAvailability::Unavailable { .. }
    ));

    fixture
        .store
        .delete_session(fixture.session_id)
        .await
        .unwrap();
    assert_eq!(
        fixture
            .store
            .pin_resolved_range(request.clone())
            .await
            .unwrap_err()
            .code,
        krometrail_core::ErrorCode::NotFound
    );
    assert_eq!(
        fixture
            .store
            .query_pin_state(request)
            .await
            .unwrap_err()
            .code,
        krometrail_core::ErrorCode::NotFound
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pin_serializes_with_concurrent_session_deletion_without_resurrection() {
    let fixture = fixture(1_000_000).await;
    let request =
        RetentionPinRequest::from_resolved(&resolved(&fixture, fixture.frame_ids.clone(), 1, 3))
            .unwrap();
    let start = Arc::new(Barrier::new(3));
    let pin = {
        let store = Arc::clone(&fixture.store);
        let request = request.clone();
        let start = Arc::clone(&start);
        tokio::spawn(async move {
            start.wait().await;
            store.pin_resolved_range(request).await
        })
    };
    let deletion = {
        let store = Arc::clone(&fixture.store);
        let start = Arc::clone(&start);
        tokio::spawn(async move {
            start.wait().await;
            store.delete_session(fixture.session_id).await
        })
    };
    start.wait().await;

    let pin = pin.await.unwrap();
    deletion.await.unwrap().unwrap();
    if let Err(error) = pin {
        assert_eq!(error.code, krometrail_core::ErrorCode::NotFound);
    }
    assert_eq!(
        fixture
            .store
            .query_pin_state(request)
            .await
            .unwrap_err()
            .code,
        krometrail_core::ErrorCode::NotFound
    );
    assert_eq!(fixture.store.status().await.unwrap().pinned_usage_bytes, 0);
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
