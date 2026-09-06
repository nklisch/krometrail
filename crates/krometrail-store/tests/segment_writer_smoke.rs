use std::{fs, sync::Arc, time::Duration};

use krometrail_core::{
    CaptureOrdinal, CapturedFrame, DeviceScaleFactor, EncodedFrame, ErrorCode, FrameAddress,
    FrameId, ImageFormat, ObservedTime, PixelDimensions, SessionId, SessionTime, SourceTime,
    TargetId,
};
use krometrail_store::{
    RotationConfig, SegmentStoreConfig, SegmentWriter,
    segments::{
        SEALED_FOOTER_LEN, SEGMENT_HEADER_LEN, SealedFooter, read_frame_at, sealed_segment_path,
    },
};
use tempfile::TempDir;
use uuid::Uuid;

fn writer(directory: &TempDir, rotation: RotationConfig) -> Arc<SegmentWriter> {
    Arc::new(
        SegmentWriter::open(SegmentStoreConfig {
            directory: directory.path().to_path_buf(),
            rotation,
        })
        .unwrap(),
    )
}

fn frame(
    session_id: SessionId,
    target_id: TargetId,
    ordinal: u64,
    session_nanos: u64,
    payload: &[u8],
) -> EncodedFrame {
    EncodedFrame::new(
        CapturedFrame::new(
            FrameId::from_uuid(Uuid::from_u128(1_000 + u128::from(ordinal))),
            session_id,
            target_id,
            CaptureOrdinal::new(ordinal).unwrap(),
            Some(SourceTime::from_nanos(i128::from(session_nanos))),
            ObservedTime::from_nanos(session_nanos + 100),
            SessionTime::from_nanos(session_nanos),
            if ordinal.is_multiple_of(2) {
                ImageFormat::Png
            } else {
                ImageFormat::Jpeg
            },
            PixelDimensions::new(640, 480).unwrap(),
            PixelDimensions::new(640, 480).unwrap(),
            DeviceScaleFactor::new(2.0).unwrap(),
            vec![],
        )
        .unwrap(),
        payload.to_vec(),
    )
    .unwrap()
}

fn sealed_bytes(directory: &TempDir, address: FrameAddress) -> Vec<u8> {
    fs::read(sealed_segment_path(directory.path(), address.segment_id)).unwrap()
}

fn footer(bytes: &[u8]) -> SealedFooter {
    SealedFooter::decode(&bytes[bytes.len() - SEALED_FOOTER_LEN..]).unwrap()
}

#[tokio::test]
async fn real_writes_round_trip_two_target_streams_by_address() {
    let directory = TempDir::new().unwrap();
    let sink = writer(&directory, RotationConfig::suggested());
    let session_id = SessionId::from_uuid(Uuid::from_u128(10));
    let targets = [
        TargetId::from_uuid(Uuid::from_u128(20)),
        TargetId::from_uuid(Uuid::from_u128(21)),
    ];
    let expected = [
        frame(session_id, targets[0], 1, 10, &[1, 2, 3]),
        frame(session_id, targets[1], 1, 11, &[4, 5]),
        frame(session_id, targets[0], 2, 12, &[6]),
        frame(session_id, targets[1], 2, 13, &[7, 8, 9, 10]),
    ];

    let mut addresses = Vec::new();
    for value in expected.iter().cloned() {
        addresses.push(sink.append_indexable(value).await.unwrap().address);
    }
    sink.flush_indexable(session_id).await.unwrap();

    assert_ne!(addresses[0].segment_id, addresses[1].segment_id);
    assert_eq!(addresses[0].segment_id, addresses[2].segment_id);
    assert_eq!(addresses[1].segment_id, addresses[3].segment_id);
    assert!(addresses[0].byte_offset < addresses[2].byte_offset);
    assert!(addresses[1].byte_offset < addresses[3].byte_offset);
    for (expected, address) in expected.iter().zip(addresses) {
        let bytes = sealed_bytes(&directory, address);
        assert_eq!(read_frame_at(&bytes, address).unwrap(), *expected);
    }
}

#[tokio::test]
async fn rotates_before_triggering_frame_for_duration_or_size() {
    for rotation in [
        RotationConfig {
            max_duration: Duration::from_nanos(5),
            max_size: u64::MAX,
        },
        RotationConfig {
            max_duration: Duration::from_secs(60),
            max_size: SEGMENT_HEADER_LEN as u64 + 1,
        },
    ] {
        let directory = TempDir::new().unwrap();
        let sink = writer(&directory, rotation);
        let session_id = SessionId::from_uuid(Uuid::new_v4());
        let target_id = TargetId::from_uuid(Uuid::new_v4());
        let first = sink
            .append_indexable(frame(session_id, target_id, 1, 0, &[1, 2, 3]))
            .await
            .unwrap()
            .address;
        let second = sink
            .append_indexable(frame(session_id, target_id, 2, 10, &[4, 5, 6]))
            .await
            .unwrap()
            .address;
        assert_ne!(first.segment_id, second.segment_id);
        let first_bytes = sealed_bytes(&directory, first);
        assert_eq!(footer(&first_bytes).record_count, 1);
        assert_eq!(
            read_frame_at(&first_bytes, first).unwrap().bytes(),
            &[1, 2, 3]
        );

        sink.flush_indexable(session_id).await.unwrap();
        let second_bytes = sealed_bytes(&directory, second);
        assert_eq!(footer(&second_bytes).record_count, 1);
        assert_eq!(
            read_frame_at(&second_bytes, second).unwrap().bytes(),
            &[4, 5, 6]
        );
    }
}

#[tokio::test]
async fn flush_seals_all_session_targets_with_accurate_footer_summaries() {
    let directory = TempDir::new().unwrap();
    let sink = writer(&directory, RotationConfig::suggested());
    let session_id = SessionId::from_uuid(Uuid::from_u128(30));
    let target_a = TargetId::from_uuid(Uuid::from_u128(31));
    let target_b = TargetId::from_uuid(Uuid::from_u128(32));

    let sink_a = Arc::clone(&sink);
    let a = tokio::spawn(async move {
        let first = sink_a
            .append_indexable(frame(session_id, target_a, 1, 5, &[1, 2]))
            .await
            .unwrap()
            .address;
        sink_a
            .append_indexable(frame(session_id, target_a, 2, 8, &[3, 4, 5]))
            .await
            .unwrap();
        first
    });
    let sink_b = Arc::clone(&sink);
    let b = tokio::spawn(async move {
        sink_b
            .append_indexable(frame(session_id, target_b, 1, 6, &[6]))
            .await
            .unwrap()
            .address
    });
    let (address_a, address_b) = (a.await.unwrap(), b.await.unwrap());
    sink.flush_indexable(session_id).await.unwrap();

    let footer_a = footer(&sealed_bytes(&directory, address_a));
    assert_eq!(footer_a.record_count, 2);
    assert_eq!(footer_a.total_payload, 5);
    assert_eq!(footer_a.first_session_time, SessionTime::from_nanos(5));
    assert_eq!(footer_a.last_session_time, SessionTime::from_nanos(8));

    let footer_b = footer(&sealed_bytes(&directory, address_b));
    assert_eq!(footer_b.record_count, 1);
    assert_eq!(footer_b.total_payload, 1);
    assert_eq!(footer_b.first_session_time, SessionTime::from_nanos(6));
    assert_eq!(footer_b.last_session_time, SessionTime::from_nanos(6));
}

#[test]
fn open_creates_directories_and_rejects_a_file_as_the_directory() {
    let root = TempDir::new().unwrap();
    let nested = root.path().join("nested/segments");
    SegmentWriter::open(SegmentStoreConfig {
        directory: nested.clone(),
        rotation: RotationConfig::suggested(),
    })
    .unwrap();
    assert!(nested.is_dir());

    let file = root.path().join("not-a-directory");
    fs::write(&file, b"occupied").unwrap();
    let error = SegmentWriter::open(SegmentStoreConfig {
        directory: file,
        rotation: RotationConfig::suggested(),
    })
    .err()
    .expect("file path cannot be a segment directory");
    assert_eq!(error.code, ErrorCode::PersistenceFailed);
}
