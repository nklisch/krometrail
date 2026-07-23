use std::{
    collections::HashMap,
    num::{NonZeroU32, NonZeroUsize},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::Instant,
};

use image::ImageEncoder as _;
use krometrail_core::{
    AnalysisScale, ArtifactCacheKey, ArtifactEpochSelection, ArtifactFailurePolicy,
    ArtifactGeneration, ArtifactGenerationContext, ArtifactGenerationRequest,
    ArtifactGeneratorRequest, ArtifactLabelsRequest, ArtifactLookup, ArtifactManifest,
    ArtifactOutcome, ArtifactPublication, ArtifactPublish, ArtifactSourceFingerprint,
    ArtifactStore, CaptureOrdinal, CapturedFrame, DeviceScaleFactor, DifferenceMapRequest,
    EncodedFrame, FrameAvailability, FrameId, FrameSelector, FrameSource, IdSource, IdValue,
    ImageFormat, MotionHistoryRequest, NonEmptyText, NormalizationRequest, ObservedTime,
    OrientationPolicy, OutputLimitsRequest, PixelDimensions, PortFuture, RangeResolutionOptions,
    RegionFilmstripRequest, ResolvedRange, SessionId, SessionRange, SessionTime, StoredArtifact,
    StoryboardRequest, TargetId, TemporalRangeAnchorKind,
};
use temporal_vision::{ArtifactKind, FrequencyMode, RegionDefinition, Rgb8, SignedPixelRect};
use uuid::Uuid;

use super::{
    TemporalVisionArtifactService,
    epoch::{WorkCancellation, validate_and_plan},
    generators::{estimated_normalized_bytes, prepare_generator, reserved_output_bytes},
    scheduler::ArtifactWorkLimits,
    service::select_epoch_plans,
};

const PNG: &[u8] = include_bytes!("../../tests/fixtures/artifacts/chrome-rgba.png");

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
                Ok(ArtifactPublish::Published(artifact, false))
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

fn encoded_fixture(image: PixelDimensions, position: usize) -> Vec<u8> {
    if image.width() == 2 && image.height() == 2 {
        return PNG.to_vec();
    }
    let pixel_count =
        usize::try_from(image.width()).unwrap() * usize::try_from(image.height()).unwrap();
    let mut rgba = vec![0_u8; pixel_count * 4];
    rgba[0..4].copy_from_slice(&[position as u8, 96, 192, 255]);
    let mut encoded = Vec::new();
    image::codecs::png::PngEncoder::new(&mut encoded)
        .write_image(
            &rgba,
            image.width(),
            image.height(),
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
    encoded
}

fn rig(two_epochs: bool, limits: ArtifactWorkLimits) -> TestRig {
    rig_with_transition(two_epochs, false, limits)
}

fn rig_with_transition(
    viewport_transition: bool,
    scale_transition: bool,
    limits: ArtifactWorkLimits,
) -> TestRig {
    rig_with_transition_and_frame_count(3, viewport_transition, scale_transition, limits)
}

fn rig_with_transition_and_frame_count(
    frame_count: usize,
    viewport_transition: bool,
    scale_transition: bool,
    limits: ArtifactWorkLimits,
) -> TestRig {
    rig_with_frame_dimensions(
        frame_count,
        PixelDimensions::new(2, 2).unwrap(),
        viewport_transition,
        scale_transition,
        limits,
    )
}

fn rig_with_frame_dimensions(
    frame_count: usize,
    image: PixelDimensions,
    viewport_transition: bool,
    scale_transition: bool,
    limits: ArtifactWorkLimits,
) -> TestRig {
    let session = SessionId::from_uuid(Uuid::from_u128(1));
    let target = TargetId::from_uuid(Uuid::from_u128(2));
    let frames: Vec<_> = (0..frame_count)
        .map(|position| {
            let ordinal = u64::try_from(position + 1).unwrap();
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
                    image,
                    PixelDimensions::new(
                        if viewport_transition && position == 2 {
                            image.width().saturating_add(1)
                        } else {
                            image.width()
                        },
                        image.height(),
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
                encoded_fixture(image, position),
            )
            .unwrap()
        })
        .collect();
    let range = SessionRange::new(
        SessionTime::from_nanos(1),
        SessionTime::from_nanos(frame_count as u64),
    )
    .unwrap();
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
            sampling: krometrail_core::ArtifactSampling::Exhaustive,
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
            sampling: krometrail_core::ArtifactSampling::Exhaustive,
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
async fn allow_partial_keeps_bounded_storyboard_when_exhaustive_planning_is_refused() {
    let mut rig =
        rig_with_transition_and_frame_count(367, false, false, ArtifactWorkLimits::default());
    let mut generators = rig.request.generators().to_vec();
    generators.truncate(2);
    let ArtifactGeneratorRequest::Storyboard(storyboard) = &mut generators[0] else {
        panic!("default first generator is storyboard");
    };
    storyboard.tile_limit = 8;
    storyboard.include_orientation = false;
    rig.request = ArtifactGenerationRequest::new(
        rig.request.range().clone(),
        vec![],
        generators,
        ArtifactFailurePolicy::AllowPartial,
    )
    .unwrap();

    let result = rig
        .service
        .generate(rig.request, ArtifactGenerationContext::default())
        .await
        .unwrap();
    assert!(matches!(
        result.outcomes.first(),
        Some(ArtifactOutcome::Available { artifact, .. })
            if artifact.manifest.artifact_kind() == temporal_vision::ArtifactKind::Storyboard
    ));
    let ArtifactOutcome::Available { artifact, .. } = &result.outcomes[0] else {
        panic!("storyboard should be available");
    };
    assert!(
        artifact
            .manifest
            .parameters()
            .get("analysis_sampling")
            .is_some()
    );

    let small = rig_with_transition_and_frame_count(3, false, false, ArtifactWorkLimits::default());
    let small_result = small
        .service
        .generate(small.request, ArtifactGenerationContext::default())
        .await
        .expect("small range generation should succeed");
    let ArtifactOutcome::Available { artifact, .. } = &small_result.outcomes[0] else {
        panic!("small storyboard should be available");
    };
    assert!(
        artifact
            .manifest
            .parameters()
            .get("analysis_sampling")
            .is_none()
    );

    assert!(matches!(
        result.outcomes.get(1),
        Some(ArtifactOutcome::Unavailable { error, artifact_kind, .. })
            if *artifact_kind == temporal_vision::ArtifactKind::DifferenceMap
                && error.message.as_str().contains("count-mode")
                && error.message.as_str().contains("exceeds limit")
                && error.recovery.as_ref().is_some_and(|recovery| {
                    recovery.as_str().contains("narrow the range")
                        && recovery.as_str().contains("normalized_frequency")
                })
    ));
}

#[tokio::test]
async fn analysis_generators_sample_by_default_and_disclose_their_source_counts() {
    let mut rig =
        rig_with_transition_and_frame_count(367, false, false, ArtifactWorkLimits::default());
    let mut generators = rig.request.generators().to_vec();
    let ArtifactGeneratorRequest::DifferenceMap(difference) = &mut generators[1] else {
        panic!("default second generator is difference map");
    };
    difference.frequency_mode = FrequencyMode::NormalizedFrequency;
    difference.sampling = krometrail_core::ArtifactSampling::UniformBounded;
    let ArtifactGeneratorRequest::MotionHistory(motion) = &mut generators[3] else {
        panic!("default fourth generator is motion history");
    };
    motion.sampling = krometrail_core::ArtifactSampling::UniformBounded;
    generators.truncate(4);
    rig.request = ArtifactGenerationRequest::new(
        rig.request.range().clone(),
        vec![],
        vec![generators[1].clone(), generators[3].clone()],
        ArtifactFailurePolicy::RequireAll,
    )
    .unwrap();

    let result = rig
        .service
        .generate(rig.request, ArtifactGenerationContext::default())
        .await
        .expect("bounded analysis generators should succeed");
    assert_eq!(result.outcomes.len(), 2);
    for outcome in result.outcomes {
        let ArtifactOutcome::Available { artifact, .. } = outcome else {
            panic!("bounded analysis generator should be available");
        };
        let sampling = artifact
            .manifest
            .parameters()
            .get("analysis_sampling")
            .expect("sampled analysis must disclose source counts");
        let temporal_vision::ParameterValue::Object(values) = sampling else {
            panic!("analysis_sampling must be an object");
        };
        assert_eq!(
            values["source_frame_count"],
            temporal_vision::ParameterValue::Unsigned(367)
        );
        assert_eq!(
            values["analyzed_frame_count"],
            temporal_vision::ParameterValue::Unsigned(120)
        );
        assert_eq!(
            values["mode"],
            temporal_vision::ParameterValue::Text("uniform_bounded".into())
        );
        assert_eq!(
            values["spacing"],
            temporal_vision::ParameterValue::Text("uniform".into())
        );
    }
}

#[tokio::test]
async fn realistic_dimensions_sample_to_the_decoded_byte_budget_by_default() {
    const WIDTH: u32 = 1_673;
    const HEIGHT: u32 = 1_288;
    let per_frame_decoded_bytes = WIDTH as usize * HEIGHT as usize * 4;
    let limits = ArtifactWorkLimits {
        max_decoded_bytes: NonZeroUsize::new(per_frame_decoded_bytes * 3).unwrap(),
        // This test pins byte-budget sampling, not wall-time enforcement.
        // Debug-build image work on a slow shared CI runner can exceed the
        // production wall-clock, so decouple the assertion from it.
        max_wall_time: std::time::Duration::from_secs(120),
        ..ArtifactWorkLimits::default()
    };
    let mut rig = rig_with_frame_dimensions(
        4,
        PixelDimensions::new(WIDTH, HEIGHT).unwrap(),
        false,
        false,
        limits,
    );
    let difference = serde_json::from_value::<DifferenceMapRequest>(serde_json::json!({}))
        .expect("difference-map wire defaults should be complete");
    let motion = serde_json::from_value::<MotionHistoryRequest>(serde_json::json!({}))
        .expect("motion-history wire defaults should be complete");
    rig.request = ArtifactGenerationRequest::new(
        rig.request.range().clone(),
        vec![],
        vec![
            ArtifactGeneratorRequest::DifferenceMap(difference),
            ArtifactGeneratorRequest::MotionHistory(motion),
        ],
        ArtifactFailurePolicy::RequireAll,
    )
    .unwrap();

    let result = rig
        .service
        .generate(rig.request, ArtifactGenerationContext::default())
        .await
        .expect("byte-bounded default analysis should succeed");
    assert_eq!(result.outcomes.len(), 2);
    for outcome in result.outcomes {
        let ArtifactOutcome::Available { artifact, .. } = outcome else {
            panic!("realistic byte-bounded analysis should produce both artifacts");
        };
        let temporal_vision::ParameterValue::Object(values) = artifact
            .manifest
            .parameters()
            .get("analysis_sampling")
            .expect("sampled analysis must disclose its source counts")
        else {
            panic!("analysis_sampling must be an object");
        };
        assert_eq!(
            values["source_frame_count"],
            temporal_vision::ParameterValue::Unsigned(4)
        );
        assert_eq!(
            values["analyzed_frame_count"],
            temporal_vision::ParameterValue::Unsigned(3)
        );
    }
}

async fn oversized_exact_difference_refuses_at_the_byte_bound(frequency_mode: FrequencyMode) {
    const WIDTH: u32 = 1_673;
    const HEIGHT: u32 = 1_288;
    let per_frame_decoded_bytes = WIDTH as usize * HEIGHT as usize * 4;
    let mut rig = rig_with_frame_dimensions(
        2,
        PixelDimensions::new(WIDTH, HEIGHT).unwrap(),
        false,
        false,
        ArtifactWorkLimits {
            max_decoded_bytes: NonZeroUsize::new(per_frame_decoded_bytes).unwrap(),
            // Pins the byte-bound refusal, not wall-time enforcement; keep the
            // realistic-dimension work clear of the production wall-clock on
            // slow debug-build CI runners.
            max_wall_time: std::time::Duration::from_secs(120),
            ..ArtifactWorkLimits::default()
        },
    );
    let mut difference = match rig.request.generators()[1].clone() {
        ArtifactGeneratorRequest::DifferenceMap(request) => request,
        _ => panic!("default second generator is difference map"),
    };
    difference.frequency_mode = frequency_mode;
    difference.sampling = krometrail_core::ArtifactSampling::UniformBounded;
    rig.request = ArtifactGenerationRequest::new(
        rig.request.range().clone(),
        vec![],
        vec![ArtifactGeneratorRequest::DifferenceMap(difference)],
        ArtifactFailurePolicy::RequireAll,
    )
    .unwrap();

    let error = rig
        .service
        .generate(rig.request, ArtifactGenerationContext::default())
        .await
        .expect_err("oversized exact difference maps must not be sampled");
    assert!(error.message.as_str().contains("exceeds limit"));
    assert!(
        error
            .recovery
            .as_ref()
            .is_some_and(|recovery| recovery.as_str().contains("narrow the range"))
    );
    assert!(
        error
            .recovery
            .as_ref()
            .is_some_and(|recovery| recovery.as_str().contains("normalized_frequency"))
    );
}

#[tokio::test]
async fn count_maps_refuse_when_bytes_force_sampling_below_frame_limit() {
    oversized_exact_difference_refuses_at_the_byte_bound(FrequencyMode::Count).await;
}

#[tokio::test]
async fn magnitude_maps_refuse_when_bytes_force_sampling_below_frame_limit() {
    oversized_exact_difference_refuses_at_the_byte_bound(FrequencyMode::Magnitude).await;
}

#[tokio::test]
async fn count_maps_refuse_sampling_and_offer_the_two_real_levers() {
    let mut rig =
        rig_with_transition_and_frame_count(367, false, false, ArtifactWorkLimits::default());
    let mut difference = match rig.request.generators()[1].clone() {
        ArtifactGeneratorRequest::DifferenceMap(request) => request,
        _ => panic!("default second generator is difference map"),
    };
    difference.sampling = krometrail_core::ArtifactSampling::UniformBounded;
    rig.request = ArtifactGenerationRequest::new(
        rig.request.range().clone(),
        vec![],
        vec![ArtifactGeneratorRequest::DifferenceMap(difference)],
        ArtifactFailurePolicy::RequireAll,
    )
    .unwrap();

    let error = rig
        .service
        .generate(rig.request, ArtifactGenerationContext::default())
        .await
        .expect_err("count maps must not be silently sampled");
    assert!(error.message.as_str().contains("count-mode"));
    assert!(error.message.as_str().contains("exceeds limit"));
    assert!(
        error
            .recovery
            .as_ref()
            .is_some_and(|recovery| recovery.as_str().contains("narrow the range"))
    );
    assert!(
        error
            .recovery
            .as_ref()
            .is_some_and(|recovery| recovery.as_str().contains("normalized_frequency"))
    );
}

#[tokio::test]
async fn magnitude_maps_refuse_sampling_like_count_maps() {
    let mut rig =
        rig_with_transition_and_frame_count(367, false, false, ArtifactWorkLimits::default());
    let mut difference = match rig.request.generators()[1].clone() {
        ArtifactGeneratorRequest::DifferenceMap(request) => request,
        _ => panic!("default second generator is difference map"),
    };
    difference.frequency_mode = FrequencyMode::Magnitude;
    difference.sampling = krometrail_core::ArtifactSampling::UniformBounded;
    rig.request = ArtifactGenerationRequest::new(
        rig.request.range().clone(),
        vec![],
        vec![ArtifactGeneratorRequest::DifferenceMap(difference)],
        ArtifactFailurePolicy::RequireAll,
    )
    .unwrap();

    let error = rig
        .service
        .generate(rig.request, ArtifactGenerationContext::default())
        .await
        .expect_err("magnitude maps must not be silently sampled");
    assert!(error.message.as_str().contains("magnitude-mode"));
    assert!(
        error
            .recovery
            .as_ref()
            .is_some_and(|recovery| recovery.as_str().contains("normalized_frequency"))
    );
}

#[tokio::test]
async fn exhaustive_analysis_refusal_names_a_working_sampling_lever() {
    let mut rig =
        rig_with_transition_and_frame_count(367, false, false, ArtifactWorkLimits::default());
    let mut difference = match rig.request.generators()[1].clone() {
        ArtifactGeneratorRequest::DifferenceMap(request) => request,
        _ => panic!("default second generator is difference map"),
    };
    difference.frequency_mode = FrequencyMode::NormalizedFrequency;
    difference.sampling = krometrail_core::ArtifactSampling::Exhaustive;
    rig.request = ArtifactGenerationRequest::new(
        rig.request.range().clone(),
        vec![],
        vec![ArtifactGeneratorRequest::DifferenceMap(difference)],
        ArtifactFailurePolicy::RequireAll,
    )
    .unwrap();

    let error = rig
        .service
        .generate(rig.request, ArtifactGenerationContext::default())
        .await
        .expect_err("exhaustive oversized analysis should refuse");
    assert!(error.message.as_str().contains("exhaustive"));
    assert!(error.recovery.as_ref().is_some_and(|recovery| {
        recovery
            .as_str()
            .contains("narrow the resolved range so at most 120 frames")
            && recovery
                .as_str()
                .contains("uniform_bounded sampling which analyzes a bounded subset of any range")
    }));
}

#[tokio::test]
async fn exhaustive_analysis_plan_cap_accepts_boundary_and_structures_next_frame_refusal() {
    let request = |frame_count| {
        let mut rig = rig_with_transition_and_frame_count(
            frame_count,
            false,
            false,
            ArtifactWorkLimits::default(),
        );
        let mut difference = match rig.request.generators()[1].clone() {
            ArtifactGeneratorRequest::DifferenceMap(request) => request,
            _ => panic!("default second generator is difference map"),
        };
        difference.frequency_mode = FrequencyMode::NormalizedFrequency;
        difference.sampling = krometrail_core::ArtifactSampling::Exhaustive;
        rig.request = ArtifactGenerationRequest::new(
            rig.request.range().clone(),
            vec![],
            vec![ArtifactGeneratorRequest::DifferenceMap(difference)],
            ArtifactFailurePolicy::RequireAll,
        )
        .unwrap();
        rig
    };

    let at_cap = request(120);
    at_cap
        .service
        .generate(at_cap.request, ArtifactGenerationContext::default())
        .await
        .expect("the exhaustive analysis frame cap is inclusive");

    let over_cap = request(121);
    let error = over_cap
        .service
        .generate(over_cap.request, ArtifactGenerationContext::default())
        .await
        .expect_err("the first frame beyond the exhaustive analysis cap must refuse");
    assert_eq!(
        error.code,
        krometrail_core::ErrorCode::ResourceLimitExceeded
    );
    assert_eq!(
        error.message.as_str(),
        "exhaustive analysis source plan: 121 frames and 1936 decoded bytes exceeds limit 120 frames and 805306368 decoded bytes"
    );
    let recovery = error
        .recovery
        .as_ref()
        .expect("the bounded refusal carries recovery guidance")
        .as_str();
    assert!(recovery.contains("narrow the resolved range so at most 120 frames fall inside it"));
    assert!(
        recovery
            .contains("use uniform_bounded sampling which analyzes a bounded subset of any range")
    );
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
async fn anchor_epoch_is_selected_before_output_limits_and_keeps_original_index() {
    let limits = ArtifactWorkLimits {
        max_outputs: NonZeroUsize::new(2).unwrap(),
        ..ArtifactWorkLimits::default()
    };
    let mut rig = rig(true, limits);
    rig.request = ArtifactGenerationRequest::new(
        rig.request.range().clone(),
        vec![],
        vec![rig.request.generators()[0].clone()],
        ArtifactFailurePolicy::RequireAll,
    )
    .unwrap();

    let all_error = rig
        .service
        .generate(rig.request.clone(), ArtifactGenerationContext::default())
        .await
        .unwrap_err();
    assert_eq!(
        all_error.code,
        krometrail_core::ErrorCode::ResourceLimitExceeded
    );

    let selected = rig
        .service
        .generate(
            rig.request,
            ArtifactGenerationContext {
                epoch_selection: ArtifactEpochSelection::Anchor(SessionTime::from_nanos(3)),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(selected.epochs.len(), 1);
    assert_eq!(selected.epochs[0].index, 1);
    assert!(selected.outcomes.iter().all(|outcome| match outcome {
        ArtifactOutcome::Available { epoch_index, .. }
        | ArtifactOutcome::Unavailable { epoch_index, .. } => *epoch_index == 1,
    }));
}

#[test]
fn anchor_epoch_ties_choose_the_earlier_original_epoch() {
    let rig = rig(true, ArtifactWorkLimits::default());
    let cancellation = WorkCancellation::default();
    let mut plans = validate_and_plan(
        rig.request.range(),
        rig.frames.frames.clone(),
        rig.request.markers(),
        ArtifactWorkLimits::default().adaptation(),
        &cancellation,
    )
    .unwrap();
    let shared_boundary = plans[1].frames[0].clone();
    plans[0].frames.push(shared_boundary);

    let selected = select_epoch_plans(
        plans,
        ArtifactEpochSelection::Anchor(SessionTime::from_nanos(3)),
    )
    .unwrap();
    assert_eq!(selected[0].descriptor.index, 0);
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
                ..Default::default()
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
                ..Default::default()
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
        sampling: krometrail_core::ArtifactSampling::Exhaustive,
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

#[test]
fn default_limits_fit_reproduced_high_dpi_sequence_with_fixed_combined_budget() {
    let session = SessionId::from_uuid(Uuid::from_u128(8_000));
    let target = TargetId::from_uuid(Uuid::from_u128(8_001));
    let image = PixelDimensions::new(2_400, 1_410).unwrap();
    let viewport = PixelDimensions::new(1_200, 705).unwrap();
    let frame = |position: u64| {
        EncodedFrame::new(
            CapturedFrame::new(
                FrameId::from_uuid(Uuid::from_u128(8_100 + u128::from(position))),
                session,
                target,
                CaptureOrdinal::new(position + 1).unwrap(),
                None,
                ObservedTime::from_nanos(position + 1),
                SessionTime::from_nanos(position + 1),
                ImageFormat::Png,
                image,
                viewport,
                DeviceScaleFactor::new(2.0).unwrap(),
                vec![],
            )
            .unwrap(),
            PNG.to_vec(),
        )
        .unwrap()
    };
    let frames: Vec<_> = (0_u64..53).map(&frame).collect();
    let time = SessionRange::new(SessionTime::from_nanos(1), SessionTime::from_nanos(53)).unwrap();
    let range = ResolvedRange::new(
        session,
        target,
        TemporalRangeAnchorKind::SessionTime,
        time,
        time,
        frames.iter().map(|frame| frame.metadata().id()).collect(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        RangeResolutionOptions::DEFAULT,
    )
    .unwrap();
    let limits = ArtifactWorkLimits::default();
    assert_eq!(limits.max_combined_request_bytes.get(), 1024 * 1024 * 1024);
    let plans = validate_and_plan(
        &range,
        frames,
        &[],
        limits.adaptation(),
        &WorkCancellation::default(),
    )
    .unwrap();
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].decoded_bytes, 717_408_000);
    assert_eq!(limits.max_decoded_bytes.get(), 768 * 1024 * 1024);

    let mut peak_reservation = 0;
    for generator in
        crate::debug_bundle::default_generators(&range, krometrail_core::OrientationPolicy::Include)
    {
        let prepared = prepare_generator(&generator, false, &plans[0], limits).unwrap();
        let repeated = prepare_generator(&generator, false, &plans[0], limits).unwrap();
        assert_eq!(prepared.canonical_parameters, repeated.canonical_parameters);
        let normalization = match &prepared.request {
            ArtifactGeneratorRequest::Storyboard(request) => request.normalization,
            ArtifactGeneratorRequest::DifferenceMap(request) => request.normalization,
            _ => unreachable!(),
        };
        assert_eq!(normalization.scale, AnalysisScale::Down(2));
        let output_bytes = match &prepared.request {
            ArtifactGeneratorRequest::Storyboard(request) => {
                request.output.max_encoded_bytes() as usize * prepared.request.output_kinds().len()
            }
            ArtifactGeneratorRequest::DifferenceMap(request) => {
                request.output.max_encoded_bytes() as usize
            }
            _ => unreachable!(),
        };
        peak_reservation = peak_reservation.max(
            plans[0].decoded_bytes
                + estimated_normalized_bytes(&prepared, &plans[0]).unwrap()
                + output_bytes,
        );
    }
    assert_eq!(peak_reservation, 1_053_544_864);
    assert!(peak_reservation <= limits.max_combined_request_bytes.get());

    let oversized_frames: Vec<_> = (0_u64..55).map(frame).collect();
    let oversized_time =
        SessionRange::new(SessionTime::from_nanos(1), SessionTime::from_nanos(55)).unwrap();
    let oversized_range = ResolvedRange::new(
        session,
        target,
        TemporalRangeAnchorKind::SessionTime,
        oversized_time,
        oversized_time,
        oversized_frames
            .iter()
            .map(|frame| frame.metadata().id())
            .collect(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        RangeResolutionOptions::DEFAULT,
    )
    .unwrap();
    let oversized_plan = validate_and_plan(
        &oversized_range,
        oversized_frames,
        &[],
        limits.adaptation(),
        &WorkCancellation::default(),
    )
    .unwrap();
    let difference_map = crate::debug_bundle::default_generators(
        &oversized_range,
        krometrail_core::OrientationPolicy::Include,
    )
    .pop()
    .unwrap();
    let prepared = prepare_generator(&difference_map, false, &oversized_plan[0], limits).unwrap();
    let ArtifactGeneratorRequest::DifferenceMap(request) = prepared.request else {
        unreachable!()
    };
    assert_eq!(request.normalization.scale, AnalysisScale::Down(3));
}

#[tokio::test]
async fn proportional_high_dpi_bundle_executes_below_peak_not_cumulative_reservation() {
    const WIDTH: u32 = 1_200;
    const HEIGHT: u32 = 704;
    const FRAME_COUNT: u64 = 9;
    const COMBINED_LIMIT: usize = 109_250_000;

    let session = SessionId::from_uuid(Uuid::from_u128(9_000));
    let target = TargetId::from_uuid(Uuid::from_u128(9_001));
    let image = PixelDimensions::new(WIDTH, HEIGHT).unwrap();
    let viewport = PixelDimensions::new(WIDTH / 2, HEIGHT / 2).unwrap();
    let frames: Vec<_> = (0_u64..FRAME_COUNT)
        .map(|position| {
            let mut rgba = vec![0_u8; WIDTH as usize * HEIGHT as usize * 4];
            for pixel in rgba.chunks_exact_mut(4) {
                pixel.copy_from_slice(&[
                    24 + position as u8 * 8,
                    72,
                    144_u8.saturating_sub(position as u8 * 6),
                    255,
                ]);
            }
            let stripe_start = usize::try_from(position * 80).unwrap();
            for y in 240_usize..304 {
                for x in stripe_start..(stripe_start + 80) {
                    let offset = (y * WIDTH as usize + x) * 4;
                    rgba[offset..offset + 4].copy_from_slice(&[255, 176, 0, 255]);
                }
            }
            let mut encoded = Vec::new();
            image::codecs::png::PngEncoder::new(&mut encoded)
                .write_image(&rgba, WIDTH, HEIGHT, image::ExtendedColorType::Rgba8)
                .unwrap();
            EncodedFrame::new(
                CapturedFrame::new(
                    FrameId::from_uuid(Uuid::from_u128(9_100 + u128::from(position))),
                    session,
                    target,
                    CaptureOrdinal::new(position + 1).unwrap(),
                    None,
                    ObservedTime::from_nanos(position + 1),
                    SessionTime::from_nanos(position + 1),
                    ImageFormat::Png,
                    image,
                    viewport,
                    DeviceScaleFactor::new(2.0).unwrap(),
                    vec![],
                )
                .unwrap(),
                encoded,
            )
            .unwrap()
        })
        .collect();
    let frame_ids: Vec<_> = frames.iter().map(|frame| frame.metadata().id()).collect();
    let time = SessionRange::new(
        SessionTime::from_nanos(1),
        SessionTime::from_nanos(FRAME_COUNT),
    )
    .unwrap();
    let range = ResolvedRange::new(
        session,
        target,
        TemporalRangeAnchorKind::SessionTime,
        time,
        time,
        frame_ids.clone(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        RangeResolutionOptions::DEFAULT,
    )
    .unwrap();
    let limits = ArtifactWorkLimits {
        max_decoded_bytes: NonZeroUsize::new(32 * 1024 * 1024).unwrap(),
        max_normalized_bytes: NonZeroUsize::new(48 * 1024 * 1024).unwrap(),
        max_combined_request_bytes: NonZeroUsize::new(COMBINED_LIMIT).unwrap(),
        max_output_bytes_total: NonZeroUsize::new(64 * 1024 * 1024).unwrap(),
        ..ArtifactWorkLimits::default()
    };
    let plans = validate_and_plan(
        &range,
        frames.clone(),
        &[],
        limits.adaptation(),
        &WorkCancellation::default(),
    )
    .unwrap();
    assert_eq!(plans[0].decoded_bytes, 30_412_800);

    let generators = crate::debug_bundle::default_generators(&range, OrientationPolicy::Include);
    let prepared: Vec<_> = generators
        .iter()
        .map(|generator| prepare_generator(generator, false, &plans[0], limits).unwrap())
        .collect();
    for generator in &prepared {
        let normalization = match &generator.request {
            ArtifactGeneratorRequest::Storyboard(request) => request.normalization,
            ArtifactGeneratorRequest::DifferenceMap(request) => request.normalization,
            _ => unreachable!(),
        };
        assert_eq!(normalization.scale, AnalysisScale::Down(2));
    }
    let normalized_bytes = estimated_normalized_bytes(&prepared[0], &plans[0]).unwrap();
    assert_eq!(normalized_bytes, 11_404_800);
    let output_reservations: Vec<_> = prepared
        .iter()
        .map(|generator| reserved_output_bytes(generator, limits).unwrap())
        .collect();
    assert_eq!(output_reservations, [32 * 1024 * 1024, 64 * 1024 * 1024]);
    let peak_reservation = plans[0].decoded_bytes
        + normalized_bytes
        + output_reservations.iter().copied().max().unwrap();
    let old_cumulative_reservation =
        plans[0].decoded_bytes + normalized_bytes + output_reservations.iter().sum::<usize>();
    assert_eq!(peak_reservation, 108_926_464);
    assert!(peak_reservation <= COMBINED_LIMIT);
    assert!(old_cumulative_reservation > COMBINED_LIMIT);

    let frames = Arc::new(FakeFrames {
        frames,
        loads: AtomicUsize::new(0),
    });
    let artifacts = Arc::new(FakeArtifacts::default());
    let ids = Arc::new(FakeIds {
        next: AtomicU64::new(20_000),
    });
    let service = TemporalVisionArtifactService::new(
        Arc::clone(&frames) as Arc<dyn FrameSource>,
        Arc::clone(&artifacts) as Arc<dyn ArtifactStore>,
        Arc::clone(&ids) as Arc<dyn IdSource>,
        limits,
    )
    .unwrap();
    let request = ArtifactGenerationRequest::new(
        range,
        vec![],
        generators,
        ArtifactFailurePolicy::RequireAll,
    )
    .unwrap();
    let result = service
        .generate(request, ArtifactGenerationContext::default())
        .await
        .unwrap();

    assert_eq!(frames.loads.load(Ordering::SeqCst), 1);
    assert_eq!(artifacts.publications.load(Ordering::SeqCst), 3);
    let kinds: Vec<_> = result
        .outcomes
        .iter()
        .map(|outcome| match outcome {
            ArtifactOutcome::Available { artifact, .. } => {
                let manifest = &artifact.manifest;
                assert_eq!(manifest.source_frame_ids(), frame_ids);
                assert_eq!(manifest.source_frame_count(), FRAME_COUNT);
                assert!(manifest.output_dimensions().width() > 0);
                assert!(manifest.output_dimensions().height() > 0);
                assert!(
                    manifest
                        .output_hash()
                        .as_bytes()
                        .iter()
                        .any(|byte| *byte != 0)
                );
                let serialized = serde_json::to_vec(manifest).unwrap();
                let round_trip: ArtifactManifest = serde_json::from_slice(&serialized).unwrap();
                assert_eq!(round_trip, *manifest);
                manifest.artifact_kind()
            }
            ArtifactOutcome::Unavailable { .. } => panic!("default generator must publish"),
        })
        .collect();
    assert_eq!(
        kinds,
        [
            temporal_vision::ArtifactKind::Storyboard,
            temporal_vision::ArtifactKind::BeforeDuringAfter,
            temporal_vision::ArtifactKind::DifferenceMap,
        ]
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

fn published_bytes(
    rig: &TestRig,
    result: &krometrail_core::ArtifactGenerationResult,
    kind: ArtifactKind,
) -> Vec<u8> {
    let artifact_id = result
        .outcomes
        .iter()
        .find_map(|outcome| match outcome {
            ArtifactOutcome::Available { artifact, .. }
                if artifact.manifest.artifact_kind() == kind =>
            {
                Some(*artifact.manifest.artifact_id())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("{kind} output was not available"));
    rig.artifacts
        .by_id
        .lock()
        .unwrap()
        .get(&artifact_id)
        .unwrap_or_else(|| panic!("{kind} output was not stored"))
        .encoded_bytes
        .to_vec()
}

async fn mixed_plan_order_matches_unshared(order: [usize; 3]) {
    let base_rig =
        rig_with_transition_and_frame_count(6, false, false, ArtifactWorkLimits::default());
    let mut base_generators = base_rig.request.generators().to_vec();
    let ArtifactGeneratorRequest::Storyboard(storyboard) = &mut base_generators[0] else {
        unreachable!()
    };
    storyboard.include_orientation = false;
    storyboard.tile_limit = 3;
    let generators = order
        .into_iter()
        .map(|index| base_generators[index].clone())
        .collect::<Vec<_>>();
    let mixed_request = ArtifactGenerationRequest::new(
        base_rig.request.range().clone(),
        vec![],
        generators.clone(),
        ArtifactFailurePolicy::RequireAll,
    )
    .unwrap();
    let mixed_result = base_rig
        .service
        .generate(mixed_request, ArtifactGenerationContext::default())
        .await
        .expect("mixed sampled plans must generate all artifacts");

    for generator in generators {
        let single_rig =
            rig_with_transition_and_frame_count(6, false, false, ArtifactWorkLimits::default());
        let kind = generator.output_kinds()[0];
        let single_result = single_rig
            .service
            .generate(
                one_generator_request(&single_rig, generator, vec![]),
                ArtifactGenerationContext::default(),
            )
            .await
            .expect("unshared generator must succeed");
        assert_eq!(
            published_bytes(&base_rig, &mixed_result, kind),
            published_bytes(&single_rig, &single_result, kind),
            "mixed-plan {kind} output differs from its unshared path",
        );
    }
}

#[tokio::test]
async fn mixed_sampled_plans_fall_back_in_either_generator_order() {
    // The storyboard is bounded to three frames while difference and motion use all six.
    // Both orders exercise the historical short-analysis and out-of-range failure modes.
    mixed_plan_order_matches_unshared([0, 1, 3]).await;
    mixed_plan_order_matches_unshared([3, 1, 0]).await;
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

/// Defect 3: wire-default `uniform_bounded` sampling used to drop an explicitly
/// named reference frame that fell off the uniform grid, failing with a message
/// claiming the frame was outside the epoch when sampling had discarded it.
#[tokio::test]
async fn uniform_bounded_sampling_retains_an_explicitly_named_reference_frame() {
    // 367 sources bound to 120 analyzed frames. The uniform grid starts 0, 3, 6…
    // so source position 1 is never selected by spacing alone.
    let off_grid = FrameId::from_uuid(Uuid::from_u128(11));
    let mut rig =
        rig_with_transition_and_frame_count(367, false, false, ArtifactWorkLimits::default());
    let mut generators = rig.request.generators().to_vec();
    let ArtifactGeneratorRequest::DifferenceMap(difference) = &mut generators[1] else {
        panic!("default second generator is difference map");
    };
    difference.frequency_mode = FrequencyMode::NormalizedFrequency;
    difference.sampling = krometrail_core::ArtifactSampling::UniformBounded;
    difference.reference = FrameSelector::Frame(off_grid);
    let ArtifactGeneratorRequest::MotionHistory(motion) = &mut generators[3] else {
        panic!("default fourth generator is motion history");
    };
    motion.sampling = krometrail_core::ArtifactSampling::UniformBounded;
    motion.reference = FrameSelector::Frame(off_grid);
    rig.request = ArtifactGenerationRequest::new(
        rig.request.range().clone(),
        vec![],
        vec![generators[1].clone(), generators[3].clone()],
        ArtifactFailurePolicy::RequireAll,
    )
    .unwrap();

    let result = rig
        .service
        .generate(rig.request, ArtifactGenerationContext::default())
        .await
        .expect("an explicit reference frame must survive bounded sampling");
    assert_eq!(result.outcomes.len(), 2);
    for outcome in result.outcomes {
        let ArtifactOutcome::Available { artifact, .. } = outcome else {
            panic!("bounded analysis with an explicit reference should be available");
        };
        assert!(
            artifact.manifest.analyzed_frame_ids().contains(&off_grid),
            "sampling dropped the reference frame"
        );
        assert_eq!(artifact.manifest.selected_frame_ids(), &[off_grid]);
    }
}

/// Defect 1: the manifest counts and the sampling disclosure that agent surfaces
/// turn into a warning must describe the same evidence. `omitted_frame_count`
/// counts source frames that contributed nothing, not frames left unreferenced.
#[tokio::test]
async fn sampled_analysis_manifest_counts_match_its_sampling_disclosure() {
    let mut rig =
        rig_with_transition_and_frame_count(367, false, false, ArtifactWorkLimits::default());
    let mut generators = rig.request.generators().to_vec();
    let ArtifactGeneratorRequest::DifferenceMap(difference) = &mut generators[1] else {
        panic!("default second generator is difference map");
    };
    difference.frequency_mode = FrequencyMode::NormalizedFrequency;
    difference.sampling = krometrail_core::ArtifactSampling::UniformBounded;
    rig.request = ArtifactGenerationRequest::new(
        rig.request.range().clone(),
        vec![],
        vec![generators[1].clone()],
        ArtifactFailurePolicy::RequireAll,
    )
    .unwrap();

    let result = rig
        .service
        .generate(rig.request, ArtifactGenerationContext::default())
        .await
        .expect("bounded difference map should succeed");
    let ArtifactOutcome::Available { artifact, .. } = &result.outcomes[0] else {
        panic!("bounded difference map should be available");
    };
    let manifest = &artifact.manifest;
    assert_eq!(manifest.source_frame_count(), 367);
    assert_eq!(manifest.analyzed_frame_count(), 120);
    // 247 source frames were dropped by sampling. The single referenced frame does
    // not make the other 119 analyzed frames omitted evidence.
    assert_eq!(manifest.omitted_frame_count(), 247);
    assert_eq!(manifest.selected_frame_ids().len(), 1);

    let temporal_vision::ParameterValue::Object(values) = manifest
        .parameters()
        .get("analysis_sampling")
        .expect("a sampled analysis must disclose its sampling")
    else {
        panic!("analysis_sampling must be an object");
    };
    assert_eq!(
        values["source_frame_count"],
        temporal_vision::ParameterValue::Unsigned(manifest.source_frame_count())
    );
    assert_eq!(
        values["analyzed_frame_count"],
        temporal_vision::ParameterValue::Unsigned(manifest.analyzed_frame_count())
    );
}

/// Defect 2: production always attaches source provenance, so an exhaustive run
/// reached the same code path as a sampled one and claimed `uniform_bounded`.
#[tokio::test]
async fn exhaustive_analysis_manifest_claims_no_sampling_mode() {
    let mut rig =
        rig_with_transition_and_frame_count(4, false, false, ArtifactWorkLimits::default());
    let mut generators = rig.request.generators().to_vec();
    let ArtifactGeneratorRequest::DifferenceMap(difference) = &mut generators[1] else {
        panic!("default second generator is difference map");
    };
    difference.sampling = krometrail_core::ArtifactSampling::Exhaustive;
    rig.request = ArtifactGenerationRequest::new(
        rig.request.range().clone(),
        vec![],
        vec![generators[1].clone()],
        ArtifactFailurePolicy::RequireAll,
    )
    .unwrap();

    let result = rig
        .service
        .generate(rig.request, ArtifactGenerationContext::default())
        .await
        .expect("exhaustive difference map should succeed");
    let ArtifactOutcome::Available { artifact, .. } = &result.outcomes[0] else {
        panic!("exhaustive difference map should be available");
    };
    assert_eq!(artifact.manifest.source_frame_count(), 4);
    assert_eq!(artifact.manifest.analyzed_frame_count(), 4);
    assert_eq!(artifact.manifest.omitted_frame_count(), 0);
    assert_eq!(
        artifact.manifest.parameters().get("analysis_sampling"),
        None,
        "an exhaustive run must not claim a sampling mode"
    );
}

#[test]
fn analysis_frame_budget_survives_a_zero_byte_per_frame_divisor() {
    // The non-empty-plan invariant keeps the divisor above zero today. The floor
    // makes the division structurally safe rather than invariant-dependent.
    let limits = ArtifactWorkLimits::default();
    let plan = super::epoch::EpochPlan {
        descriptor: krometrail_core::VisualEpoch {
            index: 0,
            frame_ids: vec![],
            image: PixelDimensions::new(1, 1).unwrap(),
            viewport: PixelDimensions::new(1, 1).unwrap(),
            device_scale_factor: DeviceScaleFactor::new(1.0).unwrap(),
        },
        source_fingerprints: vec![],
        cache_sources: vec![],
        frames: vec![],
        markers: vec![],
        gaps: vec![],
        decoded_bytes: 0,
        source_frame_ids: vec![],
        source_indices: vec![],
        source_range: temporal_vision::TimeRange::new(
            temporal_vision::Timestamp::from_nanos(0),
            temporal_vision::Timestamp::from_nanos(1),
        )
        .unwrap(),
    };
    assert_eq!(
        super::service::analysis_effective_max_frames(&plan, limits).unwrap(),
        limits.max_source_frames.get()
    );
}
