use std::{fs, sync::Arc, time::Duration};

use krometrail_core::{
    CaptureGap, CaptureGapReason, CaptureGapStore, CaptureOrdinal, CapturedFrame,
    DeviceScaleFactor, EncodedFrame, ErrorCode, FrameId, FrameSource, GapId, ImageFormat,
    ObservedTime, PixelDimensions, RecordingSink, SessionId, SessionRange, SessionTime, TargetId,
};
use krometrail_store::{
    IndexStoreConfig, IndexedRecordingSink, RotationConfig, SegmentStoreConfig, SegmentWriter,
    SqliteIndex,
    segments::{read_frame_at, scan_complete_records},
};
use rusqlite::Connection;
use tempfile::TempDir;
use uuid::Uuid;

struct Fixture {
    directory: TempDir,
    index: Arc<SqliteIndex>,
    sink: Arc<IndexedRecordingSink>,
    session: SessionId,
    target: TargetId,
}

impl Fixture {
    fn new(rotation: RotationConfig) -> Self {
        let directory = TempDir::new().unwrap();
        let segments_directory = directory.path().join("segments");
        let index = Arc::new(
            SqliteIndex::open(IndexStoreConfig {
                database_path: directory.path().join("index.sqlite3"),
                segments_directory: segments_directory.clone(),
                busy_timeout: Duration::from_secs(1),
            })
            .unwrap(),
        );
        let writer = Arc::new(
            SegmentWriter::open(SegmentStoreConfig {
                directory: segments_directory,
                rotation,
            })
            .unwrap(),
        );
        let sink = Arc::new(IndexedRecordingSink::new(writer, Arc::clone(&index)));
        Self {
            directory,
            index,
            sink,
            session: SessionId::from_uuid(Uuid::from_u128(1)),
            target: TargetId::from_uuid(Uuid::from_u128(2)),
        }
    }

    fn frame(&self, id: u128, target: TargetId, ordinal: u64, at: u64) -> EncodedFrame {
        EncodedFrame::new(
            CapturedFrame::new(
                FrameId::from_uuid(Uuid::from_u128(id)),
                self.session,
                target,
                CaptureOrdinal::new(ordinal).unwrap(),
                None,
                ObservedTime::from_nanos(at + 10),
                SessionTime::from_nanos(at),
                ImageFormat::Jpeg,
                PixelDimensions::new(2, 2).unwrap(),
                PixelDimensions::new(2, 2).unwrap(),
                DeviceScaleFactor::new(1.0).unwrap(),
                vec![],
            )
            .unwrap(),
            vec![id as u8, ordinal as u8],
        )
        .unwrap()
    }

    fn database_path(&self) -> std::path::PathBuf {
        self.directory.path().join("index.sqlite3")
    }

    fn segments_directory(&self) -> std::path::PathBuf {
        self.directory.path().join("segments")
    }
}

#[tokio::test]
async fn indexed_writes_are_queryable_by_id_range_and_open_or_sealed_address() {
    let fixture = Fixture::new(RotationConfig {
        max_duration: Duration::from_nanos(5),
        max_size: u64::MAX,
    });
    let first = fixture.frame(10, fixture.target, 1, 1);
    let second = fixture.frame(11, fixture.target, 2, 10);
    let first_address = fixture.sink.append_frame(first.clone()).await.unwrap();
    let second_address = fixture.sink.append_frame(second.clone()).await.unwrap();
    assert_ne!(first_address.segment_id, second_address.segment_id);

    assert_eq!(
        fixture
            .index
            .frames_by_id(vec![second.metadata().id(), first.metadata().id()])
            .await
            .unwrap(),
        [second.clone(), first.clone()]
    );
    assert_eq!(
        fixture
            .index
            .frames_in_range(
                fixture.session,
                fixture.target,
                SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(20)).unwrap(),
            )
            .await
            .unwrap(),
        [first.clone(), second.clone()]
    );

    fixture.sink.flush(fixture.session).await.unwrap();
    assert_eq!(
        fixture
            .index
            .frames_by_id(vec![first.metadata().id(), second.metadata().id()])
            .await
            .unwrap(),
        [first, second]
    );
}

#[tokio::test]
async fn index_failure_after_append_leaves_only_a_readable_orphan_record() {
    let fixture = Fixture::new(RotationConfig::suggested());
    let first = fixture.frame(20, fixture.target, 1, 1);
    let duplicate_id = fixture.frame(20, fixture.target, 2, 2);
    let address = fixture.sink.append_frame(first).await.unwrap();
    assert_eq!(
        fixture
            .sink
            .append_frame(duplicate_id.clone())
            .await
            .unwrap_err()
            .code,
        ErrorCode::PersistenceFailed
    );

    let open_path = fixture
        .segments_directory()
        .join(format!("{}.open", address.segment_id));
    let bytes = fs::read(open_path).unwrap();
    let scan = scan_complete_records(&bytes).unwrap();
    assert_eq!(scan.records.len(), 2);
    let orphan_address =
        krometrail_core::FrameAddress::new(address.segment_id, scan.records[1].byte_offset);
    assert_eq!(read_frame_at(&bytes, orphan_address).unwrap(), duplicate_id);

    let connection = Connection::open(fixture.database_path()).unwrap();
    let counts: (u32, u32) = connection
        .query_row(
            "SELECT (SELECT count(*) FROM frames), \
                    (SELECT count(*) FROM timeline_observations WHERE kind='frame')",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(counts, (1, 1));
}

#[tokio::test]
async fn gaps_bypass_segments_and_concurrent_targets_remain_consistent() {
    let fixture = Fixture::new(RotationConfig::suggested());
    let other_target = TargetId::from_uuid(Uuid::from_u128(3));
    let left = Arc::clone(&fixture.sink);
    let frame_left = fixture.frame(30, fixture.target, 1, 1);
    let right = Arc::clone(&fixture.sink);
    let frame_right = fixture.frame(31, other_target, 1, 1);
    let (left_result, right_result) = tokio::join!(
        left.append_frame(frame_left.clone()),
        right.append_frame(frame_right.clone())
    );
    left_result.unwrap();
    right_result.unwrap();
    let before: Vec<_> = fs::read_dir(fixture.segments_directory())
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (entry.file_name(), entry.metadata().unwrap().len())
        })
        .collect();

    let gap = CaptureGap::new(
        GapId::from_uuid(Uuid::from_u128(32)),
        fixture.session,
        fixture.target,
        SessionRange::new(SessionTime::from_nanos(2), SessionTime::from_nanos(3)).unwrap(),
        ObservedTime::from_nanos(4),
        CaptureGapReason::PersistenceRejected,
        None,
        Some("index backpressure".into()),
    )
    .unwrap();
    fixture.sink.append_gap(gap.clone()).await.unwrap();
    let after: Vec<_> = fs::read_dir(fixture.segments_directory())
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (entry.file_name(), entry.metadata().unwrap().len())
        })
        .collect();
    assert_eq!(after, before);
    assert_eq!(
        fixture
            .index
            .gaps(
                fixture.session,
                fixture.target,
                SessionRange::new(SessionTime::from_nanos(3), SessionTime::from_nanos(3)).unwrap(),
            )
            .await
            .unwrap(),
        std::slice::from_ref(&gap)
    );

    assert_eq!(
        fixture
            .index
            .frames_by_id(vec![
                frame_right.metadata().id(),
                frame_left.metadata().id()
            ])
            .await
            .unwrap(),
        [frame_right, frame_left]
    );
}

#[tokio::test]
async fn missing_frame_ids_fail_the_whole_ordered_request() {
    let fixture = Fixture::new(RotationConfig::suggested());
    let frame = fixture.frame(40, fixture.target, 1, 1);
    fixture.sink.append_frame(frame.clone()).await.unwrap();
    let error = fixture
        .index
        .frames_by_id(vec![
            frame.metadata().id(),
            FrameId::from_uuid(Uuid::from_u128(999)),
        ])
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::NotFound);
}
