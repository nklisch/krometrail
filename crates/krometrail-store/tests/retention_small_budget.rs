use std::{sync::Arc, time::Duration};

use krometrail_core::{
    CaptureOrdinal, CapturedFrame, DeviceScaleFactor, DiskBudgetBytes, EncodedFrame, ErrorCode,
    FrameId, FrameSource, ImageFormat, ObservedTime, PixelDimensions, RecordingSink,
    RetentionRange, RetentionStore, SessionId, SessionRange, SessionTime, TargetId,
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
