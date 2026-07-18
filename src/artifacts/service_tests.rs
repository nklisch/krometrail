use std::{
    collections::HashMap,
    num::{NonZeroU32, NonZeroUsize},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::Instant,
};

use krometrail_core::{
    AnalysisScale, ArtifactCacheKey, ArtifactFailurePolicy, ArtifactGeneration,
    ArtifactGenerationContext, ArtifactGenerationRequest, ArtifactGeneratorRequest,
    ArtifactLabelsRequest, ArtifactLookup, ArtifactOutcome, ArtifactPublication, ArtifactPublish,
    ArtifactSourceFingerprint, ArtifactStore, CaptureOrdinal, CapturedFrame, DeviceScaleFactor,
    DifferenceMapRequest, EncodedFrame, FrameAvailability, FrameId, FrameSelector, FrameSource,
    IdSource, IdValue, ImageFormat, MotionHistoryRequest, NonEmptyText, NormalizationRequest,
    ObservedTime, OutputLimitsRequest, PixelDimensions, PortFuture, RangeResolutionOptions,
    RegionFilmstripRequest, ResolvedRange, SessionId, SessionRange, SessionTime, StoredArtifact,
    StoryboardRequest, TargetId, TemporalRangeAnchorKind,
};
use temporal_vision::{FrequencyMode, RegionDefinition, Rgb8, SignedPixelRect};
use uuid::Uuid;

use super::{
    TemporalVisionArtifactService, epoch::WorkCancellation, scheduler::ArtifactWorkLimits,
};

const PNG: &[u8] = include_bytes!("../../tests/fixtures/artifacts/chrome-rgba.png");

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
    rig_with_transition(two_epochs, false, limits)
}

fn rig_with_transition(
    viewport_transition: bool,
    scale_transition: bool,
    limits: ArtifactWorkLimits,
) -> TestRig {
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
                    PixelDimensions::new(
                        if viewport_transition && position == 2 {
                            3
                        } else {
                            2
                        },
                        2,
                    )
                    .unwrap(),
                    DeviceScaleFactor::new(if scale_transition && position == 2 {
                        2.0
                    } else {
                        1.0
                    })
                    .unwrap(),
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
async fn device_scale_transition_starts_a_new_visual_epoch_without_normalizing_sources() {
    let mut rig = rig_with_transition(false, true, ArtifactWorkLimits::default());
    rig.request = ArtifactGenerationRequest::new(
        rig.request.range().clone(),
        rig.request.markers().to_vec(),
        rig.request.generators().to_vec(),
        ArtifactFailurePolicy::AllowPartial,
    )
    .unwrap();
    let result = rig
        .service
        .generate(rig.request, ArtifactGenerationContext::default())
        .await
        .unwrap();

    assert_eq!(result.epochs.len(), 2);
    assert_eq!(result.epochs[0].device_scale_factor.get(), 1.0);
    assert_eq!(result.epochs[0].frame_ids.len(), 2);
    assert_eq!(result.epochs[1].device_scale_factor.get(), 2.0);
    assert_eq!(result.epochs[1].frame_ids.len(), 1);
    assert_eq!(result.epochs[0].image, result.epochs[1].image);
    assert_eq!(result.epochs[0].viewport, result.epochs[1].viewport);
    assert!(
        result
            .outcomes
            .iter()
            .any(|outcome| matches!(outcome, ArtifactOutcome::Available { epoch_index: 1, .. }))
    );
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
async fn storyboard_anchor_before_first_retained_frame_is_clamped() {
    let mut rig = rig(false, ArtifactWorkLimits::default());
    let range = SessionRange::new(SessionTime::from_nanos(0), SessionTime::from_nanos(3)).unwrap();
    let resolved = ResolvedRange::new(
        rig.request.range().session_id,
        rig.request.range().target_id,
        TemporalRangeAnchorKind::SessionTime,
        range,
        range,
        rig.request.range().frame_ids.clone(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        RangeResolutionOptions::DEFAULT,
    )
    .unwrap();
    let mut generator = rig.request.generators()[0].clone();
    let ArtifactGeneratorRequest::Storyboard(storyboard) = &mut generator else {
        unreachable!()
    };
    storyboard.anchor = SessionTime::from_nanos(0);
    storyboard.include_orientation = false;
    rig.request = ArtifactGenerationRequest::new(
        resolved,
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

    assert!(matches!(
        result.outcomes.as_slice(),
        [ArtifactOutcome::Available { .. }]
    ));
}

#[tokio::test]
async fn storyboard_anchor_is_clamped_for_every_visual_epoch() {
    let mut rig = rig(true, ArtifactWorkLimits::default());
    let generator = rig.request.generators()[0].clone();
    let semantic_anchor = rig.request.range().resolved_anchor.clone();
    rig.request = ArtifactGenerationRequest::new(
        rig.request.range().clone(),
        vec![],
        vec![generator],
        ArtifactFailurePolicy::RequireAll,
    )
    .unwrap();

    let result = rig
        .service
        .generate(rig.request.clone(), ArtifactGenerationContext::default())
        .await
        .unwrap();

    for epoch_index in [0, 1] {
        for kind in [
            temporal_vision::ArtifactKind::Storyboard,
            temporal_vision::ArtifactKind::BeforeDuringAfter,
        ] {
            assert!(result.outcomes.iter().any(|outcome| matches!(
                outcome,
                ArtifactOutcome::Available { epoch_index: actual_epoch, artifact, .. }
                    if *actual_epoch == epoch_index && artifact.manifest.artifact_kind() == kind
            )));
        }
    }
    assert_eq!(rig.request.range().resolved_anchor, semantic_anchor);
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
    let exact_memory = 1024 * 1024 + 48 + 72;
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

fn test_error(
    code: krometrail_core::ErrorCode,
    message: &'static str,
) -> krometrail_core::KrometrailError {
    krometrail_core::KrometrailError::new(code, NonEmptyText::new(message).unwrap())
}
