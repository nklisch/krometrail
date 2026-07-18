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

pub(crate) fn generated_input_schema(schema: schemars::Schema) -> Result<Arc<JsonObject>> {
    object_schema(
        serde_json::to_value(schema)
            .map_err(|_| schema_error("generated operation schema could not be serialized"))?,
    )
}

pub(crate) fn object_schema(value: Value) -> Result<Arc<JsonObject>> {
    match dereference_local_schema(value)? {
        Value::Object(object) if object.get("type") == Some(&Value::String("object".into())) => {
            Ok(Arc::new(object))
        }
        _ => Err(schema_error("MCP tool schema root must be an object")),
    }
}

fn dereference_local_schema(root: Value) -> Result<Value> {
    let mut stack = Vec::new();
    let mut resolved = resolve_value(&root, &root, &mut stack)?;
    if let Value::Object(object) = &mut resolved {
        object.remove("$defs");
        object.remove("definitions");
    }
    Ok(resolved)
}

fn resolve_value(root: &Value, value: &Value, stack: &mut Vec<String>) -> Result<Value> {
    match value {
        Value::Array(values) => values
            .iter()
            .map(|value| resolve_value(root, value, stack))
            .collect::<Result<Vec<_>>>()
            .map(Value::Array),
        Value::Object(object) => {
            if let Some(reference) = object.get("$ref").and_then(Value::as_str) {
                let resolved = resolve_ref(root, reference, stack)?;
                return merge_reference_site(root, resolved, object, stack);
            }
            let mut resolved = JsonObject::new();
            for (key, value) in object {
                if key != "$defs" && key != "definitions" {
                    resolved.insert(key.clone(), resolve_value(root, value, stack)?);
                }
            }
            Ok(Value::Object(resolved))
        }
        _ => Ok(value.clone()),
    }
}

fn resolve_ref(root: &Value, reference: &str, stack: &mut Vec<String>) -> Result<Value> {
    let pointer = reference
        .strip_prefix('#')
        .ok_or_else(|| schema_error("generated schema contains a non-local reference"))?;
    if !pointer.starts_with("/$defs/") && !pointer.starts_with("/definitions/") {
        return Err(schema_error(
            "generated schema contains an unsupported local reference",
        ));
    }
    if stack.iter().any(|entry| entry == reference) {
        return Err(schema_error("generated schema contains a reference cycle"));
    }
    let target = root
        .pointer(pointer)
        .ok_or_else(|| schema_error("generated schema contains an unresolved reference"))?;
    stack.push(reference.to_owned());
    let resolved = resolve_value(root, target, stack);
    stack.pop();
    resolved
}

fn merge_reference_site(
    root: &Value,
    resolved: Value,
    site: &JsonObject,
    stack: &mut Vec<String>,
) -> Result<Value> {
    let mut annotations = JsonObject::new();
    let mut constraints = JsonObject::new();
    for (key, value) in site {
        if key == "$ref" {
            continue;
        }
        let value = resolve_value(root, value, stack)?;
        if is_annotation(key) {
            annotations.insert(key.clone(), value);
        } else {
            constraints.insert(key.clone(), value);
        }
    }

    if constraints.is_empty() {
        if let Value::Object(mut target) = resolved {
            target.extend(annotations);
            return Ok(Value::Object(target));
        }
        if annotations.is_empty() {
            return Ok(resolved);
        }
    }

    let mut combined = annotations;
    let mut branches = vec![resolved];
    if !constraints.is_empty() {
        branches.push(Value::Object(constraints));
    }
    combined.insert("allOf".into(), Value::Array(branches));
    Ok(Value::Object(combined))
}

fn is_annotation(key: &str) -> bool {
    matches!(
        key,
        "title"
            | "description"
            | "$comment"
            | "default"
            | "examples"
            | "deprecated"
            | "readOnly"
            | "writeOnly"
    )
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

    #[test]
    fn published_operation_schemas_inline_all_local_references() {
        let config = McpConfig::default();
        for kind in BrowserOperationKind::ALL {
            let schema = operation_input_schema(*kind, &config).unwrap();
            assert_no_references(&Value::Object(schema.as_ref().clone()));
        }
    }

    #[test]
    fn published_viewport_schema_matches_runtime_bounds() {
        let config = McpConfig::default();
        let schema = operation_input_schema(BrowserOperationKind::SetViewport, &config).unwrap();
        let metrics = &schema["properties"]["viewport"]["oneOf"][0]["properties"]["metrics"];
        let properties = &metrics["properties"];
        for dimension in ["width", "height"] {
            assert_eq!(properties[dimension]["minimum"], 1);
            assert_eq!(properties[dimension]["maximum"], 10_000);
        }
        assert_eq!(properties["device_scale_factor"]["exclusiveMinimum"], 0.0);
        assert_eq!(properties["device_scale_factor"]["maximum"], 8.0);
    }

    #[test]
    fn published_wait_schema_explains_unscoped_exact_text_semantics() {
        let config = McpConfig::default();
        let schema = operation_input_schema(BrowserOperationKind::Wait, &config).unwrap();
        let schema = Value::Object(schema.as_ref().clone());
        let text = find_tagged_variant(&schema, "condition", "text")
            .expect("wait schema contains the text condition");
        let fields = &text["properties"]["value"]["properties"];
        let locator = fields["locator"]["description"].as_str().unwrap();
        let match_mode = fields["match_mode"]["description"].as_str().unwrap();
        assert!(locator.contains("full document body text"));
        assert!(locator.contains("use a locator"));
        assert!(match_mode.contains("complete text in that scope"));
        assert!(match_mode.contains("contains"));
    }

    #[test]
    fn local_references_preserve_nested_constraints_and_site_annotations() {
        let schema = object_schema(serde_json::json!({
            "$defs": {
                "locator": {
                    "type": "object",
                    "required": ["reference"],
                    "properties": {"reference": {"type": "string"}}
                }
            },
            "type": "object",
            "properties": {
                "locator": {"$ref": "#/$defs/locator", "description": "target"}
            }
        }))
        .unwrap();
        let locator = &schema["properties"]["locator"];
        assert_eq!(locator["description"], "target");
        assert_eq!(locator["properties"]["reference"]["type"], "string");
        assert_eq!(locator["required"], serde_json::json!(["reference"]));
        assert_no_references(&Value::Object(schema.as_ref().clone()));
    }

    #[test]
    fn missing_and_cyclic_local_references_fail_closed() {
        assert!(
            object_schema(serde_json::json!({
                "type": "object",
                "properties": {"bad": {"$ref": "#/$defs/missing"}}
            }))
            .is_err()
        );
        assert!(
            object_schema(serde_json::json!({
                "$defs": {"cycle": {"$ref": "#/$defs/cycle"}},
                "type": "object",
                "properties": {"bad": {"$ref": "#/$defs/cycle"}}
            }))
            .is_err()
        );
    }

    fn assert_no_references(value: &Value) {
        match value {
            Value::Array(values) => values.iter().for_each(assert_no_references),
            Value::Object(object) => {
                assert!(!object.contains_key("$ref"));
                assert!(!object.contains_key("$defs"));
                assert!(!object.contains_key("definitions"));
                object.values().for_each(assert_no_references);
            }
            _ => {}
        }
    }

    fn find_tagged_variant<'a>(value: &'a Value, tag: &str, expected: &str) -> Option<&'a Value> {
        if value
            .pointer(&format!("/properties/{tag}/const"))
            .and_then(Value::as_str)
            == Some(expected)
        {
            return Some(value);
        }
        match value {
            Value::Array(values) => values
                .iter()
                .find_map(|value| find_tagged_variant(value, tag, expected)),
            Value::Object(object) => object
                .values()
                .find_map(|value| find_tagged_variant(value, tag, expected)),
            _ => None,
        }
    }
}
