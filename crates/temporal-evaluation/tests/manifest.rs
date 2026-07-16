use std::fs;
use std::path::PathBuf;

use temporal_evaluation::{
    BrowserAvailability, BrowserProduct, EvaluationStatus, EvidenceAvailability, OutputIdentity,
    RunFailureCode, RunManifest, canonical_json, run_manifest_schema, sample_manifest,
};

const SAMPLE_BYTES: &[u8] =
    include_bytes!("../../../docs/evidence/temporal-evaluation/v1/sample-manifest.json");
const SCHEMA_PATH: &str = "../../docs/evidence/temporal-evaluation/v1/run-manifest.schema.json";

#[test]
fn committed_sample_is_the_canonical_manifest_contract() {
    let manifest = RunManifest::from_canonical_json(SAMPLE_BYTES)
        .expect("committed run manifest sample must be canonical and valid");
    assert_eq!(manifest, sample_manifest());
    assert_eq!(manifest.canonical_bytes().unwrap(), SAMPLE_BYTES);
    assert_eq!(
        manifest.digest().unwrap(),
        sample_manifest().digest().unwrap()
    );
    assert_ne!(manifest.input_digest().unwrap(), manifest.digest().unwrap());
}

#[test]
fn generated_manifest_schema_matches_the_committed_schema() {
    let mut expected = serde_json::to_vec_pretty(&run_manifest_schema()).unwrap();
    expected.push(b'\n');
    let committed = fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(SCHEMA_PATH))
        .expect("generated run manifest schema must be committed");
    assert_eq!(committed, expected);
}

#[test]
fn capture_stride_identity_has_strict_bounds_in_validation_and_schema() {
    let mut minimum = sample_manifest();
    minimum.krometrail.capture_config.every_nth_frame = 1;
    assert!(minimum.validate().is_ok());

    let mut maximum = sample_manifest();
    maximum.krometrail.capture_config.every_nth_frame = 60;
    assert!(maximum.validate().is_ok());

    for invalid in [0, 61] {
        let mut manifest = sample_manifest();
        manifest.krometrail.capture_config.every_nth_frame = invalid;
        assert!(
            manifest.validate().is_err(),
            "stride {invalid} must be rejected"
        );
    }

    let schema = serde_json::to_value(run_manifest_schema()).unwrap();
    let stride_schema = &schema["$defs"]["CaptureConfigIdentity"]["properties"]["every_nth_frame"];
    assert_eq!(stride_schema["type"], "integer");
    assert_eq!(stride_schema["minimum"], 1);
    assert_eq!(stride_schema["maximum"], 60);
}

#[test]
fn status_and_dependency_contradictions_are_rejected() {
    let mut blocked_without_reason = sample_manifest();
    blocked_without_reason.status = EvaluationStatus::Blocked;
    assert!(blocked_without_reason.validate().is_err());

    let mut incomplete_failure = sample_manifest();
    incomplete_failure.status = EvaluationStatus::Fail;
    incomplete_failure.failure = Some(temporal_evaluation::FailureRecord {
        code: temporal_evaluation::RunFailureCode::Threshold,
        phase: "contract".into(),
        reason: "not complete".into(),
        recovery: "collect the required rows".into(),
        retryable: false,
    });
    assert!(incomplete_failure.validate().is_err());

    let mut wrong_fixture = sample_manifest();
    wrong_fixture.fixture.root_relative_path = "tests/fixtures/browser/other".into();
    assert!(wrong_fixture.validate().is_err());

    let mut wrong_hash = sample_manifest();
    wrong_hash.benchmark_definition.sha256 =
        "sha256:1111111111111111111111111111111111111111111111111111111111111111".into();
    assert!(wrong_hash.validate().is_err());
}

#[test]
fn unknown_fields_and_unsafe_machine_details_are_rejected() {
    let mut unknown = serde_json::from_slice::<serde_json::Value>(SAMPLE_BYTES).unwrap();
    unknown["unexpected"] = serde_json::json!(true);
    assert!(
        RunManifest::from_canonical_json(serde_json::to_string(&unknown).unwrap().as_bytes())
            .is_err()
    );

    let mut unsafe_manifest = sample_manifest();
    unsafe_manifest.non_claims[0] = "https://example.test/private/page-body".into();
    assert!(unsafe_manifest.validate().is_err());

    let mut unsafe_path = sample_manifest();
    unsafe_path.fixture.root_relative_path = "/home/operator/fixture".into();
    assert!(unsafe_path.validate().is_err());

    let mut raw_adapter_error = sample_manifest();
    raw_adapter_error.non_claims[0] = "CDP adapter error: websocket payload".into();
    assert!(raw_adapter_error.validate().is_err());
}

#[test]
fn canonical_numbers_are_finite_and_normalize_negative_zero() {
    let value = serde_json::json!({"negative_zero": -0.0, "integer": 7});
    let bytes = canonical_json(&value).unwrap();
    assert_eq!(
        String::from_utf8(bytes).unwrap(),
        "{\n  \"integer\": 7,\n  \"negative_zero\": 0\n}\n"
    );
}

#[test]
fn observed_inputs_and_retained_claim_evidence_are_required_for_decisive_runs() {
    let mut capture = sample_manifest();
    capture.run.threshold_profile = "capture-v1".into();
    capture.run.repetitions = 30;
    capture.browser = BrowserAvailability::Observed {
        product: BrowserProduct::Chromium,
        product_version: "123.0".into(),
        protocol_version: "1.3".into(),
        revision: "123456".into(),
        capability_id: "browser-get-version".into(),
    };
    capture.artifact.output_ids = vec![OutputIdentity {
        id: "frame-1".into(),
        sha256: Some(
            "sha256:0000000000000000000000000000000000000000000000000000000000000000".into(),
        ),
        availability: EvidenceAvailability::Retained,
    }];
    capture.rows[0].artifact_ids = vec!["frame-1".into()];
    capture.rows[0].accepted_claims = vec![temporal_evaluation::AcceptedClaim {
        claim_id: "claim-1".into(),
        evidence_ids: vec!["frame-1".into()],
    }];
    capture.rows[0].retention_state = temporal_evaluation::RetentionState::Retained;
    assert!(capture.validate().is_ok());

    let canonical = capture.canonical_bytes().unwrap();
    let input_digest = capture.input_digest().unwrap();
    let mut changed_stride = capture.clone();
    changed_stride.krometrail.capture_config.every_nth_frame = 60;
    assert!(changed_stride.validate().is_ok());
    assert_ne!(changed_stride.canonical_bytes().unwrap(), canonical);
    assert_ne!(changed_stride.input_digest().unwrap(), input_digest);
    assert_eq!(
        changed_stride.rows[0].accepted_claims,
        capture.rows[0].accepted_claims
    );
    assert_eq!(
        changed_stride.artifact.output_ids,
        capture.artifact.output_ids
    );

    let mut missing_browser = capture.clone();
    missing_browser.browser = BrowserAvailability::NotRequired;
    assert!(missing_browser.validate().is_err());

    let mut duplicate_trial = capture.clone();
    duplicate_trial
        .run
        .ordered_trials
        .push(duplicate_trial.run.ordered_trials[0].clone());
    assert!(duplicate_trial.validate().is_err());

    let mut unsorted_dimensions = capture;
    unsorted_dimensions.scoring.dimension_ids.reverse();
    assert!(unsorted_dimensions.validate().is_err());
}

#[test]
fn blocked_model_and_optional_linux_chromium_states_remain_explicit() {
    let mut blocked = sample_manifest();
    blocked.run.threshold_profile = "interpretation-v1".into();
    blocked.run.repetitions = 10;
    blocked.run.order_policy = temporal_evaluation::MatrixOrder::SeededFisherYates;
    blocked.browser = BrowserAvailability::Observed {
        product: BrowserProduct::Chromium,
        product_version: "123.0".into(),
        protocol_version: "1.3".into(),
        revision: "123456".into(),
        capability_id: "browser-get-version".into(),
    };
    blocked.model = temporal_evaluation::ModelAvailability::Blocked {
        reason: "operator authorization is unavailable".into(),
        recovery: "authorize the selected model before running".into(),
    };
    blocked.status = EvaluationStatus::Blocked;
    blocked.rows[0].status = EvaluationStatus::Blocked;
    blocked.rows[0].failure = Some(temporal_evaluation::FailureRecord {
        code: RunFailureCode::Authorization,
        phase: "model".into(),
        reason: "operator authorization is unavailable".into(),
        recovery: "authorize the selected model before running".into(),
        retryable: false,
    });
    blocked.failure = Some(temporal_evaluation::FailureRecord {
        code: RunFailureCode::Authorization,
        phase: "model".into(),
        reason: "operator authorization is unavailable".into(),
        recovery: "authorize the selected model before running".into(),
        retryable: false,
    });
    assert!(blocked.validate().is_ok());

    let mut skipped = sample_manifest();
    skipped.run.optional_configuration = true;
    skipped.status = EvaluationStatus::Skipped;
    skipped.browser = BrowserAvailability::Skipped {
        product: BrowserProduct::Chromium,
        reason: "optional browser is unavailable".into(),
        recovery: "install the optional browser".into(),
    };
    skipped.failure = Some(temporal_evaluation::FailureRecord {
        code: RunFailureCode::OptionalUnavailable,
        phase: "browser".into(),
        reason: "optional browser is unavailable".into(),
        recovery: "install the optional browser".into(),
        retryable: false,
    });
    skipped.rows[0].status = EvaluationStatus::Skipped;
    skipped.rows[0].failure = Some(temporal_evaluation::FailureRecord {
        code: RunFailureCode::OptionalUnavailable,
        phase: "browser".into(),
        reason: "optional browser is unavailable".into(),
        recovery: "install the optional browser".into(),
        retryable: false,
    });
    assert!(skipped.validate().is_ok());

    let mut wrong_row_failure = skipped.clone();
    wrong_row_failure.rows[0].failure.as_mut().unwrap().code = RunFailureCode::Unavailable;
    assert!(wrong_row_failure.validate().is_err());

    for row_status in [
        EvaluationStatus::Pass,
        EvaluationStatus::Fail,
        EvaluationStatus::Inconclusive,
        EvaluationStatus::Blocked,
    ] {
        let mut hidden_non_skipped_row = skipped.clone();
        hidden_non_skipped_row.rows[0].status = row_status;
        if row_status == EvaluationStatus::Pass {
            hidden_non_skipped_row.rows[0].failure = None;
        }
        assert!(
            hidden_non_skipped_row.validate().is_err(),
            "a skipped run must reject a {:?} row",
            row_status
        );
    }
}
