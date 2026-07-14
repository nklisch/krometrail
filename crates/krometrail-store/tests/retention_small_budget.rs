use std::{sync::Arc, time::Duration};

use krometrail_core::{
    BrowserEvent, BrowserEventBatch, BrowserEventClass, BrowserEventId, BrowserEventOrdinal,
    BrowserEventPayload, BrowserEventSelector, BrowserEventSeverity, BrowserEventSink,
    BrowserEventSource, BrowserEventUnavailableReason, CaptureOrdinal, CapturedFrame,
    DeviceScaleFactor, DiskBudgetBytes, EncodedFrame, ErrorCode, FrameId, FrameSource, ImageFormat,
    ObservedTime, PixelDimensions, RecordingSink, RetentionRange, RetentionStore, SessionId,
    SessionRange, SessionTime, TargetId, TargetLifecycle, TargetLifecycleEvent,
};
use krometrail_store::{
    IndexStoreConfig, RecordingStore, RotationConfig, SegmentStoreConfig, SegmentWriter,
    SqliteIndex,
};
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

fn browser_event(id: u128, session: u128, target: u128, ordinal: u64, time: u64) -> BrowserEvent {
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

fn event_selector(session: SessionId, target: TargetId) -> BrowserEventSelector {
    BrowserEventSelector::new(
        session,
        target,
        SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(100)).unwrap(),
        Vec::<BrowserEventClass>::new(),
        BrowserEventSeverity::Debug,
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
                max_size: 100_000,
            },
        })
        .unwrap(),
    );
    (index, writer)
}

#[tokio::test]
async fn pinned_budget_pauses_then_unpin_evicts_and_resumes() {
    let directory = TempDir::new().unwrap();
    let (index, writer) = fixture(&directory);
    let probe = RecordingStore::new(Arc::clone(&writer), Arc::clone(&index)).unwrap();
    let baseline = probe.status().await.unwrap().usage.total_bytes().unwrap();
    drop(probe);

    let store = RecordingStore::with_budget(
        Arc::clone(&writer),
        Arc::clone(&index),
        DiskBudgetBytes::new(baseline + 125_000).unwrap(),
    )
    .unwrap();
    let first = frame(1, 10, 100, 1, 80_000);
    let first_id = first.metadata().id();
    store.append_frame(first).await.unwrap();
    let pin = RetentionRange {
        session_id: SessionId::from_uuid(Uuid::from_u128(1)),
        target_id: TargetId::from_uuid(Uuid::from_u128(10)),
        range: SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(2)).unwrap(),
    };
    assert_eq!(
        store.pin_range(pin).await.unwrap().protected_segments.len(),
        1
    );

    let second = frame(2, 20, 200, 1, 80_000);
    let error = store.append_frame(second.clone()).await.unwrap_err();
    assert_eq!(error.code, ErrorCode::BudgetExhausted);
    assert_eq!(error.message.as_str(), "disk budget paused capture");
    assert_eq!(error.retry, krometrail_core::RetryAdvice::AfterRecovery);
    assert_eq!(
        error.recovery.as_ref().map(|value| value.as_str()),
        Some("unpin or delete retained evidence, or increase the disk budget")
    );
    assert_eq!(
        error.context.session_id,
        Some(second.metadata().session_id())
    );
    assert!(store.status().await.unwrap().recording_blocked);

    store.unpin_range(pin).await.unwrap();
    store.wait_until_recording_allowed().await.unwrap();
    assert!(!store.status().await.unwrap().recording_blocked);
    store.append_frame(second.clone()).await.unwrap();
    assert!(index.frames_by_id(vec![first_id]).await.is_err());
    assert_eq!(
        index
            .frames_by_id(vec![second.metadata().id()])
            .await
            .unwrap()[0]
            .metadata()
            .id(),
        second.metadata().id()
    );
}

#[tokio::test]
async fn global_retention_sequence_evicts_oldest_unpinned_session_first() {
    let directory = TempDir::new().unwrap();
    let (index, writer) = fixture(&directory);
    let baseline = RecordingStore::new(Arc::clone(&writer), Arc::clone(&index))
        .unwrap()
        .status()
        .await
        .unwrap()
        .usage
        .total_bytes()
        .unwrap();
    let store = RecordingStore::with_budget(
        Arc::clone(&writer),
        Arc::clone(&index),
        DiskBudgetBytes::new(baseline + 150_000).unwrap(),
    )
    .unwrap();

    let first = frame(11, 110, 111, 1, 60_000);
    let second = frame(12, 120, 121, 1, 60_000);
    let third = frame(13, 130, 131, 1, 60_000);
    store.append_frame(first.clone()).await.unwrap();
    store.flush(first.metadata().session_id()).await.unwrap();
    store.append_frame(second.clone()).await.unwrap();
    store.flush(second.metadata().session_id()).await.unwrap();
    store
        .pin_range(RetentionRange {
            session_id: second.metadata().session_id(),
            target_id: second.metadata().target_id(),
            range: SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(2)).unwrap(),
        })
        .await
        .unwrap();

    store.append_frame(third.clone()).await.unwrap();

    assert!(
        index
            .frames_by_id(vec![first.metadata().id()])
            .await
            .is_err()
    );
    assert_eq!(
        index
            .frames_by_id(vec![second.metadata().id()])
            .await
            .unwrap()[0]
            .metadata()
            .id(),
        second.metadata().id()
    );
    assert_eq!(
        index
            .frames_by_id(vec![third.metadata().id()])
            .await
            .unwrap()[0]
            .metadata()
            .id(),
        third.metadata().id()
    );
}

#[tokio::test]
async fn overlapping_pins_keep_a_segment_protected_until_the_last_unpin() {
    let directory = TempDir::new().unwrap();
    let (index, writer) = fixture(&directory);
    let store = RecordingStore::new(writer, Arc::clone(&index)).unwrap();
    let item = frame(14, 140, 141, 1, 1024);
    let request_a = RetentionRange {
        session_id: item.metadata().session_id(),
        target_id: item.metadata().target_id(),
        range: SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(1)).unwrap(),
    };
    let request_b = RetentionRange {
        range: SessionRange::new(SessionTime::from_nanos(1), SessionTime::from_nanos(2)).unwrap(),
        ..request_a
    };
    store.append_frame(item).await.unwrap();
    store.flush(request_a.session_id).await.unwrap();
    assert!(store.pin_range(request_a).await.unwrap().pinned_usage_bytes > 0);
    assert!(store.pin_range(request_b).await.unwrap().pinned_usage_bytes > 0);
    store.unpin_range(request_a).await.unwrap();
    assert!(store.status().await.unwrap().pinned_usage_bytes > 0);
    store.unpin_range(request_b).await.unwrap();
    assert_eq!(store.status().await.unwrap().pinned_usage_bytes, 0);
}

#[tokio::test]
async fn open_segment_usage_reports_a_single_bounded_overhead() {
    let directory = TempDir::new().unwrap();
    let (index, writer) = fixture(&directory);
    let baseline = RecordingStore::new(Arc::clone(&writer), Arc::clone(&index))
        .unwrap()
        .status()
        .await
        .unwrap()
        .usage
        .total_bytes()
        .unwrap();
    let budget = DiskBudgetBytes::new(baseline + 50_000).unwrap();
    let store =
        RecordingStore::with_budget(Arc::clone(&writer), Arc::clone(&index), budget).unwrap();
    store
        .append_frame(frame(15, 150, 151, 1, 20_000))
        .await
        .unwrap();
    let status = store.status().await.unwrap();
    assert_eq!(status.open_segment_count, 1);
    assert!(status.usage.open_segment_bytes > 0);
    assert!(
        status.usage.total_bytes().unwrap()
            <= budget.get() + status.open_segment_overhead_limit_bytes
    );
}

#[tokio::test]
async fn unpolled_append_has_no_storage_side_effect() {
    let directory = TempDir::new().unwrap();
    let (index, writer) = fixture(&directory);
    let store = RecordingStore::new(writer, Arc::clone(&index)).unwrap();
    let item = frame(16, 160, 161, 1, 1024);
    let frame_id = item.metadata().id();
    let future = store.append_frame(item);
    tokio::task::yield_now().await;
    assert!(index.frames_by_id(vec![frame_id]).await.is_err());
    drop(future);
    assert!(index.frames_by_id(vec![frame_id]).await.is_err());
}

#[tokio::test]
async fn destructive_session_deletion_preserves_another_session() {
    let directory = TempDir::new().unwrap();
    let (index, writer) = fixture(&directory);
    let store = RecordingStore::new(writer, Arc::clone(&index)).unwrap();
    let first = frame(17, 170, 171, 1, 1024);
    let second = frame(18, 180, 181, 1, 1024);
    store.append_frame(first.clone()).await.unwrap();
    store.flush(first.metadata().session_id()).await.unwrap();
    store.append_frame(second.clone()).await.unwrap();
    store.flush(second.metadata().session_id()).await.unwrap();

    let removed = store
        .delete_session(first.metadata().session_id())
        .await
        .unwrap();
    assert_eq!(removed.removed_frames, 1);
    assert!(
        index
            .frames_by_id(vec![first.metadata().id()])
            .await
            .is_err()
    );
    assert_eq!(
        index
            .frames_by_id(vec![second.metadata().id()])
            .await
            .unwrap()[0]
            .metadata()
            .id(),
        second.metadata().id()
    );
    assert!(store.status().await.unwrap().usage.segment_bytes > 0);
}

#[tokio::test]
async fn destructive_session_deletion_removes_payload_and_rejects_resurrection() {
    let directory = TempDir::new().unwrap();
    let (index, writer) = fixture(&directory);
    let store = RecordingStore::new(writer, Arc::clone(&index)).unwrap();
    let frame = frame(3, 30, 300, 1, 1024);
    let session = frame.metadata().session_id();
    let frame_id = frame.metadata().id();
    store.append_frame(frame.clone()).await.unwrap();
    store.flush(session).await.unwrap();

    let removed = store.delete_session(session).await.unwrap();
    assert_eq!(removed.session_id, session);
    assert_eq!(removed.removed_segments, 1);
    assert_eq!(removed.removed_frames, 1);
    assert!(removed.removed_bytes > 0);
    assert!(index.frames_by_id(vec![frame_id]).await.is_err());
    assert_eq!(
        store.append_frame(frame).await.unwrap_err().code,
        ErrorCode::NotFound
    );
    assert_eq!(
        store.delete_session(session).await.unwrap().removed_bytes,
        0
    );
}

#[tokio::test]
async fn pinned_source_frames_survive_older_event_eviction_with_tombstones() {
    let directory = TempDir::new().unwrap();
    let (index, writer) = fixture(&directory);
    let store = RecordingStore::new(Arc::clone(&writer), Arc::clone(&index)).unwrap();
    let event = browser_event(401, 400, 410, 1, 1);
    let second_event = browser_event(402, 400, 410, 2, 2);
    let third_event = browser_event(403, 400, 410, 3, 3);
    store
        .append_event_batch(
            BrowserEventBatch::new(
                event.session_id(),
                vec![event.clone(), second_event, third_event],
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let item = frame(400, 410, 411, 1, 150_000);
    store.append_frame(item.clone()).await.unwrap();
    store.flush(item.metadata().session_id()).await.unwrap();
    let pin = RetentionRange {
        session_id: item.metadata().session_id(),
        target_id: item.metadata().target_id(),
        range: SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(4)).unwrap(),
    };
    store.pin_range(pin).await.unwrap();
    let status = store.status().await.unwrap();
    let budget = DiskBudgetBytes::new(
        status.usage.total_bytes().unwrap() - status.open_segment_overhead_limit_bytes - 1,
    )
    .unwrap();
    drop(store);

    let store =
        RecordingStore::with_budget(Arc::clone(&writer), Arc::clone(&index), budget).unwrap();
    let _ = store.enforce_budget().await.unwrap();
    assert_eq!(
        store
            .count_events(event_selector(event.session_id(), event.target_id()))
            .await
            .unwrap(),
        0
    );
    let unavailable = store
        .unavailable_ranges(
            event.session_id(),
            event.target_id(),
            SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(4)).unwrap(),
            10,
        )
        .await
        .unwrap();
    assert_eq!(unavailable.len(), 1);
    assert_eq!(
        unavailable[0].reason(),
        BrowserEventUnavailableReason::RetentionEvicted
    );
    assert_eq!(unavailable[0].event_count().get(), 3);
    assert_eq!(unavailable[0].first_ordinal().unwrap().get(), 1);
    assert_eq!(unavailable[0].last_ordinal().unwrap().get(), 3);
    assert_eq!(
        index
            .frames_by_id(vec![item.metadata().id()])
            .await
            .unwrap()[0]
            .metadata()
            .id(),
        item.metadata().id()
    );
}

#[tokio::test]
async fn event_append_under_file_backed_pressure_fails_without_deleting_segments() {
    let directory = TempDir::new().unwrap();
    let (index, writer) = fixture(&directory);
    let store = RecordingStore::new(Arc::clone(&writer), Arc::clone(&index)).unwrap();
    let item = frame(450, 460, 461, 1, 20_000);
    store.append_frame(item.clone()).await.unwrap();
    store.flush(item.metadata().session_id()).await.unwrap();
    let status = store.status().await.unwrap();
    let budget = DiskBudgetBytes::new(status.usage.total_bytes().unwrap() - 1).unwrap();
    drop(store);

    let store =
        RecordingStore::with_budget(Arc::clone(&writer), Arc::clone(&index), budget).unwrap();
    let event = browser_event(451, 450, 460, 1, 1);
    assert_eq!(
        store
            .append_event_batch(
                BrowserEventBatch::new(event.session_id(), vec![event.clone()]).unwrap()
            )
            .await
            .unwrap_err()
            .code,
        ErrorCode::BudgetExhausted
    );
    assert_eq!(
        index
            .frames_by_id(vec![item.metadata().id()])
            .await
            .unwrap()[0]
            .metadata()
            .id(),
        item.metadata().id()
    );
    assert_eq!(
        store
            .count_events(event_selector(event.session_id(), event.target_id()))
            .await
            .unwrap(),
        0
    );
    assert!(
        store
            .unavailable_ranges(
                event.session_id(),
                event.target_id(),
                SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(2)).unwrap(),
                10,
            )
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn older_unpinned_segment_is_removed_before_newer_event() {
    let directory = TempDir::new().unwrap();
    let (index, writer) = fixture(&directory);
    let store = RecordingStore::new(Arc::clone(&writer), Arc::clone(&index)).unwrap();
    let item = frame(500, 510, 511, 1, 150_000);
    store.append_frame(item.clone()).await.unwrap();
    store.flush(item.metadata().session_id()).await.unwrap();
    let event = browser_event(501, 500, 510, 1, 2);
    store
        .append_event_batch(
            BrowserEventBatch::new(event.session_id(), vec![event.clone()]).unwrap(),
        )
        .await
        .unwrap();
    let status = store.status().await.unwrap();
    let budget = DiskBudgetBytes::new(
        status.usage.total_bytes().unwrap() - status.open_segment_overhead_limit_bytes - 1,
    )
    .unwrap();
    drop(store);

    let store = RecordingStore::with_budget(writer, Arc::clone(&index), budget).unwrap();
    let _ = store.enforce_budget().await.unwrap();
    assert!(
        index
            .frames_by_id(vec![item.metadata().id()])
            .await
            .is_err()
    );
    assert_eq!(
        store
            .count_events(event_selector(event.session_id(), event.target_id()))
            .await
            .unwrap(),
        1
    );
}
