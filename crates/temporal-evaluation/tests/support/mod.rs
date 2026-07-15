use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use temporal_evaluation::{
    AnswerRegion, AnswerTruth, ArtifactCacheIdentity, ArtifactEvidenceReference, ArtifactKind,
    CaseFamily, ConditionId, ConditionPackage, ConditionPackager, DimensionOutcome, DimensionScore,
    EvidenceAvailability, EvidenceReference, EvidenceReferenceKind, FailureRecord,
    InterpretationAnswer, Judgment, MotionBehavior, NamedVersion, ProgressiveConditionEvidence,
    ProgressiveRetrievalRecord, RetentionState, RunFailureCode, ScopeIdentity, ScoringDimensionId,
    SourceFrameEvidence, SourceInterval, StateLabel, TemporalBundleEvidence, ThresholdProfile,
    TimeRangeNs, TrialScore, sha256_prefixed,
};

pub const FRAME_COUNT: usize = 12;

pub fn digest(value: impl AsRef<[u8]>) -> String {
    sha256_prefixed(value.as_ref())
}

pub fn hash(value: u8) -> String {
    format!("sha256:{value:0>64}")
}

/// Test-only monotonic time source. Qualification deliberately keeps its readings out of the
/// source identity so incidental clock reads cannot become evidence or alter canonical bytes.
#[derive(Clone, Debug)]
pub struct FakeMonotonicClock {
    calls: Arc<AtomicUsize>,
    next: Arc<AtomicUsize>,
}

impl FakeMonotonicClock {
    pub fn new() -> Self {
        Self {
            calls: Arc::new(AtomicUsize::new(0)),
            next: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn read(&self) -> u64 {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.next.fetch_add(1, Ordering::Relaxed) as u64
    }

    pub fn call_count(&self) -> usize {
        self.calls.load(Ordering::Relaxed)
    }
}

#[derive(Clone, Debug)]
struct SourceRecord {
    id: String,
    capture_ordinal: u64,
    source_time_ns: Option<u64>,
    observed_time_ns: u64,
    session_time_ns: u64,
    encoded_sha256: String,
}

fn source_records() -> Vec<SourceRecord> {
    (0..FRAME_COUNT)
        .map(|index| SourceRecord {
            id: format!("frame-{index}"),
            capture_ordinal: index as u64 + 1,
            source_time_ns: Some(index as u64 * 1_000),
            observed_time_ns: index as u64 * 1_000 + 10_000,
            session_time_ns: index as u64 * 1_000,
            encoded_sha256: hash(index as u8 + 1),
        })
        .collect()
}

/// Build one deterministic source interval after emulating discovery from a filesystem and
/// completion from workers. Both orderings are normalized back to capture order before the
/// existing SourceInterval authority validates them.
pub fn interval_with_orders(
    clock: &FakeMonotonicClock,
    filesystem_order: &[usize],
    completion_order: &[usize],
    extra_clock_reads: usize,
    host_wall_clock_ns: u64,
) -> SourceInterval {
    let records = source_records();
    let mut discovered = Vec::with_capacity(filesystem_order.len());
    for &index in filesystem_order {
        let _ = clock.read();
        discovered.push(records[index].clone());
    }
    for _ in 0..extra_clock_reads {
        let _ = clock.read();
    }
    // The host wall clock is intentionally not an identity input. Keeping the binding makes the
    // qualification test explicit without calling the host clock or serializing this value.
    let _ = host_wall_clock_ns;

    let mut completed = completion_order
        .iter()
        .map(|&position| discovered[position].clone())
        .collect::<Vec<_>>();
    completed.sort_by_key(|record| (record.capture_ordinal, record.id.clone()));
    SourceInterval::new(
        "qualification-interval",
        ScopeIdentity::new("qualification-session", "qualification-target").unwrap(),
        TimeRangeNs::new(0, 11_000).unwrap(),
        TimeRangeNs::new(0, 11_000).unwrap(),
        5_000,
        completed
            .into_iter()
            .map(|record| SourceFrameEvidence {
                id: record.id,
                capture_ordinal: record.capture_ordinal,
                source_time_ns: record.source_time_ns,
                observed_time_ns: record.observed_time_ns,
                session_time_ns: record.session_time_ns,
                encoded_sha256: record.encoded_sha256,
                availability: EvidenceAvailability::Retained,
            })
            .collect(),
        Vec::new(),
        RetentionState::Retained,
    )
    .unwrap()
}

pub fn interval() -> SourceInterval {
    let clock = FakeMonotonicClock::new();
    let order = (0..FRAME_COUNT).collect::<Vec<_>>();
    interval_with_orders(&clock, &order, &order, 0, 0)
}

pub fn interval_with_frame_availability(
    frame_index: usize,
    availability: EvidenceAvailability,
) -> SourceInterval {
    let base = interval();
    let mut frames = base.frames.clone();
    frames[frame_index].availability = availability;
    let gaps = if availability == EvidenceAvailability::Gap {
        vec![
            temporal_evaluation::GapEvidence::new(
                "gap-1",
                frames[frame_index].session_time_ns,
                frames[frame_index].session_time_ns,
                "deterministic qualification gap",
                None,
            )
            .unwrap(),
        ]
    } else {
        Vec::new()
    };
    let retained = frames
        .iter()
        .filter(|frame| frame.availability == EvidenceAvailability::Retained)
        .count();
    let retention = if retained == frames.len() {
        RetentionState::Retained
    } else if retained == 0
        && frames
            .iter()
            .all(|frame| frame.availability == EvidenceAvailability::Evicted)
    {
        RetentionState::Evicted
    } else if retained > 0 {
        RetentionState::PartiallyRetained
    } else {
        RetentionState::Unavailable
    };
    SourceInterval::new(
        base.interval_id,
        base.session_scope,
        base.requested_range,
        base.resolved_range,
        base.anchor_session_time_ns,
        frames,
        gaps,
        retention,
    )
    .unwrap()
}

fn reference(
    id: &str,
    kind: EvidenceReferenceKind,
    availability: EvidenceAvailability,
) -> EvidenceReference {
    let output_hash = match availability {
        EvidenceAvailability::Retained | EvidenceAvailability::Corrupt => {
            Some(digest(format!("output:{id}")))
        }
        EvidenceAvailability::Evicted
        | EvidenceAvailability::NotCollected
        | EvidenceAvailability::Gap => None,
    };
    EvidenceReference::new(id, kind, output_hash, availability).unwrap()
}

fn artifact_authority(kind: ArtifactKind) -> (&'static str, &'static str) {
    match kind {
        ArtifactKind::BeforeDuringAfter | ArtifactKind::ChangeAwareStoryboard => {
            ("temporal-storyboard", "1.1.0")
        }
        ArtifactKind::DifferenceMap => ("temporal-difference-map", "v1"),
        ArtifactKind::RegionFilmstrip => ("region-filmstrip", "1.0.0"),
        ArtifactKind::FinalScreenshot
        | ArtifactKind::UniformStoryboard
        | ArtifactKind::SourceFrame
        | ArtifactKind::TemporalDebugBundle => {
            panic!("qualification only constructs source-derived artifact projections")
        }
    }
}

pub fn artifact(
    interval: &SourceInterval,
    id: &str,
    kind: ArtifactKind,
    selected_frame_ids: Vec<String>,
    availability: EvidenceAvailability,
) -> ArtifactEvidenceReference {
    let (algorithm, version) = artifact_authority(kind);
    ArtifactEvidenceReference {
        output: reference(id, EvidenceReferenceKind::Artifact(kind), availability),
        resolved_range: interval.resolved_range,
        manifest_sha256: digest(format!("manifest:{id}")),
        source_frame_ids: interval.frame_ids(),
        selected_frame_ids,
        gap_ids: interval.gap_ids(),
        algorithm_versions: vec![NamedVersion {
            name: algorithm.into(),
            version: version.into(),
        }],
        cache: ArtifactCacheIdentity {
            cache_schema_version: 1,
            cache_key: digest(format!("cache:{id}")),
            source_fingerprint: digest(format!("sources:{id}")),
            parameter_hash: digest(format!("parameters:{id}")),
            visual_epoch_hash: digest(format!("epoch:{id}")),
            adapter_version: NamedVersion {
                name: "qualification-authority-adapter".into(),
                version: "1".into(),
            },
            generator: NamedVersion {
                name: algorithm.into(),
                version: version.into(),
            },
        },
    }
}

pub fn change_aware_selection() -> Vec<String> {
    [0, 2, 5, 8, 10, 11]
        .into_iter()
        .map(|index| format!("frame-{index}"))
        .collect()
}

pub fn bundle(interval: &SourceInterval) -> TemporalBundleEvidence {
    let selected = change_aware_selection();
    TemporalBundleEvidence {
        bundle: reference(
            "bundle-1",
            EvidenceReferenceKind::Artifact(ArtifactKind::TemporalDebugBundle),
            EvidenceAvailability::Retained,
        ),
        before_during_after: vec![artifact(
            interval,
            "before-during-after-1",
            ArtifactKind::BeforeDuringAfter,
            selected.clone(),
            EvidenceAvailability::Retained,
        )],
        storyboards: vec![artifact(
            interval,
            "storyboard-1",
            ArtifactKind::ChangeAwareStoryboard,
            selected.clone(),
            EvidenceAvailability::Retained,
        )],
        difference_maps: vec![artifact(
            interval,
            "difference-map-1",
            ArtifactKind::DifferenceMap,
            selected,
            EvidenceAvailability::Retained,
        )],
        capture_summary: reference(
            "capture-summary-1",
            EvidenceReferenceKind::CaptureSummary,
            EvidenceAvailability::Retained,
        ),
        context_summary: reference(
            "context-summary-1",
            EvidenceReferenceKind::ContextSummary,
            EvidenceAvailability::Retained,
        ),
        evidence_references: Vec::new(),
    }
}

fn source_reference(interval: &SourceInterval, id: &str) -> EvidenceReference {
    let frame = interval.frame(id).unwrap();
    EvidenceReference::new(
        id,
        EvidenceReferenceKind::SourceFrame,
        Some(frame.encoded_sha256.clone()),
        EvidenceAvailability::Retained,
    )
    .unwrap()
}

pub fn packages() -> Vec<ConditionPackage> {
    let interval = interval();
    let a = ConditionPackager::final_screenshot(
        &interval,
        "frame-11",
        reference(
            "current-observation-1",
            EvidenceReferenceKind::CurrentObservation,
            EvidenceAvailability::Retained,
        ),
    )
    .unwrap();
    let b = ConditionPackager::uniform_storyboard(&interval).unwrap();
    let c = ConditionPackager::change_aware_storyboard(
        &interval,
        vec![artifact(
            &interval,
            "storyboard-change-aware-1",
            ArtifactKind::ChangeAwareStoryboard,
            change_aware_selection(),
            EvidenceAvailability::Retained,
        )],
    )
    .unwrap();
    let d_bundle = bundle(&interval);
    let d = ConditionPackager::temporal_bundle(&interval, d_bundle.clone()).unwrap();
    let e = ConditionPackager::progressive_source(
        &interval,
        ProgressiveConditionEvidence {
            bundle: d_bundle,
            source_retrievals: vec![
                ProgressiveRetrievalRecord {
                    request_id: "source-request-1".into(),
                    requested_frame_ids: vec!["frame-2".into(), "frame-5".into()],
                    returned_frames: vec![
                        source_reference(&interval, "frame-2"),
                        source_reference(&interval, "frame-5"),
                    ],
                    unavailable_frame_ids: Vec::new(),
                },
                ProgressiveRetrievalRecord {
                    request_id: "source-request-2".into(),
                    requested_frame_ids: vec!["frame-8".into()],
                    returned_frames: vec![source_reference(&interval, "frame-8")],
                    unavailable_frame_ids: Vec::new(),
                },
            ],
            region_filmstrip: Some(artifact(
                &interval,
                "region-filmstrip-1",
                ArtifactKind::RegionFilmstrip,
                change_aware_selection(),
                EvidenceAvailability::Retained,
            )),
        },
    )
    .unwrap();
    vec![a, b, c, d, e]
}

pub fn unavailable_retrieval_package() -> ConditionPackage {
    let interval = interval();
    ConditionPackager::progressive_source(
        &interval,
        ProgressiveConditionEvidence {
            bundle: bundle(&interval),
            source_retrievals: vec![ProgressiveRetrievalRecord {
                request_id: "source-request-unavailable".into(),
                requested_frame_ids: vec!["frame-2".into(), "frame-5".into()],
                returned_frames: vec![source_reference(&interval, "frame-2")],
                unavailable_frame_ids: vec!["frame-5".into()],
            }],
            region_filmstrip: None,
        },
    )
    .unwrap()
}

pub fn corrupt_change_aware_package() -> ConditionPackage {
    let interval = interval();
    ConditionPackager::change_aware_storyboard(
        &interval,
        vec![artifact(
            &interval,
            "storyboard-corrupt-1",
            ArtifactKind::ChangeAwareStoryboard,
            change_aware_selection(),
            EvidenceAvailability::Corrupt,
        )],
    )
    .unwrap()
}

pub fn gap_package() -> ConditionPackage {
    let interval = interval_with_frame_availability(5, EvidenceAvailability::Gap);
    ConditionPackager::uniform_storyboard(&interval).unwrap()
}

pub fn partial_eviction_package() -> ConditionPackage {
    let interval = interval_with_frame_availability(0, EvidenceAvailability::Evicted);
    ConditionPackager::uniform_storyboard(&interval).unwrap()
}

pub fn uncertainty_answer(
    evidence_ref: &str,
    reason: temporal_evaluation::UncertaintyReason,
) -> Vec<u8> {
    serde_json::to_vec(&InterpretationAnswer {
        temporary_state: AnswerTruth::Uncertain,
        state_order: vec![StateLabel::Baseline, StateLabel::Unknown],
        affected_region: AnswerRegion::Rect {
            x: 49,
            y: 73,
            width: 480,
            height: 120,
        },
        motion_behavior: MotionBehavior::Uncertain,
        judgment: Judgment::Uncertain,
        uncertainty_reasons: vec![reason],
        evidence_refs: vec![evidence_ref.into()],
    })
    .unwrap()
}

pub fn perfect_answer(evidence_ref: &str) -> Vec<u8> {
    serde_json::to_vec(&InterpretationAnswer {
        temporary_state: AnswerTruth::Yes,
        state_order: vec![StateLabel::Baseline, StateLabel::Changed, StateLabel::Final],
        affected_region: AnswerRegion::Rect {
            x: 49,
            y: 73,
            width: 480,
            height: 120,
        },
        motion_behavior: MotionBehavior::Reversal,
        judgment: Judgment::Defective,
        uncertainty_reasons: Vec::new(),
        evidence_refs: vec![evidence_ref.into()],
    })
    .unwrap()
}

pub fn movement_trial(package: &ConditionPackage) -> temporal_evaluation::TrialIdentity {
    temporal_evaluation::TrialIdentity {
        trial_id: format!("qualification/{}/movement/0", package.condition_id),
        case_id: "movement-reversal/basic".into(),
        family: CaseFamily::MovementReversal,
        duration_ms: 100,
        repetition: 0,
        condition_id: package.condition_id,
    }
}

/// A bounded synthetic score for status/aggregation qualification. It is not a model answer and
/// is never used as hidden truth; result tests attach the deterministic-CI non-claim registry.
pub fn synthetic_score(
    package: &ConditionPackage,
    family: CaseFamily,
    index: u16,
    status: temporal_evaluation::EvaluationStatus,
) -> TrialScore {
    let (case_id, answer) = match family {
        CaseFamily::StableControl => (
            "stable/smooth-panel",
            InterpretationAnswer {
                temporary_state: AnswerTruth::No,
                state_order: vec![StateLabel::IntentionalMotion, StateLabel::Final],
                affected_region: AnswerRegion::Rect {
                    x: 49,
                    y: 73,
                    width: 480,
                    height: 120,
                },
                motion_behavior: MotionBehavior::Monotonic,
                judgment: Judgment::Intentional,
                uncertainty_reasons: Vec::new(),
                evidence_refs: Vec::new(),
            },
        ),
        _ => (
            match family {
                CaseFamily::MovementReversal => "movement-reversal/basic",
                CaseFamily::Flicker => "flicker/visibility",
                CaseFamily::TransientLayout => "layout/width",
                CaseFamily::DomOpaqueMotion => "dom-opaque/path-reversal",
                CaseFamily::StableControl => unreachable!(),
            },
            InterpretationAnswer {
                temporary_state: AnswerTruth::Yes,
                state_order: vec![StateLabel::Baseline, StateLabel::Changed, StateLabel::Final],
                affected_region: AnswerRegion::Rect {
                    x: 49,
                    y: 73,
                    width: 480,
                    height: 120,
                },
                motion_behavior: MotionBehavior::Reversal,
                judgment: Judgment::Defective,
                uncertainty_reasons: Vec::new(),
                evidence_refs: Vec::new(),
            },
        ),
    };
    let dimensions = vec![
        dimension(
            ScoringDimensionId::TransientDefectIdentification,
            if family == CaseFamily::StableControl {
                DimensionOutcome::NotApplicable
            } else {
                DimensionOutcome::Correct
            },
        ),
        dimension(ScoringDimensionId::StateOrder, DimensionOutcome::Correct),
        dimension(
            ScoringDimensionId::AffectedRegion,
            DimensionOutcome::Correct,
        ),
        dimension(
            ScoringDimensionId::MotionBehavior,
            DimensionOutcome::Correct,
        ),
        dimension(
            ScoringDimensionId::GapUncertainty,
            DimensionOutcome::NotApplicable,
        ),
        dimension(
            ScoringDimensionId::StableControlFalsePositive,
            if family == CaseFamily::StableControl {
                DimensionOutcome::Correct
            } else {
                DimensionOutcome::NotApplicable
            },
        ),
    ];
    let (earned_points, possible_points) = (
        dimensions
            .iter()
            .filter(|dimension| dimension.outcome == DimensionOutcome::Correct)
            .count() as u16,
        dimensions
            .iter()
            .filter(|dimension| {
                matches!(
                    dimension.outcome,
                    DimensionOutcome::Correct | DimensionOutcome::Incorrect
                )
            })
            .count() as u16,
    );
    TrialScore {
        trial_id: format!("qualification/{}/{family:?}/{index}", package.condition_id),
        condition_id: package.condition_id,
        package_digest: package.digest.clone(),
        source_interval_digest: package.source_interval_digest.clone(),
        source_frame_tile_count: match package.condition_id {
            ConditionId::AFinalScreenshot => 1,
            ConditionId::BUniformStoryboard => 8,
            _ => 6,
        },
        case_id: case_id.into(),
        answer,
        answer_digest: hash(index as u8 + 40),
        raw_answer_ref: format!("qualification-sidecar-{index}"),
        dimensions,
        accepted_claims: Vec::new(),
        earned_points,
        possible_points,
        status,
        failure: match status {
            temporal_evaluation::EvaluationStatus::Pass => None,
            temporal_evaluation::EvaluationStatus::Fail => Some(FailureRecord {
                code: RunFailureCode::Threshold,
                phase: "qualification".into(),
                reason: "complete synthetic row is below its threshold".into(),
                recovery: "replace the synthetic row".into(),
                retryable: false,
            }),
            temporal_evaluation::EvaluationStatus::Inconclusive => Some(FailureRecord {
                code: RunFailureCode::InsufficientEvidence,
                phase: "qualification".into(),
                reason: "synthetic row has incomplete evidence".into(),
                recovery: "provide complete retained evidence".into(),
                retryable: true,
            }),
            temporal_evaluation::EvaluationStatus::Blocked => Some(FailureRecord {
                code: RunFailureCode::Unavailable,
                phase: "qualification".into(),
                reason: "synthetic row is blocked".into(),
                recovery: "provide the required input".into(),
                retryable: true,
            }),
            temporal_evaluation::EvaluationStatus::Skipped => Some(FailureRecord {
                code: RunFailureCode::OptionalUnavailable,
                phase: "qualification".into(),
                reason: "optional qualification input is unavailable".into(),
                recovery: "run the optional input when available".into(),
                retryable: true,
            }),
        },
    }
}

fn dimension(id: ScoringDimensionId, outcome: DimensionOutcome) -> DimensionScore {
    DimensionScore {
        dimension_id: id,
        outcome,
        observed_value: "qualification".into(),
        expected_value: "qualification".into(),
        evidence_ids: Vec::new(),
        rationale_code: "qualification".into(),
    }
}

pub fn threshold_profile() -> ThresholdProfile {
    ThresholdProfile::canonical()
}
