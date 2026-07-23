use std::{
    num::{NonZeroU32, NonZeroUsize},
    sync::Arc,
    time::Duration,
};

use krometrail_core::{
    ArtifactCacheKey, ArtifactCacheMetadata, ArtifactId, ArtifactLookup, ArtifactMarkerId,
    ArtifactPublication, ArtifactPublish, ArtifactSourceFingerprint, ArtifactStore, CaptureOrdinal,
    CapturedFrame, DeviceScaleFactor, DiskBudgetBytes, EncodedFrame, FrameId, FrameSource,
    ImageFormat, MonotonicClock, NonEmptyText, ObservedTime, PixelDimensions, RecordingBudgetState,
    RecordingSink, RetentionLifecycle, RetentionRange, RetentionStore, SessionId, SessionRange,
    SessionTime, TargetId,
};
use krometrail_store::{
    IndexStoreConfig, RecordingStore, RotationConfig, SegmentStoreConfig, SegmentWriter,
    SqliteIndex,
};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use temporal_vision::{
    ArtifactKind, ArtifactLabels, DeclaredGap, Frame, FrameSequence, IntegerScale, Marker,
    MeasurementParameters, NormalizationParameters, PixelFormat, ProcessingLimits, RenderLimits,
    Rgb8, StoryboardParameters, StoryboardTileLimit, Timestamp, generate_storyboard,
    generator_descriptor, normalize_sequence,
};
use tokio::time::timeout;
use uuid::Uuid;

struct Fixture {
    directory: TempDir,
    index: Arc<SqliteIndex>,
    writer: Arc<SegmentWriter>,
    session: SessionId,
    target: TargetId,
    frame_ids: Vec<FrameId>,
    source_bytes: Vec<Vec<u8>>,
}

fn store_test_clock() -> Arc<dyn MonotonicClock> {
    struct Fixed;
    impl MonotonicClock for Fixed {
        fn now(&self) -> ObservedTime {
            ObservedTime::from_nanos(0)
        }
    }
    Arc::new(Fixed)
}

fn frame(session: SessionId, target: TargetId, id: FrameId, ordinal: u64) -> EncodedFrame {
    EncodedFrame::new(
        CapturedFrame::new(
            id,
            session,
            target,
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
        vec![ordinal as u8; 40_000],
    )
    .unwrap()
}

async fn fixture(rotation_size: u64, count: u128) -> Fixture {
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
    let store =
        RecordingStore::new(Arc::clone(&writer), Arc::clone(&index), store_test_clock()).unwrap();
    let session = SessionId::from_uuid(Uuid::from_u128(1));
    let target = TargetId::from_uuid(Uuid::from_u128(2));
    let frame_ids = (0..count)
        .map(|offset| FrameId::from_uuid(Uuid::from_u128(100 + offset)))
        .collect::<Vec<_>>();
    let source_bytes = frame_ids
        .iter()
        .enumerate()
        .map(|(offset, _)| vec![(offset + 1) as u8; 40_000])
        .collect::<Vec<_>>();
    for (offset, frame_id) in frame_ids.iter().enumerate() {
        store
            .append_frame(frame(session, target, *frame_id, (offset + 1) as u64))
            .await
            .unwrap();
    }
    store.flush(session).await.unwrap();
    drop(store);
    Fixture {
        directory,
        index,
        writer,
        session,
        target,
        frame_ids,
        source_bytes,
    }
}

fn open_with_budget(fixture: &Fixture, budget: DiskBudgetBytes) -> RecordingStore {
    RecordingStore::with_budget(
        Arc::clone(&fixture.writer),
        Arc::clone(&fixture.index),
        budget,
        store_test_clock(),
    )
    .unwrap()
}

fn open_with_retention(
    fixture: &Fixture,
    budget: DiskBudgetBytes,
    trim_high_water_percent: u8,
) -> RecordingStore {
    RecordingStore::with_retention(
        Arc::clone(&fixture.writer),
        Arc::clone(&fixture.index),
        RetentionLifecycle::new(budget, None, trim_high_water_percent, Duration::ZERO).unwrap(),
        None,
        store_test_clock(),
    )
    .unwrap()
}

fn artifact_publication(
    fixture: &Fixture,
    frame_index: usize,
    artifact_id: u128,
    cache_key: u8,
) -> ArtifactPublication {
    let dimensions = temporal_vision::PixelDimensions::new(1, 1).unwrap();
    let source_frame = Frame::new(
        fixture.frame_ids[frame_index],
        Timestamp::from_nanos((frame_index + 1) as u64),
        dimensions,
        PixelFormat::Rgba8SrgbStraight,
        vec![frame_index as u8, 0, 0, 255].into_boxed_slice(),
    )
    .unwrap();
    let sequence = FrameSequence::new(
        vec![source_frame],
        Vec::<Marker<ArtifactMarkerId>>::new(),
        Vec::<DeclaredGap<krometrail_core::GapId>>::new(),
        None,
        None,
    )
    .unwrap();
    let normalized = normalize_sequence(
        &sequence,
        NormalizationParameters::new(
            Rgb8::new(0, 0, 0),
            None,
            IntegerScale::IDENTITY,
            ProcessingLimits::new(
                NonZeroUsize::new(3).unwrap(),
                NonZeroUsize::new(1).unwrap(),
                NonZeroUsize::new(18).unwrap(),
            ),
        ),
    )
    .unwrap();
    let generated = generate_storyboard(
        ArtifactId::from_uuid(Uuid::from_u128(artifact_id)),
        None,
        &sequence,
        &normalized,
        StoryboardParameters::new(
            Timestamp::from_nanos((frame_index + 1) as u64),
            StoryboardTileLimit::new(3).unwrap(),
            MeasurementParameters::new(0),
            ArtifactLabels::new("retention", "grace-ordering").unwrap(),
            RenderLimits::new(
                NonZeroU32::new(1024).unwrap(),
                NonZeroU32::new(1024).unwrap(),
                NonZeroUsize::new(8 * 1024 * 1024).unwrap(),
                NonZeroUsize::new(8 * 1024 * 1024).unwrap(),
            ),
        ),
    )
    .unwrap();
    let artifact = generated.storyboard();
    let descriptor = generator_descriptor(ArtifactKind::Storyboard);
    ArtifactPublication::new(
        fixture.session,
        fixture.target,
        vec![ArtifactSourceFingerprint {
            frame_id: fixture.frame_ids[frame_index],
            encoded_sha256: Sha256::digest(&fixture.source_bytes[frame_index]).into(),
        }],
        ArtifactCacheMetadata {
            cache_key: ArtifactCacheKey::from_bytes([cache_key; 32]),
            source_fingerprint: [11; 32],
            parameter_hash: [12; 32],
            visual_epoch_hash: [13; 32],
            cache_schema_version: 1,
            adapter_version: NonEmptyText::new("adapter-v1").unwrap(),
            generator_name: NonEmptyText::new(descriptor.name).unwrap(),
            generator_version: NonEmptyText::new(descriptor.version).unwrap(),
        },
        artifact.manifest().clone(),
        NonEmptyText::new("image/png").unwrap(),
        artifact.image().bytes().to_vec(),
    )
    .unwrap()
}

fn segment_file_bytes(directory: &TempDir) -> Vec<u64> {
    let connection = Connection::open(directory.path().join("index.sqlite3")).unwrap();
    let mut statement = connection
        .prepare("SELECT file_bytes_be FROM segments ORDER BY retention_sequence")
        .unwrap();
    statement
        .query_map([], |row| {
            let bytes: Vec<u8> = row.get(0)?;
            Ok(u64::from_be_bytes(bytes.try_into().unwrap()))
        })
        .unwrap()
        .map(|value| value.unwrap())
        .collect()
}

#[tokio::test]
async fn grace_skips_oldest_artifact_segment_and_reclaims_newer_segments() {
    let fixture = fixture(1, 3).await;
    let unbounded = RecordingStore::new(
        Arc::clone(&fixture.writer),
        Arc::clone(&fixture.index),
        store_test_clock(),
    )
    .unwrap();
    let publication = artifact_publication(&fixture, 0, 200, 201);
    unbounded
        .publish_artifact(publication.clone())
        .await
        .unwrap();
    let before = unbounded
        .status()
        .await
        .unwrap()
        .usage
        .total_bytes()
        .unwrap();
    drop(unbounded);

    let segments = segment_file_bytes(&fixture.directory);
    assert_eq!(
        segments.len(),
        3,
        "each source frame must have its own segment"
    );
    let budget = DiskBudgetBytes::new(before - segments[1] - segments[2]).unwrap();
    let store = open_with_budget(&fixture, budget);
    store.enforce_budget().await.unwrap();

    assert!(matches!(
        store
            .lookup_artifact(publication.cache.cache_key, publication.sources.clone())
            .await
            .unwrap(),
        ArtifactLookup::Hit(_)
    ));
    assert!(
        fixture
            .index
            .frames_by_id(vec![fixture.frame_ids[0]])
            .await
            .is_ok()
    );
    assert!(
        fixture
            .index
            .frames_by_id(vec![fixture.frame_ids[1]])
            .await
            .is_err()
    );
    assert!(
        fixture
            .index
            .frames_by_id(vec![fixture.frame_ids[2]])
            .await
            .is_err()
    );
}

#[tokio::test]
async fn pinned_grace_override_is_reported_by_the_publishing_operation() {
    let fixture = fixture(1, 2).await;
    let unbounded = RecordingStore::new(
        Arc::clone(&fixture.writer),
        Arc::clone(&fixture.index),
        store_test_clock(),
    )
    .unwrap();
    let first = artifact_publication(&fixture, 0, 210, 211);
    unbounded.publish_artifact(first.clone()).await.unwrap();
    unbounded
        .pin_range(RetentionRange {
            session_id: fixture.session,
            target_id: fixture.target,
            range: SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(3)).unwrap(),
        })
        .await
        .unwrap();
    let before = unbounded
        .status()
        .await
        .unwrap()
        .usage
        .total_bytes()
        .unwrap();
    let budget = DiskBudgetBytes::new(
        before.saturating_add(u64::try_from(first.encoded_bytes.len()).unwrap() / 2),
    )
    .unwrap();
    drop(unbounded);

    let store = open_with_budget(&fixture, budget);
    let second = artifact_publication(&fixture, 1, 220, 221);
    let ArtifactPublish::Published(_, grace_overridden) =
        store.publish_artifact(second.clone()).await.unwrap()
    else {
        panic!("a new publication must publish rather than hit the cache")
    };
    assert!(
        grace_overridden,
        "the publication must report its own override"
    );
    assert_eq!(
        store
            .lookup_artifact(first.cache.cache_key, first.sources.clone())
            .await
            .unwrap(),
        ArtifactLookup::Miss
    );
    assert!(matches!(
        store
            .lookup_artifact(second.cache.cache_key, second.sources.clone())
            .await
            .unwrap(),
        ArtifactLookup::Hit(_)
    ));
    assert!(
        fixture
            .index
            .frames_by_id(fixture.frame_ids.clone())
            .await
            .is_ok(),
        "absolute pins must preserve every source segment"
    );
}

#[tokio::test]
async fn pinned_trim_exhaustion_short_circuits_repeated_pressure_walks() {
    let fixture = fixture(1_000_000, 1).await;
    let pinned = RecordingStore::new(
        Arc::clone(&fixture.writer),
        Arc::clone(&fixture.index),
        store_test_clock(),
    )
    .unwrap();
    pinned
        .pin_range(RetentionRange {
            session_id: fixture.session,
            target_id: fixture.target,
            range: SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(2)).unwrap(),
        })
        .await
        .unwrap();
    let baseline = pinned.status().await.unwrap().usage.total_bytes().unwrap();
    let budget = DiskBudgetBytes::new(baseline.saturating_mul(2)).unwrap();
    drop(pinned);

    let store = open_with_retention(&fixture, budget, 50);
    for ordinal in 2..=4_u64 {
        let append = store.append_frame(frame(
            fixture.session,
            fixture.target,
            FrameId::from_uuid(Uuid::from_u128(300 + u128::from(ordinal))),
            ordinal,
        ));
        timeout(Duration::from_secs(2), append)
            .await
            .expect("pinned trim exhaustion must not loop forever")
            .unwrap();
    }
    assert!(
        fixture
            .index
            .frames_by_id(vec![fixture.frame_ids[0]])
            .await
            .is_ok()
    );
    assert_eq!(
        store.status().await.unwrap().budget_state,
        RecordingBudgetState::Available
    );
    assert!(store.status().await.unwrap().pinned_usage_bytes > 0);
}
