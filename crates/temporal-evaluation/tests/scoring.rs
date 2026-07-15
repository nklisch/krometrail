use temporal_evaluation::{
    AnswerRegion, AnswerTruth, ConditionId, ConditionPackager, DimensionOutcome,
    EvidenceAvailability, GroundTruthDefinition, InterpretationAnswer, Judgment, MotionBehavior,
    RetentionState, RunFailureCode, ScopeIdentity, ScoreInput, ScoringDimensionId,
    SourceFrameEvidence, SourceInterval, StateLabel, TimeRangeNs, TrialIdentity,
    score_interpretation,
};

fn hash(value: u8) -> String {
    format!("sha256:{value:0>64}")
}

fn interval() -> SourceInterval {
    SourceInterval::new(
        "interval-1",
        ScopeIdentity::new("session-1", "target-1").unwrap(),
        TimeRangeNs::new(0, 11_000).unwrap(),
        TimeRangeNs::new(0, 11_000).unwrap(),
        5_000,
        (0..12)
            .map(|index| SourceFrameEvidence {
                id: format!("frame-{index}"),
                capture_ordinal: index + 1,
                source_time_ns: Some(index * 1_000),
                observed_time_ns: index * 1_000 + 10_000,
                session_time_ns: index * 1_000,
                encoded_sha256: hash(index as u8 + 1),
                availability: EvidenceAvailability::Retained,
            })
            .collect(),
        Vec::new(),
        RetentionState::Retained,
    )
    .unwrap()
}

fn trial(condition_id: ConditionId, case_id: &str) -> TrialIdentity {
    let definition = temporal_evaluation::BenchmarkDefinition::canonical();
    let case = definition.case(case_id).unwrap();
    TrialIdentity {
        trial_id: format!("interpretation:{case_id}/100/{condition_id}/0"),
        case_id: case_id.into(),
        family: case.family,
        duration_ms: 100,
        repetition: 0,
        condition_id,
    }
}

fn answer(evidence_refs: Vec<&str>) -> Vec<u8> {
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
        uncertainty_reasons: vec![],
        evidence_refs: evidence_refs.into_iter().map(str::to_owned).collect(),
    })
    .unwrap()
}

fn movement_truth() -> GroundTruthDefinition {
    temporal_evaluation::BenchmarkDefinition::canonical()
        .case("movement-reversal/basic")
        .unwrap()
        .ground_truth
        .clone()
}

fn uniform_package() -> temporal_evaluation::ConditionPackage {
    ConditionPackager::uniform_storyboard(&interval()).unwrap()
}

#[test]
fn correct_structured_answer_scores_all_decisive_dimensions_and_is_byte_stable() {
    let package = uniform_package();
    let trial = trial(ConditionId::BUniformStoryboard, "movement-reversal/basic");
    let truth = movement_truth();
    let raw = answer(vec!["frame-0"]);
    let first = score_interpretation(ScoreInput {
        trial: &trial,
        package: &package,
        truth: &truth,
        raw_answer: &raw,
        raw_answer_ref: "sidecar-1",
    })
    .unwrap();
    let second = score_interpretation(ScoreInput {
        trial: &trial,
        package: &package,
        truth: &truth,
        raw_answer: &raw,
        raw_answer_ref: "sidecar-1",
    })
    .unwrap();

    assert_eq!(first, second);
    assert_eq!(first.status, temporal_evaluation::EvaluationStatus::Pass);
    assert_eq!(first.earned_points, 4);
    assert_eq!(first.possible_points, 4);
    assert_eq!(first.accepted_claims.len(), 4);
    assert_eq!(first.dimensions.len(), ScoringDimensionId::ALL.len());
    assert_eq!(
        first
            .dimensions
            .iter()
            .map(|dimension| dimension.outcome)
            .collect::<Vec<_>>(),
        vec![
            DimensionOutcome::Correct,
            DimensionOutcome::Correct,
            DimensionOutcome::Correct,
            DimensionOutcome::Correct,
            DimensionOutcome::NotApplicable,
            DimensionOutcome::NotApplicable,
        ]
    );
    assert!(first.failure.is_none());
    assert_eq!(
        first.canonical_bytes().unwrap(),
        second.canonical_bytes().unwrap()
    );
    assert_eq!(
        first.answer_digest,
        temporal_evaluation::sha256_prefixed(&raw)
    );
}

#[test]
fn final_screenshot_is_not_allowed_to_claim_historical_order_or_motion() {
    let interval = interval();
    let package = ConditionPackager::final_screenshot(
        &interval,
        "frame-11",
        temporal_evaluation::EvidenceReference::new(
            "observation-1",
            temporal_evaluation::EvidenceReferenceKind::CurrentObservation,
            Some(hash(200)),
            EvidenceAvailability::Retained,
        )
        .unwrap(),
    )
    .unwrap();
    let raw = answer(vec!["frame-11"]);
    let score = score_interpretation(ScoreInput {
        trial: &trial(ConditionId::AFinalScreenshot, "movement-reversal/basic"),
        package: &package,
        truth: &movement_truth(),
        raw_answer: &raw,
        raw_answer_ref: "sidecar-final",
    })
    .unwrap();
    assert_eq!(score.status, temporal_evaluation::EvaluationStatus::Fail);
    assert_eq!(score.dimensions[0].outcome, DimensionOutcome::Incorrect);
    assert_eq!(score.dimensions[1].outcome, DimensionOutcome::Incorrect);
    assert_eq!(score.dimensions[3].outcome, DimensionOutcome::Incorrect);
}

#[test]
fn a_gap_requires_named_uncertainty_and_never_becomes_negative_visual_evidence() {
    let source = interval();
    let frames = source
        .frames
        .into_iter()
        .enumerate()
        .map(|(index, mut frame)| {
            if index == 5 {
                frame.availability = EvidenceAvailability::Gap;
            }
            frame
        })
        .collect();
    let source = SourceInterval::new(
        source.interval_id,
        source.session_scope,
        source.requested_range,
        source.resolved_range,
        source.anchor_session_time_ns,
        frames,
        vec![
            temporal_evaluation::GapEvidence::new("gap-1", 5_000, 5_000, "capture gap", None)
                .unwrap(),
        ],
        RetentionState::PartiallyRetained,
    )
    .unwrap();
    let package = ConditionPackager::uniform_storyboard(&source).unwrap();
    let answer = serde_json::to_vec(&InterpretationAnswer {
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
        uncertainty_reasons: vec![temporal_evaluation::UncertaintyReason::CaptureGap],
        evidence_refs: vec!["frame-0".into()],
    })
    .unwrap();
    let score = score_interpretation(ScoreInput {
        trial: &trial(ConditionId::BUniformStoryboard, "movement-reversal/basic"),
        package: &package,
        truth: &movement_truth(),
        raw_answer: &answer,
        raw_answer_ref: "sidecar-gap",
    })
    .unwrap();
    assert_eq!(
        score.status,
        temporal_evaluation::EvaluationStatus::Inconclusive
    );
    assert_eq!(score.dimensions[0].outcome, DimensionOutcome::Inconclusive);
    assert_eq!(score.dimensions[1].outcome, DimensionOutcome::Inconclusive);
    assert_eq!(score.dimensions[2].outcome, DimensionOutcome::Correct);
    assert_eq!(score.dimensions[3].outcome, DimensionOutcome::Inconclusive);
    assert_eq!(score.dimensions[4].outcome, DimensionOutcome::Correct);
    assert_eq!(
        score.failure.as_ref().unwrap().code,
        RunFailureCode::CaptureGap
    );
    assert!(
        score
            .failure
            .as_ref()
            .unwrap()
            .recovery
            .contains("recapture")
    );
}

#[test]
fn stable_control_false_positive_is_separate_from_defect_identification() {
    let truth = temporal_evaluation::BenchmarkDefinition::canonical()
        .case("stable/smooth-panel")
        .unwrap()
        .ground_truth
        .clone();
    let package = uniform_package();
    let answer = serde_json::to_vec(&InterpretationAnswer {
        temporary_state: AnswerTruth::No,
        state_order: vec![StateLabel::IntentionalMotion, StateLabel::Final],
        affected_region: AnswerRegion::Rect {
            x: 49,
            y: 73,
            width: 480,
            height: 120,
        },
        motion_behavior: MotionBehavior::Monotonic,
        judgment: Judgment::Defective,
        uncertainty_reasons: vec![],
        evidence_refs: vec!["frame-0".into()],
    })
    .unwrap();
    let score = score_interpretation(ScoreInput {
        trial: &trial(ConditionId::BUniformStoryboard, "stable/smooth-panel"),
        package: &package,
        truth: &truth,
        raw_answer: &answer,
        raw_answer_ref: "sidecar-stable",
    })
    .unwrap();
    assert_eq!(score.dimensions[0].outcome, DimensionOutcome::NotApplicable);
    assert_eq!(score.dimensions[5].outcome, DimensionOutcome::Incorrect);
    assert_eq!(score.status, temporal_evaluation::EvaluationStatus::Fail);
}

#[test]
fn parser_and_boundary_reject_oversized_unknown_or_unsafe_inputs() {
    let package = uniform_package();
    let trial = trial(ConditionId::BUniformStoryboard, "movement-reversal/basic");
    let truth = movement_truth();
    let oversized = vec![b' '; temporal_evaluation::MAX_RAW_ANSWER_BYTES + 1];
    assert!(
        score_interpretation(ScoreInput {
            trial: &trial,
            package: &package,
            truth: &truth,
            raw_answer: &oversized,
            raw_answer_ref: "sidecar-too-large",
        })
        .is_err()
    );

    let mut unknown = serde_json::to_value(
        serde_json::from_slice::<InterpretationAnswer>(&answer(vec!["frame-0"])).unwrap(),
    )
    .unwrap();
    unknown["unexpected"] = serde_json::json!(true);
    let unknown = serde_json::to_vec(&unknown).unwrap();
    assert!(
        score_interpretation(ScoreInput {
            trial: &trial,
            package: &package,
            truth: &truth,
            raw_answer: &unknown,
            raw_answer_ref: "sidecar-unknown",
        })
        .is_err()
    );

    let mut bad_ref_answer =
        serde_json::from_slice::<InterpretationAnswer>(&answer(vec!["frame-0"])).unwrap();
    bad_ref_answer.evidence_refs = vec!["frame-999".into()];
    let bad_ref = serde_json::to_vec(&bad_ref_answer).unwrap();
    assert!(
        score_interpretation(ScoreInput {
            trial: &trial,
            package: &package,
            truth: &truth,
            raw_answer: &bad_ref,
            raw_answer_ref: "sidecar-bad-evidence",
        })
        .is_err()
    );

    assert!(
        score_interpretation(ScoreInput {
            trial: &trial,
            package: &package,
            truth: &truth,
            raw_answer: &answer(vec!["frame-0"]),
            raw_answer_ref: "../unsafe",
        })
        .is_err()
    );
}

#[test]
fn accepted_claims_require_retained_citations() {
    let package = uniform_package();
    let trial = trial(ConditionId::BUniformStoryboard, "movement-reversal/basic");
    let truth = movement_truth();
    let raw = answer(vec![]);
    assert!(
        score_interpretation(ScoreInput {
            trial: &trial,
            package: &package,
            truth: &truth,
            raw_answer: &raw,
            raw_answer_ref: "sidecar-no-citation",
        })
        .is_err()
    );

    let mut unavailable =
        serde_json::from_slice::<InterpretationAnswer>(&answer(vec!["frame-0"])).unwrap();
    unavailable.evidence_refs = vec!["frame-10".into()];
    let unavailable = serde_json::to_vec(&unavailable).unwrap();
    // frame-10 is a known source identity, but is not one of B's retained selected references.
    let score = score_interpretation(ScoreInput {
        trial: &trial,
        package: &package,
        truth: &truth,
        raw_answer: &unavailable,
        raw_answer_ref: "sidecar-unavailable",
    });
    assert!(score.is_err());
}
