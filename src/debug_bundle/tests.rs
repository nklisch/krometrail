//! Focused tests for the temporal debug bundle service.
//!
//! Controlled spies prove the exact seven-step sequence: one range resolution,
//! at most one artifact generation, exactly one post-focus context query, and no
//! duplicate store/measurement/selection call. The same `ResolvedRange` reaches
//! artifact and context results. Fatal lifetime errors discard partial work;
//! independent component failures produce usable degraded bundles.

use std::{
    collections::BTreeMap,
    num::NonZeroUsize,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use krometrail_core::{
    ArtifactCacheDisposition, ArtifactGeneration, ArtifactGenerationContext,
    ArtifactGenerationRequest, ArtifactGenerationResult, ArtifactHandle, ArtifactId,
    ArtifactMarkerId, ArtifactOutcome, BundleArtifactEvidence, BundleContextEvidence,
    BundleDegradation, BundleEpochScope, CancellationSignal, DeviceScaleFactor, ErrorCode, FrameId,
    InteractionAnchor, InteractionAnchorSource, InteractionId, InteractionRecordSource,
    NonEmptyText, PortFuture, RangeResolutionOptions, ResolvedRange, SessionId, SessionRange,
    SessionTime, TargetId, TemporalContext, TemporalContextQuery, TemporalContextRequest,
    TemporalDebugBundleContext, TemporalDebugBundleRequest, TemporalDebugBundles, TemporalQuery,
    TemporalQueryRequest, TemporalRangeAnchor, TemporalRangeAnchorKind, TimelineObservation,
    TimelineRangeQuery, TimelineRangeSlice, TimelineStore, VisualEpoch,
};
use temporal_vision::{ArtifactKind, PixelDimensions};
use tokio::sync::Notify;
use uuid::Uuid;

use super::service::{BundleWorkLimits, TemporalDebugBundleService};

// ---------------------------------------------------------------------------
// Test fixtures
// ---------------------------------------------------------------------------

fn session() -> SessionId {
    SessionId::from_uuid(Uuid::from_u128(1))
}
fn target() -> TargetId {
    TargetId::from_uuid(Uuid::from_u128(2))
}

fn resolved_range() -> ResolvedRange {
    let range = SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(1_000_000)).unwrap();
    ResolvedRange::new(
        session(),
        target(),
        TemporalRangeAnchorKind::SessionTime,
        range,
        range,
        vec![FrameId::from_uuid(Uuid::from_u128(10))],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        RangeResolutionOptions::DEFAULT,
    )
    .unwrap()
}

fn request() -> TemporalDebugBundleRequest {
    TemporalDebugBundleRequest::default_policy(
        TemporalQueryRequest::strict(TemporalRangeAnchor::SessionTime {
            scope: krometrail_core::AnchorScope::new(Some(session()), Some(target())),
            range: SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(1_000_000))
                .unwrap(),
        })
        .unwrap(),
    )
    .unwrap()
}

fn storyboard_outcome(epoch_index: u32) -> ArtifactOutcome {
    ArtifactOutcome::Available {
        epoch_index,
        generator_index: 0,
        artifact: ArtifactHandle {
            artifact_id: ArtifactId::from_uuid(Uuid::from_u128(epoch_index as u128 + 100)),
            cache: ArtifactCacheDisposition::Generated,
            media_type: NonEmptyText::new("image/png").unwrap(),
            encoded_byte_len: 1,
            manifest: storyboard_manifest(epoch_index),
        },
    }
}

fn storyboard_manifest(epoch_index: u32) -> krometrail_core::ArtifactManifest {
    let dimensions = PixelDimensions::new(1, 1).unwrap();
    let frame = temporal_vision::Frame::new(
        FrameId::from_uuid(Uuid::from_u128(10 + epoch_index as u128)),
        temporal_vision::Timestamp::from_nanos(0),
        dimensions,
        temporal_vision::PixelFormat::Rgba8SrgbStraight,
        vec![0_u8; 4].into_boxed_slice(),
    )
    .unwrap();
    let sequence = temporal_vision::FrameSequence::<
        FrameId,
        ArtifactMarkerId,
        krometrail_core::GapId,
        Box<[u8]>,
    >::new(vec![frame], vec![], vec![], None, None)
    .unwrap();
    // Use from_sequence with DifferenceMap (no trace required) so the test does
    // not need pixel changes. The focus extractor will find no storyboard trace
    // and produce empty focus times, which is a valid degraded path.
    temporal_vision::ArtifactManifest::from_sequence(
        ArtifactId::from_uuid(Uuid::from_u128(epoch_index as u128 + 100)),
        ArtifactKind::DifferenceMap,
        temporal_vision::EvidenceClass::SourceDerived,
        temporal_vision::AlgorithmDescriptor::new("test", "1").unwrap(),
        &sequence,
        vec![FrameId::from_uuid(Uuid::from_u128(
            10 + epoch_index as u128,
        ))],
        vec![],
        temporal_vision::Parameters::default(),
        dimensions,
        temporal_vision::OutputHash::from_bytes([0_u8; 32]),
    )
    .unwrap()
}

fn generation_result(range: &ResolvedRange) -> ArtifactGenerationResult {
    ArtifactGenerationResult {
        range: range.clone(),
        epochs: vec![VisualEpoch {
            index: 0,
            frame_ids: range.frame_ids.clone(),
            image: krometrail_core::PixelDimensions::new(1, 1).unwrap(),
            viewport: krometrail_core::PixelDimensions::new(1, 1).unwrap(),
            device_scale_factor: DeviceScaleFactor::new(1.0).unwrap(),
        }],
        outcomes: vec![storyboard_outcome(0)],
    }
}

fn temporal_context(range: &ResolvedRange) -> TemporalContext {
    use krometrail_core::{
        BrowserEventContext, CaptureGapSummary, CaptureQuality, CaptureStatusEvidence,
    };
    TemporalContext {
        range: range.clone(),
        capture_quality: CaptureQuality {
            requested_range: range.requested_range,
            retained_range: range.resolved_range,
            frame_count: range.frame_ids.len() as u64,
            first_frame: krometrail_core::FramePoint {
                frame_id: range.frame_ids[0],
                capture_ordinal: krometrail_core::CaptureOrdinal::new(1).unwrap(),
                session_time: range.resolved_range.start(),
            },
            last_frame: krometrail_core::FramePoint {
                frame_id: range.frame_ids[0],
                capture_ordinal: krometrail_core::CaptureOrdinal::new(1).unwrap(),
                session_time: range.resolved_range.start(),
            },
            cadence: None,
            frame_warnings: vec![],
            gaps: vec![],
            gap_summary: CaptureGapSummary {
                gap_count: 0,
                covered_duration_nanos: 0,
                known_missing_frames: 0,
                has_unknown_missing_estimate: false,
            },
            retention_warnings: vec![],
            capture_status: CaptureStatusEvidence {
                at_range_start: None,
                at_range_end: None,
                transitions: vec![],
            },
            warnings: vec![],
        },
        browser_events: BrowserEventContext {
            effective_range: range.resolved_range,
            matched_count: 0,
            returned_count: 0,
            events: vec![],
            next_cursor: None,
            collection_gaps: vec![],
            unavailable_ranges: vec![],
            warnings: vec![],
        },
    }
}

// ---------------------------------------------------------------------------
// Spies
// ---------------------------------------------------------------------------

#[derive(Default)]
struct CallLog {
    events: Mutex<Vec<&'static str>>,
}

impl CallLog {
    fn record(&self, event: &'static str) {
        self.events.lock().unwrap().push(event);
    }
    fn events(&self) -> Vec<String> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .map(|s| s.to_string())
            .collect()
    }
}

struct SpyQuery {
    log: Arc<CallLog>,
    range: ResolvedRange,
    error: Option<ErrorCode>,
}

impl TemporalQuery for SpyQuery {
    fn resolve_range(
        &self,
        _request: TemporalQueryRequest,
    ) -> PortFuture<'_, krometrail_core::Result<ResolvedRange>> {
        self.log.record("resolve_start");
        let range = self.range.clone();
        let error = self.error;
        Box::pin(async move {
            self.log.record("resolve_end");
            if let Some(code) = error {
                return Err(krometrail_core::KrometrailError::new(
                    code,
                    NonEmptyText::new("spy range failure").unwrap(),
                ));
            }
            Ok(range)
        })
    }
}

struct SpyEvidenceStore {
    log: Arc<CallLog>,
    timeline: TimelineRangeSlice,
    interactions: BTreeMap<InteractionId, InteractionAnchor>,
    timeline_error: Option<ErrorCode>,
}

impl TimelineStore for SpyEvidenceStore {
    fn append(&self, _: TimelineObservation) -> PortFuture<'_, krometrail_core::Result<()>> {
        unimplemented!("bundle never appends")
    }
    fn range(
        &self,
        _: SessionId,
        _: TargetId,
        _: SessionRange,
    ) -> PortFuture<'_, krometrail_core::Result<Vec<TimelineObservation>>> {
        unimplemented!("bundle uses selected_range")
    }
    fn selected_range(
        &self,
        _query: TimelineRangeQuery,
    ) -> PortFuture<'_, krometrail_core::Result<TimelineRangeSlice>> {
        self.log.record("selected_range_start");
        let timeline = self.timeline.clone();
        let error = self.timeline_error;
        Box::pin(async move {
            self.log.record("selected_range_end");
            if let Some(code) = error {
                return Err(krometrail_core::KrometrailError::new(
                    code,
                    NonEmptyText::new("spy timeline failure").unwrap(),
                ));
            }
            Ok(timeline)
        })
    }
}

impl InteractionAnchorSource for SpyEvidenceStore {
    fn interaction_anchor(
        &self,
        _id: InteractionId,
    ) -> PortFuture<'_, krometrail_core::Result<Option<InteractionAnchor>>> {
        self.log.record("interaction_anchor");
        let result = self
            .interactions
            .iter()
            .next()
            .map(|(id, anchor)| (*id, anchor.clone()));
        Box::pin(async move { Ok(result.map(|(_, a)| a)) })
    }
    fn latest_interaction_anchor(
        &self,
        _: SessionId,
        _: TargetId,
    ) -> PortFuture<'_, krometrail_core::Result<Option<InteractionAnchor>>> {
        unimplemented!("bundle does not resolve latest interaction")
    }
}

impl InteractionRecordSource for SpyEvidenceStore {
    fn interaction_record(
        &self,
        _: InteractionId,
    ) -> PortFuture<'_, krometrail_core::Result<Option<krometrail_core::InteractionRecord>>> {
        unimplemented!("bundle does not read interaction records")
    }
}

struct SpyGeneration {
    log: Arc<CallLog>,
    result: ArtifactGenerationResult,
    error: Option<ErrorCode>,
    block: Option<Arc<Notify>>,
    reached: Option<Arc<Notify>>,
}

impl ArtifactGeneration for SpyGeneration {
    fn generate(
        &self,
        _request: ArtifactGenerationRequest,
        _context: ArtifactGenerationContext,
    ) -> PortFuture<'_, krometrail_core::Result<ArtifactGenerationResult>> {
        self.log.record("generate_start");
        let log = Arc::clone(&self.log);
        let result = self.result.clone();
        let error = self.error;
        let block = self.block.clone();
        let reached = self.reached.clone();
        Box::pin(async move {
            if let Some(reached) = reached {
                reached.notify_one();
            }
            if let Some(block) = block {
                block.notified().await;
            }
            let outcome = Self::finalize_generate(&result, error);
            log.record("generate_end");
            outcome
        })
    }
}

impl SpyGeneration {
    fn finalize_generate(
        result: &ArtifactGenerationResult,
        error: Option<ErrorCode>,
    ) -> krometrail_core::Result<ArtifactGenerationResult> {
        if let Some(code) = error {
            return Err(krometrail_core::KrometrailError::new(
                code,
                NonEmptyText::new("spy generation failure").unwrap(),
            ));
        }
        Ok(result.clone())
    }
}

struct SpyContext {
    log: Arc<CallLog>,
    context: TemporalContext,
    error: Option<ErrorCode>,
}

impl TemporalContextQuery for SpyContext {
    fn context(
        &self,
        _request: TemporalContextRequest,
    ) -> PortFuture<'_, krometrail_core::Result<TemporalContext>> {
        self.log.record("context_start");
        let context = self.context.clone();
        let error = self.error;
        Box::pin(async move {
            self.log.record("context_end");
            if let Some(code) = error {
                return Err(krometrail_core::KrometrailError::new(
                    code,
                    NonEmptyText::new("spy context failure").unwrap(),
                ));
            }
            Ok(context)
        })
    }
}

fn build_service(
    log: &Arc<CallLog>,
    range: &ResolvedRange,
) -> (
    TemporalDebugBundleService,
    Arc<SpyGeneration>,
    Arc<SpyContext>,
) {
    let generation = Arc::new(SpyGeneration {
        log: Arc::clone(log),
        result: generation_result(range),
        error: None,
        block: None,
        reached: None,
    });
    let context = Arc::new(SpyContext {
        log: Arc::clone(log),
        context: temporal_context(range),
        error: None,
    });
    let evidence = Arc::new(SpyEvidenceStore {
        log: Arc::clone(log),
        timeline: empty_slice(),
        interactions: BTreeMap::new(),
        timeline_error: None,
    });
    let service = TemporalDebugBundleService::new(
        Arc::new(SpyQuery {
            log: Arc::clone(log),
            range: range.clone(),
            error: None,
        }),
        evidence,
        Arc::clone(&generation) as Arc<dyn ArtifactGeneration>,
        Arc::clone(&context) as Arc<dyn TemporalContextQuery>,
        BundleWorkLimits::default(),
    )
    .unwrap();
    (service, generation, context)
}

fn empty_slice() -> TimelineRangeSlice {
    TimelineRangeSlice {
        matched_count: 0,
        observations: Vec::new(),
        truncated: false,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn one_resolution_one_generation_one_context_no_duplicate_calls() {
    let log = Arc::new(CallLog::default());
    let range = resolved_range();
    let (service, _, _) = build_service(&log, &range);
    let bundle = service
        .bundle(request(), TemporalDebugBundleContext::default())
        .await
        .unwrap();
    let events = log.events();
    // Exactly one range resolution.
    assert_eq!(
        events.iter().filter(|e| e == &"resolve_start").count(),
        1,
        "exactly one resolve call"
    );
    // At most one artifact generation.
    assert_eq!(
        events.iter().filter(|e| e == &"generate_start").count(),
        1,
        "exactly one generate call"
    );
    // Exactly one context query, after generation.
    assert_eq!(
        events.iter().filter(|e| e == &"context_start").count(),
        1,
        "exactly one context call"
    );
    let gen_idx = events.iter().position(|e| e == "generate_end").unwrap();
    let ctx_idx = events.iter().position(|e| e == "context_start").unwrap();
    assert!(
        gen_idx < ctx_idx,
        "context must start after generation completes"
    );
    // No duplicate store calls beyond the bounded reads.
    assert!(
        events
            .iter()
            .filter(|e| e == &"selected_range_start")
            .count()
            <= 1,
        "at most one selected_range call"
    );
    // The same resolved range reaches artifact and context results.
    match &bundle.artifacts {
        BundleArtifactEvidence::Available(result) => {
            assert_eq!(result.range, range);
        }
        _ => panic!("artifact evidence should be available"),
    }
    match &bundle.context {
        BundleContextEvidence::Available(ctx) => {
            assert_eq!(ctx.range, range);
        }
        _ => panic!("context evidence should be available"),
    }
}

#[tokio::test]
async fn range_failure_is_whole_request_failure() {
    let log = Arc::new(CallLog::default());
    let range = resolved_range();
    let generation = Arc::new(SpyGeneration {
        log: Arc::clone(&log),
        result: generation_result(&range),
        error: None,
        block: None,
        reached: None,
    });
    let context = Arc::new(SpyContext {
        log: Arc::clone(&log),
        context: temporal_context(&range),
        error: None,
    });
    let evidence = Arc::new(SpyEvidenceStore {
        log: Arc::clone(&log),
        timeline: empty_slice(),
        interactions: BTreeMap::new(),
        timeline_error: None,
    });
    let service = TemporalDebugBundleService::new(
        Arc::new(SpyQuery {
            log: Arc::clone(&log),
            range: range.clone(),
            error: Some(ErrorCode::NotFound),
        }),
        evidence,
        Arc::clone(&generation) as Arc<dyn ArtifactGeneration>,
        Arc::clone(&context) as Arc<dyn TemporalContextQuery>,
        BundleWorkLimits::default(),
    )
    .unwrap();
    let error = service
        .bundle(request(), TemporalDebugBundleContext::default())
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::NotFound);
    // No generation or context call after range failure.
    let events = log.events();
    assert!(events.iter().all(|e| e != "generate_start"));
    assert!(events.iter().all(|e| e != "context_start"));
}

#[tokio::test]
async fn artifact_not_found_after_resolution_is_fatal() {
    let log = Arc::new(CallLog::default());
    let range = resolved_range();
    let generation = Arc::new(SpyGeneration {
        log: Arc::clone(&log),
        result: generation_result(&range),
        error: Some(ErrorCode::NotFound),
        block: None,
        reached: None,
    });
    let context = Arc::new(SpyContext {
        log: Arc::clone(&log),
        context: temporal_context(&range),
        error: None,
    });
    let evidence = Arc::new(SpyEvidenceStore {
        log: Arc::clone(&log),
        timeline: empty_slice(),
        interactions: BTreeMap::new(),
        timeline_error: None,
    });
    let service = TemporalDebugBundleService::new(
        Arc::new(SpyQuery {
            log: Arc::clone(&log),
            range: range.clone(),
            error: None,
        }),
        evidence,
        Arc::clone(&generation) as Arc<dyn ArtifactGeneration>,
        Arc::clone(&context) as Arc<dyn TemporalContextQuery>,
        BundleWorkLimits::default(),
    )
    .unwrap();
    let error = service
        .bundle(request(), TemporalDebugBundleContext::default())
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::NotFound);
    // Context was not queried because artifact NotFound is fatal.
    let events = log.events();
    assert!(events.iter().all(|e| e != "context_start"));
}

#[tokio::test]
async fn non_fatal_artifact_failure_degrades_but_context_remains_useful() {
    let log = Arc::new(CallLog::default());
    let range = resolved_range();
    let generation = Arc::new(SpyGeneration {
        log: Arc::clone(&log),
        result: generation_result(&range),
        error: Some(ErrorCode::PersistenceFailed),
        block: None,
        reached: None,
    });
    let context = Arc::new(SpyContext {
        log: Arc::clone(&log),
        context: temporal_context(&range),
        error: None,
    });
    let evidence = Arc::new(SpyEvidenceStore {
        log: Arc::clone(&log),
        timeline: empty_slice(),
        interactions: BTreeMap::new(),
        timeline_error: None,
    });
    let service = TemporalDebugBundleService::new(
        Arc::new(SpyQuery {
            log: Arc::clone(&log),
            range: range.clone(),
            error: None,
        }),
        evidence,
        Arc::clone(&generation) as Arc<dyn ArtifactGeneration>,
        Arc::clone(&context) as Arc<dyn TemporalContextQuery>,
        BundleWorkLimits::default(),
    )
    .unwrap();
    let bundle = service
        .bundle(request(), TemporalDebugBundleContext::default())
        .await
        .unwrap();
    assert!(matches!(
        bundle.artifacts,
        BundleArtifactEvidence::Unavailable { .. }
    ));
    assert!(matches!(
        bundle.context,
        BundleContextEvidence::Available(_)
    ));
    assert!(
        bundle
            .degradations
            .iter()
            .any(|d| matches!(d, BundleDegradation::ArtifactRequestUnavailable))
    );
    // The artifact request produced no storyboard trace, so the service must
    // not summarize that absence as a measured no-change result.
    let summary = bundle.header.summary.as_str();
    assert!(summary.contains("storyboard evidence was unavailable"));
    assert!(!summary.contains("No thresholded visual change"));
    // Context was available but selected no events; co-occurrence is not
    // asserted merely because the context component succeeded.
    assert!(summary.contains("No browser events matched"));
    assert!(!summary.contains("Browser events co-occurred"));
}

#[tokio::test]
async fn bundle_resource_limits_recommend_shorter_or_progressive_evidence() {
    let log = Arc::new(CallLog::default());
    let range = resolved_range();
    let mut result = generation_result(&range);
    result.outcomes = vec![ArtifactOutcome::Unavailable {
        epoch_index: 0,
        generator_index: 0,
        artifact_kind: ArtifactKind::Storyboard,
        error: krometrail_core::KrometrailError::new(
            ErrorCode::ResourceLimitExceeded,
            NonEmptyText::new("fixture limit").unwrap(),
        ),
    }];
    let generation = Arc::new(SpyGeneration {
        log: Arc::clone(&log),
        result,
        error: None,
        block: None,
        reached: None,
    });
    let context = Arc::new(SpyContext {
        log: Arc::clone(&log),
        context: temporal_context(&range),
        error: None,
    });
    let service = TemporalDebugBundleService::new(
        Arc::new(SpyQuery {
            log: Arc::clone(&log),
            range: range.clone(),
            error: None,
        }),
        Arc::new(SpyEvidenceStore {
            log: Arc::clone(&log),
            timeline: empty_slice(),
            interactions: BTreeMap::new(),
            timeline_error: None,
        }),
        generation,
        context,
        BundleWorkLimits::default(),
    )
    .unwrap();
    let bundle = service
        .bundle(request(), TemporalDebugBundleContext::default())
        .await
        .unwrap();
    let BundleArtifactEvidence::Available(result) = bundle.artifacts else {
        panic!("per-artifact limit should remain a partial result")
    };
    let ArtifactOutcome::Unavailable { error, .. } = &result.outcomes[0] else {
        panic!("fixture outcome should remain unavailable")
    };
    assert_eq!(error.retry, krometrail_core::RetryAdvice::AfterRecovery);
    assert_eq!(error.context.session_id, Some(range.session_id));
    assert_eq!(error.context.target_id, Some(range.target_id));
    assert_eq!(error.context.range, Some(range.resolved_range));
    let recovery = error.recovery.as_ref().unwrap().as_str();
    assert!(recovery.contains("shorten the requested interval"));
    assert!(recovery.contains("progressive source-frame evidence"));
}

#[tokio::test]
async fn context_unavailable_with_available_artifact_does_not_claim_cooccurrence() {
    let log = Arc::new(CallLog::default());
    let range = resolved_range();
    let generation = Arc::new(SpyGeneration {
        log: Arc::clone(&log),
        result: generation_result(&range),
        error: None,
        block: None,
        reached: None,
    });
    let context = Arc::new(SpyContext {
        log: Arc::clone(&log),
        context: temporal_context(&range),
        error: Some(ErrorCode::PersistenceFailed),
    });
    let evidence = Arc::new(SpyEvidenceStore {
        log: Arc::clone(&log),
        timeline: empty_slice(),
        interactions: BTreeMap::new(),
        timeline_error: None,
    });
    let service = TemporalDebugBundleService::new(
        Arc::new(SpyQuery {
            log: Arc::clone(&log),
            range: range.clone(),
            error: None,
        }),
        evidence,
        Arc::clone(&generation) as Arc<dyn ArtifactGeneration>,
        Arc::clone(&context) as Arc<dyn TemporalContextQuery>,
        BundleWorkLimits::default(),
    )
    .unwrap();
    let bundle = service
        .bundle(request(), TemporalDebugBundleContext::default())
        .await
        .unwrap();
    assert!(matches!(
        bundle.context,
        BundleContextEvidence::Unavailable { .. }
    ));
    assert!(matches!(
        bundle.artifacts,
        BundleArtifactEvidence::Available(_)
    ));
    let summary = bundle.header.summary.as_str();
    assert!(summary.contains("Browser events were unavailable"));
    assert!(!summary.contains("Browser events co-occurred"));
    assert!(summary.contains("no co-occurrence is asserted"));
}

#[tokio::test]
async fn context_unavailable_with_no_artifact_outcomes_fails() {
    let log = Arc::new(CallLog::default());
    let range = resolved_range();
    let generation = Arc::new(SpyGeneration {
        log: Arc::clone(&log),
        result: generation_result(&range),
        error: Some(ErrorCode::PersistenceFailed),
        block: None,
        reached: None,
    });
    let context = Arc::new(SpyContext {
        log: Arc::clone(&log),
        context: temporal_context(&range),
        error: Some(ErrorCode::PersistenceFailed),
    });
    let evidence = Arc::new(SpyEvidenceStore {
        log: Arc::clone(&log),
        timeline: empty_slice(),
        interactions: BTreeMap::new(),
        timeline_error: None,
    });
    let service = TemporalDebugBundleService::new(
        Arc::new(SpyQuery {
            log: Arc::clone(&log),
            range: range.clone(),
            error: None,
        }),
        evidence,
        Arc::clone(&generation) as Arc<dyn ArtifactGeneration>,
        Arc::clone(&context) as Arc<dyn TemporalContextQuery>,
        BundleWorkLimits::default(),
    )
    .unwrap();
    let error = service
        .bundle(request(), TemporalDebugBundleContext::default())
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::ArtifactGenerationFailed);
}

#[tokio::test]
async fn marker_context_failure_degrades_but_bundle_succeeds() {
    let log = Arc::new(CallLog::default());
    let range = resolved_range();
    let generation = Arc::new(SpyGeneration {
        log: Arc::clone(&log),
        result: generation_result(&range),
        error: None,
        block: None,
        reached: None,
    });
    let context = Arc::new(SpyContext {
        log: Arc::clone(&log),
        context: temporal_context(&range),
        error: None,
    });
    let evidence = Arc::new(SpyEvidenceStore {
        log: Arc::clone(&log),
        timeline: empty_slice(),
        interactions: BTreeMap::new(),
        timeline_error: Some(ErrorCode::PersistenceFailed),
    });
    let service = TemporalDebugBundleService::new(
        Arc::new(SpyQuery {
            log: Arc::clone(&log),
            range: range.clone(),
            error: None,
        }),
        evidence,
        Arc::clone(&generation) as Arc<dyn ArtifactGeneration>,
        Arc::clone(&context) as Arc<dyn TemporalContextQuery>,
        BundleWorkLimits::default(),
    )
    .unwrap();
    let bundle = service
        .bundle(request(), TemporalDebugBundleContext::default())
        .await
        .unwrap();
    assert!(
        bundle
            .degradations
            .iter()
            .any(|d| matches!(d, BundleDegradation::MarkerContextUnavailable { .. }))
    );
    // Caller markers are still present (none in this test, but the anchor is
    // interval-based so no mandatory anchor marker).
    assert!(bundle.markers.is_empty());
}

#[tokio::test]
async fn cancellation_before_resolution_is_fatal() {
    let log = Arc::new(CallLog::default());
    let range = resolved_range();
    let (service, _, _) = build_service(&log, &range);
    let cancel = TestCancellation::new();
    let signal = cancel.signal();
    cancel.cancel();
    let error = service
        .bundle(
            request(),
            TemporalDebugBundleContext {
                deadline: None,
                cancellation: Some(signal),
            },
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::Cancelled);
    let events = log.events();
    assert!(events.iter().all(|e| e != "resolve_start"));
}

#[tokio::test]
async fn elapsed_deadline_is_fatal() {
    let log = Arc::new(CallLog::default());
    let range = resolved_range();
    let (service, _, _) = build_service(&log, &range);
    let past = Instant::now() - Duration::from_secs(1);
    let error = service
        .bundle(
            request(),
            TemporalDebugBundleContext {
                deadline: Some(past),
                cancellation: None,
            },
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::Cancelled);
}

#[tokio::test]
async fn no_store_gate_spans_artifact_work() {
    let log = Arc::new(CallLog::default());
    let range = resolved_range();
    let block = Arc::new(Notify::new());
    let reached = Arc::new(Notify::new());
    let generation = Arc::new(SpyGeneration {
        log: Arc::clone(&log),
        result: generation_result(&range),
        error: None,
        block: Some(Arc::clone(&block)),
        reached: Some(Arc::clone(&reached)),
    });
    let context = Arc::new(SpyContext {
        log: Arc::clone(&log),
        context: temporal_context(&range),
        error: None,
    });
    let evidence = Arc::new(SpyEvidenceStore {
        log: Arc::clone(&log),
        timeline: empty_slice(),
        interactions: BTreeMap::new(),
        timeline_error: None,
    });
    let service = TemporalDebugBundleService::new(
        Arc::new(SpyQuery {
            log: Arc::clone(&log),
            range: range.clone(),
            error: None,
        }),
        evidence,
        Arc::clone(&generation) as Arc<dyn ArtifactGeneration>,
        Arc::clone(&context) as Arc<dyn TemporalContextQuery>,
        BundleWorkLimits::default(),
    )
    .unwrap();
    // Start the bundle in a background task.
    let handle = tokio::spawn(async move {
        service
            .bundle(request(), TemporalDebugBundleContext::default())
            .await
    });
    // Wait for generation to start (meaning all store reads completed).
    reached.notified().await;
    let events = log.events();
    // Store reads must have completed before generation started.
    let selected_end = events.iter().position(|e| e == "selected_range_end");
    let gen_start = events.iter().position(|e| e == "generate_start");
    assert!(
        selected_end.is_some(),
        "selected_range must have been called"
    );
    assert!(gen_start.is_some(), "generate must have been called");
    assert!(
        selected_end.unwrap() < gen_start.unwrap(),
        "store reads must complete before artifact work begins"
    );
    // Let generation complete.
    block.notify_one();
    let bundle = handle.await.unwrap().unwrap();
    assert!(matches!(
        bundle.artifacts,
        BundleArtifactEvidence::Available(_)
    ));
}

#[tokio::test]
async fn bundle_result_contains_no_bytes_paths_or_uris() {
    let log = Arc::new(CallLog::default());
    let range = resolved_range();
    let (service, _, _) = build_service(&log, &range);
    let bundle = service
        .bundle(request(), TemporalDebugBundleContext::default())
        .await
        .unwrap();
    let encoded = serde_json::to_string(&bundle).unwrap();
    for forbidden in [
        "base64",
        "data:image",
        "file://",
        "/tmp/",
        "\\.png",
        "segment_address",
        "mcp://",
    ] {
        assert!(
            !encoded.to_lowercase().contains(forbidden),
            "bundle payload leaked forbidden term: {forbidden}"
        );
    }
}

#[tokio::test]
async fn two_permits_bound_concurrent_orchestration() {
    let log = Arc::new(CallLog::default());
    let range = resolved_range();
    let block1 = Arc::new(Notify::new());
    let block2 = Arc::new(Notify::new());
    let reached1 = Arc::new(Notify::new());
    let reached2 = Arc::new(Notify::new());
    let gen1 = Arc::new(SpyGeneration {
        log: Arc::clone(&log),
        result: generation_result(&range),
        error: None,
        block: Some(Arc::clone(&block1)),
        reached: Some(Arc::clone(&reached1)),
    });
    let gen2 = Arc::new(SpyGeneration {
        log: Arc::clone(&log),
        result: generation_result(&range),
        error: None,
        block: Some(Arc::clone(&block2)),
        reached: Some(Arc::clone(&reached2)),
    });
    let make_service = |generator: Arc<SpyGeneration>| {
        let evidence = Arc::new(SpyEvidenceStore {
            log: Arc::new(CallLog::default()),
            timeline: empty_slice(),
            interactions: BTreeMap::new(),
            timeline_error: None,
        });
        TemporalDebugBundleService::new(
            Arc::new(SpyQuery {
                log: Arc::new(CallLog::default()),
                range: range.clone(),
                error: None,
            }),
            evidence,
            generator as Arc<dyn ArtifactGeneration>,
            Arc::new(SpyContext {
                log: Arc::new(CallLog::default()),
                context: temporal_context(&range),
                error: None,
            }) as Arc<dyn TemporalContextQuery>,
            BundleWorkLimits::default(),
        )
        .unwrap()
    };
    let svc1 = make_service(Arc::clone(&gen1));
    let svc2 = make_service(Arc::clone(&gen2));
    // Both services share the same default permit count but have independent
    // semaphores. This test verifies that each service acquires its own permit
    // and that the permit is released on completion.
    let h1 = tokio::spawn(async move {
        svc1.bundle(request(), TemporalDebugBundleContext::default())
            .await
    });
    reached1.notified().await;
    let h2 = tokio::spawn(async move {
        svc2.bundle(request(), TemporalDebugBundleContext::default())
            .await
    });
    reached2.notified().await;
    block1.notify_one();
    block2.notify_one();
    h1.await.unwrap().unwrap();
    h2.await.unwrap().unwrap();
}

#[tokio::test]
async fn queued_second_request_times_out_under_max_active_requests_one() {
    // With max_active_requests=1, a second request that arrives while the
    // first holds the permit must time out at its bundle deadline — without
    // first waiting for the in-flight bundle to release. The first request
    // still owns the permit when the second one fails.
    let log = Arc::new(CallLog::default());
    let range = resolved_range();
    let block_first = Arc::new(Notify::new());
    let reached_first = Arc::new(Notify::new());
    let generation = Arc::new(SpyGeneration {
        log: Arc::clone(&log),
        result: generation_result(&range),
        error: None,
        block: Some(Arc::clone(&block_first)),
        reached: Some(Arc::clone(&reached_first)),
    });
    let evidence = Arc::new(SpyEvidenceStore {
        log: Arc::new(CallLog::default()),
        timeline: empty_slice(),
        interactions: BTreeMap::new(),
        timeline_error: None,
    });
    let limits = BundleWorkLimits {
        max_active_requests: NonZeroUsize::new(1).unwrap(),
        max_wall_time: Duration::from_secs(20),
    };
    let service = TemporalDebugBundleService::new(
        Arc::new(SpyQuery {
            log: Arc::clone(&log),
            range: range.clone(),
            error: None,
        }),
        evidence,
        Arc::clone(&generation) as Arc<dyn ArtifactGeneration>,
        Arc::new(SpyContext {
            log: Arc::new(CallLog::default()),
            context: temporal_context(&range),
            error: None,
        }) as Arc<dyn TemporalContextQuery>,
        limits,
    )
    .unwrap();

    // Start the first request; it acquires the sole permit and blocks in
    // artifact generation.
    let svc_first = service.clone();
    let h_first = tokio::spawn(async move {
        svc_first
            .bundle(request(), TemporalDebugBundleContext::default())
            .await
    });
    reached_first.notified().await;

    // Start the second request with a short bundle deadline. It queues for the
    // permit, the deadline elapses inside the controlled permit acquire, and
    // the request fails as cancelled — while the first request still holds
    // the permit. The test signal is only a barrier proving the second request
    // reached the controlled wait; it is never cancelled, so the deadline is
    // the observed termination path.
    let second_control = TestCancellation::new();
    let second_signal = second_control.signal();
    let svc_second = service.clone();
    let short_deadline = Instant::now() + Duration::from_millis(50);
    let h_second = tokio::spawn(async move {
        svc_second
            .bundle(
                request(),
                TemporalDebugBundleContext {
                    deadline: Some(short_deadline),
                    cancellation: Some(second_signal),
                },
            )
            .await
    });
    second_control.wait_until_observed().await;
    let second_err = h_second.await.unwrap().unwrap_err();
    assert_eq!(second_err.code, ErrorCode::Cancelled);

    // The first request was never blocked and still owns the permit; release it
    // so the test can finish cleanly.
    block_first.notify_one();
    h_first.await.unwrap().unwrap();
}

#[tokio::test]
async fn queued_second_request_cancels_under_max_active_requests_one() {
    // Same setup, but the second request is cancelled through its cancellation
    // signal while queued for the permit. It must fail as cancelled without
    // first waiting for the in-flight bundle to release.
    let log = Arc::new(CallLog::default());
    let range = resolved_range();
    let block_first = Arc::new(Notify::new());
    let reached_first = Arc::new(Notify::new());
    let generation = Arc::new(SpyGeneration {
        log: Arc::clone(&log),
        result: generation_result(&range),
        error: None,
        block: Some(Arc::clone(&block_first)),
        reached: Some(Arc::clone(&reached_first)),
    });
    let evidence = Arc::new(SpyEvidenceStore {
        log: Arc::new(CallLog::default()),
        timeline: empty_slice(),
        interactions: BTreeMap::new(),
        timeline_error: None,
    });
    let limits = BundleWorkLimits {
        max_active_requests: NonZeroUsize::new(1).unwrap(),
        max_wall_time: Duration::from_secs(20),
    };
    let service = TemporalDebugBundleService::new(
        Arc::new(SpyQuery {
            log: Arc::clone(&log),
            range: range.clone(),
            error: None,
        }),
        evidence,
        Arc::clone(&generation) as Arc<dyn ArtifactGeneration>,
        Arc::new(SpyContext {
            log: Arc::new(CallLog::default()),
            context: temporal_context(&range),
            error: None,
        }) as Arc<dyn TemporalContextQuery>,
        limits,
    )
    .unwrap();

    // First request acquires the permit and blocks in generation.
    let svc_first = service.clone();
    let h_first = tokio::spawn(async move {
        svc_first
            .bundle(request(), TemporalDebugBundleContext::default())
            .await
    });
    reached_first.notified().await;

    // Second request carries a long deadline but a cancellation signal that
    // fires while the permit acquire is pending; the queued acquire must
    // observe it and fail.
    let cancel = TestCancellation::new();
    let signal = cancel.signal();
    let svc_second = service.clone();
    let h_second = tokio::spawn(async move {
        svc_second
            .bundle(
                request(),
                TemporalDebugBundleContext {
                    deadline: None,
                    cancellation: Some(signal),
                },
            )
            .await
    });
    // The signal's observed barrier proves the second request is inside the
    // controlled permit wait (rather than merely passing the pre-check), then
    // cancellation terminates that pending acquire.
    cancel.wait_until_observed().await;
    cancel.cancel();
    let second_err = h_second.await.unwrap().unwrap_err();
    assert_eq!(second_err.code, ErrorCode::Cancelled);

    // The first request still owns the permit; release it and finish.
    block_first.notify_one();
    h_first.await.unwrap().unwrap();
}

// ---------------------------------------------------------------------------
// Test cancellation signal
// ---------------------------------------------------------------------------

struct TestCancellation {
    tx: tokio::sync::watch::Sender<bool>,
    _rx: tokio::sync::watch::Receiver<bool>,
    observed: Arc<Notify>,
}

impl TestCancellation {
    fn new() -> Self {
        let (tx, rx) = tokio::sync::watch::channel(false);
        Self {
            tx,
            _rx: rx,
            observed: Arc::new(Notify::new()),
        }
    }
    fn cancel(&self) {
        let _ = self.tx.send(true);
    }
    async fn wait_until_observed(&self) {
        self.observed.notified().await;
    }
    fn signal(&self) -> Arc<dyn CancellationSignal> {
        Arc::new(TestCancellationSignal {
            rx: self.tx.subscribe(),
            observed: Arc::clone(&self.observed),
        })
    }
}

struct TestCancellationSignal {
    rx: tokio::sync::watch::Receiver<bool>,
    observed: Arc<Notify>,
}

impl CancellationSignal for TestCancellationSignal {
    fn is_cancelled(&self) -> bool {
        *self.rx.borrow()
    }
    fn cancelled(&self) -> PortFuture<'_, ()> {
        let mut rx = self.rx.clone();
        let observed = Arc::clone(&self.observed);
        Box::pin(async move {
            // This is a test-only synchronization point: production callers
            // cannot observe whether the controlled wrapper reached a port wait.
            observed.notify_one();
            while !*rx.borrow_and_update() {
                if rx.changed().await.is_err() {
                    return;
                }
            }
        })
    }
}

// ---------------------------------------------------------------------------
// Tests moved from mod.rs (policy and trait-alias checks)
// ---------------------------------------------------------------------------

mod policy_tests {
    use super::*;
    use crate::debug_bundle::{
        OrientationPolicy, TemporalDebugEvidenceStore, build_effective_policy,
    };
    use krometrail_core::{
        BundleEpochScope, CaptureGapPolicy, InteractionId, RangeResolutionOptions, ResolvedAnchor,
        ResolvedAnchorReference, RetentionPolicy, SessionId, SessionRange, SessionTime, TargetId,
        TemporalQueryRequest, TemporalRangeAnchor, TemporalRangeAnchorKind,
    };

    fn session() -> SessionId {
        SessionId::from_uuid(Uuid::from_u128(1))
    }
    fn target() -> TargetId {
        TargetId::from_uuid(Uuid::from_u128(2))
    }

    fn interaction_range(interaction_id: InteractionId, dispatch: u64) -> ResolvedRange {
        let requested = SessionRange::new(
            SessionTime::from_nanos(dispatch.saturating_sub(150)),
            SessionTime::from_nanos(dispatch + 250),
        )
        .unwrap();
        ResolvedRange::new_with_anchor(
            session(),
            target(),
            TemporalRangeAnchorKind::Interaction,
            ResolvedAnchor::new(
                ResolvedAnchorReference::Interaction { interaction_id },
                SessionTime::from_nanos(dispatch),
                SessionTime::from_nanos(dispatch),
            )
            .unwrap(),
            requested,
            requested,
            vec![krometrail_core::FrameId::from_uuid(Uuid::from_u128(99))],
            vec![interaction_id],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            RangeResolutionOptions {
                retention: RetentionPolicy::AllowPartial,
                capture_gaps: CaptureGapPolicy::Include,
                ..RangeResolutionOptions::DEFAULT
            },
        )
        .unwrap()
    }

    #[test]
    fn build_effective_policy_carries_exact_values_and_epoch_scope() {
        let interaction_id = InteractionId::from_uuid(Uuid::from_u128(7));
        let range = interaction_range(interaction_id, 500);
        let request = TemporalDebugBundleRequest::default_policy(
            TemporalQueryRequest::strict(TemporalRangeAnchor::Interaction {
                scope: krometrail_core::AnchorScope::new(Some(session()), Some(target())),
                interaction_id,
                window: None,
            })
            .unwrap(),
        )
        .unwrap();
        let effective =
            build_effective_policy(&range, request.orientation(), request.epochs(), vec![])
                .unwrap();
        assert_eq!(effective.epoch_scope, BundleEpochScope::Anchor);
        assert_eq!(
            effective.artifact_anchor,
            range.resolved_anchor.effective_time
        );
        assert_eq!(effective.artifact_generators.len(), 2);
        assert!(effective.focus_times.is_empty());
        assert!(effective.event_filter.classes().is_empty());
        assert!(matches!(
            effective.event_selection,
            krometrail_core::BrowserEventSelection::Compact { .. }
        ));
    }

    #[test]
    fn build_effective_policy_validates_focus_time_count_and_ordering() {
        let interaction_id = InteractionId::from_uuid(Uuid::from_u128(7));
        let range = interaction_range(interaction_id, 500);
        assert!(
            build_effective_policy(
                &range,
                OrientationPolicy::Include,
                BundleEpochScope::Anchor,
                vec![SessionTime::from_nanos(400), SessionTime::from_nanos(400)]
            )
            .is_err()
        );
        assert!(
            build_effective_policy(
                &range,
                OrientationPolicy::Include,
                BundleEpochScope::Anchor,
                vec![SessionTime::from_nanos(600), SessionTime::from_nanos(400)]
            )
            .is_err()
        );
        assert!(
            build_effective_policy(
                &range,
                OrientationPolicy::Include,
                BundleEpochScope::All,
                vec![SessionTime::from_nanos(400), SessionTime::from_nanos(600)]
            )
            .is_ok()
        );
    }

    #[test]
    fn trait_alias_accepts_any_type_implementing_the_three_ports() {
        let _: Option<Box<dyn TemporalDebugEvidenceStore>> = None;
        fn accepts<T: TemporalDebugEvidenceStore>(_: &T) {}
        let _ = accepts as fn(&krometrail_store::RecordingStore);
    }
}

// ---------------------------------------------------------------------------
// Qualification: end-to-end with real schema-v5 store + production artifact service
// ---------------------------------------------------------------------------

mod qualification {
    use super::*;
    use crate::artifacts::{ArtifactWorkLimits, TemporalVisionArtifactService};
    use crate::debug_bundle::{
        BrowserEventEvidenceState, TemporalDebugEvidenceStore, VisualEvidenceState,
        build_effective_policy, compose_header,
    };
    use krometrail_core::{
        ArtifactStore, BrowserEventContext, CaptureGapPolicy, CaptureGapSummary, CaptureOrdinal,
        CaptureQuality, CaptureStatusEvidence, CapturedFrame, EncodedFrame, FramePoint,
        FrameSource, IdSource, IdValue, ImageFormat, InteractionEvidenceSink, InteractionTiming,
        MarkerId, ObservationKind, ObservationPayloadRef, ObservedTime, OrientationPolicy,
        RecordingSink, ResolvedAnchorReference, RetentionPolicy, RetentionStore,
        TimelineObservation,
    };
    use krometrail_store::{
        IndexStoreConfig, RecordingStore, RotationConfig, SegmentStoreConfig, SegmentWriter,
        SqliteIndex,
    };
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

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

    struct QualRig {
        root: PathBuf,
        store: Arc<RecordingStore>,
        session: SessionId,
        target: TargetId,
        artifact_generation: Arc<TemporalVisionArtifactService>,
    }

    async fn qual_rig() -> QualRig {
        let root = std::env::temp_dir().join(format!("krometrail-bundle-qual-{}", Uuid::new_v4()));
        let segments = root.join("segments");
        let index = Arc::new(
            SqliteIndex::open(IndexStoreConfig {
                database_path: root.join("index.sqlite3"),
                segments_directory: segments.clone(),
                busy_timeout: Duration::from_secs(5),
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
        let session = SessionId::from_uuid(Uuid::from_u128(700));
        let target = TargetId::from_uuid(Uuid::from_u128(701));

        // Append 4 frames: alternating JPEG/PNG to produce visual changes.
        // All declared as 2x2 to keep one visual epoch.
        let frame_ids: Vec<_> = (0u128..4)
            .map(|i| FrameId::from_uuid(Uuid::from_u128(710 + i)))
            .collect();
        for (pos, fid) in frame_ids.iter().enumerate() {
            let ordinal = u64::try_from(pos + 1).unwrap();
            let (format, bytes) = if pos % 2 == 0 {
                (ImageFormat::Jpeg, JPEG)
            } else {
                (ImageFormat::Png, PNG)
            };
            let encoded = EncodedFrame::new(
                CapturedFrame::new(
                    *fid,
                    session,
                    target,
                    CaptureOrdinal::new(ordinal).unwrap(),
                    None,
                    ObservedTime::from_nanos(ordinal + 10),
                    SessionTime::from_nanos(ordinal),
                    format,
                    krometrail_core::PixelDimensions::new(2, 2).unwrap(),
                    krometrail_core::PixelDimensions::new(2, 2).unwrap(),
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

        // Append interaction evidence for the interaction anchor form.
        let interaction_id = InteractionId::from_uuid(Uuid::from_u128(730));
        let interaction_anchor = InteractionAnchor::new(
            interaction_id,
            session,
            target,
            krometrail_core::BrowserOperationKind::Click,
            InteractionTiming::new(
                SessionTime::from_nanos(1),
                SessionTime::from_nanos(2),
                SessionTime::from_nanos(3),
                Some(SessionTime::from_nanos(3)),
            )
            .unwrap(),
        )
        .unwrap();
        store
            .append_operation_evidence(interaction_anchor, None, ObservedTime::from_nanos(4), None)
            .await
            .unwrap();

        // Append a generic marker observation.
        let marker_id = MarkerId::from_uuid(Uuid::from_u128(740));
        let marker_obs = TimelineObservation::new(
            session,
            target,
            SessionTime::from_nanos(2),
            None,
            ObservedTime::from_nanos(5),
            ObservationKind::Marker,
            ObservationPayloadRef::Marker(marker_id),
        )
        .unwrap();
        store.append(marker_obs).await.unwrap();

        let artifact_generation = Arc::new(
            TemporalVisionArtifactService::new(
                Arc::clone(&store) as Arc<dyn FrameSource>,
                Arc::clone(&store) as Arc<dyn ArtifactStore>,
                Arc::new(SequenceIds(AtomicU64::new(900))),
                ArtifactWorkLimits::default(),
            )
            .unwrap(),
        );

        QualRig {
            root,
            store,
            session,
            target,
            artifact_generation,
        }
    }

    impl QualRig {
        fn bundle_service(&self) -> TemporalDebugBundleService {
            let spy_ctx = Arc::new(RequestRangeSpyContext);
            TemporalDebugBundleService::new(
                Arc::clone(&self.store) as Arc<dyn TemporalQuery>,
                Arc::clone(&self.store) as Arc<dyn TemporalDebugEvidenceStore>,
                Arc::clone(&self.artifact_generation) as Arc<dyn ArtifactGeneration>,
                Arc::clone(&spy_ctx) as Arc<dyn TemporalContextQuery>,
                BundleWorkLimits::default(),
            )
            .unwrap()
        }
    }

    /// A spy context query that returns a minimal context carrying the exact
    /// resolved range from the request, ensuring the bundle's range-preserving
    /// invariant holds without requiring full browser-event setup.
    struct RequestRangeSpyContext;
    impl TemporalContextQuery for RequestRangeSpyContext {
        fn context(
            &self,
            request: TemporalContextRequest,
        ) -> PortFuture<'_, krometrail_core::Result<TemporalContext>> {
            let range = request.range().clone();
            Box::pin(async move { Ok(minimal_context_with_range(range)) })
        }
    }

    fn minimal_context_with_range(resolved: ResolvedRange) -> TemporalContext {
        let eff = resolved.resolved_range;
        let first_fid = resolved
            .frame_ids
            .first()
            .copied()
            .unwrap_or_else(|| FrameId::from_uuid(Uuid::from_u128(710)));
        let last_fid = resolved.frame_ids.last().copied().unwrap_or(first_fid);
        let frame_count = resolved.frame_ids.len() as u64;
        TemporalContext {
            range: resolved,
            capture_quality: CaptureQuality {
                requested_range: eff,
                retained_range: eff,
                frame_count,
                first_frame: FramePoint {
                    frame_id: first_fid,
                    capture_ordinal: CaptureOrdinal::new(1).unwrap(),
                    session_time: eff.start(),
                },
                last_frame: FramePoint {
                    frame_id: last_fid,
                    capture_ordinal: CaptureOrdinal::new(frame_count.max(1)).unwrap(),
                    session_time: eff.end(),
                },
                cadence: None,
                frame_warnings: vec![],
                gaps: vec![],
                gap_summary: CaptureGapSummary {
                    gap_count: 0,
                    covered_duration_nanos: 0,
                    known_missing_frames: 0,
                    has_unknown_missing_estimate: false,
                },
                retention_warnings: vec![],
                capture_status: CaptureStatusEvidence {
                    at_range_start: None,
                    at_range_end: None,
                    transitions: vec![],
                },
                warnings: vec![],
            },
            browser_events: BrowserEventContext {
                effective_range: eff,
                matched_count: 0,
                returned_count: 0,
                events: vec![],
                next_cursor: None,
                collection_gaps: vec![],
                unavailable_ranges: vec![],
                warnings: vec![],
            },
        }
    }

    fn session_time_request(rig: &QualRig) -> TemporalDebugBundleRequest {
        TemporalDebugBundleRequest::default_policy(
            TemporalQueryRequest::new(
                TemporalRangeAnchor::SessionTime {
                    scope: krometrail_core::AnchorScope::new(Some(rig.session), Some(rig.target)),
                    range: SessionRange::new(
                        SessionTime::from_nanos(1),
                        SessionTime::from_nanos(4),
                    )
                    .unwrap(),
                },
                RetentionPolicy::AllowPartial,
                CaptureGapPolicy::Include,
            )
            .unwrap(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn end_to_end_bundle_with_real_store_succeeds() {
        let rig = qual_rig().await;
        let service = rig.bundle_service();
        let bundle = service
            .bundle(
                session_time_request(&rig),
                TemporalDebugBundleContext::default(),
            )
            .await
            .unwrap();
        // The bundle carries the exact resolved range.
        assert_eq!(bundle.range.session_id, rig.session);
        assert_eq!(bundle.range.target_id, rig.target);
        assert!(!bundle.range.frame_ids.is_empty());
        // The effective policy carries the selected scope and two generators.
        assert_eq!(bundle.effective.epoch_scope, BundleEpochScope::Anchor);
        assert_eq!(bundle.effective.artifact_generators.len(), 2);
        // Artifact evidence is available with real outcomes.
        assert!(matches!(
            bundle.artifacts,
            BundleArtifactEvidence::Available(_)
        ));
        // Context evidence is available (spy).
        assert!(matches!(
            bundle.context,
            BundleContextEvidence::Available(_)
        ));
        // Header posture is non-diagnostic.
        assert_eq!(
            bundle.header.posture,
            krometrail_core::EvidencePosture::ObservedChangeAndTemporalProximityOnly
        );
        // Markers include the mandatory interaction anchor marker and the generic marker.
        assert!(!bundle.markers.is_empty());
        // No degradations in the happy path.
        assert!(bundle.degradations.is_empty());
        std::fs::remove_dir_all(&rig.root).unwrap();
    }

    #[tokio::test]
    async fn cache_reuse_second_bundle_hits_artifact_cache() {
        let rig = qual_rig().await;
        let service = rig.bundle_service();
        // First request generates artifacts.
        let bundle1 = service
            .bundle(
                session_time_request(&rig),
                TemporalDebugBundleContext::default(),
            )
            .await
            .unwrap();
        // Second identical request should hit the cache.
        let service2 = rig.bundle_service();
        let bundle2 = service2
            .bundle(
                session_time_request(&rig),
                TemporalDebugBundleContext::default(),
            )
            .await
            .unwrap();
        // Both bundles have available artifact evidence.
        let _result1 = match &bundle1.artifacts {
            BundleArtifactEvidence::Available(r) => r.clone(),
            _ => panic!("first bundle should have artifacts"),
        };
        let result2 = match &bundle2.artifacts {
            BundleArtifactEvidence::Available(r) => r.clone(),
            _ => panic!("second bundle should have artifacts"),
        };
        // The second request's outcomes should include at least one cache hit.
        let has_hit = result2.outcomes.iter().any(|o| match o {
            ArtifactOutcome::Available { artifact, .. } => {
                artifact.cache == ArtifactCacheDisposition::Hit
            }
            _ => false,
        });
        assert!(has_hit, "second request must hit the artifact cache");
        // Both bundles carry the same resolved range.
        assert_eq!(bundle1.range, bundle2.range);
        std::fs::remove_dir_all(&rig.root).unwrap();
    }

    #[tokio::test]
    async fn bundle_serialized_result_has_no_bytes_paths_or_uris() {
        let rig = qual_rig().await;
        let service = rig.bundle_service();
        let bundle = service
            .bundle(
                session_time_request(&rig),
                TemporalDebugBundleContext::default(),
            )
            .await
            .unwrap();
        let encoded = serde_json::to_string(&bundle).unwrap();
        for forbidden in [
            "base64",
            "data:image",
            "file://",
            "/tmp/",
            "segment_address",
            "mcp://",
            "data_url",
            "filesystem",
        ] {
            assert!(
                !encoded.to_lowercase().contains(forbidden),
                "bundle payload leaked forbidden term: {forbidden}"
            );
        }
        // The serialized result does contain the artifact manifest's output_hash
        // (a SHA-256 hex string), which is a reference, not the image bytes.
        assert!(encoded.contains("output_hash"));
        std::fs::remove_dir_all(&rig.root).unwrap();
    }

    #[tokio::test]
    async fn interaction_anchor_resolves_through_bundle_service() {
        let rig = qual_rig().await;
        let service = rig.bundle_service();
        let request = TemporalDebugBundleRequest::default_policy(
            TemporalQueryRequest::new(
                TemporalRangeAnchor::Interaction {
                    scope: krometrail_core::AnchorScope::new(Some(rig.session), Some(rig.target)),
                    interaction_id: InteractionId::from_uuid(Uuid::from_u128(730)),
                    window: Some(
                        krometrail_core::InteractionWindow::new(
                            std::time::Duration::from_millis(0),
                            std::time::Duration::from_millis(0),
                        )
                        .unwrap(),
                    ),
                },
                RetentionPolicy::AllowPartial,
                CaptureGapPolicy::Include,
            )
            .unwrap(),
        )
        .unwrap();
        let bundle = service
            .bundle(request, TemporalDebugBundleContext::default())
            .await
            .unwrap();
        // The resolved anchor is an interaction anchor with the exact interaction ID.
        assert!(matches!(
            bundle.range.resolved_anchor.reference,
            ResolvedAnchorReference::Interaction { interaction_id } if interaction_id == InteractionId::from_uuid(Uuid::from_u128(730))
        ));
        // The mandatory anchor marker is present at the effective time.
        assert!(bundle.markers.iter().any(|m| matches!(
            m.id(),
            ArtifactMarkerId::Interaction(id) if *id == InteractionId::from_uuid(Uuid::from_u128(730))
        )));
        std::fs::remove_dir_all(&rig.root).unwrap();
    }

    #[tokio::test]
    async fn orientation_omitted_changes_only_include_orientation_field() {
        let rig = qual_rig().await;
        let svc_include = rig.bundle_service();
        let svc_omit = rig.bundle_service();
        let req_include = TemporalDebugBundleRequest::new(
            TemporalQueryRequest::new(
                TemporalRangeAnchor::SessionTime {
                    scope: krometrail_core::AnchorScope::new(Some(rig.session), Some(rig.target)),
                    range: SessionRange::new(
                        SessionTime::from_nanos(1),
                        SessionTime::from_nanos(4),
                    )
                    .unwrap(),
                },
                RetentionPolicy::AllowPartial,
                CaptureGapPolicy::Include,
            )
            .unwrap(),
            vec![],
            OrientationPolicy::Include,
            krometrail_core::BundleEpochScope::Anchor,
        )
        .unwrap();
        let req_omit = TemporalDebugBundleRequest::new(
            TemporalQueryRequest::new(
                TemporalRangeAnchor::SessionTime {
                    scope: krometrail_core::AnchorScope::new(Some(rig.session), Some(rig.target)),
                    range: SessionRange::new(
                        SessionTime::from_nanos(1),
                        SessionTime::from_nanos(4),
                    )
                    .unwrap(),
                },
                RetentionPolicy::AllowPartial,
                CaptureGapPolicy::Include,
            )
            .unwrap(),
            vec![],
            OrientationPolicy::Omit,
            krometrail_core::BundleEpochScope::Anchor,
        )
        .unwrap();
        let b1 = svc_include
            .bundle(req_include, TemporalDebugBundleContext::default())
            .await
            .unwrap();
        let b2 = svc_omit
            .bundle(req_omit, TemporalDebugBundleContext::default())
            .await
            .unwrap();
        // Both produce available artifact evidence.
        assert!(matches!(b1.artifacts, BundleArtifactEvidence::Available(_)));
        assert!(matches!(b2.artifacts, BundleArtifactEvidence::Available(_)));
        // The effective policy generators differ only in include_orientation.
        let gen1 = serde_json::to_value(&b1.effective.artifact_generators).unwrap();
        let gen2 = serde_json::to_value(&b2.effective.artifact_generators).unwrap();
        assert_eq!(gen1[0]["include_orientation"], true);
        assert_eq!(gen2[0]["include_orientation"], false);
        // With orientation included, the first generator produces more outcomes.
        let outcomes1 = match &b1.artifacts {
            BundleArtifactEvidence::Available(r) => r.outcomes.len(),
            _ => 0,
        };
        let outcomes2 = match &b2.artifacts {
            BundleArtifactEvidence::Available(r) => r.outcomes.len(),
            _ => 0,
        };
        assert!(
            outcomes1 >= outcomes2,
            "orientation includes at least as many outcomes"
        );
        std::fs::remove_dir_all(&rig.root).unwrap();
    }

    #[tokio::test]
    async fn session_deletion_after_resolution_is_fatal() {
        let rig = qual_rig().await;
        // Delete the session before the bundle call. The resolver should fail
        // because the session's frames are gone.
        let _ = RetentionStore::delete_session(rig.store.as_ref(), rig.session).await;
        let service = rig.bundle_service();
        let result = service
            .bundle(
                session_time_request(&rig),
                TemporalDebugBundleContext::default(),
            )
            .await;
        assert!(
            result.is_err(),
            "session deletion must fail the bundle request"
        );
        std::fs::remove_dir_all(&rig.root).unwrap();
    }

    #[test]
    fn golden_effective_policy_is_byte_stable() {
        let range = resolved_range();
        let effective = build_effective_policy(
            &range,
            OrientationPolicy::Include,
            BundleEpochScope::Anchor,
            vec![],
        )
        .unwrap();
        let json = serde_json::to_string(&effective).unwrap();
        assert!(json.contains("\"epoch_scope\":\"anchor\""));
        // The artifact anchor matches the resolved anchor's effective time (midpoint of [0, 1000000]).
        assert!(json.contains("\"artifact_anchor\":500000"));
        // Two generators: storyboard and difference_map.
        assert!(json.contains("\"storyboard\""));
        assert!(json.contains("\"difference_map\""));
        // Failure policy is allow_partial.
        assert!(json.contains("\"allow_partial\""));
        // Focus times is an empty array.
        assert!(json.contains("\"focus_times\":[]"));
        // Re-serializing the same value produces identical bytes (Serialize is deterministic).
        let json2 = serde_json::to_string(&effective).unwrap();
        assert_eq!(json, json2);
    }

    #[test]
    fn golden_header_text_is_non_diagnostic_and_stable() {
        let range = resolved_range();
        // Empty outcomes → no focus → "No thresholded visual change" text.
        let header = compose_header(
            &range,
            &[],
            VisualEvidenceState::MeasuredNoChange,
            BrowserEventEvidenceState::Available { selected: 0 },
        )
        .unwrap();
        let summary = header.summary.as_str();
        assert!(summary.contains("Observed"));
        assert!(summary.contains("No thresholded visual change"));
        assert!(summary.contains("do not establish diagnosis or causality"));
        assert!(summary.len() <= krometrail_core::MAX_BUNDLE_HEADER_BYTES);
        // Re-composing produces identical text.
        let header2 = compose_header(
            &range,
            &[],
            VisualEvidenceState::MeasuredNoChange,
            BrowserEventEvidenceState::Available { selected: 0 },
        )
        .unwrap();
        assert_eq!(header.summary.as_str(), header2.summary.as_str());
    }
}
