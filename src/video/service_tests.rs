use std::{
    collections::HashMap,
    num::NonZeroU64,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    time::Duration,
};

use image::ImageEncoder as _;
use krometrail_core::{
    ArtifactGenerationContext, ArtifactId, ArtifactLookup, ArtifactPublication, ArtifactPublish,
    ArtifactSourceFingerprint, ArtifactStore, BrowserEvent, BrowserEventBatch, BrowserEventId,
    BrowserEventOrdinal, BrowserEventPayload, BrowserEventSeverity, BrowserEventSink, CaptureGap,
    CaptureGapReason, CaptureOrdinal, CapturedFrame, DeviceScaleFactor, EncodedFrame,
    FrameAvailability, FrameId, FrameSource, IdSource, IdValue, ImageFormat, ObservedTime,
    OutputLimitsRequest, PixelDimensions, PortFuture, RangeResolutionOptions, RecordingSink,
    ResolvedRange, RetentionStore, RetrieveArtifactRequest, SessionId, SessionRange, SessionTime,
    StoredArtifact, StoredVideoArtifact, TargetId, TargetLifecycle, TargetLifecycleEvent,
    TemporalRangeAnchorKind, TemporalVideoEncoder, TemporalVideoGeneration,
    TemporalVideoGenerationRequest, VideoArtifactLookup, VideoArtifactPublication,
    VideoArtifactPublish, VideoEncodeRequest, VideoEncodedClip, VideoEncoderIdentity,
    VideoEncodingContext, VideoPresentationPolicy,
};
use krometrail_store::{
    IndexStoreConfig, RecordingStore, RotationConfig, SegmentStoreConfig, SegmentWriter,
    SqliteIndex,
};
use sha2::{Digest, Sha256};
use temporal_vision::OutputHash;
use uuid::Uuid;

use super::{
    TemporalVideoGenerationService, VideoGenerationLimits,
    adapt::output_geometry,
    slate::{GAP_SLATE_MIN_HEIGHT, GAP_SLATE_MIN_WIDTH, render_gap_slate},
};

struct FakeFrames {
    frames: Vec<EncodedFrame>,
    loads: AtomicUsize,
}

impl FrameSource for FakeFrames {
    fn list_source_frames(
        &self,
        _: krometrail_core::SourceFramesRequest,
    ) -> PortFuture<'_, krometrail_core::Result<krometrail_core::SourceFrameList>> {
        panic!("unused")
    }
    fn fetch_source_frames(
        &self,
        _: krometrail_core::SourceFramesRequest,
    ) -> PortFuture<'_, krometrail_core::Result<krometrail_core::SourceFrameBatch>> {
        panic!("unused")
    }
    fn read_source_frame(
        &self,
        _: krometrail_core::RetrieveSourceFrameRequest,
    ) -> PortFuture<'_, krometrail_core::Result<krometrail_core::SourceFrameRead>> {
        panic!("unused")
    }

    fn frames_by_id(
        &self,
        frame_ids: Vec<FrameId>,
    ) -> PortFuture<'_, krometrail_core::Result<Vec<EncodedFrame>>> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        let result = frame_ids
            .into_iter()
            .map(|id| {
                self.frames
                    .iter()
                    .find(|frame| frame.metadata().id() == id)
                    .cloned()
                    .ok_or_else(|| {
                        test_error(krometrail_core::ErrorCode::NotFound, "missing frame")
                    })
            })
            .collect();
        Box::pin(std::future::ready(result))
    }

    fn frame_metadata_by_id(
        &self,
        _: Vec<FrameId>,
    ) -> PortFuture<'_, krometrail_core::Result<Vec<CapturedFrame>>> {
        panic!("unused")
    }
    fn frames_in_range(
        &self,
        _: SessionId,
        _: TargetId,
        _: SessionRange,
    ) -> PortFuture<'_, krometrail_core::Result<Vec<EncodedFrame>>> {
        panic!("unused")
    }
    fn frames_in_ordinal_range(
        &self,
        _: SessionId,
        _: TargetId,
        _: CaptureOrdinal,
        _: CaptureOrdinal,
    ) -> PortFuture<'_, krometrail_core::Result<Vec<EncodedFrame>>> {
        panic!("unused")
    }
    fn frame_metadata_in_range(
        &self,
        _: SessionId,
        _: TargetId,
        _: SessionRange,
    ) -> PortFuture<'_, krometrail_core::Result<Vec<CapturedFrame>>> {
        panic!("unused")
    }
    fn frame_metadata_in_ordinal_range(
        &self,
        _: SessionId,
        _: TargetId,
        _: CaptureOrdinal,
        _: CaptureOrdinal,
    ) -> PortFuture<'_, krometrail_core::Result<Vec<CapturedFrame>>> {
        panic!("unused")
    }
    fn frame_availability(
        &self,
        _: SessionId,
        _: TargetId,
    ) -> PortFuture<'_, krometrail_core::Result<FrameAvailability>> {
        panic!("unused")
    }
}

#[derive(Default)]
struct FakeArtifacts {
    by_key: Mutex<
        HashMap<
            krometrail_core::ArtifactCacheKey,
            (Vec<ArtifactSourceFingerprint>, StoredVideoArtifact),
        >,
    >,
    by_id: Mutex<HashMap<ArtifactId, StoredVideoArtifact>>,
    forced_existing: Mutex<Option<StoredVideoArtifact>>,
    publications: AtomicUsize,
    fail_publication: AtomicBool,
    pause_publication: AtomicBool,
    publication_started: tokio::sync::Notify,
    publication_cleanup_complete: AtomicBool,
}

impl ArtifactStore for FakeArtifacts {
    fn lookup_artifact(
        &self,
        _: krometrail_core::ArtifactCacheKey,
        _: Vec<ArtifactSourceFingerprint>,
    ) -> PortFuture<'_, krometrail_core::Result<ArtifactLookup>> {
        Box::pin(std::future::ready(Ok(ArtifactLookup::Miss)))
    }

    fn publish_artifact(
        &self,
        _: ArtifactPublication,
    ) -> PortFuture<'_, krometrail_core::Result<ArtifactPublish>> {
        panic!("unused")
    }

    fn artifact(
        &self,
        _: ArtifactId,
    ) -> PortFuture<'_, krometrail_core::Result<Option<StoredArtifact>>> {
        Box::pin(std::future::ready(Ok(None)))
    }

    fn lookup_video_artifact(
        &self,
        key: krometrail_core::ArtifactCacheKey,
        expected_sources: Vec<ArtifactSourceFingerprint>,
    ) -> PortFuture<'_, krometrail_core::Result<VideoArtifactLookup>> {
        let result = self.by_key.lock().unwrap().get(&key).cloned().map_or(
            VideoArtifactLookup::Miss,
            |(sources, stored)| {
                if sources == expected_sources {
                    VideoArtifactLookup::Hit(Box::new(stored))
                } else {
                    VideoArtifactLookup::Invalidated
                }
            },
        );
        Box::pin(std::future::ready(Ok(result)))
    }

    fn publish_video_artifact(
        &self,
        publication: VideoArtifactPublication,
    ) -> PortFuture<'_, krometrail_core::Result<VideoArtifactPublish>> {
        Box::pin(async move {
            if self.fail_publication.load(Ordering::SeqCst) {
                return Err(test_error(
                    krometrail_core::ErrorCode::PersistenceFailed,
                    "fake store publication failed",
                ));
            }
            if self.pause_publication.load(Ordering::SeqCst) {
                let cancellation = publication
                    .cancellation()
                    .expect("service publication carries its work cancellation");
                self.publication_started.notify_one();
                cancellation.cancelled().await;
                self.publication_cleanup_complete
                    .store(true, Ordering::SeqCst);
                return Err(test_error(
                    krometrail_core::ErrorCode::Cancelled,
                    "publication cancelled",
                ));
            }
            if publication
                .cancellation()
                .is_some_and(|signal| signal.is_cancelled())
            {
                return Err(test_error(
                    krometrail_core::ErrorCode::Cancelled,
                    "publication cancelled",
                ));
            }
            if let Some(stored) = self.forced_existing.lock().unwrap().take() {
                return Ok(VideoArtifactPublish::Existing(stored));
            }
            let mut by_key = self.by_key.lock().unwrap();
            if let Some((_, stored)) = by_key.get(&publication.cache.cache_key) {
                Ok(VideoArtifactPublish::Existing(stored.clone()))
            } else {
                let stored = StoredVideoArtifact {
                    cache: publication.cache.clone(),
                    manifest: publication.manifest.clone(),
                    encoded_bytes: Arc::clone(&publication.encoded_bytes),
                };
                by_key.insert(
                    publication.cache.cache_key,
                    (publication.sources, stored.clone()),
                );
                self.by_id
                    .lock()
                    .unwrap()
                    .insert(stored.manifest.artifact_id(), stored.clone());
                self.publications.fetch_add(1, Ordering::SeqCst);
                Ok(VideoArtifactPublish::Published(stored))
            }
        })
    }

    fn invalidate_video_artifact(
        &self,
        artifact_id: ArtifactId,
    ) -> PortFuture<'_, krometrail_core::Result<()>> {
        self.by_id.lock().unwrap().remove(&artifact_id);
        self.by_key
            .lock()
            .unwrap()
            .retain(|_, (_, stored)| stored.manifest.artifact_id() != artifact_id);
        Box::pin(std::future::ready(Ok(())))
    }

    fn video_artifact(
        &self,
        artifact_id: ArtifactId,
    ) -> PortFuture<'_, krometrail_core::Result<Option<StoredVideoArtifact>>> {
        Box::pin(std::future::ready(Ok(self
            .by_id
            .lock()
            .unwrap()
            .get(&artifact_id)
            .cloned())))
    }
}

struct FakeIds(AtomicU64);

impl IdSource for FakeIds {
    fn next(&self) -> IdValue {
        IdValue::from_uuid(Uuid::from_u128(u128::from(
            self.0.fetch_add(1, Ordering::SeqCst),
        )))
    }
}

struct FakeEncoder {
    identity: VideoEncoderIdentity,
    encodes: AtomicUsize,
    mismatch_identity: AtomicBool,
    mismatch_profile: AtomicBool,
    pause: AtomicBool,
    fail_on_encode: AtomicUsize,
    started: tokio::sync::Notify,
    release: tokio::sync::Semaphore,
    requests: Mutex<Vec<VideoEncodeRequest>>,
}

impl FakeEncoder {
    fn new() -> Self {
        Self {
            identity: encoder_identity(4),
            encodes: AtomicUsize::new(0),
            mismatch_identity: AtomicBool::new(false),
            mismatch_profile: AtomicBool::new(false),
            pause: AtomicBool::new(false),
            fail_on_encode: AtomicUsize::new(0),
            started: tokio::sync::Notify::new(),
            release: tokio::sync::Semaphore::new(0),
            requests: Mutex::new(Vec::new()),
        }
    }
}

impl TemporalVideoEncoder for FakeEncoder {
    fn identity(&self) -> &VideoEncoderIdentity {
        &self.identity
    }

    fn encode(
        &self,
        request: VideoEncodeRequest,
        context: VideoEncodingContext,
    ) -> PortFuture<'_, krometrail_core::Result<VideoEncodedClip>> {
        let encode_number = self.encodes.fetch_add(1, Ordering::SeqCst) + 1;
        self.requests.lock().unwrap().push(request.clone());
        Box::pin(async move {
            if self.pause.load(Ordering::SeqCst) {
                self.started.notify_one();
                tokio::select! {
                    () = context.cancellation.cancelled() => {
                        return Err(test_error(
                            krometrail_core::ErrorCode::Cancelled,
                            "fake encode cancelled",
                        ));
                    }
                    permit = self.release.acquire() => {
                        permit.expect("fake encoder release semaphore is open").forget();
                    }
                }
            }
            if self.fail_on_encode.load(Ordering::SeqCst) == encode_number {
                return Err(test_error(
                    krometrail_core::ErrorCode::VideoEncodingFailed,
                    "fake encode failed",
                ));
            }
            let bytes: Arc<[u8]> = Arc::from(b"fake bounded mp4".as_slice());
            let hash: [u8; 32] = Sha256::digest(&bytes).into();
            let identity = if self.mismatch_identity.load(Ordering::SeqCst) {
                encoder_identity(9)
            } else {
                self.identity.clone()
            };
            let profile = if self.mismatch_profile.load(Ordering::SeqCst) {
                krometrail_core::VideoEncodingProfile::new(
                    request.profile().geometry(),
                    request.profile().max_encoded_bytes() + 1,
                )
                .unwrap()
            } else {
                request.profile()
            };
            VideoEncodedClip::new(identity, profile, OutputHash::from_bytes(hash), bytes)
        })
    }
}

struct Fixture {
    service: TemporalVideoGenerationService,
    frames: Arc<FakeFrames>,
    artifacts: Arc<FakeArtifacts>,
    encoder: Arc<FakeEncoder>,
    request: TemporalVideoGenerationRequest,
}

struct RealFixture {
    directory: std::path::PathBuf,
    index: Arc<SqliteIndex>,
    store: Arc<RecordingStore>,
    service: TemporalVideoGenerationService,
    encoder: Arc<FakeEncoder>,
    request: TemporalVideoGenerationRequest,
    session: SessionId,
    target: TargetId,
}

fn fixture(policy: VideoPresentationPolicy) -> Fixture {
    let session = SessionId::from_uuid(Uuid::from_u128(1));
    let target = TargetId::from_uuid(Uuid::from_u128(2));
    let dimensions = PixelDimensions::new(4, 4).unwrap();
    fixture_with_frames(
        policy,
        vec![
            frame(session, target, 3, 1, 1, dimensions, [20, 30, 40, 255]),
            frame(session, target, 4, 2, 3, dimensions, [220, 210, 200, 255]),
        ],
        4,
        4,
    )
}

fn fixture_with_frames(
    policy: VideoPresentationPolicy,
    source_frames: Vec<EncodedFrame>,
    max_width: u32,
    max_height: u32,
) -> Fixture {
    let session = source_frames[0].metadata().session_id();
    let target = source_frames[0].metadata().target_id();
    let frames = Arc::new(FakeFrames {
        frames: source_frames,
        loads: AtomicUsize::new(0),
    });
    let frame_ids = frames
        .frames
        .iter()
        .map(|frame| frame.metadata().id())
        .collect();
    let range = ResolvedRange::new(
        session,
        target,
        TemporalRangeAnchorKind::SessionTime,
        SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(5)).unwrap(),
        SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(5)).unwrap(),
        frame_ids,
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        RangeResolutionOptions::DEFAULT,
    )
    .unwrap();
    let request = TemporalVideoGenerationRequest::new(
        range,
        policy,
        OutputLimitsRequest::new(max_width, max_height, 1_024).unwrap(),
    )
    .unwrap();
    let artifacts = Arc::new(FakeArtifacts::default());
    let encoder = Arc::new(FakeEncoder::new());
    let service = TemporalVideoGenerationService::new(
        Arc::clone(&frames) as Arc<dyn FrameSource>,
        Arc::clone(&artifacts) as Arc<dyn ArtifactStore>,
        Arc::new(FakeIds(AtomicU64::new(100))),
        Arc::clone(&encoder) as Arc<dyn TemporalVideoEncoder>,
        VideoGenerationLimits::default(),
    )
    .unwrap();
    Fixture {
        service,
        frames,
        artifacts,
        encoder,
        request,
    }
}

async fn real_fixture() -> RealFixture {
    let directory = std::env::temp_dir().join(format!("krometrail-video-{}", Uuid::new_v4()));
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
    let store =
        Arc::new(RecordingStore::new(writer, Arc::clone(&index), store_test_clock()).unwrap());
    let session = SessionId::from_uuid(Uuid::from_u128(20_001));
    let target = TargetId::from_uuid(Uuid::from_u128(20_002));
    let dimensions = PixelDimensions::new(4, 4).unwrap();
    let frames = vec![
        frame(session, target, 20_003, 1, 1, dimensions, [20, 30, 40, 255]),
        frame(
            session,
            target,
            20_004,
            2,
            3,
            dimensions,
            [220, 210, 200, 255],
        ),
    ];
    for frame in &frames {
        store.append_frame(frame.clone()).await.unwrap();
    }
    store.flush(session).await.unwrap();
    let range = ResolvedRange::new(
        session,
        target,
        TemporalRangeAnchorKind::SessionTime,
        SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(5)).unwrap(),
        SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(5)).unwrap(),
        frames.iter().map(|frame| frame.metadata().id()).collect(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        RangeResolutionOptions::DEFAULT,
    )
    .unwrap();
    let request = TemporalVideoGenerationRequest::new(
        range,
        VideoPresentationPolicy::RealTime,
        OutputLimitsRequest::new(4, 4, 1_024).unwrap(),
    )
    .unwrap();
    let encoder = Arc::new(FakeEncoder::new());
    let service = TemporalVideoGenerationService::new(
        Arc::clone(&index) as Arc<dyn FrameSource>,
        Arc::clone(&store) as Arc<dyn ArtifactStore>,
        Arc::new(FakeIds(AtomicU64::new(20_100))),
        Arc::clone(&encoder) as Arc<dyn TemporalVideoEncoder>,
        VideoGenerationLimits::default(),
    )
    .unwrap();
    RealFixture {
        directory,
        index,
        store,
        service,
        encoder,
        request,
        session,
        target,
    }
}

#[derive(Default)]
struct ManualCancellation {
    cancelled: AtomicBool,
    notify: tokio::sync::Notify,
}

impl ManualCancellation {
    fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }
}

impl krometrail_core::CancellationSignal for ManualCancellation {
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    fn cancelled(&self) -> PortFuture<'_, ()> {
        Box::pin(async move {
            loop {
                let notified = self.notify.notified();
                tokio::pin!(notified);
                notified.as_mut().enable();
                if self.is_cancelled() {
                    return;
                }
                notified.await;
            }
        })
    }
}

#[tokio::test]
async fn fake_encoder_receives_exact_plan_and_repeat_requests_reuse_cache() {
    let fixture = fixture(VideoPresentationPolicy::RealTime);
    let first = fixture
        .service
        .generate_video(
            fixture.request.clone(),
            ArtifactGenerationContext::default(),
        )
        .await
        .unwrap();
    let second = fixture
        .service
        .generate_video(fixture.request, ArtifactGenerationContext::default())
        .await
        .unwrap();
    assert_eq!(first.clips.len(), 1);
    assert_eq!(
        first.clips[0].cache,
        krometrail_core::ArtifactCacheDisposition::Generated
    );
    assert_eq!(
        second.clips[0].cache,
        krometrail_core::ArtifactCacheDisposition::Hit
    );
    assert_eq!(
        first.clips[0].artifact.artifact_id,
        second.clips[0].artifact.artifact_id
    );
    assert_eq!(fixture.encoder.encodes.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.artifacts.publications.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.frames.loads.load(Ordering::SeqCst), 2);
    let requests = fixture.encoder.requests.lock().unwrap();
    assert_eq!(
        requests[0].frames().len(),
        requests[0].plan().segments().len()
    );
    assert!(
        requests[0]
            .frames()
            .iter()
            .zip(requests[0].plan().segments())
            .all(|(frame, segment)| frame.segment_index() == segment.index()
                && frame.source() == segment.source())
    );
}

#[tokio::test]
async fn concurrent_equal_requests_encode_once() {
    let fixture = fixture(VideoPresentationPolicy::RealTime);
    let (left, right) = tokio::join!(
        fixture.service.generate_video(
            fixture.request.clone(),
            ArtifactGenerationContext::default()
        ),
        fixture.service.generate_video(
            fixture.request.clone(),
            ArtifactGenerationContext::default()
        )
    );
    assert!(left.is_ok() && right.is_ok());
    assert_eq!(fixture.encoder.encodes.load(Ordering::SeqCst), 1);
    assert_eq!(fixture.artifacts.publications.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn model_selection_is_deterministic_and_provenance_bound() {
    let fixture = fixture(VideoPresentationPolicy::ModelOptimized);
    let first = fixture
        .service
        .generate_video(
            fixture.request.clone(),
            ArtifactGenerationContext::default(),
        )
        .await
        .unwrap();
    let second = fixture
        .service
        .generate_video(fixture.request, ArtifactGenerationContext::default())
        .await
        .unwrap();
    let first_manifest = &first.clips[0].artifact.provenance;
    let second_manifest = &second.clips[0].artifact.provenance;
    assert!(!first_manifest.plan().meaningful_frame_ids().is_empty());
    assert_eq!(first_manifest.selection(), second_manifest.selection());
    assert_eq!(
        first_manifest.selection().unwrap().parameters_sha256(),
        &[
            57, 221, 220, 46, 236, 237, 178, 222, 72, 193, 22, 194, 146, 224, 105, 75, 159, 117,
            91, 235, 27, 12, 131, 230, 144, 14, 219, 3, 118, 106, 102, 219,
        ],
        "selection provenance must bind the selector, filter, max edge, and normalization profile"
    );
    assert_eq!(fixture.encoder.encodes.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn contradictory_encoder_identity_never_publishes() {
    let fixture = fixture(VideoPresentationPolicy::RealTime);
    fixture
        .encoder
        .mismatch_identity
        .store(true, Ordering::SeqCst);
    let error = fixture
        .service
        .generate_video(fixture.request, ArtifactGenerationContext::default())
        .await
        .unwrap_err();
    assert_eq!(
        error.code,
        krometrail_core::ErrorCode::ArtifactGenerationFailed
    );
    assert_eq!(fixture.artifacts.publications.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn contradictory_encoder_profile_never_publishes() {
    let fixture = fixture(VideoPresentationPolicy::RealTime);
    fixture
        .encoder
        .mismatch_profile
        .store(true, Ordering::SeqCst);
    let error = fixture
        .service
        .generate_video(fixture.request, ArtifactGenerationContext::default())
        .await
        .unwrap_err();
    assert_eq!(
        error.code,
        krometrail_core::ErrorCode::ArtifactGenerationFailed
    );
    assert_eq!(fixture.artifacts.publications.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn store_failure_returns_no_success_or_cached_artifact() {
    let fixture = fixture(VideoPresentationPolicy::RealTime);
    fixture
        .artifacts
        .fail_publication
        .store(true, Ordering::SeqCst);
    let error = fixture
        .service
        .generate_video(fixture.request, ArtifactGenerationContext::default())
        .await
        .unwrap_err();
    assert_eq!(error.code, krometrail_core::ErrorCode::PersistenceFailed);
    assert_eq!(fixture.artifacts.publications.load(Ordering::SeqCst), 0);
    assert!(fixture.artifacts.by_id.lock().unwrap().is_empty());
}

#[tokio::test]
async fn contradictory_cache_hit_is_invalidated_and_regenerated() {
    let fixture = fixture(VideoPresentationPolicy::RealTime);
    fixture
        .service
        .generate_video(
            fixture.request.clone(),
            ArtifactGenerationContext::default(),
        )
        .await
        .unwrap();
    {
        let mut cached = fixture.artifacts.by_key.lock().unwrap();
        let (_, stored) = cached.values_mut().next().unwrap();
        stored.cache.adapter_version =
            krometrail_core::NonEmptyText::new("contradictory-adapter").unwrap();
    }
    let regenerated = fixture
        .service
        .generate_video(fixture.request, ArtifactGenerationContext::default())
        .await
        .unwrap();
    assert_eq!(
        regenerated.clips[0].cache,
        krometrail_core::ArtifactCacheDisposition::RegeneratedAfterInvalidation
    );
    assert_eq!(fixture.encoder.encodes.load(Ordering::SeqCst), 2);
    assert_eq!(fixture.artifacts.by_id.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn contradictory_concurrent_existing_result_is_invalidated_and_rejected() {
    let fixture = fixture(VideoPresentationPolicy::RealTime);
    fixture
        .service
        .generate_video(
            fixture.request.clone(),
            ArtifactGenerationContext::default(),
        )
        .await
        .unwrap();
    let mut contradictory = fixture
        .artifacts
        .by_key
        .lock()
        .unwrap()
        .values()
        .next()
        .unwrap()
        .1
        .clone();
    contradictory.cache.adapter_version =
        krometrail_core::NonEmptyText::new("contradictory-existing").unwrap();
    fixture.artifacts.by_key.lock().unwrap().clear();
    *fixture.artifacts.forced_existing.lock().unwrap() = Some(contradictory);

    let error = fixture
        .service
        .generate_video(fixture.request, ArtifactGenerationContext::default())
        .await
        .unwrap_err();
    assert_eq!(
        error.code,
        krometrail_core::ErrorCode::ArtifactGenerationFailed
    );
    assert!(fixture.artifacts.by_id.lock().unwrap().is_empty());
}

#[tokio::test]
async fn cancellation_during_encode_stops_work_before_publication() {
    let fixture = fixture(VideoPresentationPolicy::RealTime);
    fixture.encoder.pause.store(true, Ordering::SeqCst);
    let cancellation = Arc::new(ManualCancellation::default());
    let task = tokio::spawn({
        let service = fixture.service.clone();
        let request = fixture.request;
        let cancellation = Arc::clone(&cancellation);
        async move {
            service
                .generate_video(
                    request,
                    ArtifactGenerationContext {
                        deadline: None,
                        cancellation: Some(cancellation),
                        ..Default::default()
                    },
                )
                .await
        }
    });
    fixture.encoder.started.notified().await;
    cancellation.cancel();
    let error = task.await.unwrap().unwrap_err();
    assert_eq!(error.code, krometrail_core::ErrorCode::Cancelled);
    assert_eq!(fixture.artifacts.publications.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn caller_cancellation_signals_publication_and_awaits_store_cleanup() {
    let fixture = fixture(VideoPresentationPolicy::RealTime);
    fixture
        .artifacts
        .pause_publication
        .store(true, Ordering::SeqCst);
    let cancellation = Arc::new(ManualCancellation::default());
    let task = tokio::spawn({
        let service = fixture.service.clone();
        let request = fixture.request;
        let cancellation = Arc::clone(&cancellation);
        async move {
            service
                .generate_video(
                    request,
                    ArtifactGenerationContext {
                        deadline: None,
                        cancellation: Some(cancellation),
                        ..Default::default()
                    },
                )
                .await
        }
    });
    fixture.artifacts.publication_started.notified().await;
    cancellation.cancel();
    let error = task.await.unwrap().unwrap_err();
    assert_eq!(error.code, krometrail_core::ErrorCode::Cancelled);
    assert!(
        fixture
            .artifacts
            .publication_cleanup_complete
            .load(Ordering::SeqCst)
    );
    assert!(fixture.artifacts.by_id.lock().unwrap().is_empty());
}

#[tokio::test]
async fn caller_deadline_signals_publication_and_awaits_store_cleanup() {
    let fixture = fixture(VideoPresentationPolicy::RealTime);
    fixture
        .artifacts
        .pause_publication
        .store(true, Ordering::SeqCst);
    let error = fixture
        .service
        .generate_video(
            fixture.request,
            ArtifactGenerationContext {
                deadline: Some(std::time::Instant::now() + Duration::from_millis(25)),
                cancellation: None,
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
    assert_eq!(
        error.code,
        krometrail_core::ErrorCode::ArtifactGenerationFailed
    );
    assert!(
        fixture
            .artifacts
            .publication_cleanup_complete
            .load(Ordering::SeqCst)
    );
    assert!(fixture.artifacts.by_id.lock().unwrap().is_empty());
}

#[tokio::test]
async fn cancellation_and_deadline_are_checked_before_source_load() {
    let cancelled_fixture = fixture(VideoPresentationPolicy::RealTime);
    let cancellation = Arc::new(ManualCancellation::default());
    cancellation.cancel();
    let error = cancelled_fixture
        .service
        .generate_video(
            cancelled_fixture.request,
            ArtifactGenerationContext {
                deadline: None,
                cancellation: Some(cancellation),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, krometrail_core::ErrorCode::Cancelled);
    assert_eq!(cancelled_fixture.frames.loads.load(Ordering::SeqCst), 0);

    let expired_fixture = fixture(VideoPresentationPolicy::RealTime);
    let error = expired_fixture
        .service
        .generate_video(
            expired_fixture.request,
            ArtifactGenerationContext {
                deadline: Some(std::time::Instant::now()),
                cancellation: None,
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
    assert_eq!(
        error.code,
        krometrail_core::ErrorCode::ArtifactGenerationFailed
    );
    assert_eq!(expired_fixture.frames.loads.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn later_epoch_failure_returns_no_partial_result() {
    let session = SessionId::from_uuid(Uuid::from_u128(1));
    let target = TargetId::from_uuid(Uuid::from_u128(2));
    let first_geometry = PixelDimensions::new(4, 4).unwrap();
    let second_geometry = PixelDimensions::new(6, 4).unwrap();
    let fixture = fixture_with_frames(
        VideoPresentationPolicy::RealTime,
        vec![
            frame(session, target, 3, 1, 1, first_geometry, [20, 30, 40, 255]),
            frame(session, target, 4, 2, 2, first_geometry, [60, 70, 80, 255]),
            frame(
                session,
                target,
                5,
                3,
                3,
                second_geometry,
                [90, 100, 110, 255],
            ),
        ],
        6,
        4,
    );
    fixture.encoder.fail_on_encode.store(2, Ordering::SeqCst);
    let error = fixture
        .service
        .generate_video(fixture.request, ArtifactGenerationContext::default())
        .await
        .unwrap_err();
    assert_eq!(error.code, krometrail_core::ErrorCode::VideoEncodingFailed);
    assert_eq!(fixture.encoder.encodes.load(Ordering::SeqCst), 2);
    assert_eq!(fixture.artifacts.publications.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn real_store_cache_corruption_regenerates_through_the_service() {
    let fixture = real_fixture().await;
    let first = fixture
        .service
        .generate_video(
            fixture.request.clone(),
            ArtifactGenerationContext::default(),
        )
        .await
        .unwrap();
    let repeat = fixture
        .service
        .generate_video(
            fixture.request.clone(),
            ArtifactGenerationContext::default(),
        )
        .await
        .unwrap();
    assert_eq!(
        repeat.clips[0].cache,
        krometrail_core::ArtifactCacheDisposition::Hit
    );
    assert_eq!(fixture.encoder.encodes.load(Ordering::SeqCst), 1);
    let path = fixture
        .directory
        .join("artifacts")
        .join(format!("{}.mp4", first.clips[0].artifact.artifact_id));
    std::fs::write(path, b"corrupt").unwrap();
    let regenerated = fixture
        .service
        .generate_video(fixture.request, ArtifactGenerationContext::default())
        .await
        .unwrap();
    assert_eq!(
        regenerated.clips[0].cache,
        krometrail_core::ArtifactCacheDisposition::RegeneratedAfterInvalidation
    );
    assert_eq!(fixture.encoder.encodes.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn retained_video_read_stays_scoped_and_byte_bounded_behind_the_service() {
    let fixture = real_fixture().await;
    let generated = fixture
        .service
        .generate_video(
            fixture.request.clone(),
            ArtifactGenerationContext::default(),
        )
        .await
        .unwrap();
    let handle = &generated.clips[0].artifact;
    let read = fixture
        .service
        .read_video_artifact(
            RetrieveArtifactRequest::new(handle.scope, handle.artifact_id, handle.encoded_byte_len)
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(read.handle, *handle);
    assert_eq!(read.encoded_bytes(), b"fake bounded mp4");

    let wrong_scope = krometrail_core::EvidenceScope::new(
        fixture.session,
        TargetId::from_uuid(Uuid::from_u128(99_999)),
    )
    .unwrap();
    let error = fixture
        .service
        .read_video_artifact(
            RetrieveArtifactRequest::new(wrong_scope, handle.artifact_id, handle.encoded_byte_len)
                .unwrap(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, krometrail_core::ErrorCode::NotFound);
}

#[tokio::test]
async fn session_deletion_completes_during_encode_and_fences_late_publication() {
    let fixture = real_fixture().await;
    fixture.encoder.pause.store(true, Ordering::SeqCst);
    let task = tokio::spawn({
        let service = fixture.service.clone();
        let request = fixture.request;
        async move {
            service
                .generate_video(request, ArtifactGenerationContext::default())
                .await
        }
    });
    fixture.encoder.started.notified().await;
    tokio::time::timeout(
        Duration::from_secs(1),
        fixture.store.delete_session(fixture.session),
    )
    .await
    .expect("session deletion must not wait for video encoding")
    .unwrap();
    fixture.encoder.release.add_permits(1);
    assert!(task.await.unwrap().is_err());
    assert_eq!(
        fixture.store.status().await.unwrap().usage.artifact_bytes,
        0
    );
    assert!(
        fixture
            .index
            .frames_by_id(vec![FrameId::from_uuid(Uuid::from_u128(20_003))])
            .await
            .is_err()
    );
    let artifacts = fixture.directory.join("artifacts");
    assert!(
        !artifacts.exists() || std::fs::read_dir(artifacts).unwrap().next().is_none(),
        "late video work must leave no ready, staged, or temporary artifact file"
    );
}

#[tokio::test]
async fn paused_video_encode_does_not_block_frame_gap_or_event_ingestion() {
    let fixture = real_fixture().await;
    fixture.encoder.pause.store(true, Ordering::SeqCst);
    let task = tokio::spawn({
        let service = fixture.service.clone();
        let request = fixture.request;
        async move {
            service
                .generate_video(request, ArtifactGenerationContext::default())
                .await
        }
    });
    fixture.encoder.started.notified().await;
    let dimensions = PixelDimensions::new(4, 4).unwrap();
    let retained = frame(
        fixture.session,
        fixture.target,
        20_050,
        3,
        4,
        dimensions,
        [80, 90, 100, 255],
    );
    tokio::time::timeout(Duration::from_secs(1), fixture.store.append_frame(retained))
        .await
        .expect("frame ingestion must not wait for video encoding")
        .unwrap();
    let gap = CaptureGap::new(
        krometrail_core::GapId::from_uuid(Uuid::from_u128(20_051)),
        fixture.session,
        fixture.target,
        SessionRange::new(SessionTime::from_nanos(4), SessionTime::from_nanos(5)).unwrap(),
        ObservedTime::from_nanos(5),
        CaptureGapReason::FrameRejected,
        NonZeroU64::new(1),
        None,
    )
    .unwrap();
    tokio::time::timeout(Duration::from_secs(1), fixture.store.append_gap(gap))
        .await
        .expect("gap ingestion must not wait for video encoding")
        .unwrap();
    let event = BrowserEvent::new(
        BrowserEventId::from_uuid(Uuid::from_u128(20_052)),
        fixture.session,
        fixture.target,
        1,
        BrowserEventOrdinal::new(1).unwrap(),
        SessionTime::from_nanos(4),
        None,
        ObservedTime::from_nanos(6),
        BrowserEventSeverity::Info,
        BrowserEventPayload::TargetLifecycle(TargetLifecycleEvent::new(TargetLifecycle::Attached)),
    )
    .unwrap();
    tokio::time::timeout(
        Duration::from_secs(1),
        fixture
            .store
            .append_event_batch(BrowserEventBatch::new(fixture.session, vec![event]).unwrap()),
    )
    .await
    .expect("event ingestion must not wait for video encoding")
    .unwrap();
    fixture.encoder.release.add_permits(1);
    task.await.unwrap().unwrap();
    assert_eq!(
        fixture
            .store
            .frames_by_id(vec![FrameId::from_uuid(Uuid::from_u128(20_050))])
            .await
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn geometry_and_gap_slate_are_exact_bounded_and_deterministic() {
    let geometry = output_geometry(
        PixelDimensions::new(10, 6).unwrap(),
        OutputLimitsRequest::new(6, 4, 1_024).unwrap(),
    )
    .unwrap();
    assert_eq!(geometry.scaled(), PixelDimensions::new(5, 3).unwrap());
    assert_eq!(geometry.canvas(), PixelDimensions::new(6, 4).unwrap());

    let geometry = output_geometry(
        PixelDimensions::new(16, 8).unwrap(),
        OutputLimitsRequest::new(12, 8, 1_024).unwrap(),
    )
    .unwrap();
    assert_eq!(geometry.scaled(), PixelDimensions::new(12, 6).unwrap());
    assert_eq!(geometry.canvas(), PixelDimensions::new(12, 6).unwrap());
    assert!(
        output_geometry(
            PixelDimensions::new(1, 3).unwrap(),
            OutputLimitsRequest::new(2, 2, 1_024).unwrap(),
        )
        .is_err()
    );

    let range = SessionRange::new(
        SessionTime::from_nanos(u64::MAX - 1),
        SessionTime::from_nanos(u64::MAX),
    )
    .unwrap();
    let boundary = PixelDimensions::new(GAP_SLATE_MIN_WIDTH, GAP_SLATE_MIN_HEIGHT).unwrap();
    let first = render_gap_slate(boundary, range).unwrap();
    let second = render_gap_slate(boundary, range).unwrap();
    assert_eq!(first, second);
    let decoded = image::load_from_memory_with_format(&first, image::ImageFormat::Png)
        .unwrap()
        .into_rgba8();
    assert_eq!(
        (decoded.width(), decoded.height()),
        (GAP_SLATE_MIN_WIDTH, GAP_SLATE_MIN_HEIGHT)
    );
    let label_pixels = decoded
        .enumerate_pixels()
        .filter(|(_, y, pixel)| *y < GAP_SLATE_MIN_HEIGHT / 2 && pixel.0 == [255, 245, 138, 255])
        .count();
    let interval_pixels = decoded
        .enumerate_pixels()
        .filter(|(_, y, pixel)| *y >= GAP_SLATE_MIN_HEIGHT / 2 && pixel.0 == [255, 245, 138, 255])
        .count();
    assert!(
        label_pixels > 0,
        "capture-gap label must be visibly rendered"
    );
    assert!(
        interval_pixels > 0,
        "source-time interval must be visibly rendered"
    );
    let too_narrow = PixelDimensions::new(GAP_SLATE_MIN_WIDTH - 1, GAP_SLATE_MIN_HEIGHT).unwrap();
    assert_eq!(
        render_gap_slate(too_narrow, range).unwrap_err().code,
        krometrail_core::ErrorCode::ResourceLimitExceeded
    );
}

fn frame(
    session: SessionId,
    target: TargetId,
    id: u128,
    ordinal: u64,
    time: u64,
    dimensions: PixelDimensions,
    color: [u8; 4],
) -> EncodedFrame {
    let metadata = CapturedFrame::new(
        FrameId::from_uuid(Uuid::from_u128(id)),
        session,
        target,
        CaptureOrdinal::new(ordinal).unwrap(),
        None,
        ObservedTime::from_nanos(time),
        SessionTime::from_nanos(time),
        ImageFormat::Png,
        dimensions,
        dimensions,
        DeviceScaleFactor::new(1.0).unwrap(),
        vec![],
    )
    .unwrap();
    let image = image::RgbaImage::from_pixel(dimensions.width(), dimensions.height(), color.into());
    let mut bytes = Vec::new();
    image::codecs::png::PngEncoder::new(&mut bytes)
        .write_image(
            image.as_raw(),
            dimensions.width(),
            dimensions.height(),
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
    EncodedFrame::new(metadata, bytes).unwrap()
}

fn encoder_identity(seed: u8) -> VideoEncoderIdentity {
    VideoEncoderIdentity::new(
        format!("fake-{seed}"),
        [seed; 32],
        "fake-encoder",
        "fake-adapter-v1",
        "fake-arguments-v1",
    )
    .unwrap()
}

fn test_error(
    code: krometrail_core::ErrorCode,
    message: &'static str,
) -> krometrail_core::KrometrailError {
    krometrail_core::KrometrailError::new(
        code,
        krometrail_core::NonEmptyText::new(message).unwrap(),
    )
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
