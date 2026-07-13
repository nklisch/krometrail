use serde_json::json;

use super::evaluation::decode_evaluation;
use krometrail_core::{ErrorCode, EvaluationValue, TargetId};

const UUID: &str = "123e4567-e89b-12d3-a456-426614174000";
fn target() -> TargetId {
    TargetId::from_uuid(UUID.parse().unwrap())
}

#[test]
fn evaluation_distinguishes_undefined_null_and_refuses_remote_values() {
    assert_eq!(
        decode_evaluation(&json!({"result":{"type":"undefined"}}), target()).unwrap(),
        EvaluationValue::Undefined
    );
    assert_eq!(
        decode_evaluation(&json!({"result":{"type":"object","value":null}}), target()).unwrap(),
        EvaluationValue::Json(json!(null))
    );
    let error = decode_evaluation(
        &json!({"result":{"type":"object","objectId":"private"}}),
        target(),
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::EvaluationFailed);
    assert!(!error.message.as_str().contains("private"));
}

#[test]
fn evaluation_refuses_exceptions_and_oversized_values() {
    let error = decode_evaluation(
        &json!({"exceptionDetails":{"text":"private stack"}}),
        target(),
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::EvaluationFailed);
    assert!(!error.message.as_str().contains("private stack"));
    let oversized = "x".repeat((1 << 20) + 1);
    let error = decode_evaluation(
        &json!({"result":{"type":"string","value":oversized}}),
        target(),
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::EvaluationFailed);
}
