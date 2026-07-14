use std::{
    num::{NonZeroU32, NonZeroUsize},
    sync::Arc,
    time::Duration,
};

use krometrail_core::{
    ArtifactCacheKey, ArtifactCacheMetadata, ArtifactLookup, ArtifactMarkerId, ArtifactPublication,
    ArtifactPublish, ArtifactSourceFingerprint, ArtifactStore, CaptureOrdinal, CapturedFrame,
    DeviceScaleFactor, EncodedFrame, FrameId, ImageFormat, NonEmptyText, ObservedTime,
    PixelDimensions as CoreDimensions, RecordingSink, RetentionStore, SessionId, SessionTime,
    TargetId,
};
use krometrail_store::{
    IndexStoreConfig, RecordingStore, RotationConfig, SegmentStoreConfig, SegmentWriter,
    SqliteIndex,
};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use temporal_vision::{
    ArtifactKind, ArtifactLabels, DeclaredGap, Frame, FrameSequence, IntegerScale, Marker,
    MeasurementParameters, NormalizationParameters, PixelDimensions, PixelFormat, ProcessingLimits,
    RenderLimits, Rgb8, StoryboardParameters, StoryboardTileLimit, Timestamp, generate_storyboard,
    generator_descriptor, normalize_sequence,
};
use uuid::Uuid;

struct Fixture {
    directory: TempDir,
    store: Arc<RecordingStore>,
    session: SessionId,
    target: TargetId,
    frame_ids: Vec<FrameId>,
    source_bytes: Vec<Vec<u8>>,
}

fn open_store(directory: &std::path::Path) -> Arc<RecordingStore> {
    let segments = directory.join("segments");
    let index = Arc::new(
        SqliteIndex::open(IndexStoreConfig {
            database_path: directory.join("index.sqlite3"),
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
    Arc::new(RecordingStore::new(writer, index).unwrap())
}

async fn fixture() -> Fixture {
    let directory = TempDir::new().unwrap();
    let store = open_store(directory.path());
    let session = SessionId::from_uuid(Uuid::from_u128(1));
    let target = TargetId::from_uuid(Uuid::from_u128(2));
    let frame_ids: Vec<_> = (3..=5)
        .map(|id| FrameId::from_uuid(Uuid::from_u128(id)))
        .collect();
    let source_bytes = vec![
        b"encoded-jpeg-a".to_vec(),
        b"encoded-jpeg-b".to_vec(),
        b"encoded-jpeg-c".to_vec(),
    ];
    for (position, (frame_id, bytes)) in frame_ids.iter().zip(&source_bytes).enumerate() {
        let ordinal = u64::try_from(position + 1).unwrap();
        let metadata = CapturedFrame::new(
            *frame_id,
            session,
            target,
            CaptureOrdinal::new(ordinal).unwrap(),
            None,
            ObservedTime::from_nanos(ordinal),
            SessionTime::from_nanos(ordinal),
            ImageFormat::Jpeg,
            CoreDimensions::new(1, 1).unwrap(),
            CoreDimensions::new(1, 1).unwrap(),
            DeviceScaleFactor::new(1.0).unwrap(),
            vec![],
        )
        .unwrap();
        store
            .append_frame(EncodedFrame::new(metadata, bytes.clone()).unwrap())
            .await
            .unwrap();
    }
    store.flush(session).await.unwrap();
    Fixture {
        directory,
        store,
        session,
        target,
        frame_ids,
        source_bytes,
    }
}

fn publication(fixture: &Fixture, artifact_id: u128, key: u8) -> ArtifactPublication {
    let dimensions = PixelDimensions::new(1, 1).unwrap();
    let sequence = FrameSequence::new(
        fixture
            .frame_ids
            .iter()
            .enumerate()
            .map(|(position, id)| {
                Frame::new(
                    *id,
                    Timestamp::from_nanos(u64::try_from(position + 1).unwrap()),
                    dimensions,
                    PixelFormat::Rgba8SrgbStraight,
                    vec![position as u8 * 80, 0, 0, 255].into_boxed_slice(),
                )
                .unwrap()
            })
            .collect(),
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
        krometrail_core::ArtifactId::from_uuid(Uuid::from_u128(artifact_id)),
        None,
        &sequence,
        &normalized,
        StoryboardParameters::new(
            Timestamp::from_nanos(2),
            StoryboardTileLimit::new(3).unwrap(),
            MeasurementParameters::new(0),
            ArtifactLabels::new("story", "fixture").unwrap(),
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
    let sources: Vec<_> = fixture
        .frame_ids
        .iter()
        .zip(&fixture.source_bytes)
        .map(|(frame_id, bytes)| ArtifactSourceFingerprint {
            frame_id: *frame_id,
            encoded_sha256: Sha256::digest(bytes).into(),
        })
        .collect();
    ArtifactPublication::new(
        fixture.session,
        fixture.target,
        sources,
        ArtifactCacheMetadata {
            cache_key: ArtifactCacheKey::from_bytes([key; 32]),
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

#[tokio::test]
async fn publication_lookup_corruption_and_usage_share_one_authority() {
    let fixture = fixture().await;
    let publication = publication(&fixture, 20, 21);
    let expected_len = publication.encoded_bytes.len() as u64;
    assert!(matches!(
        fixture
            .store
            .publish_artifact(publication.clone())
            .await
            .unwrap(),
        ArtifactPublish::Published(_)
    ));
    let hit = fixture
        .store
        .lookup_artifact(publication.cache.cache_key, publication.sources.clone())
        .await
        .unwrap();
    let ArtifactLookup::Hit(hit) = hit else {
        panic!("ready artifact must be visible")
    };
    assert_eq!(hit.manifest, publication.manifest);
    assert_eq!(hit.encoded_bytes, publication.encoded_bytes);
    assert_eq!(
        fixture.store.status().await.unwrap().usage.artifact_bytes,
        expected_len
    );

    let path = fixture
        .directory
        .path()
        .join("artifacts")
        .join(format!("{}.png", publication.manifest.artifact_id()));
    std::fs::write(&path, b"corrupt").unwrap();
    assert_eq!(
        fixture
            .store
            .lookup_artifact(publication.cache.cache_key, publication.sources.clone())
            .await
            .unwrap(),
        ArtifactLookup::Invalidated,
    );
    assert!(!path.exists());
    assert!(matches!(
        fixture.store.publish_artifact(publication).await.unwrap(),
        ArtifactPublish::Published(_)
    ));
}

#[tokio::test]
async fn equal_cache_key_publications_have_one_ready_winner() {
    let fixture = fixture().await;
    let publication = publication(&fixture, 30, 31);
    let (first, second) = tokio::join!(
        fixture.store.publish_artifact(publication.clone()),
        fixture.store.publish_artifact(publication.clone()),
    );
    let outcomes = [first.unwrap(), second.unwrap()];
    assert_eq!(
        outcomes
            .iter()
            .filter(|value| matches!(value, ArtifactPublish::Published(_)))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|value| matches!(value, ArtifactPublish::Existing(_)))
            .count(),
        1
    );
    let loaded = fixture
        .store
        .artifact(*publication.manifest.artifact_id())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(loaded.encoded_bytes, publication.encoded_bytes);
}

#[tokio::test]
async fn startup_finalizes_durable_staging_and_invalidates_corruption_idempotently() {
    let fixture = fixture().await;
    let publication = publication(&fixture, 40, 41);
    fixture
        .store
        .publish_artifact(publication.clone())
        .await
        .unwrap();
    let database = fixture.directory.path().join("index.sqlite3");
    let artifact_path = fixture
        .directory
        .path()
        .join("artifacts")
        .join(format!("{}.png", publication.manifest.artifact_id()));
    let orphan_temp = fixture.directory.path().join("artifacts").join(format!(
        "{}.tmp",
        krometrail_core::ArtifactId::from_uuid(Uuid::from_u128(999))
    ));
    drop(fixture.store);

    let connection = rusqlite::Connection::open(&database).unwrap();
    connection
        .execute(
            "UPDATE artifacts SET state='staging' WHERE cache_key=?1",
            [publication.cache.cache_key.as_bytes().to_vec()],
        )
        .unwrap();
    drop(connection);
    std::fs::write(&orphan_temp, b"orphan").unwrap();

    let reopened = open_store(fixture.directory.path());
    assert!(matches!(
        reopened
            .lookup_artifact(publication.cache.cache_key, publication.sources.clone())
            .await
            .unwrap(),
        ArtifactLookup::Hit(_)
    ));
    assert!(!orphan_temp.exists());
    drop(reopened);

    std::fs::write(&artifact_path, b"corrupt").unwrap();
    let recovered = open_store(fixture.directory.path());
    assert_eq!(
        recovered
            .lookup_artifact(publication.cache.cache_key, publication.sources.clone())
            .await
            .unwrap(),
        ArtifactLookup::Miss
    );
    assert!(!artifact_path.exists());
    drop(recovered);

    let second_pass = open_store(fixture.directory.path());
    assert_eq!(
        second_pass
            .lookup_artifact(publication.cache.cache_key, publication.sources)
            .await
            .unwrap(),
        ArtifactLookup::Miss
    );
}
