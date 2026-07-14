use krometrail_core::{ErrorCode, Result, SelectOptionRequest, SelectValue};
use serde_json::{Value, json};

use super::{
    BoundTarget,
    interaction::{ResolvedTarget, send_cdp},
    navigation::OperationCancellation,
    operation_error,
};
use crate::transport::CdpTransport;

const SELECT_OPTION_FUNCTION: &str = "function(kind,value){const options=Array.from(this.options);let option=null;if(kind==='value')option=options.find(o=>o.value===value);else if(kind==='index')option=options[value]||null;else option=options.find(o=>o.text.trim()===value);if(!option)return false;this.value=option.value;option.selected=true;this.dispatchEvent(new Event('input',{bubbles:true}));this.dispatchEvent(new Event('change',{bubbles:true}));return true;}";

pub(super) async fn select_option(
    transport: &dyn CdpTransport,
    bound: &BoundTarget,
    request: &SelectOptionRequest,
    target: &ResolvedTarget,
    cancel: &OperationCancellation,
    generation: u64,
) -> Result<()> {
    let node = target.node(bound.target_id)?;
    let resolved = send_cdp(
        transport,
        bound,
        "DOM.resolveNode",
        json!({"backendNodeId":node.backend_node_id}),
        cancel,
        generation,
    )
    .await?;
    let object_id = resolved
        .pointer("/object/objectId")
        .or_else(|| resolved.pointer("/result/object/objectId"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            operation_error(
                ErrorCode::StaleReference,
                bound.target_id,
                "select element no longer has a runtime object",
            )
        })?;
    let (kind, value) = match &request.value {
        SelectValue::Value(value) => ("value", value.clone().map_or(Value::Null, Value::String)),
        SelectValue::Index(index) => ("index", json!(index.get() - 1)),
        SelectValue::Label(label) => ("label", Value::String(label.as_str().to_owned())),
    };
    let response = send_cdp(
        transport,
        bound,
        "Runtime.callFunctionOn",
        json!({
            "objectId":object_id,
            "functionDeclaration":SELECT_OPTION_FUNCTION,
            "arguments":[{"value":kind},{"value":value}],
            "returnByValue":true,
            "awaitPromise":false,
            "silent":true,
        }),
        cancel,
        generation,
    )
    .await?;
    let matched = response
        .pointer("/result/value")
        .or_else(|| response.pointer("/result/result/value"))
        .and_then(Value::as_bool);
    if matched != Some(true) {
        return Err(operation_error(
            ErrorCode::InvalidInput,
            bound.target_id,
            "select_value_not_matched: no option matches the requested value",
        ));
    }
    Ok(())
}
