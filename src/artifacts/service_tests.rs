use std::{
    alloc::{GlobalAlloc, Layout, System},
    collections::HashMap,
    io::Cursor,
    num::{NonZeroU32, NonZeroUsize},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::Instant,
};

use image::{ColorType, ImageEncoder, codecs::png::PngEncoder};
use krometrail_core::{
    AnalysisScale, ArtifactCacheKey, ArtifactFailurePolicy, ArtifactGeneration,
    ArtifactGenerationContext, ArtifactGenerationRequest, ArtifactGeneratorRequest,
    ArtifactLabelsRequest, ArtifactLookup, ArtifactOutcome, ArtifactPublication, ArtifactPublish,
    ArtifactSourceFingerprint, ArtifactStore, CaptureOrdinal, CapturedFrame, DeviceScaleFactor,
    DifferenceMapRequest, EncodedFrame, FrameAvailability, FrameId, FrameSelector, FrameSource,
    IdSource, IdValue, ImageFormat, MotionHistoryRequest, NonEmptyText, NormalizationRequest,
    ObservedTime, OutputLimitsRequest, PixelDimensions, PortFuture, RangeResolutionOptions,
    RecordingSink, RegionFilmstripRequest, ResolvedRange, SessionId, SessionRange, SessionTime,
    StoredArtifact, StoryboardRequest, TargetId, TemporalRangeAnchorKind,
};
use krometrail_store::{
    IndexStoreConfig, RecordingStore, RotationConfig, SegmentStoreConfig, SegmentWriter,
    SqliteIndex,
};
use temporal_vision::{FrequencyMode, RegionDefinition, Rgb8, SignedPixelRect};
use uuid::Uuid;

use super::{
    TemporalVisionArtifactService, epoch::WorkCancellation, scheduler::ArtifactWorkLimits,
};

const PNG: &[u8] = include_bytes!("../../tests/fixtures/artifacts/chrome-rgba.png");

struct CountingAllocator;

static ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);

#[global_allocator]
static GLOBAL_ALLOCATOR: CountingAllocator = CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarding the caller's valid layout to the system allocator.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            ALLOCATED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarding the caller's valid layout to the system allocator.
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            ALLOCATED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: the pointer and layout were returned by this allocator.
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the pointer, old layout, and new size follow GlobalAlloc's realloc contract.
        let replacement = unsafe { System.realloc(pointer, layout, new_size) };
        if !replacement.is_null() {
            ALLOCATED_BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
        }
        replacement
    }
}

struct ProductionSequenceIds(AtomicU64);

impl IdSource for ProductionSequenceIds {
    fn next(&self) -> IdValue {
        IdValue::from_uuid(Uuid::from_u128(
            self.0.fetch_add(1, Ordering::Relaxed) as u128 + 0x9000,
        ))
    }
}

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
                        test_error(krometrail_core::ErrorCode::NotFound, "source missing")
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
    by_key: Mutex<HashMap<ArtifactCacheKey, (Vec<ArtifactSourceFingerprint>, StoredArtifact)>>,
    by_id: Mutex<HashMap<krometrail_core::ArtifactId, StoredArtifact>>,
    publications: AtomicUsize,
}

impl ArtifactStore for FakeArtifacts {
    fn lookup_artifact(
        &self,
        key: ArtifactCacheKey,
        expected_sources: Vec<ArtifactSourceFingerprint>,
    ) -> PortFuture<'_, krometrail_core::Result<ArtifactLookup>> {
        let result = self.by_key.lock().unwrap().get(&key).cloned().map_or(
            ArtifactLookup::Miss,
            |(sources, artifact)| {
                if sources == expected_sources {
                    ArtifactLookup::Hit(Box::new(artifact))
                } else {
                    ArtifactLookup::Invalidated
                }
            },
        );
        Box::pin(std::future::ready(Ok(result)))
    }

    fn publish_artifact(
        &self,
        publication: ArtifactPublication,
    ) -> PortFuture<'_, krometrail_core::Result<ArtifactPublish>> {
        let cancelled = publication
            .cancellation()
            .is_some_and(|signal| signal.is_cancelled());
        let result = if cancelled {
            Err(test_error(
                krometrail_core::ErrorCode::Cancelled,
                "publication cancelled",
            ))
        } else {
            let mut by_key = self.by_key.lock().unwrap();
            if let Some((_, existing)) = by_key.get(&publication.cache.cache_key) {
                Ok(ArtifactPublish::Existing(existing.clone()))
            } else {
                self.publications.fetch_add(1, Ordering::SeqCst);
                let artifact = StoredArtifact {
                    cache: publication.cache.clone(),
                    manifest: publication.manifest.clone(),
                    media_type: publication.media_type.clone(),
                    encoded_bytes: Arc::clone(&publication.encoded_bytes),
                };
                by_key.insert(
                    publication.cache.cache_key,
                    (publication.sources, artifact.clone()),
                );
                self.by_id
                    .lock()
                    .unwrap()
                    .insert(*artifact.manifest.artifact_id(), artifact.clone());
                Ok(ArtifactPublish::Published(artifact))
            }
        };
        Box::pin(std::future::ready(result))
    }

    fn artifact(
        &self,
        artifact_id: krometrail_core::ArtifactId,
    ) -> PortFuture<'_, krometrail_core::Result<Option<StoredArtifact>>> {
        Box::pin(std::future::ready(Ok(self
            .by_id
            .lock()
            .unwrap()
            .get(&artifact_id)
            .cloned())))
    }
}

struct FakeIds {
    next: AtomicU64,
}
impl IdSource for FakeIds {
    fn next(&self) -> IdValue {
        IdValue::from_uuid(Uuid::from_u128(u128::from(
            self.next.fetch_add(1, Ordering::SeqCst) + 1000,
        )))
    }
}

struct TestRig {
    service: TemporalVisionArtifactService,
    frames: Arc<FakeFrames>,
    artifacts: Arc<FakeArtifacts>,
    ids: Arc<FakeIds>,
    request: ArtifactGenerationRequest,
}

fn rig(two_epochs: bool, limits: ArtifactWorkLimits) -> TestRig {
    let session = SessionId::from_uuid(Uuid::from_u128(1));
    let target = TargetId::from_uuid(Uuid::from_u128(2));
    let frames: Vec<_> = (0..3)
        .map(|position| {
            let ordinal = position + 1;
            EncodedFrame::new(
                CapturedFrame::new(
                    FrameId::from_uuid(Uuid::from_u128(10 + position as u128)),
                    session,
                    target,
                    CaptureOrdinal::new(ordinal).unwrap(),
                    None,
                    ObservedTime::from_nanos(ordinal + 10),
                    SessionTime::from_nanos(ordinal),
                    ImageFormat::Png,
                    PixelDimensions::new(2, 2).unwrap(),
                    PixelDimensions::new(if two_epochs && position == 2 { 3 } else { 2 }, 2)
                        .unwrap(),
                    DeviceScaleFactor::new(1.0).unwrap(),
                    vec![],
                )
                .unwrap(),
                PNG.to_vec(),
            )
            .unwrap()
        })
        .collect();
    let range = SessionRange::new(SessionTime::from_nanos(1), SessionTime::from_nanos(3)).unwrap();
    let resolved = ResolvedRange::new(
        session,
        target,
        TemporalRangeAnchorKind::SessionTime,
        range,
        range,
        frames.iter().map(|frame| frame.metadata().id()).collect(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        RangeResolutionOptions::DEFAULT,
    )
    .unwrap();
    let output = OutputLimitsRequest::new(4096, 4096, 16 * 1024 * 1024).unwrap();
    let labels = ArtifactLabelsRequest::new(
        NonEmptyText::new("artifact").unwrap(),
        NonEmptyText::new("fixture").unwrap(),
    );
    let normalization =
        NormalizationRequest::new(None, Rgb8::new(0, 0, 0), AnalysisScale::Identity).unwrap();
    let generators = vec![
        ArtifactGeneratorRequest::Storyboard(StoryboardRequest {
            anchor: SessionTime::from_nanos(2),
            tile_limit: 3,
            noise_floor: 0,
            normalization,
            labels: labels.clone(),
            include_orientation: true,
            output,
        }),
        ArtifactGeneratorRequest::DifferenceMap(DifferenceMapRequest {
            reference: FrameSelector::First,
            frequency_mode: FrequencyMode::Count,
            repeated_change_separation_nanos: None,
            noise_floor: 0,
            normalization,
            canvas_background: Rgb8::new(0, 0, 0),
            output,
        }),
        ArtifactGeneratorRequest::RegionFilmstrip(RegionFilmstripRequest {
            region: RegionDefinition::FixedSourceImage {
                rect: SignedPixelRect::new(
                    0,
                    0,
                    NonZeroU32::new(1).unwrap(),
                    NonZeroU32::new(1).unwrap(),
                )
                .unwrap(),
            },
            mask: Some(
                temporal_vision::BinaryMask::new(
                    temporal_vision::PixelDimensions::new(2, 2).unwrap(),
                    [0x80],
                )
                .unwrap(),
            ),
            anchor: SessionTime::from_nanos(2),
            tile_limit: 3,
            locator: None,
            background: Rgb8::new(0, 0, 0),
            padding: Rgb8::new(255, 0, 255),
            display_scale: AnalysisScale::Identity,
            labels: labels.clone(),
            output,
        }),
        ArtifactGeneratorRequest::MotionHistory(MotionHistoryRequest {
            reference: FrameSelector::Last,
            noise_floor: 0,
            normalization,
            decay_peak: u16::MAX,
            decay_half_life_ranks: 1,
            reference_strength: 64,
            accent: Rgb8::new(255, 176, 0),
            outline: Rgb8::new(255, 255, 255),
            labels,
            output,
        }),
    ];
    let request = ArtifactGenerationRequest::new(
        resolved,
        vec![],
        generators,
        ArtifactFailurePolicy::RequireAll,
    )
    .unwrap();
    let frames = Arc::new(FakeFrames {
        frames,
        loads: AtomicUsize::new(0),
    });
    let artifacts = Arc::new(FakeArtifacts::default());
    let ids = Arc::new(FakeIds {
        next: AtomicU64::new(0),
    });
    let service = TemporalVisionArtifactService::new(
        Arc::clone(&frames) as Arc<dyn FrameSource>,
        Arc::clone(&artifacts) as Arc<dyn ArtifactStore>,
        Arc::clone(&ids) as Arc<dyn IdSource>,
        limits,
    )
    .unwrap();
    TestRig {
        service,
        frames,
        artifacts,
        ids,
        request,
    }
}

#[tokio::test]
async fn all_generator_families_are_ordered_deterministic_and_cached() {
    let rig = rig(false, ArtifactWorkLimits::default());
    let first = rig
        .service
        .generate(rig.request.clone(), ArtifactGenerationContext::default())
        .await
        .unwrap();
    let kinds: Vec<_> = first
        .outcomes
        .iter()
        .map(|outcome| match outcome {
            ArtifactOutcome::Available { artifact, .. } => artifact.manifest.artifact_kind(),
            ArtifactOutcome::Unavailable { .. } => panic!("all generators should succeed"),
        })
        .collect();
    assert_eq!(
        kinds,
        [
            temporal_vision::ArtifactKind::Storyboard,
            temporal_vision::ArtifactKind::BeforeDuringAfter,
            temporal_vision::ArtifactKind::DifferenceMap,
            temporal_vision::ArtifactKind::RegionFilmstrip,
            temporal_vision::ArtifactKind::MotionHistory,
        ]
    );
    assert!(first.outcomes.iter().any(|outcome| matches!(
        outcome,
        ArtifactOutcome::Available { artifact, .. }
            if artifact.manifest.artifact_kind()
                == temporal_vision::ArtifactKind::RegionFilmstrip
                && artifact.manifest.mask().is_some()
    )));
    let ids: Vec<_> = first
        .outcomes
        .iter()
        .map(|outcome| match outcome {
            ArtifactOutcome::Available { artifact, .. } => artifact.artifact_id,
            _ => unreachable!(),
        })
        .collect();
    let second = rig
        .service
        .generate(rig.request.clone(), ArtifactGenerationContext::default())
        .await
        .unwrap();
    let repeated_ids: Vec<_> = second
        .outcomes
        .iter()
        .map(|outcome| match outcome {
            ArtifactOutcome::Available { artifact, .. } => {
                assert_eq!(
                    artifact.cache,
                    krometrail_core::ArtifactCacheDisposition::Hit
                );
                artifact.artifact_id
            }
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(repeated_ids, ids);
    assert_eq!(rig.artifacts.publications.load(Ordering::SeqCst), 5);
    assert_eq!(rig.ids.next.load(Ordering::SeqCst), 5);
}

#[tokio::test]
async fn compatible_context_matches_service_baseline_bytes_manifests_and_ids() {
    let context_rig = rig(false, ArtifactWorkLimits::default());
    let mut baseline_rig = rig(false, ArtifactWorkLimits::default());
    baseline_rig.service = baseline_rig.service.with_pair_context_enabled(false);
    let context = context_rig
        .service
        .generate(
            context_rig.request.clone(),
            ArtifactGenerationContext::default(),
        )
        .await
        .unwrap();
    let baseline = baseline_rig
        .service
        .generate(
            baseline_rig.request.clone(),
            ArtifactGenerationContext::default(),
        )
        .await
        .unwrap();
    assert_eq!(context.outcomes.len(), baseline.outcomes.len());
    for (context_outcome, baseline_outcome) in context.outcomes.iter().zip(&baseline.outcomes) {
        let (
            ArtifactOutcome::Available {
                artifact: context_artifact,
                ..
            },
            ArtifactOutcome::Available {
                artifact: baseline_artifact,
                ..
            },
        ) = (context_outcome, baseline_outcome)
        else {
            panic!("baseline and context must both publish every fixture output");
        };
        assert_eq!(context_artifact.artifact_id, baseline_artifact.artifact_id);
        assert_eq!(context_artifact.cache, baseline_artifact.cache);
        assert_eq!(context_artifact.manifest, baseline_artifact.manifest);
        let context_stored = context_rig
            .artifacts
            .by_id
            .lock()
            .unwrap()
            .get(&context_artifact.artifact_id)
            .cloned()
            .unwrap();
        let baseline_stored = baseline_rig
            .artifacts
            .by_id
            .lock()
            .unwrap()
            .get(&baseline_artifact.artifact_id)
            .cloned()
            .unwrap();
        assert_eq!(context_stored.encoded_bytes, baseline_stored.encoded_bytes);
        assert_eq!(context_stored.manifest, baseline_stored.manifest);
    }
    assert_eq!(context_rig.ids.next.load(Ordering::SeqCst), 5);
    assert_eq!(baseline_rig.ids.next.load(Ordering::SeqCst), 5);
}

#[tokio::test]
async fn incompatible_measurement_identity_matches_baseline_without_cross_reuse() {
    let context_rig = rig(false, ArtifactWorkLimits::default());
    let mut baseline_rig = rig(false, ArtifactWorkLimits::default());
    baseline_rig.service = baseline_rig.service.with_pair_context_enabled(false);
    let mut generators = context_rig.request.generators().to_vec();
    let ArtifactGeneratorRequest::DifferenceMap(request) = &mut generators[1] else {
        unreachable!();
    };
    request.noise_floor = 1;
    let request = ArtifactGenerationRequest::new(
        context_rig.request.range().clone(),
        context_rig.request.markers().to_vec(),
        generators,
        ArtifactFailurePolicy::RequireAll,
    )
    .unwrap();
    let context = context_rig
        .service
        .generate(request.clone(), ArtifactGenerationContext::default())
        .await
        .unwrap();
    let baseline = baseline_rig
        .service
        .generate(request, ArtifactGenerationContext::default())
        .await
        .unwrap();
    for (context_outcome, baseline_outcome) in context.outcomes.iter().zip(&baseline.outcomes) {
        let (
            ArtifactOutcome::Available {
                artifact: context_artifact,
                ..
            },
            ArtifactOutcome::Available {
                artifact: baseline_artifact,
                ..
            },
        ) = (context_outcome, baseline_outcome)
        else {
            panic!("incompatible identity must not change partial-output policy");
        };
        assert_eq!(context_artifact.manifest, baseline_artifact.manifest);
        assert_eq!(
            context_rig
                .artifacts
                .by_id
                .lock()
                .unwrap()
                .get(&context_artifact.artifact_id)
                .unwrap()
                .encoded_bytes,
            baseline_rig
                .artifacts
                .by_id
                .lock()
                .unwrap()
                .get(&baseline_artifact.artifact_id)
                .unwrap()
                .encoded_bytes,
        );
    }
}

#[tokio::test]
async fn concurrent_identical_misses_share_generation_and_waiters() {
    let rig = rig(false, ArtifactWorkLimits::default());
    let (first, second) = tokio::join!(
        rig.service
            .generate(rig.request.clone(), ArtifactGenerationContext::default()),
        rig.service
            .generate(rig.request.clone(), ArtifactGenerationContext::default()),
    );
    assert_eq!(first.unwrap().outcomes.len(), 5);
    assert_eq!(second.unwrap().outcomes.len(), 5);
    assert_eq!(rig.artifacts.publications.load(Ordering::SeqCst), 5);
    assert_eq!(rig.ids.next.load(Ordering::SeqCst), 5);
    assert_eq!(rig.frames.loads.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn deadlines_cancellation_and_partial_epoch_reference_fail_explicitly() {
    let cancellation_rig = rig(false, ArtifactWorkLimits::default());
    let error = cancellation_rig
        .service
        .generate(
            cancellation_rig.request.clone(),
            ArtifactGenerationContext {
                deadline: Some(Instant::now()),
                cancellation: None,
            },
        )
        .await
        .unwrap_err();
    assert_eq!(
        error.code,
        krometrail_core::ErrorCode::ArtifactGenerationFailed
    );

    let cancellation = WorkCancellation::default();
    cancellation.cancel();
    let error = cancellation_rig
        .service
        .generate(
            cancellation_rig.request,
            ArtifactGenerationContext {
                deadline: None,
                cancellation: Some(Arc::new(cancellation)),
            },
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, krometrail_core::ErrorCode::Cancelled);

    let mut rig = rig(true, ArtifactWorkLimits::default());
    let first_frame = rig.request.range().frame_ids[0];
    let generator = ArtifactGeneratorRequest::DifferenceMap(DifferenceMapRequest {
        reference: FrameSelector::Frame(first_frame),
        frequency_mode: FrequencyMode::Count,
        repeated_change_separation_nanos: None,
        noise_floor: 0,
        normalization: NormalizationRequest::new(None, Rgb8::new(0, 0, 0), AnalysisScale::Identity)
            .unwrap(),
        canvas_background: Rgb8::new(0, 0, 0),
        output: OutputLimitsRequest::new(4096, 4096, 16 * 1024 * 1024).unwrap(),
    });
    rig.request = ArtifactGenerationRequest::new(
        rig.request.range().clone(),
        vec![],
        vec![generator],
        ArtifactFailurePolicy::AllowPartial,
    )
    .unwrap();
    let result = rig
        .service
        .generate(rig.request, ArtifactGenerationContext::default())
        .await
        .unwrap();
    assert!(matches!(
        result.outcomes[0],
        ArtifactOutcome::Available { .. }
    ));
    assert!(matches!(
        result.outcomes[1],
        ArtifactOutcome::Unavailable { .. }
    ));
}

#[tokio::test]
async fn fit_limits_materializes_smallest_exact_divisor_in_manifest() {
    let limits = ArtifactWorkLimits {
        max_normalized_bytes: NonZeroUsize::new(18).unwrap(),
        ..ArtifactWorkLimits::default()
    };
    let mut rig = rig(false, limits);
    let generator = ArtifactGeneratorRequest::Storyboard(StoryboardRequest {
        anchor: SessionTime::from_nanos(2),
        tile_limit: 3,
        noise_floor: 0,
        normalization: NormalizationRequest::new(
            None,
            Rgb8::new(0, 0, 0),
            AnalysisScale::FitLimits,
        )
        .unwrap(),
        labels: ArtifactLabelsRequest::new(
            NonEmptyText::new("story").unwrap(),
            NonEmptyText::new("fixture").unwrap(),
        ),
        include_orientation: false,
        output: OutputLimitsRequest::new(4096, 4096, 16 * 1024 * 1024).unwrap(),
    });
    rig.request = ArtifactGenerationRequest::new(
        rig.request.range().clone(),
        vec![],
        vec![generator],
        ArtifactFailurePolicy::RequireAll,
    )
    .unwrap();
    let result = rig
        .service
        .generate(rig.request, ArtifactGenerationContext::default())
        .await
        .unwrap();
    let ArtifactOutcome::Available { artifact, .. } = &result.outcomes[0] else {
        unreachable!()
    };
    let scale = artifact
        .manifest
        .normalization()
        .iter()
        .find(|step| step.kind() == temporal_vision::NormalizationKind::IntegerScaling)
        .unwrap();
    assert_eq!(
        scale.parameters().get("factor"),
        Some(&temporal_vision::ParameterValue::Unsigned(2))
    );
}

fn one_generator_request(
    rig: &TestRig,
    generator: ArtifactGeneratorRequest,
    markers: Vec<krometrail_core::ArtifactMarker>,
) -> ArtifactGenerationRequest {
    ArtifactGenerationRequest::new(
        rig.request.range().clone(),
        markers,
        vec![generator],
        ArtifactFailurePolicy::RequireAll,
    )
    .unwrap()
}

#[tokio::test]
async fn resource_limits_accept_exact_boundaries_and_reject_the_next_unit() {
    let exact_source_bytes = PNG.len() * 3;
    let exact = ArtifactWorkLimits {
        max_source_frames: NonZeroUsize::new(3).unwrap(),
        max_encoded_source_bytes: NonZeroUsize::new(exact_source_bytes).unwrap(),
        max_dimension: NonZeroU32::new(4096).unwrap(),
        max_pixels_per_frame: NonZeroUsize::new(4).unwrap(),
        max_decoded_bytes: NonZeroUsize::new(48).unwrap(),
        max_normalized_bytes: NonZeroUsize::new(192).unwrap(),
        ..ArtifactWorkLimits::default()
    };
    let exact_rig = rig(false, exact);
    let generator = exact_rig.request.generators()[1].clone();
    exact_rig
        .service
        .generate(
            one_generator_request(&exact_rig, generator, vec![]),
            ArtifactGenerationContext::default(),
        )
        .await
        .unwrap();

    for limited in [
        ArtifactWorkLimits {
            max_source_frames: NonZeroUsize::new(2).unwrap(),
            ..ArtifactWorkLimits::default()
        },
        ArtifactWorkLimits {
            max_encoded_source_bytes: NonZeroUsize::new(exact_source_bytes - 1).unwrap(),
            ..ArtifactWorkLimits::default()
        },
        ArtifactWorkLimits {
            max_dimension: NonZeroU32::new(1).unwrap(),
            ..ArtifactWorkLimits::default()
        },
        ArtifactWorkLimits {
            max_pixels_per_frame: NonZeroUsize::new(3).unwrap(),
            ..ArtifactWorkLimits::default()
        },
        ArtifactWorkLimits {
            max_decoded_bytes: NonZeroUsize::new(47).unwrap(),
            ..ArtifactWorkLimits::default()
        },
        ArtifactWorkLimits {
            max_normalized_bytes: NonZeroUsize::new(191).unwrap(),
            ..ArtifactWorkLimits::default()
        },
    ] {
        let limited_rig = rig(false, limited);
        let generator = limited_rig.request.generators()[1].clone();
        let error = limited_rig
            .service
            .generate(
                one_generator_request(&limited_rig, generator, vec![]),
                ArtifactGenerationContext::default(),
            )
            .await
            .unwrap_err();
        assert_eq!(
            error.code,
            krometrail_core::ErrorCode::ResourceLimitExceeded
        );
    }

    let markers = vec![
        krometrail_core::ArtifactMarker::new(
            krometrail_core::ArtifactMarkerId::Caller(NonEmptyText::new("one").unwrap()),
            SessionTime::from_nanos(2),
            NonEmptyText::new("test").unwrap(),
            NonEmptyText::new("one").unwrap(),
        ),
        krometrail_core::ArtifactMarker::new(
            krometrail_core::ArtifactMarkerId::Caller(NonEmptyText::new("two").unwrap()),
            SessionTime::from_nanos(2),
            NonEmptyText::new("test").unwrap(),
            NonEmptyText::new("two").unwrap(),
        ),
    ];
    let marker_limits = ArtifactWorkLimits {
        max_markers: NonZeroUsize::new(1).unwrap(),
        ..ArtifactWorkLimits::default()
    };
    let marker_rig = rig(false, marker_limits);
    let generator = marker_rig.request.generators()[1].clone();
    marker_rig
        .service
        .generate(
            one_generator_request(&marker_rig, generator.clone(), vec![markers[0].clone()]),
            ArtifactGenerationContext::default(),
        )
        .await
        .unwrap();
    assert_eq!(
        marker_rig
            .service
            .generate(
                one_generator_request(&marker_rig, generator, markers),
                ArtifactGenerationContext::default(),
            )
            .await
            .unwrap_err()
            .code,
        krometrail_core::ErrorCode::ResourceLimitExceeded
    );

    let output_limits = ArtifactWorkLimits {
        max_outputs: NonZeroUsize::new(5).unwrap(),
        max_output_bytes_each: NonZeroUsize::new(16 * 1024 * 1024).unwrap(),
        max_output_bytes_total: NonZeroUsize::new(5 * 16 * 1024 * 1024).unwrap(),
        ..ArtifactWorkLimits::default()
    };
    let output_rig = rig(false, output_limits);
    output_rig
        .service
        .generate(
            output_rig.request.clone(),
            ArtifactGenerationContext::default(),
        )
        .await
        .unwrap();
    for limited in [
        ArtifactWorkLimits {
            max_outputs: NonZeroUsize::new(4).unwrap(),
            ..output_limits
        },
        ArtifactWorkLimits {
            max_output_bytes_each: NonZeroUsize::new(16 * 1024 * 1024 - 1).unwrap(),
            ..output_limits
        },
    ] {
        let limited_rig = rig(false, limited);
        assert_eq!(
            limited_rig
                .service
                .generate(
                    limited_rig.request.clone(),
                    ArtifactGenerationContext::default(),
                )
                .await
                .unwrap_err()
                .code,
            krometrail_core::ErrorCode::ResourceLimitExceeded
        );
    }

    let mut memory_generator = exact_rig.request.generators()[0].clone();
    let ArtifactGeneratorRequest::Storyboard(parameters) = &mut memory_generator else {
        unreachable!()
    };
    parameters.include_orientation = false;
    parameters.output = OutputLimitsRequest::new(4096, 4096, 1024 * 1024).unwrap();
    // Storyboard-only generation still builds the bounded adjacent comparison
    // trace; its exact reservation is two 80-byte comparisons plus 64 bytes.
    let exact_memory = 1024 * 1024 + 48 + 72 + (2 * 80 + 64);
    let memory_limits = ArtifactWorkLimits {
        max_decoded_bytes: NonZeroUsize::new(48).unwrap(),
        max_normalized_bytes: NonZeroUsize::new(72).unwrap(),
        max_combined_request_bytes: NonZeroUsize::new(exact_memory).unwrap(),
        max_output_bytes_each: NonZeroUsize::new(1024 * 1024).unwrap(),
        max_output_bytes_total: NonZeroUsize::new(1024 * 1024).unwrap(),
        ..ArtifactWorkLimits::default()
    };
    let memory_rig = rig(false, memory_limits);
    memory_rig
        .service
        .generate(
            one_generator_request(&memory_rig, memory_generator.clone(), vec![]),
            ArtifactGenerationContext::default(),
        )
        .await
        .unwrap();
    let limited_rig = rig(
        false,
        ArtifactWorkLimits {
            max_combined_request_bytes: NonZeroUsize::new(exact_memory - 1).unwrap(),
            ..memory_limits
        },
    );
    assert_eq!(
        limited_rig
            .service
            .generate(
                one_generator_request(&limited_rig, memory_generator, vec![]),
                ArtifactGenerationContext::default(),
            )
            .await
            .unwrap_err()
            .code,
        krometrail_core::ErrorCode::ResourceLimitExceeded
    );
}

#[derive(serde::Serialize)]
struct ProductionBenchmarkRun {
    frame_count: usize,
    mode: &'static str,
    motion: bool,
    wall_ms: f64,
    process_cpu_ms: f64,
    allocation_bytes: u64,
    peak_rss_kib: u64,
    output_bytes: u64,
    adjacent_pairs: u64,
    measurable_adjacent_pairs: u64,
    storyboard_baseline_pair_calls: u64,
    classified_pixel_passes: u64,
    classifier_pixel_calls: u64,
    pair_pass_reduction: u64,
    classifier_call_reduction: u64,
    cache_dispositions: Vec<String>,
    perf_events: &'static str,
    perf_status: String,
    perf_counter_values: Option<String>,
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "manual Rust 1.85 production service benchmark"]
async fn production_pair_classification_service_profile() {
    let frame_count = std::env::var("PERF_SERVICE_FRAMES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(60_usize);
    assert!(matches!(frame_count, 60 | 120));
    let mode = std::env::var("PERF_SERVICE_MODE").unwrap_or_else(|_| "context".to_owned());
    assert!(matches!(mode.as_str(), "context" | "baseline"));
    let motion = std::env::var("PERF_SERVICE_MOTION").is_ok_and(|value| value == "1");
    let repetitions = std::env::var("PERF_SERVICE_REPETITIONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1_usize);
    assert!((1..=5).contains(&repetitions));
    for _ in 0..repetitions {
        let run = production_benchmark_run(frame_count, &mode, motion).await;
        println!(
            "PRODUCTION_PAIR_CLASSIFICATION_REPORT {}",
            serde_json::to_string(&run).unwrap()
        );
    }
}

async fn production_benchmark_run(
    frame_count: usize,
    mode: &str,
    motion: bool,
) -> ProductionBenchmarkRun {
    let root = std::env::temp_dir().join(format!("krometrail-artifact-perf-{}", Uuid::new_v4()));
    let segments = root.join("segments");
    let index = Arc::new(
        SqliteIndex::open(IndexStoreConfig {
            database_path: root.join("index.sqlite3"),
            segments_directory: segments.clone(),
            busy_timeout: std::time::Duration::from_secs(1),
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
    let store = Arc::new(RecordingStore::new(writer, Arc::clone(&index)).unwrap());
    let session = SessionId::from_uuid(Uuid::from_u128(0xfeed));
    let target = TargetId::from_uuid(Uuid::from_u128(0xbeef));
    let dimensions = PixelDimensions::new(1_920, 1_080).unwrap();
    let frame_ids: Vec<_> = (0..frame_count)
        .map(|index| FrameId::from_uuid(Uuid::from_u128(0x1000 + index as u128)))
        .collect();
    for (position, frame_id) in frame_ids.iter().enumerate() {
        let ordinal = u64::try_from(position + 1).unwrap();
        let timestamp = u64::try_from(position).unwrap() * 10_000_000;
        let frame = EncodedFrame::new(
            CapturedFrame::new(
                *frame_id,
                session,
                target,
                CaptureOrdinal::new(ordinal).unwrap(),
                None,
                ObservedTime::from_nanos(timestamp + 1),
                SessionTime::from_nanos(timestamp),
                ImageFormat::Png,
                dimensions,
                dimensions,
                DeviceScaleFactor::new(1.0).unwrap(),
                vec![],
            )
            .unwrap(),
            production_png(position, dimensions),
        )
        .unwrap();
        store.append_frame(frame).await.unwrap();
    }
    store.flush(session).await.unwrap();

    let labels = ArtifactLabelsRequest::new(
        NonEmptyText::new("production storyboard").unwrap(),
        NonEmptyText::new("production recording store").unwrap(),
    );
    let normalization =
        NormalizationRequest::new(None, Rgb8::new(0, 0, 0), AnalysisScale::Down(2)).unwrap();
    let output = OutputLimitsRequest::new(4_096, 4_096, 64 * 1024 * 1024).unwrap();
    let mut generators = vec![
        ArtifactGeneratorRequest::Storyboard(StoryboardRequest {
            anchor: SessionTime::from_nanos(u64::try_from(frame_count / 3).unwrap() * 10_000_000),
            tile_limit: 8,
            noise_floor: 512,
            normalization,
            labels: labels.clone(),
            include_orientation: true,
            output,
        }),
        ArtifactGeneratorRequest::DifferenceMap(DifferenceMapRequest {
            reference: FrameSelector::First,
            frequency_mode: FrequencyMode::NormalizedFrequency,
            repeated_change_separation_nanos: None,
            noise_floor: 512,
            normalization,
            canvas_background: Rgb8::new(0, 0, 0),
            output,
        }),
    ];
    if motion {
        generators.push(ArtifactGeneratorRequest::MotionHistory(
            MotionHistoryRequest {
                reference: FrameSelector::First,
                noise_floor: 512,
                normalization,
                decay_peak: u16::MAX,
                decay_half_life_ranks: 1,
                reference_strength: 64,
                accent: Rgb8::new(255, 176, 0),
                outline: Rgb8::new(255, 255, 255),
                labels,
                output,
            },
        ));
    }
    let range = SessionRange::new(
        SessionTime::from_nanos(0),
        SessionTime::from_nanos(u64::try_from(frame_count - 1).unwrap() * 10_000_000),
    )
    .unwrap();
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
        generators,
        ArtifactFailurePolicy::RequireAll,
    )
    .unwrap();
    // The production scheduler counts decoded source bytes before analysis;
    // 120 retained 1920x1080 RGBA frames need just under 1 GiB. The benchmark
    // raises only these validated test-policy ceilings so the requested cold
    // workload is representable; grouping still uses the same service/store
    // and combined-request reservation path.
    let bench_limits = ArtifactWorkLimits {
        max_decoded_bytes: NonZeroUsize::new(1_100_000_000).unwrap(),
        max_combined_request_bytes: NonZeroUsize::new(2_000_000_000).unwrap(),
        ..ArtifactWorkLimits::default()
    };
    let mut service = TemporalVisionArtifactService::new(
        Arc::clone(&store) as Arc<dyn FrameSource>,
        Arc::clone(&store) as Arc<dyn ArtifactStore>,
        Arc::new(ProductionSequenceIds(AtomicU64::new(0))),
        bench_limits,
    )
    .unwrap();
    if mode == "baseline" {
        service = service.with_pair_context_enabled(false);
    }
    let allocation_before = ALLOCATED_BYTES.load(Ordering::Relaxed);
    let cpu_before = process_cpu_us();
    let started = Instant::now();
    let result = service
        .generate(request, ArtifactGenerationContext::default())
        .await
        .unwrap();
    let wall_ms = started.elapsed().as_secs_f64() * 1_000.0;
    let process_cpu_ms = (process_cpu_us().saturating_sub(cpu_before)) as f64 / 1_000.0;
    let allocation_bytes = ALLOCATED_BYTES
        .load(Ordering::Relaxed)
        .saturating_sub(allocation_before);
    let output_bytes = result
        .outcomes
        .iter()
        .map(|outcome| match outcome {
            ArtifactOutcome::Available { artifact, .. } => artifact.encoded_byte_len,
            ArtifactOutcome::Unavailable { error, .. } => {
                panic!("production output failed: {error}")
            }
        })
        .sum();
    let adjacent_pairs = u64::try_from(frame_count.saturating_sub(1)).unwrap();
    let measurable_adjacent_pairs = adjacent_pairs;
    let storyboard_baseline_pair_calls = u64::try_from(frame_count - frame_count / 3).unwrap();
    let baseline_passes =
        measurable_adjacent_pairs * if motion { 4 } else { 2 } + storyboard_baseline_pair_calls;
    let context_passes = measurable_adjacent_pairs + storyboard_baseline_pair_calls;
    let included_pixels = 518_400_u64;
    let classified_pixel_passes = if mode == "baseline" {
        baseline_passes
    } else {
        context_passes
    };
    let classifier_pixel_calls = classified_pixel_passes * included_pixels;
    let pair_pass_reduction = baseline_passes.saturating_sub(context_passes);
    let classifier_call_reduction = pair_pass_reduction * included_pixels;
    let cache_dispositions = result
        .outcomes
        .iter()
        .map(|outcome| match outcome {
            ArtifactOutcome::Available { artifact, .. } => format!("{:?}", artifact.cache),
            ArtifactOutcome::Unavailable { .. } => "unavailable".to_owned(),
        })
        .collect();
    let peak_rss_kib = peak_rss_kib();
    drop(service);
    drop(store);
    drop(index);
    let _ = std::fs::remove_dir_all(root);
    ProductionBenchmarkRun {
        frame_count,
        mode: if mode == "baseline" {
            "baseline"
        } else {
            "context"
        },
        motion,
        wall_ms,
        process_cpu_ms,
        allocation_bytes,
        peak_rss_kib,
        output_bytes,
        adjacent_pairs,
        measurable_adjacent_pairs,
        storyboard_baseline_pair_calls,
        classified_pixel_passes,
        classifier_pixel_calls,
        pair_pass_reduction,
        classifier_call_reduction,
        cache_dispositions,
        perf_events: "task-clock,cycles,instructions,cache-misses,branch-misses",
        perf_status: std::env::var("PERF_PAIR_COUNTER_STATUS")
            .unwrap_or_else(|_| "external perf stat not supplied".to_owned()),
        perf_counter_values: std::env::var("PERF_SERVICE_COUNTER_VALUES").ok(),
    }
}

fn production_png(position: usize, dimensions: PixelDimensions) -> Vec<u8> {
    let width = usize::try_from(dimensions.width()).unwrap();
    let height = usize::try_from(dimensions.height()).unwrap();
    let mut pixels = vec![0_u8; width * height * 4];
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.copy_from_slice(&[18, 22, 32, u8::MAX]);
    }
    let patch_x = (position * 19) % (width - 192);
    let patch_y = (position * 11) % (height - 108);
    for y in patch_y..patch_y + 108 {
        for x in patch_x..patch_x + 192 {
            let pixel = &mut pixels[(y * width + x) * 4..(y * width + x) * 4 + 4];
            pixel.copy_from_slice(&[220, 70, 120, u8::MAX]);
        }
    }
    let mut bytes = Vec::new();
    PngEncoder::new(Cursor::new(&mut bytes))
        .write_image(
            &pixels,
            dimensions.width(),
            dimensions.height(),
            ColorType::Rgba8.into(),
        )
        .unwrap();
    bytes
}

fn process_cpu_us() -> u128 {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::zeroed();
    // SAFETY: getrusage initializes the supplied structure on success.
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) } != 0 {
        return 0;
    }
    // SAFETY: getrusage returned success and initialized the structure.
    let usage = unsafe { usage.assume_init() };
    let user = u128::try_from(usage.ru_utime.tv_sec).unwrap_or(0) * 1_000_000
        + u128::try_from(usage.ru_utime.tv_usec).unwrap_or(0);
    let system = u128::try_from(usage.ru_stime.tv_sec).unwrap_or(0) * 1_000_000
        + u128::try_from(usage.ru_stime.tv_usec).unwrap_or(0);
    user + system
}

fn peak_rss_kib() -> u64 {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::zeroed();
    // SAFETY: getrusage initializes the supplied structure on success.
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) } != 0 {
        return 0;
    }
    // SAFETY: getrusage returned success and initialized the structure.
    u64::try_from(unsafe { usage.assume_init() }.ru_maxrss).unwrap_or(0)
}

fn test_error(
    code: krometrail_core::ErrorCode,
    message: &'static str,
) -> krometrail_core::KrometrailError {
    krometrail_core::KrometrailError::new(code, NonEmptyText::new(message).unwrap())
}
