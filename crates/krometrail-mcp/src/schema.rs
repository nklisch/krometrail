use std::{collections::BTreeSet, sync::Arc};

use krometrail_core::{
    BROWSER_OPERATION_REGISTRY, BrowserOperationKind, ErrorCode, KrometrailError, NonEmptyText,
    Result,
};
use rmcp::model::JsonObject;
use serde_json::Value;

use crate::config::McpConfig;

pub(crate) fn operation_input_schema(
    kind: BrowserOperationKind,
    config: &McpConfig,
) -> Result<Arc<JsonObject>> {
    let mut value = serde_json::to_value(kind.input_schema())
        .map_err(|_| schema_error("generated browser operation schema could not be serialized"))?;
    if kind == BrowserOperationKind::Batch {
        filter_batch_operations(&mut value, config)?;
    }
    object_schema(value)
}

pub(crate) fn type_input_schema<T: schemars::JsonSchema>() -> Result<Arc<JsonObject>> {
    object_schema(
        serde_json::to_value(schemars::schema_for!(T))
            .map_err(|_| schema_error("generated lifecycle schema could not be serialized"))?,
    )
}

pub(crate) fn object_schema(value: Value) -> Result<Arc<JsonObject>> {
    match value {
        Value::Object(object) if object.get("type") == Some(&Value::String("object".into())) => {
            Ok(Arc::new(object))
        }
        _ => Err(schema_error("MCP tool schema root must be an object")),
    }
}

fn filter_batch_operations(schema: &mut Value, config: &McpConfig) -> Result<()> {
    let all: BTreeSet<_> = BROWSER_OPERATION_REGISTRY
        .iter()
        .map(|definition| definition.stable_name)
        .collect();
    let expected: BTreeSet<_> = BROWSER_OPERATION_REGISTRY
        .iter()
        .filter(|definition| definition.batchable && config.is_enabled(definition.capability))
        .map(|definition| definition.stable_name)
        .collect();

    let mut matches = 0usize;
    visit_values_mut(schema, &mut |value| {
        let Value::Object(object) = value else {
            return;
        };
        let Some(Value::Array(branches)) = object.get_mut("oneOf") else {
            return;
        };
        let names: Option<BTreeSet<&str>> = branches
            .iter()
            .map(operation_const)
            .collect::<Option<BTreeSet<_>>>();
        if names.as_ref() != Some(&all) {
            return;
        }
        matches += 1;
        branches
            .retain(|branch| operation_const(branch).is_some_and(|name| expected.contains(name)));
    });

    if matches != 1 {
        return Err(schema_error(
            "generated batch schema did not contain exactly one complete operation union",
        ));
    }
    Ok(())
}

fn visit_values_mut(value: &mut Value, visitor: &mut impl FnMut(&mut Value)) {
    visitor(value);
    match value {
        Value::Array(values) => {
            for value in values {
                visit_values_mut(value, visitor);
            }
        }
        Value::Object(values) => {
            for value in values.values_mut() {
                visit_values_mut(value, visitor);
            }
        }
        _ => {}
    }
}

fn operation_const(branch: &Value) -> Option<&str> {
    branch
        .get("properties")?
        .get("operation")?
        .get("const")?
        .as_str()
}

fn schema_error(message: &'static str) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::Internal,
        NonEmptyText::new(message).expect("static schema error is valid"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use krometrail_core::{
        BrowserEventDetailRequest, CapabilityId, RetrieveSourceFrameRequest,
        TemporalDebugBundleRequest,
    };

    #[test]
    fn generated_temporal_request_schemas_are_object_roots() {
        assert!(type_input_schema::<BrowserEventDetailRequest>().is_ok());
        assert!(type_input_schema::<RetrieveSourceFrameRequest>().is_ok());
        assert!(type_input_schema::<TemporalDebugBundleRequest>().is_ok());
    }

    #[test]
    fn batch_schema_is_filtered_from_the_generated_complete_union() {
        let config = McpConfig::new(vec![CapabilityId::Control]).unwrap();
        let schema = operation_input_schema(BrowserOperationKind::Batch, &config).unwrap();
        let encoded = serde_json::to_string(schema.as_ref()).unwrap();
        for definition in BROWSER_OPERATION_REGISTRY {
            assert_eq!(
                encoded.contains(&format!("\"{}\"", definition.stable_name)),
                definition.batchable,
                "{} batch schema membership",
                definition.stable_name
            );
        }
    }
}
