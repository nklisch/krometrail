use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
};

use image::ImageEncoder as _;
use krometrail_core::{
    ArtifactGenerationContext, ArtifactId, ArtifactLookup, ArtifactPublication, ArtifactPublish,
    ArtifactSourceFingerprint, ArtifactStore, CaptureOrdinal, CapturedFrame, DeviceScaleFactor,
    EncodedFrame, FrameAvailability, FrameId, FrameSource, IdSource, IdValue, ImageFormat,
    ObservedTime, OutputLimitsRequest, PixelDimensions, PortFuture, RangeResolutionOptions,
    ResolvedRange, SessionId, SessionRange, SessionTime, StoredArtifact, StoredVideoArtifact,
    TargetId, TemporalRangeAnchorKind, TemporalVideoEncoder, TemporalVideoGeneration,
    TemporalVideoGenerationRequest, VideoArtifactLookup, VideoArtifactPublication,
    VideoArtifactPublish, VideoEncodeRequest, VideoEncodedClip, VideoEncoderIdentity,
    VideoEncodingContext, VideoPresentationPolicy,
};
use sha2::{Digest, Sha256};
use temporal_vision::OutputHash;
use uuid::Uuid;

use super::{
    TemporalVideoGenerationService, VideoGenerationLimits, adapt::output_geometry,
    slate::render_gap_slate,
};

struct FakeFrames {
    frames: Vec<EncodedFrame>,
    loads: AtomicUsize,
}

impl FrameSource for FakeFrames {
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
    publications: AtomicUsize,
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
        let result = if publication
            .cancellation()
            .is_some_and(|signal| signal.is_cancelled())
        {
            Err(test_error(
                krometrail_core::ErrorCode::Cancelled,
                "publication cancelled",
            ))
        } else {
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
        };
        Box::pin(std::future::ready(result))
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
    pause: AtomicBool,
    fail_on_encode: AtomicUsize,
    started: tokio::sync::Notify,
    requests: Mutex<Vec<VideoEncodeRequest>>,
}

impl FakeEncoder {
    fn new() -> Self {
        Self {
            identity: encoder_identity(4),
            encodes: AtomicUsize::new(0),
            mismatch_identity: AtomicBool::new(false),
            pause: AtomicBool::new(false),
            fail_on_encode: AtomicUsize::new(0),
            started: tokio::sync::Notify::new(),
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
                context.cancellation.cancelled().await;
                return Err(test_error(
                    krometrail_core::ErrorCode::Cancelled,
                    "fake encode cancelled",
                ));
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
            VideoEncodedClip::new(
                identity,
                request.profile(),
                OutputHash::from_bytes(hash),
                bytes,
            )
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

    let range = SessionRange::new(SessionTime::from_nanos(4), SessionTime::from_nanos(8)).unwrap();
    let first = render_gap_slate(PixelDimensions::new(120, 40).unwrap(), range).unwrap();
    let second = render_gap_slate(PixelDimensions::new(120, 40).unwrap(), range).unwrap();
    assert_eq!(first, second);
    let decoded = image::load_from_memory_with_format(&first, image::ImageFormat::Png).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (120, 40));
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
