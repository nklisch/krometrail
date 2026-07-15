use temporal_evaluation::{
    EvaluationResultRecord, RESULT_KIND, RESULT_SCHEMA_VERSION, sample_evaluation_result,
    sha256_prefixed,
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
