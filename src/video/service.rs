use std::{
    collections::{HashMap, HashSet},
    future::pending,
    num::NonZeroUsize,
    sync::{Arc, Mutex, Weak},
    time::{Duration, Instant},
};

use krometrail_core::{
    ArtifactCacheDisposition, ArtifactCacheKey, ArtifactGenerationContext, ArtifactId,
    ArtifactStore, CancellationSignal, ErrorCode, EvidenceScope, FrameSource, IdSource,
    KrometrailError, NonEmptyText, PortFuture, Result, Sha256Digest, StoredVideoArtifact,
    TemporalVideoEncoder, TemporalVideoGeneration, TemporalVideoGenerationClip,
    TemporalVideoGenerationRequest, TemporalVideoGenerationResult, TemporalVideoManifest,
    VideoArtifactEvidenceHandle, VideoArtifactLookup, VideoArtifactPublication,
    VideoArtifactPublish, VideoEncodeRequest, VideoEncodedClip, VideoEncodingContext,
    VideoEncodingProfile, VideoPlanInput, VideoPresentationPolicy, VideoSegmentSource,
    canonical_video_cache_parameters,
};
use tokio::sync::Semaphore;

use crate::artifacts::{
    cache::{VideoCacheIdentityInput, video_cache_metadata},
    epoch::{AdaptationLimits, EpochPlan, WorkCancellation, validate_and_plan},
    scheduler::{cancelled_error, controlled, deadline_error, limit_error},
};

use super::{
    adapt::{PreparedVideoEpoch, encode_inputs, meaningful_selection, output_geometry},
    plan::build_presentation_plan,
};

const ENCODER_CLEANUP_GRACE: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VideoGenerationLimits {
    pub max_active_requests: NonZeroUsize,
    pub max_blocking_jobs: NonZeroUsize,
    pub max_analysis_bytes: NonZeroUsize,
    pub max_wall_time: Duration,
}

impl Default for VideoGenerationLimits {
    fn default() -> Self {
        Self {
            max_active_requests: NonZeroUsize::new(2).expect("two is nonzero"),
            max_blocking_jobs: NonZeroUsize::new(2).expect("two is nonzero"),
            max_analysis_bytes: NonZeroUsize::new(256 * 1024 * 1024)
                .expect("analysis limit is nonzero"),
            max_wall_time: Duration::from_secs(30),
        }
    }
}

impl VideoGenerationLimits {
    fn validate(self) -> Result<Self> {
        if self.max_wall_time.is_zero() || self.max_analysis_bytes.get() > u32::MAX as usize {
            return Err(limit_error(
                "video generation limits require nonzero bounded wall time and analysis memory",
            ));
        }
        Ok(self)
    }

    fn adaptation(self) -> AdaptationLimits {
        AdaptationLimits {
            max_source_frames: krometrail_core::MAX_VIDEO_SOURCE_FRAMES,
            max_encoded_source_bytes: krometrail_core::MAX_VIDEO_ENCODED_INPUT_BYTES as usize,
            max_dimension: 8_192,
            max_pixels_per_frame: 16_777_216,
            max_decoded_bytes: 512 * 1024 * 1024,
            max_markers: 1,
        }
    }
}

#[derive(Clone)]
pub(crate) struct TemporalVideoGenerationService {
    frames: Arc<dyn FrameSource>,
    artifacts: Arc<dyn ArtifactStore>,
    ids: Arc<dyn IdSource>,
    encoder: Arc<dyn TemporalVideoEncoder>,
    limits: VideoGenerationLimits,
    requests: Arc<Semaphore>,
    blocking: Arc<Semaphore>,
    analysis: Arc<Semaphore>,
    cache_locks: Arc<VideoCacheLocks>,
}

impl TemporalVideoGenerationService {
    pub(crate) fn new(
        frames: Arc<dyn FrameSource>,
        artifacts: Arc<dyn ArtifactStore>,
        ids: Arc<dyn IdSource>,
        encoder: Arc<dyn TemporalVideoEncoder>,
        limits: VideoGenerationLimits,
    ) -> Result<Self> {
        let limits = limits.validate()?;
        Ok(Self {
            frames,
            artifacts,
            ids,
            encoder,
            limits,
            requests: Arc::new(Semaphore::new(limits.max_active_requests.get())),
            blocking: Arc::new(Semaphore::new(limits.max_blocking_jobs.get())),
            analysis: Arc::new(Semaphore::new(limits.max_analysis_bytes.get())),
            cache_locks: Arc::new(VideoCacheLocks::default()),
        })
    }

    async fn generate_inner(
        &self,
        request: TemporalVideoGenerationRequest,
        context: ArtifactGenerationContext,
    ) -> Result<TemporalVideoGenerationResult> {
        let now = Instant::now();
        let service_deadline = now
            .checked_add(self.limits.max_wall_time)
            .ok_or_else(deadline_error)?;
        let deadline = context
            .deadline
            .map_or(service_deadline, |value| value.min(service_deadline));
        if deadline <= now {
            return Err(deadline_error());
        }
        if context.is_cancelled() {
            return Err(cancelled_error());
        }
        let _request = controlled(
            self.requests.clone().acquire_owned(),
            deadline,
            context.cancellation.as_ref(),
        )
        .await?
        .map_err(|_| generation_error("video request scheduler is closed"))?;
        let frames = controlled(
            self.frames.frames_by_id(request.range().frame_ids.clone()),
            deadline,
            context.cancellation.as_ref(),
        )
        .await??;
        let cancellation = WorkCancellation::default();
        let plans = self
            .run_blocking(
                deadline,
                context.cancellation.as_ref(),
                cancellation.clone(),
                {
                    let range = request.range().clone();
                    let limits = self.limits.adaptation();
                    move || validate_and_plan(&range, frames, &[], limits, &cancellation)
                },
            )
            .await?;

        let mut clips = Vec::with_capacity(plans.len());
        for epoch in plans {
            self.check_boundary(deadline, &context)?;
            let prepared = self
                .prepare_epoch(&request, epoch, deadline, &context)
                .await?;
            clips.push(
                self.generate_epoch(&request, prepared, deadline, &context)
                    .await?,
            );
        }
        self.check_boundary(deadline, &context)?;
        Ok(TemporalVideoGenerationResult {
            range: request.range().clone(),
            clips,
        })
    }

    async fn prepare_epoch(
        &self,
        request: &TemporalVideoGenerationRequest,
        epoch: EpochPlan,
        deadline: Instant,
        context: &ArtifactGenerationContext,
    ) -> Result<PreparedVideoEpoch> {
        let geometry = output_geometry(epoch.descriptor.image, request.output())?;
        let profile = VideoEncodingProfile::new(geometry, request.output().max_encoded_bytes())?;
        let cancellation = WorkCancellation::default();
        let (meaningful, selection) = if request.policy() == VideoPresentationPolicy::ModelOptimized
        {
            let analysis_bytes = analysis_reservation(&epoch)?;
            let _analysis = controlled(
                self.analysis.clone().acquire_many_owned(analysis_bytes),
                deadline,
                context.cancellation.as_ref(),
            )
            .await?
            .map_err(|_| generation_error("video analysis scheduler is closed"))?;
            let anchor = request.range().resolved_anchor.effective_time;
            let selection = self
                .run_blocking(
                    deadline,
                    context.cancellation.as_ref(),
                    cancellation.clone(),
                    {
                        let epoch = epoch.clone();
                        move || meaningful_selection(&epoch, anchor, &cancellation)
                    },
                )
                .await?;
            (selection.0, Some(selection.1))
        } else {
            (Vec::new(), None)
        };

        let provisional = build_plan(
            request,
            &epoch,
            geometry,
            VideoPresentationPolicy::RealTime,
            Vec::new(),
        )?;
        let visible: HashSet<_> = provisional
            .segments()
            .iter()
            .filter_map(|segment| match segment.source() {
                VideoSegmentSource::SourceFrame { frame_id, .. } => Some(*frame_id),
                VideoSegmentSource::GapSlate { .. } => None,
            })
            .collect();
        let meaningful = meaningful
            .into_iter()
            .filter(|frame_id| visible.contains(frame_id))
            .collect();
        let plan = if request.policy() == VideoPresentationPolicy::RealTime {
            provisional
        } else {
            build_plan(request, &epoch, geometry, request.policy(), meaningful)?
        };
        Ok(PreparedVideoEpoch {
            sources: epoch.source_fingerprints.clone(),
            cache_sources: epoch.cache_sources.clone(),
            epoch,
            plan,
            selection,
            profile,
        })
    }

    async fn generate_epoch(
        &self,
        request: &TemporalVideoGenerationRequest,
        prepared: PreparedVideoEpoch,
        deadline: Instant,
        context: &ArtifactGenerationContext,
    ) -> Result<TemporalVideoGenerationClip> {
        let identity = self.encoder.identity().clone();
        let canonical = canonical_video_cache_parameters(
            &prepared.plan,
            &identity,
            &prepared.profile,
            prepared.selection.as_ref(),
        )?;
        let cache = video_cache_metadata(VideoCacheIdentityInput {
            session_id: request.range().session_id,
            target_id: request.range().target_id,
            sources: &prepared.cache_sources,
            canonical_parameters: &canonical,
            selector: prepared.selection.as_ref(),
        });
        let mut invalidated = false;
        match self
            .lookup_expected_video(&cache, &prepared, &identity, deadline, context)
            .await?
        {
            VideoArtifactLookup::Hit(stored) => {
                return clip_from_stored(*stored, ArtifactCacheDisposition::Hit);
            }
            VideoArtifactLookup::Invalidated => invalidated = true,
            VideoArtifactLookup::Miss => {}
        }

        let lock = self.cache_locks.for_key(cache.cache_key);
        let _cache_guard = controlled(lock.lock(), deadline, context.cancellation.as_ref()).await?;
        match self
            .lookup_expected_video(&cache, &prepared, &identity, deadline, context)
            .await?
        {
            VideoArtifactLookup::Hit(stored) => {
                return clip_from_stored(*stored, ArtifactCacheDisposition::Hit);
            }
            VideoArtifactLookup::Invalidated => invalidated = true,
            VideoArtifactLookup::Miss => {}
        }

        self.check_boundary(deadline, context)?;
        let work = WorkCancellation::default();
        let inputs = encode_inputs(&prepared, &work)?;
        let encode_request =
            VideoEncodeRequest::new(prepared.plan.clone(), inputs, prepared.profile)?;
        let encoded = self
            .encode_controlled(encode_request, deadline, context, work.clone())
            .await?;
        if encoded.identity() != &identity || encoded.profile() != prepared.profile {
            return Err(generation_error(
                "video encoder returned identity or profile that contradicts the request",
            ));
        }
        self.check_boundary(deadline, context)?;
        let artifact_id = ArtifactId::from_uuid(*self.ids.next().as_uuid());
        let manifest = TemporalVideoManifest::new(
            artifact_id,
            request.range(),
            prepared.plan.clone(),
            prepared.selection.clone(),
            &encoded,
        )?;
        let publication_work = WorkCancellation::default();
        let publication = VideoArtifactPublication::new(
            request.range().session_id,
            request.range().target_id,
            prepared.sources.clone(),
            cache.clone(),
            manifest,
            encoded.owned_encoded_bytes(),
        )?
        .with_cancellation(Arc::new(publication_work.clone()));
        self.check_boundary(deadline, context)?;
        let published = self
            .publish_controlled(publication, deadline, context, publication_work)
            .await?;
        match published {
            VideoArtifactPublish::Published(stored) => {
                self.validate_published_video(&stored, &cache, &identity)?;
                clip_from_stored(
                    stored,
                    if invalidated {
                        ArtifactCacheDisposition::RegeneratedAfterInvalidation
                    } else {
                        ArtifactCacheDisposition::Generated
                    },
                )
            }
            VideoArtifactPublish::Existing(stored) => {
                if !stored_matches_expected(&stored, &cache, &prepared, &identity) {
                    self.artifacts
                        .invalidate_video_artifact(stored.manifest.artifact_id())
                        .await?;
                    return Err(generation_error(
                        "concurrent video publication contradicted the prepared cache identity",
                    ));
                }
                clip_from_stored(stored, ArtifactCacheDisposition::Hit)
            }
        }
    }

    async fn lookup_expected_video(
        &self,
        cache: &krometrail_core::ArtifactCacheMetadata,
        prepared: &PreparedVideoEpoch,
        identity: &krometrail_core::VideoEncoderIdentity,
        deadline: Instant,
        context: &ArtifactGenerationContext,
    ) -> Result<VideoArtifactLookup> {
        let lookup = controlled(
            self.artifacts
                .lookup_video_artifact(cache.cache_key, prepared.sources.clone()),
            deadline,
            context.cancellation.as_ref(),
        )
        .await??;
        match lookup {
            VideoArtifactLookup::Hit(stored)
                if stored_matches_expected(&stored, cache, prepared, identity) =>
            {
                Ok(VideoArtifactLookup::Hit(stored))
            }
            VideoArtifactLookup::Hit(stored) => {
                self.artifacts
                    .invalidate_video_artifact(stored.manifest.artifact_id())
                    .await?;
                Ok(VideoArtifactLookup::Invalidated)
            }
            other => Ok(other),
        }
    }

    async fn publish_controlled(
        &self,
        publication: VideoArtifactPublication,
        deadline: Instant,
        context: &ArtifactGenerationContext,
        cancellation: WorkCancellation,
    ) -> Result<VideoArtifactPublish> {
        let mut publish = self.artifacts.publish_video_artifact(publication);
        tokio::select! {
            value = &mut publish => value,
            () = external_cancelled(context.cancellation.as_ref()) => {
                cancellation.cancel();
                self.finish_cancelled_publication(&mut publish).await?;
                Err(cancelled_error())
            }
            () = sleep_until(deadline) => {
                cancellation.cancel();
                self.finish_cancelled_publication(&mut publish).await?;
                Err(deadline_error())
            }
        }
    }

    async fn finish_cancelled_publication(
        &self,
        publish: &mut PortFuture<'_, Result<VideoArtifactPublish>>,
    ) -> Result<()> {
        if let Ok(VideoArtifactPublish::Published(stored)) = publish.await {
            self.artifacts
                .invalidate_video_artifact(stored.manifest.artifact_id())
                .await?;
        }
        Ok(())
    }

    fn validate_published_video(
        &self,
        stored: &StoredVideoArtifact,
        cache: &krometrail_core::ArtifactCacheMetadata,
        identity: &krometrail_core::VideoEncoderIdentity,
    ) -> Result<()> {
        if &stored.cache != cache || stored.manifest.encoder() != identity {
            return Err(generation_error(
                "published video contradicted its cache or encoder identity",
            ));
        }
        Ok(())
    }

    async fn run_blocking<T, F>(
        &self,
        deadline: Instant,
        external: Option<&Arc<dyn CancellationSignal>>,
        cancellation: WorkCancellation,
        operation: F,
    ) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce() -> Result<T> + Send + 'static,
    {
        let permit = controlled(self.blocking.clone().acquire_owned(), deadline, external)
            .await?
            .map_err(|_| generation_error("video blocking scheduler is closed"))?;
        let mut join = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            operation()
        });
        tokio::select! {
            value = &mut join => value.map_err(|_| generation_error("video blocking worker stopped"))?,
            () = external_cancelled(external) => {
                cancellation.cancel();
                let _ = join.await;
                Err(cancelled_error())
            }
            () = sleep_until(deadline) => {
                cancellation.cancel();
                let _ = join.await;
                Err(deadline_error())
            }
        }
    }

    async fn encode_controlled(
        &self,
        request: VideoEncodeRequest,
        deadline: Instant,
        context: &ArtifactGenerationContext,
        cancellation: WorkCancellation,
    ) -> Result<VideoEncodedClip> {
        let signal: Arc<dyn CancellationSignal> = Arc::new(cancellation.clone());
        let mut encode = self.encoder.encode(
            request,
            VideoEncodingContext {
                deadline,
                cancellation: signal,
            },
        );
        tokio::select! {
            value = &mut encode => value,
            () = external_cancelled(context.cancellation.as_ref()) => {
                cancellation.cancel();
                let _ = tokio::time::timeout(ENCODER_CLEANUP_GRACE, &mut encode).await;
                Err(cancelled_error())
            }
            () = sleep_until(deadline) => {
                cancellation.cancel();
                let _ = tokio::time::timeout(ENCODER_CLEANUP_GRACE, &mut encode).await;
                Err(deadline_error())
            }
        }
    }

    fn check_boundary(&self, deadline: Instant, context: &ArtifactGenerationContext) -> Result<()> {
        if context.is_cancelled() {
            Err(cancelled_error())
        } else if Instant::now() >= deadline {
            Err(deadline_error())
        } else {
            Ok(())
        }
    }
}

impl TemporalVideoGeneration for TemporalVideoGenerationService {
    fn generate_video(
        &self,
        request: TemporalVideoGenerationRequest,
        context: ArtifactGenerationContext,
    ) -> PortFuture<'_, Result<TemporalVideoGenerationResult>> {
        Box::pin(async move { self.generate_inner(request, context).await })
    }
}

#[derive(Default)]
struct VideoCacheLocks(Mutex<HashMap<ArtifactCacheKey, Weak<tokio::sync::Mutex<()>>>>);

impl VideoCacheLocks {
    fn for_key(&self, key: ArtifactCacheKey) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.0.lock().expect("video cache lock registry poisoned");
        locks.retain(|_, lock| lock.strong_count() > 0);
        locks.entry(key).or_default().upgrade().unwrap_or_else(|| {
            let lock = Arc::new(tokio::sync::Mutex::new(()));
            locks.insert(key, Arc::downgrade(&lock));
            lock
        })
    }
}

fn build_plan(
    request: &TemporalVideoGenerationRequest,
    epoch: &EpochPlan,
    geometry: krometrail_core::VideoOutputGeometry,
    policy: VideoPresentationPolicy,
    meaningful: Vec<krometrail_core::FrameId>,
) -> Result<krometrail_core::VideoPresentationPlan> {
    build_presentation_plan(VideoPlanInput::new(
        request.range().clone(),
        epoch.descriptor.clone(),
        epoch
            .frames
            .iter()
            .map(|frame| frame.metadata().clone())
            .collect(),
        meaningful,
        geometry,
        policy,
    )?)
}

fn analysis_reservation(epoch: &EpochPlan) -> Result<u32> {
    let max_pixels = usize::try_from(256_u64 * 256)
        .expect("fixed analysis dimensions fit every supported platform");
    let bytes = max_pixels
        .checked_mul(10)
        .and_then(|value| value.checked_mul(epoch.frames.len()))
        .ok_or_else(|| limit_error("video analysis reservation overflowed"))?;
    u32::try_from(bytes).map_err(|_| limit_error("video analysis reservation is too large"))
}

fn clip_from_stored(
    stored: StoredVideoArtifact,
    cache: ArtifactCacheDisposition,
) -> Result<TemporalVideoGenerationClip> {
    let scope = EvidenceScope::new(stored.manifest.session_id(), stored.manifest.target_id())?;
    let artifact = VideoArtifactEvidenceHandle::new(
        stored.manifest.artifact_id(),
        scope,
        NonEmptyText::new("video/mp4").expect("static video media type is non-empty"),
        Sha256Digest::from_bytes(*stored.manifest.output_hash().as_bytes()),
        stored.manifest.encoded_byte_len(),
        stored.manifest,
    )?;
    Ok(TemporalVideoGenerationClip {
        epoch_index: artifact.provenance.plan().epoch().index,
        cache,
        artifact,
    })
}

fn stored_matches_expected(
    stored: &StoredVideoArtifact,
    cache: &krometrail_core::ArtifactCacheMetadata,
    prepared: &PreparedVideoEpoch,
    identity: &krometrail_core::VideoEncoderIdentity,
) -> bool {
    &stored.cache == cache
        && stored.manifest.plan() == &prepared.plan
        && stored.manifest.selection() == prepared.selection.as_ref()
        && stored.manifest.encoder() == identity
        && stored.manifest.profile() == prepared.profile
}

async fn external_cancelled(cancellation: Option<&Arc<dyn CancellationSignal>>) {
    match cancellation {
        Some(signal) => signal.cancelled().await,
        None => pending().await,
    }
}

async fn sleep_until(deadline: Instant) {
    tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await;
}

fn generation_error(message: impl Into<String>) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::ArtifactGenerationFailed,
        NonEmptyText::new(message.into()).expect("video generation messages are non-empty"),
    )
}
