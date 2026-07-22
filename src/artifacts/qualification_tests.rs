use std::{
    num::NonZeroU64,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use krometrail_core::{
    AnalysisScale, ArtifactCacheDisposition, ArtifactFailurePolicy, ArtifactGeneration,
    ArtifactGenerationContext, ArtifactGenerationRequest, ArtifactGeneratorRequest,
    ArtifactLabelsRequest, ArtifactLookup, ArtifactManifest, ArtifactMarker, ArtifactMarkerId,
    ArtifactOutcome, ArtifactPublication, ArtifactPublish, ArtifactStore, BrowserEvent,
    BrowserEventBatch, BrowserEventId, BrowserEventOrdinal, BrowserEventPayload,
    BrowserEventSeverity, BrowserEventSink, CaptureGap, CaptureGapReason, CaptureOrdinal,
    CapturedFrame, DeviceScaleFactor, EncodedFrame, FrameAvailability, FrameId, FrameSelector,
    FrameSource, IdSource, IdValue, ImageFormat, NonEmptyText, NormalizationRequest, ObservedTime,
    OutputLimitsRequest, PixelDimensions, PortFuture, RangeResolutionOptions, RecordingSink,
    ResolvedRange, RetentionStore, SessionId, SessionRange, SessionTime, StoredArtifact,
    StoryboardRequest, TargetId, TargetLifecycle, TargetLifecycleEvent, TemporalRangeAnchorKind,
};
use krometrail_store::{
    IndexStoreConfig, RecordingStore, RotationConfig, SegmentStoreConfig, SegmentWriter,
    SqliteIndex,
};
use temporal_vision::Rgb8;
use tokio::sync::Semaphore;
use uuid::Uuid;

use super::{TemporalVisionArtifactService, scheduler::ArtifactWorkLimits};

const JPEG: &[u8] = include_bytes!("../../tests/fixtures/artifacts/chrome-rgb.jpg");
const PNG: &[u8] = include_bytes!("../../tests/fixtures/artifacts/chrome-rgba.png");

struct SequenceIds(AtomicU64);

impl IdSource for SequenceIds {
    fn next(&self) -> IdValue {
        IdValue::from_uuid(Uuid::from_u128(u128::from(
            self.0.fetch_add(1, Ordering::Relaxed),
        )))
    }
}

struct RealRig {
    root: PathBuf,
    index: Arc<SqliteIndex>,
    store: Arc<RecordingStore>,
    request: ArtifactGenerationRequest,
    session: SessionId,
    target: TargetId,
    frame_ids: Vec<FrameId>,
}

async fn real_rig() -> RealRig {
    let root = std::env::temp_dir().join(format!("krometrail-artifact-{}", Uuid::new_v4()));
    let segments = root.join("segments");
    let index = Arc::new(
        SqliteIndex::open(IndexStoreConfig {
            database_path: root.join("index.sqlite3"),
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
    let store =
        Arc::new(RecordingStore::new(writer, Arc::clone(&index), store_test_clock()).unwrap());
    let session = SessionId::from_uuid(Uuid::from_u128(700));
    let target = TargetId::from_uuid(Uuid::from_u128(701));
    let frame_ids: Vec<_> = (0_u128..4)
        .map(|position| FrameId::from_uuid(Uuid::from_u128(710 + position)))
        .collect();
    for (position, frame_id) in frame_ids.iter().enumerate() {
        let ordinal = u64::try_from(position + 1).unwrap();
        let (format, bytes) = if position == 0 {
            (ImageFormat::Jpeg, JPEG)
        } else {
            (ImageFormat::Png, PNG)
        };
        let encoded = EncodedFrame::new(
            CapturedFrame::new(
                *frame_id,
                session,
                target,
                CaptureOrdinal::new(ordinal).unwrap(),
                None,
                ObservedTime::from_nanos(ordinal + 10),
                SessionTime::from_nanos(if position < 2 { ordinal } else { ordinal - 1 }),
                format,
                PixelDimensions::new(2, 2).unwrap(),
                if position < 2 {
                    PixelDimensions::new(2, 2).unwrap()
                } else {
                    PixelDimensions::new(3, 2).unwrap()
                },
                DeviceScaleFactor::new(1.0).unwrap(),
                vec![],
            )
            .unwrap(),
            bytes.to_vec(),
        )
        .unwrap();
        store.append_frame(encoded).await.unwrap();
    }
    store.flush(session).await.unwrap();

    let range = SessionRange::new(SessionTime::from_nanos(1), SessionTime::from_nanos(3)).unwrap();
    let gap = CaptureGap::new(
        krometrail_core::GapId::from_uuid(Uuid::from_u128(720)),
        session,
        target,
        SessionRange::new(SessionTime::from_nanos(1), SessionTime::from_nanos(2)).unwrap(),
        ObservedTime::from_nanos(20),
        CaptureGapReason::FrameRejected,
        NonZeroU64::new(1),
        None,
    )
    .unwrap();
    let resolved = ResolvedRange::new(
        session,
        target,
        TemporalRangeAnchorKind::SessionTime,
        range,
        range,
        frame_ids.clone(),
        vec![],
        vec![],
        vec![],
        vec![gap],
        vec![],
        RangeResolutionOptions::DEFAULT,
    )
    .unwrap();
    let request = ArtifactGenerationRequest::new(
        resolved,
        vec![ArtifactMarker::new(
            ArtifactMarkerId::Caller(NonEmptyText::new("anchor").unwrap()),
            SessionTime::from_nanos(2),
            NonEmptyText::new("interaction").unwrap(),
            NonEmptyText::new("clicked").unwrap(),
        )],
        vec![ArtifactGeneratorRequest::Storyboard(StoryboardRequest {
            anchor: SessionTime::from_nanos(2),
            tile_limit: 3,
            noise_floor: 0,
            normalization: NormalizationRequest::new(
                None,
                Rgb8::new(0, 0, 0),
                AnalysisScale::Identity,
            )
            .unwrap(),
            labels: ArtifactLabelsRequest::new(
                NonEmptyText::new("story").unwrap(),
                NonEmptyText::new("real v4 store").unwrap(),
            ),
            include_orientation: false,
            output: OutputLimitsRequest::new(4096, 4096, 16 * 1024 * 1024).unwrap(),
        })],
        ArtifactFailurePolicy::RequireAll,
    )
    .unwrap();
    RealRig {
        root,
        index,
        store,
        request,
        session,
        target,
        frame_ids,
    }
}

fn service(
    frames: Arc<dyn FrameSource>,
    artifacts: Arc<dyn ArtifactStore>,
) -> Arc<TemporalVisionArtifactService> {
    Arc::new(
        TemporalVisionArtifactService::new(
            frames,
            artifacts,
            Arc::new(SequenceIds(AtomicU64::new(900))),
            ArtifactWorkLimits::default(),
        )
        .unwrap(),
    )
}

fn available(outcome: &ArtifactOutcome) -> &krometrail_core::ArtifactHandle {
    match outcome {
        ArtifactOutcome::Available { artifact, .. } => artifact,
        ArtifactOutcome::Unavailable { error, .. } => panic!("generation failed: {error}"),
    }
}

#[tokio::test]
async fn real_v4_cache_is_exact_deterministic_and_regenerates_corruption() {
    let rig = real_rig().await;
    let generator = service(
        Arc::clone(&rig.index) as Arc<dyn FrameSource>,
        Arc::clone(&rig.store) as Arc<dyn ArtifactStore>,
    );
    let first = generator
        .generate(rig.request.clone(), ArtifactGenerationContext::default())
        .await
        .unwrap();
    assert_eq!(first.outcomes.len(), 2);
    let first_handles: Vec<_> = first.outcomes.iter().map(available).cloned().collect();
    let mut first_bytes = Vec::new();
    for handle in &first_handles {
        let stored = rig
            .store
            .artifact(handle.artifact_id)
            .await
            .unwrap()
            .unwrap();
        let manifest_json = serde_json::to_vec(&stored.manifest).unwrap();
        let round_trip: ArtifactManifest = serde_json::from_slice(&manifest_json).unwrap();
        assert_eq!(round_trip, stored.manifest);
        first_bytes.push(stored.encoded_bytes);
    }

    let repeat = generator
        .generate(rig.request.clone(), ArtifactGenerationContext::default())
        .await
        .unwrap();
    assert!(repeat.outcomes.iter().all(|outcome| matches!(
        outcome,
        ArtifactOutcome::Available { artifact, .. }
            if artifact.cache == ArtifactCacheDisposition::Hit
    )));
    for (hit, generated) in repeat.outcomes.iter().map(available).zip(&first_handles) {
        assert_eq!(hit.artifact_id, generated.artifact_id);
        assert_eq!(hit.encoded_byte_len, generated.encoded_byte_len);
        assert_eq!(hit.manifest, generated.manifest);
    }

    let corrupt = rig
        .root
        .join("artifacts")
        .join(format!("{}.png", first_handles[0].artifact_id));
    std::fs::write(&corrupt, b"not a png").unwrap();
    let regenerated = generator
        .generate(rig.request.clone(), ArtifactGenerationContext::default())
        .await
        .unwrap();
    assert!(matches!(
        regenerated.outcomes[0],
        ArtifactOutcome::Available { ref artifact, .. }
            if artifact.cache == ArtifactCacheDisposition::RegeneratedAfterInvalidation
    ));
    assert_eq!(
        rig.store
            .artifact(available(&regenerated.outcomes[0]).artifact_id)
            .await
            .unwrap()
            .unwrap()
            .encoded_bytes,
        first_bytes[0]
    );
    let artifact_bytes: u64 = regenerated
        .outcomes
        .iter()
        .map(available)
        .map(|artifact| artifact.encoded_byte_len)
        .sum();
    assert_eq!(
        rig.store.status().await.unwrap().usage.artifact_bytes,
        artifact_bytes
    );
}

struct SnapshotFrames {
    inner: Arc<SqliteIndex>,
    loaded: Arc<Semaphore>,
    release: Arc<Semaphore>,
}

impl FrameSource for SnapshotFrames {
    fn list_source_frames(
        &self,
        request: krometrail_core::SourceFramesRequest,
    ) -> PortFuture<'_, krometrail_core::Result<krometrail_core::SourceFrameList>> {
        self.inner.list_source_frames(request)
    }

    fn fetch_source_frames(
        &self,
        request: krometrail_core::SourceFramesRequest,
    ) -> PortFuture<'_, krometrail_core::Result<krometrail_core::SourceFrameBatch>> {
        self.inner.fetch_source_frames(request)
    }

    fn read_source_frame(
        &self,
        request: krometrail_core::RetrieveSourceFrameRequest,
    ) -> PortFuture<'_, krometrail_core::Result<krometrail_core::SourceFrameRead>> {
        self.inner.read_source_frame(request)
    }

    fn frames_by_id(
        &self,
        frame_ids: Vec<FrameId>,
    ) -> PortFuture<'_, krometrail_core::Result<Vec<EncodedFrame>>> {
        let inner = Arc::clone(&self.inner);
        let loaded = Arc::clone(&self.loaded);
        let release = Arc::clone(&self.release);
        Box::pin(async move {
            let frames = inner.frames_by_id(frame_ids).await?;
            loaded.add_permits(1);
            let permit = release.acquire().await.expect("release semaphore is open");
            permit.forget();
            Ok(frames)
        })
    }

    fn frame_metadata_by_id(
        &self,
        frame_ids: Vec<FrameId>,
    ) -> PortFuture<'_, krometrail_core::Result<Vec<CapturedFrame>>> {
        self.inner.frame_metadata_by_id(frame_ids)
    }

    fn frames_in_range(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        range: SessionRange,
    ) -> PortFuture<'_, krometrail_core::Result<Vec<EncodedFrame>>> {
        self.inner.frames_in_range(session_id, target_id, range)
    }

    fn frames_in_ordinal_range(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        start: CaptureOrdinal,
        end: CaptureOrdinal,
    ) -> PortFuture<'_, krometrail_core::Result<Vec<EncodedFrame>>> {
        self.inner
            .frames_in_ordinal_range(session_id, target_id, start, end)
    }

    fn frame_metadata_in_range(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        range: SessionRange,
    ) -> PortFuture<'_, krometrail_core::Result<Vec<CapturedFrame>>> {
        self.inner
            .frame_metadata_in_range(session_id, target_id, range)
    }

    fn frame_metadata_in_ordinal_range(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        start: CaptureOrdinal,
        end: CaptureOrdinal,
    ) -> PortFuture<'_, krometrail_core::Result<Vec<CapturedFrame>>> {
        self.inner
            .frame_metadata_in_ordinal_range(session_id, target_id, start, end)
    }

    fn frame_availability(
        &self,
        session_id: SessionId,
        target_id: TargetId,
    ) -> PortFuture<'_, krometrail_core::Result<FrameAvailability>> {
        self.inner.frame_availability(session_id, target_id)
    }
}

#[tokio::test]
async fn session_deletion_fences_late_cpu_work_from_republishing() {
    let rig = real_rig().await;
    let loaded = Arc::new(Semaphore::new(0));
    let release = Arc::new(Semaphore::new(0));
    let frames = Arc::new(SnapshotFrames {
        inner: Arc::clone(&rig.index),
        loaded: Arc::clone(&loaded),
        release: Arc::clone(&release),
    });
    let generator = service(
        frames as Arc<dyn FrameSource>,
        Arc::clone(&rig.store) as Arc<dyn ArtifactStore>,
    );
    let request = rig.request.clone();
    let task = tokio::spawn(async move {
        generator
            .generate(request, ArtifactGenerationContext::default())
            .await
    });
    let loaded_permit = loaded.acquire().await.unwrap();
    loaded_permit.forget();
    rig.store.delete_session(rig.session).await.unwrap();
    release.add_permits(1);
    assert!(task.await.unwrap().is_err());
    for frame_id in rig.frame_ids {
        assert!(rig.index.frames_by_id(vec![frame_id]).await.is_err());
    }
    assert_eq!(rig.store.status().await.unwrap().usage.artifact_bytes, 0);
    let artifacts = rig.root.join("artifacts");
    assert!(
        !artifacts.exists() || std::fs::read_dir(artifacts).unwrap().next().is_none(),
        "session deletion must leave no staged, temporary, or ready artifact files"
    );
}

struct DelayedPublishStore {
    inner: Arc<RecordingStore>,
    reached: Arc<Semaphore>,
    release: Arc<Semaphore>,
}

impl ArtifactStore for DelayedPublishStore {
    fn lookup_artifact(
        &self,
        cache_key: krometrail_core::ArtifactCacheKey,
        sources: Vec<krometrail_core::ArtifactSourceFingerprint>,
    ) -> PortFuture<'_, krometrail_core::Result<ArtifactLookup>> {
        self.inner.lookup_artifact(cache_key, sources)
    }

    fn publish_artifact(
        &self,
        publication: ArtifactPublication,
    ) -> PortFuture<'_, krometrail_core::Result<ArtifactPublish>> {
        let inner = Arc::clone(&self.inner);
        let reached = Arc::clone(&self.reached);
        let release = Arc::clone(&self.release);
        Box::pin(async move {
            reached.add_permits(1);
            let permit = release.acquire().await.expect("release semaphore is open");
            permit.forget();
            inner.publish_artifact(publication).await
        })
    }

    fn artifact(
        &self,
        artifact_id: krometrail_core::ArtifactId,
    ) -> PortFuture<'_, krometrail_core::Result<Option<StoredArtifact>>> {
        self.inner.artifact(artifact_id)
    }
}

#[tokio::test]
async fn active_generation_does_not_block_frame_or_event_persistence() {
    let rig = real_rig().await;
    let reached = Arc::new(Semaphore::new(0));
    let release = Arc::new(Semaphore::new(0));
    let artifacts = Arc::new(DelayedPublishStore {
        inner: Arc::clone(&rig.store),
        reached: Arc::clone(&reached),
        release: Arc::clone(&release),
    });
    let generator = service(
        Arc::clone(&rig.index) as Arc<dyn FrameSource>,
        artifacts as Arc<dyn ArtifactStore>,
    );
    let request = rig.request.clone();
    let task = tokio::spawn(async move {
        generator
            .generate(request, ArtifactGenerationContext::default())
            .await
    });
    let reached_permit = reached.acquire().await.unwrap();
    reached_permit.forget();
    let gap = CaptureGap::new(
        krometrail_core::GapId::from_uuid(Uuid::from_u128(999)),
        rig.session,
        rig.target,
        SessionRange::new(SessionTime::from_nanos(4), SessionTime::from_nanos(5)).unwrap(),
        ObservedTime::from_nanos(30),
        CaptureGapReason::FrameRejected,
        NonZeroU64::new(1),
        None,
    )
    .unwrap();
    tokio::time::timeout(Duration::from_secs(1), rig.store.append_gap(gap))
        .await
        .expect("gap persistence must not wait for artifact publication")
        .unwrap();
    let persisted_frame = FrameId::from_uuid(Uuid::from_u128(998));
    tokio::time::timeout(
        Duration::from_secs(1),
        rig.store.append_frame(
            EncodedFrame::new(
                CapturedFrame::new(
                    persisted_frame,
                    rig.session,
                    rig.target,
                    CaptureOrdinal::new(5).unwrap(),
                    None,
                    ObservedTime::from_nanos(31),
                    SessionTime::from_nanos(4),
                    ImageFormat::Png,
                    PixelDimensions::new(2, 2).unwrap(),
                    PixelDimensions::new(3, 2).unwrap(),
                    DeviceScaleFactor::new(1.0).unwrap(),
                    vec![],
                )
                .unwrap(),
                PNG.to_vec(),
            )
            .unwrap(),
        ),
    )
    .await
    .expect("frame persistence must not wait for artifact publication")
    .unwrap();
    let event = BrowserEvent::new(
        BrowserEventId::from_uuid(Uuid::from_u128(997)),
        rig.session,
        rig.target,
        1,
        BrowserEventOrdinal::new(1).unwrap(),
        SessionTime::from_nanos(4),
        None,
        ObservedTime::from_nanos(32),
        BrowserEventSeverity::Info,
        BrowserEventPayload::TargetLifecycle(TargetLifecycleEvent::new(TargetLifecycle::Attached)),
    )
    .unwrap();
    tokio::time::timeout(
        Duration::from_secs(1),
        rig.store
            .append_event_batch(BrowserEventBatch::new(rig.session, vec![event]).unwrap()),
    )
    .await
    .expect("schema-v5 event persistence must not wait for artifact publication")
    .unwrap();
    release.add_permits(4);
    task.await.unwrap().unwrap();
    assert_eq!(
        rig.store.frames_by_id(vec![persisted_frame]).await.unwrap()[0]
            .metadata()
            .id(),
        persisted_frame
    );
}

#[tokio::test]
#[ignore = "manual synthetic 1080p workload; reports shape only and has no CI speed threshold"]
async fn manual_1080p_workload_reports_artifact_and_ingestion_metrics() {
    use image::ImageEncoder as _;

    let root = std::env::temp_dir().join(format!("krometrail-artifact-1080p-{}", Uuid::new_v4()));
    let segments = root.join("segments");
    let index = Arc::new(
        SqliteIndex::open(IndexStoreConfig {
            database_path: root.join("index.sqlite3"),
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
    let store =
        Arc::new(RecordingStore::new(writer, Arc::clone(&index), store_test_clock()).unwrap());
    let dimensions = PixelDimensions::new(1920, 1080).unwrap();
    let mut rgba = vec![0_u8; 1920 * 1080 * 4];
    for pixel in rgba.chunks_exact_mut(4) {
        pixel.copy_from_slice(&[32, 96, 160, 255]);
    }
    let mut encoded = Vec::new();
    image::codecs::png::PngEncoder::new(&mut encoded)
        .write_image(&rgba, 1920, 1080, image::ExtendedColorType::Rgba8)
        .unwrap();
    let session = SessionId::from_uuid(Uuid::from_u128(8_000));
    let target = TargetId::from_uuid(Uuid::from_u128(8_001));
    let mut frame_ids = Vec::new();
    for position in 0_u64..24 {
        let frame_id = FrameId::from_uuid(Uuid::from_u128(8_100 + u128::from(position)));
        frame_ids.push(frame_id);
        store
            .append_frame(
                EncodedFrame::new(
                    CapturedFrame::new(
                        frame_id,
                        session,
                        target,
                        CaptureOrdinal::new(position + 1).unwrap(),
                        None,
                        ObservedTime::from_nanos(position + 1),
                        SessionTime::from_nanos(position + 1),
                        ImageFormat::Png,
                        dimensions,
                        dimensions,
                        DeviceScaleFactor::new(1.0).unwrap(),
                        vec![],
                    )
                    .unwrap(),
                    encoded.clone(),
                )
                .unwrap(),
            )
            .await
            .unwrap();
    }
    store.flush(session).await.unwrap();
    let range = SessionRange::new(SessionTime::from_nanos(1), SessionTime::from_nanos(24)).unwrap();
    let request = ArtifactGenerationRequest::new(
        ResolvedRange::new(
            session,
            target,
            TemporalRangeAnchorKind::SessionTime,
            range,
            range,
            frame_ids,
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            RangeResolutionOptions::DEFAULT,
        )
        .unwrap(),
        vec![],
        vec![ArtifactGeneratorRequest::DifferenceMap(
            krometrail_core::DifferenceMapRequest {
                reference: FrameSelector::First,
                frequency_mode: temporal_vision::FrequencyMode::Count,
                sampling: krometrail_core::ArtifactSampling::Exhaustive,
                repeated_change_separation_nanos: None,
                noise_floor: 0,
                normalization: NormalizationRequest::new(
                    None,
                    Rgb8::new(0, 0, 0),
                    AnalysisScale::FitLimits,
                )
                .unwrap(),
                canvas_background: Rgb8::new(0, 0, 0),
                output: OutputLimitsRequest::new(8192, 8192, 64 * 1024 * 1024).unwrap(),
            },
        )],
        ArtifactFailurePolicy::RequireAll,
    )
    .unwrap();
    let limits = ArtifactWorkLimits {
        max_wall_time: Duration::from_secs(120),
        ..ArtifactWorkLimits::default()
    };
    let generator = TemporalVisionArtifactService::new(
        Arc::clone(&index) as Arc<dyn FrameSource>,
        Arc::clone(&store) as Arc<dyn ArtifactStore>,
        Arc::new(SequenceIds(AtomicU64::new(9_000))),
        limits,
    )
    .unwrap();

    let uncached_start = std::time::Instant::now();
    let first = generator
        .generate(request.clone(), ArtifactGenerationContext::default())
        .await
        .unwrap();
    let uncached = uncached_start.elapsed();
    let ingestion_start = std::time::Instant::now();
    store
        .append_gap(
            CaptureGap::new(
                krometrail_core::GapId::from_uuid(Uuid::from_u128(9_999)),
                session,
                target,
                SessionRange::new(SessionTime::from_nanos(25), SessionTime::from_nanos(26))
                    .unwrap(),
                ObservedTime::from_nanos(26),
                CaptureGapReason::FrameRejected,
                NonZeroU64::new(1),
                None,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let ingestion = ingestion_start.elapsed();
    let cached_start = std::time::Instant::now();
    generator
        .generate(request, ArtifactGenerationContext::default())
        .await
        .unwrap();
    let cached = cached_start.elapsed();
    let status = store.status().await.unwrap();
    eprintln!(
        "synthetic_1080p frames=24 encoded_source_bytes={} decoded_rgba_bytes={} \
         artifact_bytes={} uncached_ms={} cached_ms={} ingestion_us={} outcomes={}",
        encoded.len() * 24,
        1920_u64 * 1080 * 4 * 24,
        status.usage.artifact_bytes,
        uncached.as_millis(),
        cached.as_millis(),
        ingestion.as_micros(),
        first.outcomes.len(),
    );
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
