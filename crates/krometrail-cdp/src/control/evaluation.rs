use serde_json::{Value, json};

use krometrail_core::{
    BrowserOperationResult, ErrorCode, EvaluationResult, EvaluationValue, ObservationContext,
    ReadOnlyEvaluationRequest, Result,
};

use super::{BoundTarget, PageControl, operation_error, transport_error};
use crate::transport::{CdpTransport, CommandScope};

pub(super) const MAX_EVALUATION_RESULT_BYTES: usize = 1 << 20;

impl PageControl {
    pub(super) async fn evaluate(
        &self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        request: ReadOnlyEvaluationRequest,
        started_at: krometrail_core::SessionTime,
    ) -> Result<BrowserOperationResult> {
        let response = transport
            .send_raw(
                &CommandScope::Session(bound.transport_session.clone()),
                "Runtime.evaluate",
                json!({
                    "expression": request.expression.as_str(),
                    "returnByValue": true,
                    "awaitPromise": request.await_promise,
                    "throwOnSideEffect": true,
                    "silent": true,
                    "timeout": u64::try_from(self.config.evaluation_timeout.as_millis()).unwrap_or(u64::MAX),
                }),
            )
            .await
            .map_err(|error| transport_error(error, ErrorCode::EvaluationFailed, bound.target_id))?;
        let completed_at = self.session_time()?;
        let value = decode_evaluation(&response, bound.target_id)?;
        Ok(BrowserOperationResult::EvaluatePage(EvaluationResult {
            context: ObservationContext::new(
                self.session_id,
                bound.target_id,
                bound.attachment_generation,
                started_at,
                completed_at,
            )?,
            value,
        }))
    }
}

pub(super) fn decode_evaluation(
    response: &Value,
    target_id: krometrail_core::TargetId,
) -> Result<EvaluationValue> {
    if response.get("exceptionDetails").is_some() {
        return Err(operation_error(
            ErrorCode::EvaluationFailed,
            target_id,
            "page evaluation raised an exception or was refused as side-effecting",
        ));
    }
    let result = response.get("result").unwrap_or(response);
    if result.get("exceptionDetails").is_some() {
        return Err(operation_error(
            ErrorCode::EvaluationFailed,
            target_id,
            "page evaluation raised an exception or was refused as side-effecting",
        ));
    }
    let result = result
        .get("result")
        .filter(|nested| nested.is_object())
        .unwrap_or(result);
    if result.get("unserializableValue").is_some() {
        return Err(operation_error(
            ErrorCode::EvaluationFailed,
            target_id,
            "page evaluation returned an unserializable value",
        ));
    }
    if result.get("type").and_then(Value::as_str) == Some("undefined") {
        return Ok(EvaluationValue::Undefined);
    }
    let value = result.get("value").cloned().ok_or_else(|| {
        let message = if result.get("objectId").is_some() {
            "page evaluation returned a remote object instead of a by-value result"
        } else {
            "page evaluation response did not contain a by-value result"
        };
        operation_error(ErrorCode::EvaluationFailed, target_id, message)
    })?;
    let encoded = serde_json::to_vec(&value).map_err(|_| {
        operation_error(
            ErrorCode::EvaluationFailed,
            target_id,
            "page evaluation result could not be serialized",
        )
    })?;
    if encoded.len() > MAX_EVALUATION_RESULT_BYTES {
        return Err(operation_error(
            ErrorCode::EvaluationFailed,
            target_id,
            "page evaluation result exceeds the 1 MiB limit",
        ));
    }
    Ok(EvaluationValue::Json(value))
}
