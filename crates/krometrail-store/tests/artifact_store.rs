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
    fixture_padded(0).await
}

/// Builds the standard fixture, optionally padding the source segment.
///
/// Budget-driven eviction tests need the segment they are trying to evict to be
/// large relative to SQLite's page granularity; otherwise the range of budgets
/// that force eviction without also forbidding the follow-up append is narrower
/// than a single page, and the test measures page rounding rather than retention
/// behaviour. The pad frame is deliberately not part of `frame_ids`, so artifact
/// provenance is unchanged.
async fn fixture_padded(pad_bytes: usize) -> Fixture {
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
    if pad_bytes != 0 {
        let metadata = CapturedFrame::new(
            FrameId::from_uuid(Uuid::from_u128(9_000)),
            session,
            target,
            CaptureOrdinal::new(9_000).unwrap(),
            None,
            ObservedTime::from_nanos(9_000),
            SessionTime::from_nanos(9_000),
            ImageFormat::Jpeg,
            CoreDimensions::new(1, 1).unwrap(),
            CoreDimensions::new(1, 1).unwrap(),
            DeviceScaleFactor::new(1.0).unwrap(),
            vec![],
        )
        .unwrap();
        store
            .append_frame(EncodedFrame::new(metadata, vec![9; pad_bytes]).unwrap())
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

/// Derived artifacts are regenerable; source frames are not. Reclaim must
/// therefore spend the artifact first and only reach for frames when that was
/// not enough.
///
/// Observing this needs *partial* pressure. Under pressure large enough to evict
/// both, a test can only see that both are gone — and "both gone" is equally
/// consistent with the segment having been evicted first and the artifact having
/// been invalidated as collateral, which `artifact()` also reports as `None`.
/// Applying exactly enough pressure to cost one object makes the choice visible:
/// the artifact must be the one that goes, and the frames must survive.
#[tokio::test]
async fn derived_artifacts_are_evicted_before_the_frames_they_derive_from() {
    let fixture = fixture_padded(120_000).await;
    let publication = publication(&fixture, 67, 68);
    let artifact_id = *publication.manifest.artifact_id();
    fixture.store.publish_artifact(publication).await.unwrap();
    let usage = fixture.store.status().await.unwrap().usage;
    assert!(usage.artifact_bytes > 0);

    const REPLACEMENT_FRAME_BYTES: u64 = 3_000;
    const RECORD_ENVELOPE_ALLOWANCE: u64 = 4_096;
    // Room for everything currently retained *except* the artifact, plus exactly
    // one incoming frame. Losing the artifact is sufficient; losing the source
    // segment is not required, so a reclaim walk that reached for frames first
    // would take evidence it never needed to.
    let budget = DiskBudgetBytes::new(
        usage.total_bytes().unwrap() - usage.artifact_bytes
            + REPLACEMENT_FRAME_BYTES
            + RECORD_ENVELOPE_ALLOWANCE,
    )
    .unwrap();
    drop(fixture.store);
    let store = open_store_with_budget(fixture.directory.path(), Some(budget));
    let replacement = EncodedFrame::new(
        CapturedFrame::new(
            FrameId::from_uuid(Uuid::from_u128(670)),
            SessionId::from_uuid(Uuid::from_u128(671)),
            TargetId::from_uuid(Uuid::from_u128(672)),
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
        vec![7; REPLACEMENT_FRAME_BYTES as usize],
    )
    .unwrap();
    store.append_frame(replacement).await.unwrap();

    assert!(
        store.artifact(artifact_id).await.unwrap().is_none(),
        "the regenerable artifact is the first thing reclaim should spend"
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
        fixture.frame_ids.len(),
        "source frames must survive pressure that the artifact alone could relieve"
    );
    assert_eq!(store.status().await.unwrap().usage.artifact_bytes, 0);
}

#[tokio::test]
async fn source_segment_eviction_removes_linked_artifact_before_frames() {
    // A padded source segment keeps the eviction decision well clear of SQLite
    // page granularity.
    let fixture = fixture_padded(120_000).await;
    let publication = publication(&fixture, 65, 66);
    fixture
        .store
        .publish_artifact(publication.clone())
        .await
        .unwrap();
    let usage = fixture.store.status().await.unwrap().usage;
    // Budget headroom is chosen relative to the incoming frame, not to leftover
    // page slack: large enough that the replacement fits *once* the existing
    // segment and artifact are evicted, but smaller than the cost of keeping
    // them, so eviction is genuinely forced. Deriving it from the frame's own
    // storage cost keeps the test from depending on how much the SQLite index
    // happens to shrink when rows are deleted.
    const REPLACEMENT_FRAME_BYTES: u64 = 3_000;
    const RECORD_ENVELOPE_ALLOWANCE: u64 = 4_096;
    // Room for the metadata index plus exactly one new frame, and nothing else.
    // Retaining either the source segment or its derived artifact then puts the
    // store over budget, so both must be reclaimed — the artifact first, as the
    // cascade removes it along with the segment it derives from.
    let headroom = REPLACEMENT_FRAME_BYTES + RECORD_ENVELOPE_ALLOWANCE;
    let budget = DiskBudgetBytes::new(
        usage
            .total_bytes()
            .unwrap()
            .saturating_sub(usage.segment_bytes)
            .saturating_sub(usage.artifact_bytes)
            + headroom,
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
