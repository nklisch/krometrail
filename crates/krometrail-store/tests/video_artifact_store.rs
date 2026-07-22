use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use krometrail_core::{
    ArtifactCacheKey, ArtifactCacheMetadata, ArtifactSourceFingerprint, ArtifactStore,
    CancellationSignal, CaptureOrdinal, CapturedFrame, DeviceScaleFactor, DiskBudgetBytes,
    EncodedFrame, ErrorCode, EvidenceScope, FrameId, FrameSource, ImageFormat, NonEmptyText,
    ObservedTime, PixelDimensions, PortFuture, PresentationRange, PresentationTime,
    RangeResolutionOptions, RecordingSink, ResolvedRange, RetentionStore, RetrieveArtifactRequest,
    SessionId, SessionRange, SessionTime, StoredVideoArtifact, TEMPORAL_VIDEO_GENERATOR_NAME,
    TEMPORAL_VIDEO_GENERATOR_VERSION, TargetId, TemporalRangeAnchorKind, TemporalVideoManifest,
    VideoArtifactLookup, VideoArtifactPublication, VideoArtifactPublish, VideoArtifactReadLookup,
    VideoEncodedClip, VideoEncoderIdentity, VideoEncodingProfile, VideoOutputGeometry,
    VideoPresentationPlan, VideoPresentationPolicy, VideoPresentationSegment, VideoSegmentSource,
    VideoTimingBasis, VisualEpoch,
};
use krometrail_store::{
    IndexStoreConfig, RecordingStore, RotationConfig, SegmentStoreConfig, SegmentWriter,
    SqliteIndex,
};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use uuid::Uuid;

struct Fixture {
    directory: TempDir,
    store: Arc<RecordingStore>,
    session: SessionId,
    target: TargetId,
    frame_id: FrameId,
    source_bytes: Vec<u8>,
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
        Some(budget) => {
            RecordingStore::with_budget(writer, index, budget, store_test_clock()).unwrap()
        }
        None => RecordingStore::new(writer, index, store_test_clock()).unwrap(),
    })
}

async fn fixture() -> Fixture {
    let directory = TempDir::new().unwrap();
    let store = open_store(directory.path());
    let session = SessionId::from_uuid(Uuid::from_u128(1));
    let target = TargetId::from_uuid(Uuid::from_u128(2));
    let frame_id = FrameId::from_uuid(Uuid::from_u128(3));
    let source_bytes = b"retained-source-image".to_vec();
    let frame = CapturedFrame::new(
        frame_id,
        session,
        target,
        CaptureOrdinal::new(1).unwrap(),
        None,
        ObservedTime::from_nanos(2),
        SessionTime::from_nanos(2),
        ImageFormat::Jpeg,
        PixelDimensions::new(2, 2).unwrap(),
        PixelDimensions::new(2, 2).unwrap(),
        DeviceScaleFactor::new(1.0).unwrap(),
        vec![],
    )
    .unwrap();
    store
        .append_frame(EncodedFrame::new(frame, source_bytes.clone()).unwrap())
        .await
        .unwrap();
    store.flush(session).await.unwrap();
    Fixture {
        directory,
        store,
        session,
        target,
        frame_id,
        source_bytes,
    }
}

fn publication(fixture: &Fixture, artifact: u128, key: u8) -> VideoArtifactPublication {
    let requested = SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(10)).unwrap();
    let range = ResolvedRange::new(
        fixture.session,
        fixture.target,
        TemporalRangeAnchorKind::SessionTime,
        requested,
        requested,
        vec![fixture.frame_id],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        RangeResolutionOptions::DEFAULT,
    )
    .unwrap();
    let dimensions = PixelDimensions::new(2, 2).unwrap();
    let geometry = VideoOutputGeometry::new(dimensions, dimensions, dimensions).unwrap();
    let plan = VideoPresentationPlan::new(
        VideoPresentationPolicy::RealTime,
        requested,
        requested,
        SessionRange::new(SessionTime::from_nanos(2), SessionTime::from_nanos(2)).unwrap(),
        VisualEpoch {
            index: 0,
            frame_ids: vec![fixture.frame_id],
            image: dimensions,
            viewport: dimensions,
            device_scale_factor: DeviceScaleFactor::new(1.0).unwrap(),
        },
        vec![fixture.frame_id],
        vec![SessionTime::from_nanos(2)],
        vec![],
        vec![
            VideoPresentationSegment::new(
                0,
                VideoSegmentSource::source_frame(fixture.frame_id, SessionTime::from_nanos(2))
                    .unwrap(),
                PresentationRange::new(
                    PresentationTime::ZERO,
                    PresentationTime::from_nanos(250_000_000).unwrap(),
                )
                .unwrap(),
                VideoTimingBasis::TerminalHold,
            )
            .unwrap(),
        ],
        geometry,
    )
    .unwrap();
    let profile = VideoEncodingProfile::new(geometry, 1024).unwrap();
    let identity = VideoEncoderIdentity::new(
        "fake-encoder-1",
        [7; 32],
        "fake-h264",
        "adapter-v1",
        "args-v1",
    )
    .unwrap();
    let bytes: Arc<[u8]> = Arc::from(&b"fake-mp4-contract-bytes"[..]);
    let encoded = VideoEncodedClip::new(
        identity,
        profile,
        temporal_vision::OutputHash::from_bytes(Sha256::digest(&bytes).into()),
        Arc::clone(&bytes),
    )
    .unwrap();
    let manifest = TemporalVideoManifest::new(
        krometrail_core::ArtifactId::from_uuid(Uuid::from_u128(artifact)),
        &range,
        plan,
        None,
        &encoded,
    )
    .unwrap();
    VideoArtifactPublication::new(
        fixture.session,
        fixture.target,
        vec![ArtifactSourceFingerprint {
            frame_id: fixture.frame_id,
            encoded_sha256: Sha256::digest(&fixture.source_bytes).into(),
        }],
        ArtifactCacheMetadata {
            cache_key: ArtifactCacheKey::from_bytes([key; 32]),
            source_fingerprint: [11; 32],
            parameter_hash: [12; 32],
            visual_epoch_hash: [13; 32],
            cache_schema_version: 1,
            adapter_version: NonEmptyText::new("retained-video-v1").unwrap(),
            generator_name: NonEmptyText::new(TEMPORAL_VIDEO_GENERATOR_NAME).unwrap(),
            generator_version: NonEmptyText::new(TEMPORAL_VIDEO_GENERATOR_VERSION).unwrap(),
        },
        manifest,
        bytes,
    )
    .unwrap()
}

#[tokio::test]
async fn retained_video_uses_shared_publish_cache_read_recovery_and_deletion_authority() {
    let fixture = fixture().await;
    let publication = publication(&fixture, 20, 21);
    assert!(matches!(
        fixture
            .store
            .publish_video_artifact(publication.clone())
            .await
            .unwrap(),
        VideoArtifactPublish::Published(_)
    ));
    assert!(matches!(
        fixture
            .store
            .lookup_video_artifact(publication.cache.cache_key, publication.sources.clone())
            .await
            .unwrap(),
        VideoArtifactLookup::Hit(_)
    ));
    let scope = EvidenceScope::new(fixture.session, fixture.target).unwrap();
    let request = RetrieveArtifactRequest::new(
        scope,
        publication.manifest.artifact_id(),
        publication.encoded_bytes.len() as u64,
    )
    .unwrap();
    let VideoArtifactReadLookup::Available(read) = fixture
        .store
        .read_video_artifact(request.clone())
        .await
        .unwrap()
    else {
        panic!("published video must be readable")
    };
    assert_eq!(read.encoded_bytes(), publication.encoded_bytes.as_ref());
    assert_eq!(read.handle.provenance, publication.manifest);
    assert!(
        fixture
            .directory
            .path()
            .join("artifacts")
            .join(format!("{}.mp4", publication.manifest.artifact_id()))
            .is_file()
    );

    drop(fixture.store);
    let reopened = open_store(fixture.directory.path());
    let recovered: StoredVideoArtifact = reopened
        .video_artifact(publication.manifest.artifact_id())
        .await
        .unwrap()
        .expect("valid video survives restart");
    assert_eq!(recovered.manifest, publication.manifest);
    assert_eq!(recovered.encoded_bytes, publication.encoded_bytes);

    let deleted = reopened.delete_session(fixture.session).await.unwrap();
    assert_eq!(deleted.removed_artifacts, 1);
    let deleted_session_read = reopened
        .read_video_artifact(request)
        .await
        .expect_err("deleted sessions reject scoped artifact reads");
    assert_eq!(deleted_session_read.code, ErrorCode::NotFound);
    assert!(
        reopened
            .video_artifact(publication.manifest.artifact_id())
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn corrupt_video_is_invalidated_without_affecting_source_frames() {
    let fixture = fixture().await;
    let publication = publication(&fixture, 30, 31);
    fixture
        .store
        .publish_video_artifact(publication.clone())
        .await
        .unwrap();
    let path = fixture
        .directory
        .path()
        .join("artifacts")
        .join(format!("{}.mp4", publication.manifest.artifact_id()));
    std::fs::write(&path, b"corrupt").unwrap();
    assert_eq!(
        fixture
            .store
            .lookup_video_artifact(publication.cache.cache_key, publication.sources)
            .await
            .unwrap(),
        VideoArtifactLookup::Invalidated
    );
    assert!(!path.exists());
    assert_eq!(
        fixture
            .store
            .frames_by_id(vec![fixture.frame_id])
            .await
            .unwrap()
            .len(),
        1
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

struct CancelOnCheck {
    checks: AtomicUsize,
    cancel_on: usize,
}

impl CancellationSignal for CancelOnCheck {
    fn is_cancelled(&self) -> bool {
        self.checks.fetch_add(1, Ordering::SeqCst) + 1 >= self.cancel_on
    }

    fn cancelled(&self) -> PortFuture<'_, ()> {
        Box::pin(std::future::pending())
    }
}

#[tokio::test]
async fn cancelled_video_publication_leaves_no_visible_or_accounted_state() {
    let fixture = fixture().await;
    let publication = publication(&fixture, 40, 41).with_cancellation(Arc::new(AlreadyCancelled));
    let error = fixture
        .store
        .publish_video_artifact(publication.clone())
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::Cancelled);
    assert_eq!(
        fixture
            .store
            .lookup_video_artifact(publication.cache.cache_key, publication.sources)
            .await
            .unwrap(),
        VideoArtifactLookup::Miss
    );
    assert_eq!(
        fixture.store.status().await.unwrap().usage.artifact_bytes,
        0
    );
    let artifacts = fixture.directory.path().join("artifacts");
    assert!(!artifacts.exists() || std::fs::read_dir(artifacts).unwrap().next().is_none());
}

#[tokio::test]
async fn cancellation_at_every_durable_file_boundary_cannot_recover_a_video() {
    // File publication checks before writing, after temp sync, after rename, and after
    // directory sync; the fifth check is the store's pre-finalization fence.
    for cancel_on in 1..=5 {
        let fixture = fixture().await;
        let publication = publication(&fixture, 4_000 + cancel_on as u128, 80 + cancel_on as u8)
            .with_cancellation(Arc::new(CancelOnCheck {
                checks: AtomicUsize::new(0),
                cancel_on,
            }));
        let artifact_id = publication.manifest.artifact_id();
        let directory = fixture.directory.path().to_path_buf();
        let error = fixture
            .store
            .publish_video_artifact(publication)
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::Cancelled, "boundary {cancel_on}");
        assert_eq!(
            fixture.store.status().await.unwrap().usage.artifact_bytes,
            0,
            "boundary {cancel_on} must release usage"
        );
        drop(fixture.store);

        let reopened = open_store(&directory);
        assert!(
            reopened
                .video_artifact(artifact_id)
                .await
                .unwrap()
                .is_none(),
            "boundary {cancel_on} must not become ready during recovery"
        );
        assert_eq!(
            reopened.status().await.unwrap().usage.artifact_bytes,
            0,
            "boundary {cancel_on} must remain unaccounted after reopen"
        );
        let artifacts = directory.join("artifacts");
        assert!(
            !artifacts.exists() || std::fs::read_dir(artifacts).unwrap().next().is_none(),
            "boundary {cancel_on} must leave no temp or final file"
        );
    }
}

#[tokio::test]
async fn video_publication_enforces_budget_before_returning_a_live_handle() {
    let fixture = fixture().await;
    let first = publication(&fixture, 50, 51);
    fixture
        .store
        .publish_video_artifact(first.clone())
        .await
        .unwrap();
    let usage = fixture.store.status().await.unwrap().usage;
    let budget = DiskBudgetBytes::new(usage.total_bytes().unwrap()).unwrap();
    let second = publication(&fixture, 52, 53);
    drop(fixture.store);
    let store = open_store_with_budget(fixture.directory.path(), Some(budget));
    let VideoArtifactPublish::Published(published) =
        store.publish_video_artifact(second.clone()).await.unwrap()
    else {
        panic!("second publication must become the ready budget winner")
    };
    assert_eq!(
        published.manifest.artifact_id(),
        second.manifest.artifact_id()
    );
    assert!(
        store
            .video_artifact(first.manifest.artifact_id())
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .video_artifact(second.manifest.artifact_id())
            .await
            .unwrap()
            .is_some(),
        "publication must not return a handle it evicted itself"
    );
    assert!(store.status().await.unwrap().usage.total_bytes().unwrap() <= budget.get());
    assert_eq!(
        store
            .frames_by_id(vec![fixture.frame_id])
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn source_segment_eviction_removes_its_linked_video() {
    let fixture = fixture().await;
    let publication = publication(&fixture, 55, 56);
    fixture
        .store
        .publish_video_artifact(publication.clone())
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
            FrameId::from_uuid(Uuid::from_u128(5_500)),
            SessionId::from_uuid(Uuid::from_u128(5_501)),
            TargetId::from_uuid(Uuid::from_u128(5_502)),
            CaptureOrdinal::new(1).unwrap(),
            None,
            ObservedTime::from_nanos(1),
            SessionTime::from_nanos(1),
            ImageFormat::Jpeg,
            PixelDimensions::new(1, 1).unwrap(),
            PixelDimensions::new(1, 1).unwrap(),
            DeviceScaleFactor::new(1.0).unwrap(),
            vec![],
        )
        .unwrap(),
        vec![7; 3_000],
    )
    .unwrap();
    if let Err(error) = store.append_frame(replacement).await {
        assert_eq!(error.code, ErrorCode::BudgetExhausted);
    }
    assert!(
        store
            .video_artifact(publication.manifest.artifact_id())
            .await
            .unwrap()
            .is_none()
    );
    assert!(store.frames_by_id(vec![fixture.frame_id]).await.is_err());
}

#[tokio::test]
async fn startup_removes_orphan_video_files_idempotently() {
    let fixture = fixture().await;
    let artifacts = fixture.directory.path().join("artifacts");
    std::fs::create_dir_all(&artifacts).unwrap();
    let orphan = artifacts.join(format!("{}.mp4", Uuid::from_u128(60)));
    std::fs::write(&orphan, b"orphan mp4").unwrap();
    drop(fixture.store);

    let reopened = open_store(fixture.directory.path());
    assert!(!orphan.exists());
    drop(reopened);
    let second_pass = open_store(fixture.directory.path());
    assert!(!orphan.exists());
    assert_eq!(second_pass.status().await.unwrap().usage.artifact_bytes, 0);
}

#[tokio::test]
async fn startup_finalizes_a_durable_staged_video_idempotently() {
    let fixture = fixture().await;
    let publication = publication(&fixture, 70, 71);
    fixture
        .store
        .publish_video_artifact(publication.clone())
        .await
        .unwrap();
    drop(fixture.store);
    let connection =
        rusqlite::Connection::open(fixture.directory.path().join("index.sqlite3")).unwrap();
    connection
        .execute(
            "UPDATE artifacts SET state='staging' WHERE cache_key=?1",
            [publication.cache.cache_key.as_bytes().to_vec()],
        )
        .unwrap();
    drop(connection);

    let reopened = open_store(fixture.directory.path());
    assert!(matches!(
        reopened
            .lookup_video_artifact(publication.cache.cache_key, publication.sources.clone())
            .await
            .unwrap(),
        VideoArtifactLookup::Hit(_)
    ));
    drop(reopened);
    let second_pass = open_store(fixture.directory.path());
    assert!(matches!(
        second_pass
            .lookup_video_artifact(publication.cache.cache_key, publication.sources)
            .await
            .unwrap(),
        VideoArtifactLookup::Hit(_)
    ));
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
