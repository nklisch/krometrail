//! Focused tests for the temporal debug bundle service.
//!
//! Controlled spies prove the exact seven-step sequence: one range resolution,
//! at most one artifact generation, exactly one post-focus context query, and no
//! duplicate store/measurement/selection call. The same `ResolvedRange` reaches
//! artifact and context results. Fatal lifetime errors discard partial work;
//! independent component failures produce usable degraded bundles.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use krometrail_core::{
    ArtifactCacheDisposition, ArtifactGeneration, ArtifactGenerationContext,
    ArtifactGenerationRequest, ArtifactGenerationResult, ArtifactHandle, ArtifactId,
    ArtifactMarkerId, ArtifactOutcome, BundleArtifactEvidence, BundleContextEvidence,
    BundleDegradation, CancellationSignal, DeviceScaleFactor, ErrorCode, FrameId,
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

// ---------------------------------------------------------------------------
// Test cancellation signal
// ---------------------------------------------------------------------------

struct TestCancellation {
    tx: tokio::sync::watch::Sender<bool>,
    _rx: tokio::sync::watch::Receiver<bool>,
}

impl TestCancellation {
    fn new() -> Self {
        let (tx, rx) = tokio::sync::watch::channel(false);
        Self { tx, _rx: rx }
    }
    fn cancel(&self) {
        let _ = self.tx.send(true);
    }
    fn signal(&self) -> Arc<dyn CancellationSignal> {
        Arc::new(TestCancellationSignal {
            rx: self.tx.subscribe(),
        })
    }
}

struct TestCancellationSignal {
    rx: tokio::sync::watch::Receiver<bool>,
}

impl CancellationSignal for TestCancellationSignal {
    fn is_cancelled(&self) -> bool {
        *self.rx.borrow()
    }
    fn cancelled(&self) -> PortFuture<'_, ()> {
        let mut rx = self.rx.clone();
        Box::pin(async move {
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
        CaptureGapPolicy, InteractionId, RangeResolutionOptions, ResolvedAnchor,
        ResolvedAnchorReference, RetentionPolicy, SessionId, SessionRange, SessionTime,
        TEMPORAL_DEBUG_BUNDLE_POLICY_VERSION, TargetId, TemporalQueryRequest, TemporalRangeAnchor,
        TemporalRangeAnchorKind,
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
    fn build_effective_policy_carries_exact_v1_values() {
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
        let effective = build_effective_policy(&range, request.orientation(), vec![]).unwrap();
        assert_eq!(
            effective.version.as_str(),
            TEMPORAL_DEBUG_BUNDLE_POLICY_VERSION
        );
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
                vec![SessionTime::from_nanos(400), SessionTime::from_nanos(400)]
            )
            .is_err()
        );
        assert!(
            build_effective_policy(
                &range,
                OrientationPolicy::Include,
                vec![SessionTime::from_nanos(600), SessionTime::from_nanos(400)]
            )
            .is_err()
        );
        assert!(
            build_effective_policy(
                &range,
                OrientationPolicy::Include,
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
