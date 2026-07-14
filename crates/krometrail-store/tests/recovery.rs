use std::{fs, io::Write, sync::Arc, time::Duration};

use krometrail_core::{
    CaptureOrdinal, CapturedFrame, DeviceScaleFactor, EncodedFrame, ErrorCode, FrameId,
    FrameSource, ImageFormat, ObservedTime, PixelDimensions, RecordingSink, SegmentId, SessionId,
    SessionTime, TargetId,
};
use krometrail_store::{
    IndexStoreConfig, IndexedRecordingSink, RecoveryReport, RotationConfig, SegmentStoreConfig,
    SegmentWriter, SqliteIndex, recover,
    segments::{
        SEGMENT_HEADER_LEN, SealedFooter, SegmentHeader, open_segment_path, sealed_segment_path,
    },
};
use rusqlite::{Connection, params};
use tempfile::TempDir;
use uuid::Uuid;

struct Fixture {
    root: TempDir,
}

impl Fixture {
    fn new() -> Self {
        Self {
            root: TempDir::new().unwrap(),
        }
    }

    fn database_path(&self) -> std::path::PathBuf {
        self.root.path().join("index.sqlite3")
    }

    fn segments_directory(&self) -> std::path::PathBuf {
        self.root.path().join("segments")
    }

    fn index(&self) -> Arc<SqliteIndex> {
        Arc::new(
            SqliteIndex::open(IndexStoreConfig {
                database_path: self.database_path(),
                segments_directory: self.segments_directory(),
                busy_timeout: Duration::from_secs(1),
            })
            .unwrap(),
        )
    }

    fn writer(&self) -> Arc<SegmentWriter> {
        Arc::new(
            SegmentWriter::open(SegmentStoreConfig {
                directory: self.segments_directory(),
                rotation: RotationConfig::suggested(),
            })
            .unwrap(),
        )
    }

    fn connection(&self) -> Connection {
        let connection = Connection::open(self.database_path()).unwrap();
        connection
            .pragma_update(None, "foreign_keys", true)
            .unwrap();
        connection
    }
}

fn id(value: u128) -> Uuid {
    Uuid::from_u128(value)
}

fn frame(session_id: SessionId, target_id: TargetId, frame_id: u128, ordinal: u64) -> EncodedFrame {
    EncodedFrame::new(
        CapturedFrame::new(
            FrameId::from_uuid(id(frame_id)),
            session_id,
            target_id,
            CaptureOrdinal::new(ordinal).unwrap(),
            None,
            ObservedTime::from_nanos(ordinal + 100),
            SessionTime::from_nanos(ordinal),
            ImageFormat::Jpeg,
            PixelDimensions::new(2, 2).unwrap(),
            PixelDimensions::new(2, 2).unwrap(),
            DeviceScaleFactor::new(1.0).unwrap(),
            vec![],
        )
        .unwrap(),
        vec![frame_id as u8, ordinal as u8],
    )
    .unwrap()
}

fn metadata_snapshot(fixture: &Fixture) -> Vec<(String, Vec<u8>)> {
    let connection = fixture.connection();
    let mut snapshot = Vec::new();
    for (label, sql) in [
        (
            "segments",
            "SELECT hex(segment_id)||state||relative_path||hex(file_bytes_be) FROM segments ORDER BY segment_id",
        ),
        (
            "frames",
            "SELECT hex(frame_id)||hex(segment_id)||hex(byte_offset_be) FROM frames ORDER BY frame_id",
        ),
        (
            "timeline",
            "SELECT kind||payload_json FROM timeline_observations ORDER BY observation_id",
        ),
        (
            "usage",
            "SELECT class||hex(object_key)||hex(byte_len_be) FROM usage ORDER BY class, object_key",
        ),
    ] {
        let mut statement = connection.prepare(sql).unwrap();
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap();
        let encoded = rows
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .join("\n")
            .into_bytes();
        snapshot.push((label.to_owned(), encoded));
    }
    snapshot
}

async fn indexed_segment(
    fixture: &Fixture,
    session_id: SessionId,
    target_id: TargetId,
    frames: &[EncodedFrame],
) -> (Arc<SqliteIndex>, Vec<krometrail_core::FrameAddress>) {
    let index = fixture.index();
    let writer = fixture.writer();
    let sink = IndexedRecordingSink::new(Arc::clone(&writer), Arc::clone(&index));
    let mut addresses = Vec::new();
    for frame in frames {
        addresses.push(sink.append_frame(frame.clone()).await.unwrap());
    }
    sink.flush(session_id).await.unwrap();
    drop(sink);
    drop(writer);
    assert!(
        frames
            .iter()
            .all(|item| item.metadata().target_id() == target_id)
    );
    (index, addresses)
}

#[tokio::test]
async fn orphan_payload_is_inserted_and_second_recovery_is_a_no_op() {
    let fixture = Fixture::new();
    let index = fixture.index();
    let writer = fixture.writer();
    let session = SessionId::from_uuid(id(1));
    let target = TargetId::from_uuid(id(2));
    let expected = frame(session, target, 3, 1);
    let commit = writer.append_indexable(expected.clone()).await.unwrap();
    drop(writer);

    let report = recover(index.as_ref()).unwrap();
    assert_eq!(report.open_segments_sealed, 1);
    assert_eq!(report.frames_recovered, 1);
    assert_eq!(
        index
            .frames_by_id(vec![expected.metadata().id()])
            .await
            .unwrap(),
        [expected]
    );
    assert!(
        sealed_segment_path(&fixture.segments_directory(), commit.address.segment_id).is_file()
    );

    let before = metadata_snapshot(&fixture);
    assert_eq!(recover(index.as_ref()).unwrap(), RecoveryReport::default());
    assert_eq!(metadata_snapshot(&fixture), before);
}

#[tokio::test]
async fn duplicate_frame_orphan_is_stably_ignored_after_sealing() {
    let fixture = Fixture::new();
    let index = fixture.index();
    let writer = fixture.writer();
    let sink = IndexedRecordingSink::new(Arc::clone(&writer), Arc::clone(&index));
    let session = SessionId::from_uuid(id(5));
    let target = TargetId::from_uuid(id(6));
    let first = frame(session, target, 7, 1);
    let duplicate = frame(session, target, 7, 2);
    sink.append_frame(first.clone()).await.unwrap();
    assert_eq!(
        sink.append_frame(duplicate).await.unwrap_err().code,
        ErrorCode::PersistenceFailed
    );
    drop(sink);
    drop(writer);

    let report = recover(index.as_ref()).unwrap();
    assert_eq!(report.open_segments_sealed, 1);
    assert_eq!(report.frames_recovered, 0);
    assert_eq!(
        index
            .frames_by_id(vec![first.metadata().id()])
            .await
            .unwrap(),
        [first]
    );
    let before = metadata_snapshot(&fixture);
    assert_eq!(recover(index.as_ref()).unwrap(), RecoveryReport::default());
    assert_eq!(metadata_snapshot(&fixture), before);
}

#[tokio::test]
async fn truncated_open_tail_is_removed_while_complete_frames_survive() {
    let fixture = Fixture::new();
    let index = fixture.index();
    let writer = fixture.writer();
    let sink = IndexedRecordingSink::new(Arc::clone(&writer), Arc::clone(&index));
    let session = SessionId::from_uuid(id(10));
    let target = TargetId::from_uuid(id(11));
    let first = frame(session, target, 12, 1);
    let second = frame(session, target, 13, 2);
    let first_address = sink.append_frame(first.clone()).await.unwrap();
    let second_address = sink.append_frame(second.clone()).await.unwrap();
    drop(sink);
    drop(writer);

    let path = open_segment_path(&fixture.segments_directory(), first_address.segment_id);
    fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(second_address.byte_offset.get() + 8)
        .unwrap();

    let report = recover(index.as_ref()).unwrap();
    assert_eq!(report.open_segments_sealed, 1);
    assert_eq!(report.frames_removed, 1);
    assert_eq!(
        index
            .frames_by_id(vec![first.metadata().id()])
            .await
            .unwrap(),
        [first]
    );
    assert_eq!(
        index
            .frames_by_id(vec![second.metadata().id()])
            .await
            .unwrap_err()
            .code,
        ErrorCode::NotFound
    );
}

#[tokio::test]
async fn sealed_but_unreconciled_segment_resumes_crash_during_recovery() {
    let fixture = Fixture::new();
    let index = fixture.index();
    let writer = fixture.writer();
    let session = SessionId::from_uuid(id(20));
    let target = TargetId::from_uuid(id(21));
    let expected = frame(session, target, 22, 1);
    let commit = writer.append_indexable(expected.clone()).await.unwrap();
    drop(writer);

    let open = open_segment_path(&fixture.segments_directory(), commit.address.segment_id);
    let footer = SealedFooter::new(
        commit.address.segment_id,
        1,
        expected.byte_len().get(),
        expected.metadata().session_time(),
        expected.metadata().session_time(),
        expected.metadata().observed_time(),
    );
    let mut file = fs::OpenOptions::new().append(true).open(&open).unwrap();
    file.write_all(&footer.encode()).unwrap();
    file.sync_data().unwrap();
    drop(file);
    fs::rename(
        &open,
        sealed_segment_path(&fixture.segments_directory(), commit.address.segment_id),
    )
    .unwrap();

    let report = recover(index.as_ref()).unwrap();
    assert_eq!(report.open_segments_sealed, 0);
    assert_eq!(report.frames_recovered, 1);
    assert_eq!(
        index
            .frames_by_id(vec![expected.metadata().id()])
            .await
            .unwrap(),
        [expected]
    );
}

#[tokio::test]
async fn fatal_header_corruption_is_quarantined_and_index_claims_are_removed() {
    let fixture = Fixture::new();
    let session = SessionId::from_uuid(id(30));
    let target = TargetId::from_uuid(id(31));
    let expected = frame(session, target, 32, 1);
    let (index, addresses) =
        indexed_segment(&fixture, session, target, std::slice::from_ref(&expected)).await;
    let segment_id = addresses[0].segment_id;
    let path = sealed_segment_path(&fixture.segments_directory(), segment_id);
    let mut bytes = fs::read(&path).unwrap();
    bytes[SEGMENT_HEADER_LEN / 2] ^= 1;
    fs::write(&path, bytes).unwrap();

    let report = recover(index.as_ref()).unwrap();
    assert_eq!(report.segments_quarantined, 1);
    assert_eq!(report.frames_removed, 1);
    assert!(
        fixture
            .segments_directory()
            .join(format!("{segment_id}.corrupt"))
            .is_file()
    );
    assert_eq!(
        index
            .frames_by_id(vec![expected.metadata().id()])
            .await
            .unwrap_err()
            .code,
        ErrorCode::NotFound
    );
    assert_eq!(recover(index.as_ref()).unwrap(), RecoveryReport::default());
}

#[tokio::test]
async fn absent_segment_removes_only_its_index_registration() {
    let fixture = Fixture::new();
    let index = fixture.index();
    let writer = fixture.writer();
    let sink = IndexedRecordingSink::new(Arc::clone(&writer), Arc::clone(&index));
    let lost_session = SessionId::from_uuid(id(40));
    let kept_session = SessionId::from_uuid(id(41));
    let target = TargetId::from_uuid(id(42));
    let lost = frame(lost_session, target, 43, 1);
    let kept = frame(kept_session, target, 44, 1);
    let lost_address = sink.append_frame(lost.clone()).await.unwrap();
    sink.flush(lost_session).await.unwrap();
    sink.append_frame(kept.clone()).await.unwrap();
    sink.flush(kept_session).await.unwrap();
    drop(sink);
    drop(writer);
    let connection = fixture.connection();
    connection
        .execute(
            "INSERT INTO pins(pin_id, session_id, target_id, start_time_be, end_time_be) \
             VALUES (1, ?1, ?2, ?3, ?3)",
            params![
                lost_session.as_uuid().as_bytes().to_vec(),
                target.as_uuid().as_bytes().to_vec(),
                1_u64.to_be_bytes().to_vec()
            ],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO pin_segments(pin_id, segment_id) VALUES (1, ?1)",
            params![lost_address.segment_id.as_uuid().as_bytes().to_vec()],
        )
        .unwrap();
    drop(connection);
    fs::remove_file(sealed_segment_path(
        &fixture.segments_directory(),
        lost_address.segment_id,
    ))
    .unwrap();

    let report = recover(index.as_ref()).unwrap();
    assert_eq!(report.segments_removed, 1);
    assert_eq!(report.frames_removed, 1);
    assert_eq!(
        index
            .frames_by_id(vec![kept.metadata().id()])
            .await
            .unwrap(),
        [kept]
    );
    assert_eq!(
        index
            .frames_by_id(vec![lost.metadata().id()])
            .await
            .unwrap_err()
            .code,
        ErrorCode::NotFound
    );
    let connection = fixture.connection();
    let (pins, links): (u32, u32) = connection
        .query_row(
            "SELECT (SELECT count(*) FROM pins WHERE pin_id=1), \
                    (SELECT count(*) FROM pin_segments WHERE pin_id=1)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!((pins, links), (1, 0));
}

#[tokio::test]
async fn surviving_pins_are_trusted_and_usage_is_recomputed() {
    let fixture = Fixture::new();
    let session = SessionId::from_uuid(id(50));
    let target = TargetId::from_uuid(id(51));
    let expected = frame(session, target, 52, 1);
    let (index, addresses) = indexed_segment(&fixture, session, target, &[expected]).await;
    let segment_id = addresses[0].segment_id;
    let file_len = fs::metadata(sealed_segment_path(
        &fixture.segments_directory(),
        segment_id,
    ))
    .unwrap()
    .len();
    let connection = fixture.connection();
    connection
        .execute(
            "INSERT INTO pins(pin_id, session_id, target_id, start_time_be, end_time_be) \
             VALUES (1, ?1, ?2, ?3, ?3)",
            params![
                session.as_uuid().as_bytes().to_vec(),
                target.as_uuid().as_bytes().to_vec(),
                1_u64.to_be_bytes().to_vec()
            ],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO pin_segments(pin_id, segment_id) VALUES (1, ?1)",
            params![segment_id.as_uuid().as_bytes().to_vec()],
        )
        .unwrap();
    connection
        .execute(
            "DELETE FROM usage WHERE class='segment' AND object_key=?1",
            params![segment_id.as_uuid().as_bytes().to_vec()],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO usage(class, object_key, session_id, byte_len_be) \
             VALUES ('segment', X'FF', NULL, ?1)",
            params![99_u64.to_be_bytes().to_vec()],
        )
        .unwrap();
    drop(connection);

    let report = recover(index.as_ref()).unwrap();
    assert_eq!(report.usage_rows_reconciled, 2);
    let connection = fixture.connection();
    let (pins, links, usage): (u32, u32, Vec<u8>) = connection
        .query_row(
            "SELECT (SELECT count(*) FROM pins WHERE pin_id=1), \
                    (SELECT count(*) FROM pin_segments WHERE pin_id=1 AND segment_id=?1), \
                    (SELECT byte_len_be FROM usage WHERE class='segment' AND object_key=?1)",
            params![segment_id.as_uuid().as_bytes().to_vec()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!((pins, links), (1, 1));
    assert_eq!(u64::from_be_bytes(usage.try_into().unwrap()), file_len);
    let stale: u32 = connection
        .query_row(
            "SELECT count(*) FROM usage WHERE class='segment' AND object_key=X'FF'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stale, 0);
}

#[test]
fn empty_open_segment_is_sealed_registered_and_has_no_frames() {
    let fixture = Fixture::new();
    let index = fixture.index();
    let segment_id = SegmentId::from_uuid(id(60));
    let session = SessionId::from_uuid(id(61));
    let target = TargetId::from_uuid(id(62));
    let header = SegmentHeader::new(
        segment_id,
        session,
        target,
        SessionTime::from_nanos(1),
        ObservedTime::from_nanos(2),
        3,
        4,
    );
    fs::write(
        open_segment_path(&fixture.segments_directory(), segment_id),
        header.encode(),
    )
    .unwrap();

    let report = recover(index.as_ref()).unwrap();
    assert_eq!(report.open_segments_sealed, 1);
    let connection = fixture.connection();
    let (state, records, frames): (String, Vec<u8>, u32) = connection
        .query_row(
            "SELECT state, record_count_be, \
                    (SELECT count(*) FROM frames WHERE segment_id=?1) \
             FROM segments WHERE segment_id=?1",
            params![segment_id.as_uuid().as_bytes().to_vec()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(state, "sealed");
    assert_eq!(u64::from_be_bytes(records.try_into().unwrap()), 0);
    assert_eq!(frames, 0);
}

#[tokio::test]
async fn reopen_recovers_all_unflushed_targets_and_reports_open_count() {
    let fixture = Fixture::new();
    let index = fixture.index();
    let writer = fixture.writer();
    let sink = IndexedRecordingSink::new(Arc::clone(&writer), Arc::clone(&index));
    let session = SessionId::from_uuid(id(70));
    let first_target = TargetId::from_uuid(id(71));
    let second_target = TargetId::from_uuid(id(72));
    let first = frame(session, first_target, 73, 1);
    let second = frame(session, second_target, 74, 1);
    sink.append_frame(first.clone()).await.unwrap();
    sink.append_frame(second.clone()).await.unwrap();
    drop(sink);
    drop(writer);
    drop(index);

    let reopened = fixture.index();
    let report = recover(reopened.as_ref()).unwrap();
    assert_eq!(report.open_segments_sealed, 2);
    assert_eq!(
        reopened
            .frames_by_id(vec![first.metadata().id(), second.metadata().id()])
            .await
            .unwrap(),
        [first, second]
    );
    assert_eq!(
        recover(reopened.as_ref()).unwrap(),
        RecoveryReport::default()
    );
}

#[tokio::test]
async fn one_pass_repairs_orphan_and_dangling_directions_together() {
    let fixture = Fixture::new();
    let index = fixture.index();
    let writer = fixture.writer();
    let session = SessionId::from_uuid(id(80));
    let orphan_target = TargetId::from_uuid(id(81));
    let dangling_target = TargetId::from_uuid(id(82));
    let orphan = frame(session, orphan_target, 83, 1);
    writer.append_indexable(orphan.clone()).await.unwrap();
    let sink = IndexedRecordingSink::new(Arc::clone(&writer), Arc::clone(&index));
    let dangling = frame(session, dangling_target, 84, 1);
    let dangling_address = sink.append_frame(dangling.clone()).await.unwrap();
    drop(sink);
    drop(writer);
    fs::OpenOptions::new()
        .write(true)
        .open(open_segment_path(
            &fixture.segments_directory(),
            dangling_address.segment_id,
        ))
        .unwrap()
        .set_len(dangling_address.byte_offset.get() + 1)
        .unwrap();

    let report = recover(index.as_ref()).unwrap();
    assert_eq!(report.open_segments_sealed, 2);
    assert_eq!(report.frames_recovered, 1);
    assert_eq!(report.frames_removed, 1);
    assert_eq!(
        index
            .frames_by_id(vec![orphan.metadata().id()])
            .await
            .unwrap(),
        [orphan]
    );
    assert_eq!(
        index
            .frames_by_id(vec![dangling.metadata().id()])
            .await
            .unwrap_err()
            .code,
        ErrorCode::NotFound
    );
}

#[tokio::test]
async fn damaged_sealed_footer_is_repaired_without_reindexing_valid_frames() {
    let fixture = Fixture::new();
    let session = SessionId::from_uuid(id(90));
    let target = TargetId::from_uuid(id(91));
    let expected = frame(session, target, 92, 1);
    let (index, addresses) =
        indexed_segment(&fixture, session, target, std::slice::from_ref(&expected)).await;
    let path = sealed_segment_path(&fixture.segments_directory(), addresses[0].segment_id);
    let original_len = fs::metadata(&path).unwrap().len();
    fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(original_len - 10)
        .unwrap();

    let report = recover(index.as_ref()).unwrap();
    assert_eq!(report.segments_repaired, 1);
    assert!(report.bytes_truncated > 0);
    assert_eq!(report.frames_removed, 0);
    assert_eq!(report.frames_recovered, 0);
    assert_eq!(
        index
            .frames_by_id(vec![expected.metadata().id()])
            .await
            .unwrap(),
        [expected]
    );
    assert_eq!(fs::metadata(path).unwrap().len(), original_len);
}

#[test]
fn file_operation_failures_are_persistence_failures() {
    let fixture = Fixture::new();
    let index = fixture.index();
    let segment_id = SegmentId::from_uuid(id(93));
    fs::create_dir(
        fixture
            .segments_directory()
            .join(format!("{segment_id}.open")),
    )
    .unwrap();
    let error = recover(index.as_ref()).unwrap_err();
    assert_eq!(error.code, ErrorCode::PersistenceFailed);
}

#[test]
fn unreadable_segment_root_is_a_shutdown_incomplete_boundary() {
    let fixture = Fixture::new();
    let index = fixture.index();
    fs::remove_dir(fixture.segments_directory()).unwrap();
    fs::write(fixture.segments_directory(), b"not a directory").unwrap();
    let error = recover(index.as_ref()).unwrap_err();
    assert_eq!(error.code, ErrorCode::ShutdownIncomplete);
}
