#[allow(dead_code)]
mod support;

use temporal_evaluation::{
    ConditionPackager, EvaluationResultRecord, EvaluationStatus, EvidenceAvailability,
    EvidenceReference, EvidenceReferenceKind, RESULT_KIND, RESULT_SCHEMA_VERSION, ScoreInput,
    ThresholdProfile, UncertaintyReason, aggregate_condition, assess_thresholds,
    sample_evaluation_result, score_interpretation, sha256_prefixed,
};

const SAMPLE: &[u8] =
    include_bytes!("../../../docs/evidence/temporal-evaluation/v1/sample-evaluation-result.json");
const SCHEMA: &[u8] =
    include_bytes!("../../../docs/evidence/temporal-evaluation/v1/evaluation-result.schema.json");

#[test]
fn committed_result_is_canonical_schema_backed_and_byte_stable() {
    let result = EvaluationResultRecord::from_canonical_json(SAMPLE).unwrap();
    assert_eq!(result.schema_version, RESULT_SCHEMA_VERSION);
    assert_eq!(result.kind, RESULT_KIND);
    assert_eq!(result.canonical_bytes().unwrap(), SAMPLE);
    assert_eq!(result.digest().unwrap(), sha256_prefixed(SAMPLE));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(SCHEMA)
            .unwrap()
            .get("additionalProperties"),
        Some(&serde_json::Value::Bool(false))
    );
}

#[test]
fn generated_result_matches_the_constructor() {
    let generated = sample_evaluation_result().unwrap();
    let committed = EvaluationResultRecord::from_canonical_json(SAMPLE).unwrap();
    assert_eq!(generated, committed);
    assert_eq!(
        generated.canonical_bytes().unwrap(),
        committed.canonical_bytes().unwrap()
    );
}

#[test]
fn result_boundary_rejects_unknown_unsafe_duplicate_unsorted_and_contradictory_data() {
    let result = sample_evaluation_result().unwrap();

    let mut unknown = serde_json::to_value(&result).unwrap();
    unknown["unexpected"] = serde_json::json!(true);
    assert!(serde_json::from_value::<EvaluationResultRecord>(unknown).is_err());

    let mut unsafe_value = result.clone();
    unsafe_value.trials[0].score.raw_answer_ref = "/tmp/raw-answer".into();
    assert!(unsafe_value.validate().is_err());

    let mut duplicate = result.clone();
    duplicate.trials.push(duplicate.trials[0].clone());
    assert!(duplicate.validate().is_err());

    let mut unsorted = result.clone();
    unsorted.trials[0].evidence_ids.reverse();
    assert!(unsorted.validate().is_err());

    let mut unavailable = result.clone();
    unavailable.trials[0].evidence[0].availability =
        temporal_evaluation::EvidenceAvailability::Evicted;
    assert!(unavailable.validate().is_err());

    let mut contradictory = result;
    contradictory.status = temporal_evaluation::EvaluationStatus::Pass;
    assert!(contradictory.validate().is_err());
}

fn partial_result(
    condition: temporal_evaluation::ConditionPackage,
    evidence_ref: &str,
    trial_id: &str,
) -> EvaluationResultRecord {
    let truth = temporal_evaluation::BenchmarkDefinition::canonical()
        .case("movement-reversal/basic")
        .unwrap()
        .ground_truth
        .clone();
    let score = score_interpretation(ScoreInput {
        trial: &temporal_evaluation::TrialIdentity {
            trial_id: trial_id.into(),
            case_id: "movement-reversal/basic".into(),
            family: temporal_evaluation::CaseFamily::MovementReversal,
            duration_ms: 100,
            repetition: 0,
            condition_id: condition.condition_id,
        },
        package: &condition,
        truth: &truth,
        raw_answer: &support::uncertainty_answer(evidence_ref, UncertaintyReason::MissingSource),
        raw_answer_ref: "partial-result-answer",
    })
    .unwrap();
    assert_eq!(score.status, EvaluationStatus::Inconclusive);
    assert!(
        score
            .accepted_claims
            .iter()
            .any(|claim| claim.evidence_ids == [evidence_ref.to_owned()])
    );
    let profile = ThresholdProfile::canonical();
    let aggregate = aggregate_condition(
        condition.condition_id,
        std::slice::from_ref(&score),
        &profile,
    )
    .unwrap();
    let thresholds = assess_thresholds(
        std::slice::from_ref(&aggregate),
        std::slice::from_ref(&condition),
        &profile,
    )
    .unwrap();
    EvaluationResultRecord::from_scores(
        "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
        temporal_evaluation::EvidenceLayer::DeterministicCi,
        std::slice::from_ref(&condition),
        std::slice::from_ref(&score),
        vec![aggregate],
        thresholds,
    )
    .unwrap()
}

#[test]
fn partially_retained_a_and_b_preserve_retained_source_citations_in_scores_and_results() {
    for unavailable in [
        EvidenceAvailability::Evicted,
        EvidenceAvailability::Corrupt,
        EvidenceAvailability::NotCollected,
    ] {
        let partial = support::interval_with_frame_availability(0, unavailable);
        let a = ConditionPackager::final_screenshot(
            &partial,
            "frame-11",
            EvidenceReference::new(
                format!("current-observation-partial-a-{unavailable:?}"),
                EvidenceReferenceKind::CurrentObservation,
                Some(support::digest(format!(
                    "current-observation-partial-a-{unavailable:?}"
                ))),
                EvidenceAvailability::Retained,
            )
            .unwrap(),
        )
        .unwrap();
        let b = ConditionPackager::uniform_storyboard(&partial).unwrap();

        for (condition, evidence_ref, trial_id) in [
            (a, "frame-11", "partial-a-result"),
            (b, "frame-1", "partial-b-result"),
        ] {
            let result = partial_result(condition, evidence_ref, trial_id);
            let source_trace = result.trials[0]
                .evidence
                .iter()
                .find(|trace| trace.id == evidence_ref)
                .unwrap();
            assert_eq!(source_trace.availability, EvidenceAvailability::Retained);
            assert_eq!(
                result.trials[0]
                    .evidence
                    .iter()
                    .find(|trace| trace.id == "frame-0")
                    .unwrap()
                    .availability,
                unavailable
            );
            let bytes = result.canonical_bytes().unwrap();
            assert_eq!(
                EvaluationResultRecord::from_canonical_json(&bytes).unwrap(),
                result
            );
        }
    }
}

#[test]
fn partially_retained_packages_reject_unavailable_source_citations() {
    let partial = support::interval_with_frame_availability(0, EvidenceAvailability::Evicted);
    let a = ConditionPackager::final_screenshot(
        &partial,
        "frame-11",
        EvidenceReference::new(
            "current-observation-unavailable-citation",
            EvidenceReferenceKind::CurrentObservation,
            Some(support::digest("current-observation-unavailable-citation")),
            EvidenceAvailability::Retained,
        )
        .unwrap(),
    )
    .unwrap();
    let b = ConditionPackager::uniform_storyboard(&partial).unwrap();

    for condition in [a, b] {
        let truth = temporal_evaluation::BenchmarkDefinition::canonical()
            .case("movement-reversal/basic")
            .unwrap()
            .ground_truth
            .clone();
        assert!(
            score_interpretation(ScoreInput {
                trial: &temporal_evaluation::TrialIdentity {
                    trial_id: format!("unavailable-citation/{}", condition.condition_id),
                    case_id: "movement-reversal/basic".into(),
                    family: temporal_evaluation::CaseFamily::MovementReversal,
                    duration_ms: 100,
                    repetition: 0,
                    condition_id: condition.condition_id,
                },
                package: &condition,
                truth: &truth,
                raw_answer: &support::uncertainty_answer(
                    "frame-0",
                    UncertaintyReason::MissingSource
                ),
                raw_answer_ref: "unavailable-citation-answer",
            })
            .is_err()
        );
    }
}
