use std::{
    num::{NonZeroU32, NonZeroUsize},
    sync::Arc,
    time::Duration,
};

use krometrail_core::{
    ArtifactCacheKey, ArtifactCacheMetadata, ArtifactLookup, ArtifactMarkerId, ArtifactPublication,
    ArtifactPublish, ArtifactReadLookup, ArtifactSourceFingerprint, ArtifactStore,
    CancellationSignal, CaptureOrdinal, CapturedFrame, DeviceScaleFactor, DiskBudgetBytes,
    EncodedFrame, EvidenceScope, FrameId, FrameSource, ImageFormat, NonEmptyText, ObservedTime,
    PixelDimensions as CoreDimensions, PortFuture, RecordingSink, RetentionPinRequest,
    RetentionRange, RetentionStore, RetrieveArtifactRequest, SessionId, SessionRange, SessionTime,
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
    open_store_with_budget(directory, None)
}

fn open_store_with_budget(
    directory: &std::path::Path,
    budget: Option<DiskBudgetBytes>,
) -> Arc<RecordingStore> {
    let segments = directory.join("segments");
    let index = Arc::new(
        SqliteIndex::open(IndexStoreConfig {
            database_path: directory.join("index.sqlite3"),
            segments_directory: segments.clone(),
            busy_timeout: Duration::from_secs(1),
        })
        .unwrap(),
    );
    let rotation = if budget.is_some() {
        RotationConfig {
            max_duration: Duration::from_secs(60),
            max_size: 1,
        }
    } else {
        RotationConfig::suggested()
    };
    let writer = Arc::new(
        SegmentWriter::open(SegmentStoreConfig {
            directory: segments,
            rotation,
        })
        .unwrap(),
    );
    Arc::new(match budget {
        Some(budget) => RecordingStore::with_budget(writer, index, budget).unwrap(),
        None => RecordingStore::new(writer, index).unwrap(),
    })
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

fn artifact_row_values(
    connection: &rusqlite::Connection,
    artifact_id: krometrail_core::ArtifactId,
) -> Vec<rusqlite::types::Value> {
    connection
        .query_row(
            "SELECT artifact_id,session_id,target_id,state,kind,start_time_be,end_time_be,\
                    manifest_json,manifest_hash,media_type,output_hash,relative_path,byte_len_be,\
                    cache_key,source_fingerprint,parameter_hash,visual_epoch_hash,\
                    cache_schema_version,adapter_version,generator_name,generator_version \
             FROM artifacts WHERE artifact_id=?1",
            [artifact_id.as_uuid().as_bytes().to_vec()],
            |row| (0..21).map(|column| row.get(column)).collect(),
        )
        .unwrap()
}

fn downgrade_artifact_tables_to_v5(connection: &rusqlite::Connection) {
    connection
        .execute_batch(
            r#"
            PRAGMA foreign_keys=OFF;
            DROP INDEX artifact_range_idx;
            DROP INDEX artifact_ready_cache_idx;
            DROP INDEX artifact_source_frame_idx;
            ALTER TABLE artifact_frames RENAME TO artifact_frames_v6_fixture;
            ALTER TABLE artifacts RENAME TO artifacts_v6_fixture;
            CREATE TABLE artifacts (
                artifact_id BLOB PRIMARY KEY CHECK(length(artifact_id)=16),
                session_id BLOB NOT NULL CHECK(length(session_id)=16),
                target_id BLOB NOT NULL CHECK(length(target_id)=16),
                state TEXT NOT NULL CHECK(state IN ('staging','ready')),
                kind TEXT NOT NULL CHECK(kind IN (
                    'before_during_after','storyboard','difference_map','region_filmstrip','motion_history'
                )),
                start_time_be BLOB NOT NULL CHECK(length(start_time_be)=8),
                end_time_be BLOB NOT NULL CHECK(length(end_time_be)=8),
                manifest_json TEXT NOT NULL,
                manifest_hash BLOB NOT NULL CHECK(length(manifest_hash)=32),
                media_type TEXT NOT NULL CHECK(media_type='image/png'),
                output_hash BLOB NOT NULL CHECK(length(output_hash)=32),
                relative_path TEXT NOT NULL UNIQUE,
                byte_len_be BLOB NOT NULL CHECK(length(byte_len_be)=8),
                cache_key BLOB NOT NULL UNIQUE CHECK(length(cache_key)=32),
                source_fingerprint BLOB NOT NULL CHECK(length(source_fingerprint)=32),
                parameter_hash BLOB NOT NULL CHECK(length(parameter_hash)=32),
                visual_epoch_hash BLOB NOT NULL CHECK(length(visual_epoch_hash)=32),
                cache_schema_version INTEGER NOT NULL CHECK(cache_schema_version>0),
                adapter_version TEXT NOT NULL CHECK(length(adapter_version)>0),
                generator_name TEXT NOT NULL CHECK(length(generator_name)>0),
                generator_version TEXT NOT NULL CHECK(length(generator_version)>0),
                FOREIGN KEY(session_id,target_id) REFERENCES targets(session_id,target_id) ON DELETE CASCADE
            ) STRICT;
            INSERT INTO artifacts SELECT * FROM artifacts_v6_fixture;
            CREATE TABLE artifact_frames (
                artifact_id BLOB NOT NULL CHECK(length(artifact_id)=16),
                source_position INTEGER NOT NULL CHECK(source_position>=0),
                frame_id BLOB NOT NULL CHECK(length(frame_id)=16),
                encoded_hash BLOB NOT NULL CHECK(length(encoded_hash)=32),
                PRIMARY KEY(artifact_id,source_position),
                UNIQUE(artifact_id,frame_id),
                FOREIGN KEY(artifact_id) REFERENCES artifacts(artifact_id) ON DELETE CASCADE,
                FOREIGN KEY(frame_id) REFERENCES frames(frame_id)
            ) STRICT;
            INSERT INTO artifact_frames SELECT * FROM artifact_frames_v6_fixture;
            DROP TABLE artifact_frames_v6_fixture;
            DROP TABLE artifacts_v6_fixture;
            CREATE INDEX artifact_range_idx ON artifacts(session_id,target_id,state,start_time_be,end_time_be);
            CREATE INDEX artifact_ready_cache_idx ON artifacts(cache_key) WHERE state='ready';
            CREATE INDEX artifact_source_frame_idx ON artifact_frames(frame_id,artifact_id,source_position);
            PRAGMA user_version=5;
            PRAGMA foreign_keys=ON;
            "#,
        )
        .unwrap();
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
async fn valid_v5_image_fixture_migrates_and_reads_byte_identically() {
    let fixture = fixture().await;
    let publication = publication(&fixture, 2_000, 91);
    fixture
        .store
        .publish_artifact(publication.clone())
        .await
        .unwrap();
    let artifact_id = *publication.manifest.artifact_id();
    let manifest_json = serde_json::to_string(&publication.manifest).unwrap();
    let manifest_sha256: [u8; 32] = Sha256::digest(manifest_json.as_bytes()).into();
    assert_eq!(
        manifest_sha256,
        [
            68, 18, 201, 87, 50, 201, 246, 192, 17, 126, 155, 91, 148, 248, 28, 172, 102, 212, 93,
            78, 172, 56, 232, 225, 211, 78, 14, 185, 113, 92, 128, 178,
        ],
        "the stable image manifest JSON changed"
    );
    let database = fixture.directory.path().join("index.sqlite3");
    let directory = fixture.directory.path().to_path_buf();
    let session = fixture.session;
    let target = fixture.target;
    let source_count = publication.sources.len() as u64;
    let encoded_bytes = Arc::clone(&publication.encoded_bytes);
    let cache = publication.cache.clone();
    drop(fixture.store);

    let connection = rusqlite::Connection::open(&database).unwrap();
    downgrade_artifact_tables_to_v5(&connection);
    let before = artifact_row_values(&connection, artifact_id);
    let before_sources: u64 = connection
        .query_row(
            "SELECT count(*) FROM artifact_frames WHERE artifact_id=?1",
            [artifact_id.as_uuid().as_bytes().to_vec()],
            |row| row.get(0),
        )
        .unwrap();
    let before_usage: Vec<u8> = connection
        .query_row(
            "SELECT byte_len_be FROM usage WHERE class='artifact' AND object_key=?1",
            [artifact_id.as_uuid().as_bytes().to_vec()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(before_sources, source_count);
    assert!(
        directory
            .join("artifacts")
            .join(format!("{artifact_id}.png"))
            .is_file()
    );
    drop(connection);

    let reopened = open_store(&directory);
    let connection = rusqlite::Connection::open(&database).unwrap();
    assert_eq!(artifact_row_values(&connection, artifact_id), before);
    assert_eq!(
        connection
            .pragma_query_value::<u32, _>(None, "user_version", |row| row.get(0))
            .unwrap(),
        6
    );
    let after_usage: Vec<u8> = connection
        .query_row(
            "SELECT byte_len_be FROM usage WHERE class='artifact' AND object_key=?1",
            [artifact_id.as_uuid().as_bytes().to_vec()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(after_usage, before_usage);
    drop(connection);

    let read = reopened
        .read_artifact(
            RetrieveArtifactRequest::new(
                EvidenceScope::new(session, target).unwrap(),
                artifact_id,
                encoded_bytes.len() as u64,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let ArtifactReadLookup::Available(read) = read else {
        panic!("migrated v5 image must remain typed and readable")
    };
    assert_eq!(read.encoded_bytes(), encoded_bytes.as_ref());
    assert_eq!(read.handle.provenance, publication.manifest);
    assert_eq!(cache, publication.cache);
}

#[tokio::test]
async fn scoped_artifact_reads_distinguish_missing_limits_invalidation_and_source_corruption() {
    let fixture = fixture().await;
    let publication = publication(&fixture, 22, 23);
    fixture
        .store
        .publish_artifact(publication.clone())
        .await
        .unwrap();
    let scope = EvidenceScope::new(fixture.session, fixture.target).unwrap();
    let request = RetrieveArtifactRequest::new(
        scope,
        *publication.manifest.artifact_id(),
        publication.encoded_bytes.len() as u64,
    )
    .unwrap();
    let ArtifactReadLookup::Available(read) = fixture.store.read_artifact(request).await.unwrap()
    else {
        panic!("scoped retained artifact must be available")
    };
    assert_eq!(read.encoded_bytes(), publication.encoded_bytes.as_ref());
    assert_eq!(read.handle.scope, scope);
    assert_eq!(
        read.handle.content_sha256.as_bytes(),
        publication.manifest.output_hash().as_bytes()
    );

    let wrong_scope =
        EvidenceScope::new(fixture.session, TargetId::from_uuid(Uuid::from_u128(999))).unwrap();
    assert_eq!(
        fixture
            .store
            .read_artifact(
                RetrieveArtifactRequest::new(
                    wrong_scope,
                    *publication.manifest.artifact_id(),
                    publication.encoded_bytes.len() as u64,
                )
                .unwrap(),
            )
            .await
            .unwrap(),
        ArtifactReadLookup::Missing
    );
    assert_eq!(
        fixture
            .store
            .read_artifact(
                RetrieveArtifactRequest::new(
                    scope,
                    *publication.manifest.artifact_id(),
                    (publication.encoded_bytes.len() - 1) as u64,
                )
                .unwrap(),
            )
            .await
            .unwrap_err()
            .code,
        krometrail_core::ErrorCode::ResourceLimitExceeded
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
            .read_artifact(
                RetrieveArtifactRequest::new(
                    scope,
                    *publication.manifest.artifact_id(),
                    publication.encoded_bytes.len() as u64,
                )
                .unwrap(),
            )
            .await
            .unwrap(),
        ArtifactReadLookup::Invalidated
    );
    assert!(!path.exists());

    let source_corruption = self::publication(&fixture, 24, 25);
    fixture
        .store
        .publish_artifact(source_corruption.clone())
        .await
        .unwrap();
    let segment_path = std::fs::read_dir(fixture.directory.path().join("segments"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.is_file())
        .unwrap();
    std::fs::OpenOptions::new()
        .write(true)
        .open(segment_path)
        .unwrap()
        .set_len(8)
        .unwrap();
    assert_eq!(
        fixture
            .store
            .read_artifact(
                RetrieveArtifactRequest::new(
                    scope,
                    *source_corruption.manifest.artifact_id(),
                    source_corruption.encoded_bytes.len() as u64,
                )
                .unwrap(),
            )
            .await
            .unwrap_err()
            .code,
        krometrail_core::ErrorCode::PersistenceFailed
    );
}

#[tokio::test]
async fn malformed_manifest_hash_and_source_links_are_invalidated_as_misses() {
    for corruption in [
        "manifest",
        "manifest_hash",
        "source_link",
        "source_hash",
        "output_hash",
    ] {
        let fixture = fixture().await;
        let publication = publication(&fixture, 25, 26);
        fixture
            .store
            .publish_artifact(publication.clone())
            .await
            .unwrap();
        let connection =
            rusqlite::Connection::open(fixture.directory.path().join("index.sqlite3")).unwrap();
        match corruption {
            "manifest" => {
                connection
                    .execute(
                        "UPDATE artifacts SET manifest_json='{}' WHERE artifact_id=?1",
                        [publication
                            .manifest
                            .artifact_id()
                            .as_uuid()
                            .as_bytes()
                            .to_vec()],
                    )
                    .unwrap();
            }
            "manifest_hash" => {
                connection
                    .execute(
                        "UPDATE artifacts SET manifest_hash=?1 WHERE artifact_id=?2",
                        rusqlite::params![
                            vec![0_u8; 32],
                            publication
                                .manifest
                                .artifact_id()
                                .as_uuid()
                                .as_bytes()
                                .to_vec()
                        ],
                    )
                    .unwrap();
            }
            "source_link" => {
                connection
                    .execute(
                        "DELETE FROM artifact_frames WHERE artifact_id=?1 AND source_position=1",
                        [publication
                            .manifest
                            .artifact_id()
                            .as_uuid()
                            .as_bytes()
                            .to_vec()],
                    )
                    .unwrap();
            }
            "source_hash" => {
                connection
                    .execute(
                        "UPDATE artifact_frames SET encoded_hash=?1 WHERE artifact_id=?2 AND source_position=1",
                        rusqlite::params![
                            vec![0_u8; 32],
                            publication.manifest.artifact_id().as_uuid().as_bytes().to_vec()
                        ],
                    )
                    .unwrap();
            }
            "output_hash" => {
                connection
                    .execute(
                        "UPDATE artifacts SET output_hash=?1 WHERE artifact_id=?2",
                        rusqlite::params![
                            vec![0_u8; 32],
                            publication
                                .manifest
                                .artifact_id()
                                .as_uuid()
                                .as_bytes()
                                .to_vec()
                        ],
                    )
                    .unwrap();
            }
            _ => unreachable!(),
        }
        drop(connection);
        assert_eq!(
            fixture
                .store
                .lookup_artifact(publication.cache.cache_key, publication.sources)
                .await
                .unwrap(),
            ArtifactLookup::Invalidated,
            "{corruption} must not survive hit validation"
        );
        assert_eq!(
            fixture.store.status().await.unwrap().usage.artifact_bytes,
            0
        );
    }
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

struct AlreadyCancelled;

impl CancellationSignal for AlreadyCancelled {
    fn is_cancelled(&self) -> bool {
        true
    }

    fn cancelled(&self) -> PortFuture<'_, ()> {
        Box::pin(std::future::ready(()))
    }
}

#[tokio::test]
async fn cancelled_publication_never_creates_visible_or_accounted_state() {
    let fixture = fixture().await;
    let publication = publication(&fixture, 50, 51).with_cancellation(Arc::new(AlreadyCancelled));
    assert_eq!(
        fixture
            .store
            .publish_artifact(publication.clone())
            .await
            .unwrap_err()
            .code,
        krometrail_core::ErrorCode::Cancelled
    );
    assert_eq!(
        fixture
            .store
            .lookup_artifact(publication.cache.cache_key, publication.sources)
            .await
            .unwrap(),
        ArtifactLookup::Miss
    );
    assert_eq!(
        fixture.store.status().await.unwrap().usage.artifact_bytes,
        0
    );
    let directory = fixture.directory.path().join("artifacts");
    assert!(!directory.exists() || std::fs::read_dir(directory).unwrap().next().is_none());
}

#[tokio::test]
async fn pins_protect_sources_not_regenerable_artifacts() {
    let fixture = fixture().await;
    let publication = publication(&fixture, 60, 61);
    fixture
        .store
        .publish_artifact(publication.clone())
        .await
        .unwrap();
    let retained = RetentionRange {
        session_id: fixture.session,
        target_id: fixture.target,
        range: SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(4)).unwrap(),
    };
    let pin = fixture
        .store
        .pin_resolved_range(RetentionPinRequest::new(retained, fixture.frame_ids.clone()).unwrap())
        .await
        .unwrap();
    assert_eq!(
        pin.state.protection_scope,
        krometrail_core::PinProtectionScope::SourceSegmentsOnly
    );
    let before = fixture.store.status().await.unwrap();
    let budget = DiskBudgetBytes::new(
        before.usage.total_bytes().unwrap() - publication.encoded_bytes.len() as u64,
    )
    .unwrap();
    drop(fixture.store);

    let store = open_store_with_budget(fixture.directory.path(), Some(budget));
    store.enforce_budget().await.unwrap();
    assert!(
        store
            .artifact(*publication.manifest.artifact_id())
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        store.status().await.unwrap().pinned_usage_bytes,
        before.pinned_usage_bytes
    );
    let source = SqliteIndex::open(IndexStoreConfig {
        database_path: fixture.directory.path().join("index.sqlite3"),
        segments_directory: fixture.directory.path().join("segments"),
        busy_timeout: Duration::from_secs(1),
    })
    .unwrap();
    assert_eq!(
        source
            .frames_by_id(fixture.frame_ids.clone())
            .await
            .unwrap()
            .len(),
        fixture.frame_ids.len()
    );

    store.publish_artifact(publication).await.unwrap();
    store.enforce_budget().await.unwrap();
    assert_eq!(
        store.status().await.unwrap().pinned_usage_bytes,
        before.pinned_usage_bytes
    );
}

#[tokio::test]
async fn source_segment_eviction_removes_linked_artifact_before_frames() {
    let fixture = fixture().await;
    let publication = publication(&fixture, 65, 66);
    fixture
        .store
        .publish_artifact(publication.clone())
        .await
        .unwrap();
    let usage = fixture.store.status().await.unwrap().usage;
    let budget = DiskBudgetBytes::new(
        usage
            .total_bytes()
            .unwrap()
            .saturating_sub(usage.segment_bytes)
            .saturating_sub(usage.artifact_bytes)
            + 3_000,
    )
    .unwrap();
    drop(fixture.store);
    let store = open_store_with_budget(fixture.directory.path(), Some(budget));
    let replacement = EncodedFrame::new(
        CapturedFrame::new(
            FrameId::from_uuid(Uuid::from_u128(650)),
            SessionId::from_uuid(Uuid::from_u128(651)),
            TargetId::from_uuid(Uuid::from_u128(652)),
            CaptureOrdinal::new(1).unwrap(),
            None,
            ObservedTime::from_nanos(1),
            SessionTime::from_nanos(1),
            ImageFormat::Jpeg,
            CoreDimensions::new(1, 1).unwrap(),
            CoreDimensions::new(1, 1).unwrap(),
            DeviceScaleFactor::new(1.0).unwrap(),
            vec![],
        )
        .unwrap(),
        vec![7; 3_000],
    )
    .unwrap();
    store.append_frame(replacement).await.unwrap();
    assert!(
        store
            .artifact(*publication.manifest.artifact_id())
            .await
            .unwrap()
            .is_none()
    );
    let source = SqliteIndex::open(IndexStoreConfig {
        database_path: fixture.directory.path().join("index.sqlite3"),
        segments_directory: fixture.directory.path().join("segments"),
        busy_timeout: Duration::from_secs(1),
    })
    .unwrap();
    assert!(source.frames_by_id(fixture.frame_ids).await.is_err());
}

#[tokio::test]
async fn session_deletion_removes_artifact_links_files_and_usage() {
    let fixture = fixture().await;
    let publication = publication(&fixture, 70, 71);
    fixture
        .store
        .publish_artifact(publication.clone())
        .await
        .unwrap();
    let path = fixture
        .directory
        .path()
        .join("artifacts")
        .join(format!("{}.png", publication.manifest.artifact_id()));
    let deleted = fixture.store.delete_session(fixture.session).await.unwrap();
    assert_eq!(deleted.removed_artifacts, 1);
    assert!(!path.exists());
    assert_eq!(
        fixture.store.status().await.unwrap().usage.artifact_bytes,
        0
    );
    assert!(
        fixture
            .store
            .artifact(*publication.manifest.artifact_id())
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        fixture
            .store
            .lookup_artifact(publication.cache.cache_key, publication.sources)
            .await
            .unwrap(),
        ArtifactLookup::Miss
    );
}
