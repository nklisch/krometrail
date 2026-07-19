use std::{collections::BTreeSet, sync::Arc};

use krometrail_core::{
    BROWSER_OPERATION_REGISTRY, BrowserOperationKind, ErrorCode, KrometrailError, NonEmptyText,
    ResolvedRangeHandleId, Result,
};
use rmcp::model::JsonObject;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use crate::{
    config::McpConfig,
    response::{ResponseRequest, ToolResponse},
};

pub(crate) fn projected_input_schema(base: Arc<JsonObject>) -> Result<Arc<JsonObject>> {
    let mut root = (*base).clone();
    if root.get("type") != Some(&Value::String("object".into())) {
        return Err(schema_error("projected MCP tool schema must be an object"));
    }
    let response = type_input_schema::<ResponseRequest>()?;
    if let Some(branches) = root.get_mut("oneOf").and_then(Value::as_array_mut) {
        for branch in branches {
            add_response_property(
                branch
                    .as_object_mut()
                    .ok_or_else(|| schema_error("projected MCP schema branch must be an object"))?,
                response.as_ref(),
            )?;
        }
    } else {
        add_response_property(&mut root, response.as_ref())?;
    }
    Ok(Arc::new(root))
}

fn add_response_property(root: &mut JsonObject, response: &JsonObject) -> Result<()> {
    let properties = root
        .entry("properties")
        .or_insert_with(|| Value::Object(JsonObject::new()))
        .as_object_mut()
        .ok_or_else(|| schema_error("projected MCP tool schema properties must be an object"))?;
    if properties.contains_key("response") {
        return Err(schema_error(
            "projected MCP tool schema already declares response",
        ));
    }
    properties.insert("response".into(), Value::Object(response.clone()));
    Ok(())
}

#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ResolvedRangeHandleArgument {
    pub range_handle: ResolvedRangeHandleId,
}

pub(crate) fn range_handle_input_schema(base: Arc<JsonObject>) -> Result<Arc<JsonObject>> {
    if base.get("type") != Some(&Value::String("object".into())) {
        return Err(schema_error(
            "range-handle MCP tool schema must be an object",
        ));
    }
    let properties = base
        .get("properties")
        .and_then(Value::as_object)
        .ok_or_else(|| schema_error("range-handle MCP tool schema properties must be an object"))?;
    if !properties.contains_key("range") || properties.contains_key("range_handle") {
        return Err(schema_error(
            "range-handle MCP tool schema must declare exactly one range property",
        ));
    }
    let required = base
        .get("required")
        .and_then(Value::as_array)
        .ok_or_else(|| schema_error("range-handle MCP tool schema must require range"))?;
    if required
        .iter()
        .filter(|value| value.as_str() == Some("range"))
        .count()
        != 1
    {
        return Err(schema_error(
            "range-handle MCP tool schema must require range exactly once",
        ));
    }
    if base.contains_key("oneOf") {
        return Err(schema_error(
            "range-handle MCP tool schema already declares a root oneOf",
        ));
    }

    let handle_schema = type_input_schema::<ResolvedRangeHandleArgument>()?;
    let handle_property = handle_schema
        .get("properties")
        .and_then(Value::as_object)
        .and_then(|properties| properties.get("range_handle"))
        .cloned()
        .ok_or_else(|| schema_error("generated range-handle schema is missing its property"))?;
    let range_branch = (*base).clone();
    let mut handle_branch = (*base).clone();
    let handle_properties = handle_branch
        .get_mut("properties")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| schema_error("range-handle MCP tool schema properties must be an object"))?;
    handle_properties.remove("range");
    handle_properties.insert("range_handle".into(), handle_property);
    let required = handle_branch
        .get_mut("required")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| schema_error("range-handle MCP tool schema must require range"))?;
    let range_positions = required
        .iter()
        .enumerate()
        .filter_map(|(index, value)| (value.as_str() == Some("range")).then_some(index))
        .collect::<Vec<_>>();
    if range_positions.len() != 1 {
        return Err(schema_error(
            "range-handle MCP tool schema must require range exactly once",
        ));
    }
    required[range_positions[0]] = Value::String("range_handle".into());
    Ok(Arc::new(
        serde_json::json!({
            "type": "object",
            "oneOf": [range_branch, handle_branch]
        })
        .as_object()
        .expect("range-handle schema is an object")
        .clone(),
    ))
}

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

pub(crate) fn tool_response_schema(include_video_roles: bool) -> Result<Arc<JsonObject>> {
    let mut value = serde_json::to_value(schemars::schema_for!(ToolResponse))
        .map_err(|_| schema_error("generated tool response schema could not be serialized"))?;
    if !include_video_roles {
        filter_video_resource_roles(&mut value)?;
    }
    object_schema(value)
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
        let operations = branches
            .iter()
            .filter_map(operation_const)
            .map(|name| Value::String(name.to_owned()))
            .collect::<Vec<_>>();
        *object = serde_json::json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": operations,
                    "description": "A batchable standalone browser operation name."
                },
                "request": {
                    "type": "object",
                    "description": "The same arguments advertised by the named standalone operation."
                }
            },
            "required": ["operation", "request"],
            "additionalProperties": false
        })
        .as_object()
        .expect("flat batch step schema is an object")
        .clone();
    });

    if matches != 1 {
        return Err(schema_error(
            "generated batch schema did not contain exactly one complete operation union",
        ));
    }
    Ok(())
}

fn filter_video_resource_roles(schema: &mut Value) -> Result<()> {
    const ALL_ROLES: [&str; 5] = [
        "artifact",
        "artifact_manifest",
        "source_frame",
        "video",
        "video_manifest",
    ];
    let mut matches = 0usize;
    visit_values_mut(schema, &mut |value| {
        let Value::Object(object) = value else {
            return;
        };
        let Some(Value::Array(variants)) = object.get_mut("enum") else {
            return;
        };
        let names = variants
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>();
        if names != ALL_ROLES {
            return;
        }
        matches += 1;
        variants.retain(|variant| {
            variant
                .as_str()
                .is_some_and(|role| role != "video" && role != "video_manifest")
        });
    });
    if matches != 1 {
        return Err(schema_error(
            "generated tool response schema did not contain exactly one resource-role registry",
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
        BrowserEventDetailRequest, CapabilityId, ProgressiveEvidenceOperationKind,
        RetrieveSourceFrameRequest, TemporalDebugBundleRequest, TemporalVideoGenerationRequest,
    };

    #[test]
    fn generated_temporal_request_schemas_are_object_roots() {
        assert!(type_input_schema::<BrowserEventDetailRequest>().is_ok());
        assert!(type_input_schema::<RetrieveSourceFrameRequest>().is_ok());
        let bundle = type_input_schema::<TemporalDebugBundleRequest>().unwrap();
        let bundle = serde_json::to_string(bundle.as_ref()).unwrap();
        assert!(bundle.contains("\"epochs\""));
        assert!(bundle.contains("\"anchor\""));
        assert!(bundle.contains("\"all\""));
        assert!(type_input_schema::<TemporalVideoGenerationRequest>().is_ok());
    }

    #[test]
    fn stop_and_capture_failure_schemas_are_structured_and_current() {
        let stop = serde_json::to_value(schemars::schema_for!(krometrail_core::BrowserStopOutcome))
            .unwrap();
        let encoded = serde_json::to_string(&stop).unwrap();
        assert!(encoded.contains("managed_browser_closed"));
        assert!(encoded.contains("capture_stop_drain_flush"));
        assert!(encoded.contains("sealed_segment_publication_sync"));
        assert!(encoded.contains("writer_usable"));
        assert!(!encoded.contains("managed_browser_closed_degraded"));
        assert!(!encoded.contains("failure_stage"));
    }

    #[test]
    fn projected_schema_is_additive_closed_and_does_not_change_required_fields() {
        let base =
            operation_input_schema(BrowserOperationKind::NavigatePage, &McpConfig::default())
                .unwrap();
        let required = base.get("required").cloned();
        let projected = projected_input_schema(Arc::clone(&base)).unwrap();
        assert_eq!(projected.get("required").cloned(), required);
        assert_eq!(
            projected.get("additionalProperties"),
            base.get("additionalProperties")
        );
        let response = &projected["properties"]["response"];
        assert_eq!(response["additionalProperties"], false);
        assert_eq!(response["properties"].as_object().unwrap().len(), 2);
        assert_eq!(
            response["properties"]["detail"]["enum"],
            serde_json::json!(["concise", "expanded", "full"])
        );
        assert_eq!(response["properties"]["inline_images"]["type"], "boolean");
        assert!(base["properties"].get("response").is_none());
        assert_no_references(&Value::Object(projected.as_ref().clone()));
    }

    #[test]
    fn projected_range_handle_schema_has_two_complete_discoverable_branches() {
        let schema = projected_input_schema(
            range_handle_input_schema(type_input_schema::<BrowserEventDetailRequest>().unwrap())
                .unwrap(),
        )
        .unwrap();
        let branches = schema["oneOf"].as_array().unwrap();
        assert_eq!(branches.len(), 2);
        for branch in branches {
            assert_eq!(branch["type"], "object");
            assert_eq!(branch["additionalProperties"], false);
            let properties = branch["properties"].as_object().unwrap();
            for property in ["clip", "filter", "selection", "focus_times", "response"] {
                assert!(properties.contains_key(property), "missing {property}");
            }
        }
        assert!(branches[0]["properties"].get("range").is_some());
        assert!(branches[0]["properties"].get("range_handle").is_none());
        assert!(branches[1]["properties"].get("range").is_none());
        assert_eq!(branches[1]["properties"]["range_handle"]["type"], "string");
        assert_eq!(
            branches[1]["properties"]["selection"]["oneOf"][0]["properties"]["mode"]["const"],
            "chronological"
        );
    }

    #[test]
    fn temporal_video_schema_is_closed_inlined_and_publishes_fixed_output_limits() {
        let schema = type_input_schema::<TemporalVideoGenerationRequest>().unwrap();
        let schema = Value::Object(schema.as_ref().clone());
        assert_no_references(&schema);
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(
            schema["properties"]["policy"]["enum"],
            serde_json::json!(["real_time", "model_optimized"])
        );
        let output = &schema["properties"]["output"];
        assert_eq!(output["additionalProperties"], false);
        assert_eq!(output["properties"]["max_width"]["minimum"], 2);
        assert_eq!(output["properties"]["max_width"]["maximum"], 1_920);
        assert_eq!(output["properties"]["max_height"]["minimum"], 2);
        assert_eq!(output["properties"]["max_height"]["maximum"], 1_080);
        assert_eq!(
            output["properties"]["max_encoded_bytes"]["maximum"],
            67_108_864_u64
        );
    }

    #[test]
    fn temporal_followup_schemas_require_exactly_one_range_or_handle() {
        let progressive_kinds = [
            ProgressiveEvidenceOperationKind::GenerateArtifacts,
            ProgressiveEvidenceOperationKind::GenerateRegionFilmstrip,
            ProgressiveEvidenceOperationKind::ListSourceFrames,
            ProgressiveEvidenceOperationKind::FetchSourceFrames,
            ProgressiveEvidenceOperationKind::PinResolvedRange,
            ProgressiveEvidenceOperationKind::QueryPinState,
            ProgressiveEvidenceOperationKind::UnpinResolvedRange,
        ];
        let mut schemas = progressive_kinds
            .into_iter()
            .map(|kind| {
                (
                    kind.as_str(),
                    range_handle_input_schema(generated_input_schema(kind.input_schema()).unwrap())
                        .unwrap(),
                )
            })
            .collect::<Vec<_>>();
        schemas.push((
            "query_browser_events",
            range_handle_input_schema(type_input_schema::<BrowserEventDetailRequest>().unwrap())
                .unwrap(),
        ));
        schemas.push((
            "generate_temporal_video",
            range_handle_input_schema(
                type_input_schema::<TemporalVideoGenerationRequest>().unwrap(),
            )
            .unwrap(),
        ));

        for (name, schema) in schemas {
            let branches = schema["oneOf"].as_array().unwrap();
            assert_eq!(branches.len(), 2, "{name}");
            assert_eq!(branches[0]["additionalProperties"], false, "{name}");
            assert!(branches[0]["properties"].get("range").is_some(), "{name}");
            assert!(
                branches[0]["properties"].get("range_handle").is_none(),
                "{name}"
            );
            assert_eq!(branches[1]["additionalProperties"], false, "{name}");
            assert!(branches[1]["properties"].get("range").is_none(), "{name}");
            assert_eq!(branches[1]["properties"]["range_handle"]["type"], "string");
        }
    }

    #[test]
    fn exact_resource_retrieval_schemas_do_not_gain_range_handles() {
        for kind in [
            ProgressiveEvidenceOperationKind::RetrieveArtifact,
            ProgressiveEvidenceOperationKind::RetrieveSourceFrame,
        ] {
            let schema = generated_input_schema(kind.input_schema()).unwrap();
            assert!(schema["properties"].get("range_handle").is_none());
            assert!(range_handle_input_schema(schema).is_err());
        }
    }

    #[test]
    fn response_schema_publishes_optional_range_handle() {
        let schema = tool_response_schema(true).unwrap();
        assert_eq!(
            schema["properties"]["range_handle"]["type"],
            serde_json::json!(["string", "null"])
        );
        assert!(
            !schema["required"]
                .as_array()
                .unwrap()
                .iter()
                .any(|value| value == "range_handle")
        );
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
        let steps = &schema["properties"]["steps"]["items"];
        assert_eq!(steps["type"], "object");
        assert!(steps.get("oneOf").is_none());
        assert!(steps.get("anyOf").is_none());
        assert_eq!(steps["properties"]["request"]["type"], "object");
        assert_eq!(
            steps["required"],
            serde_json::json!(["operation", "request"])
        );
        let operations = steps["properties"]["operation"]["enum"].as_array().unwrap();
        assert_eq!(
            operations.len(),
            BROWSER_OPERATION_REGISTRY
                .iter()
                .filter(|definition| definition.batchable)
                .count()
        );
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
    fn activate_page_schema_keeps_the_target_optional() {
        let config = McpConfig::default();
        let schema = operation_input_schema(BrowserOperationKind::ActivatePage, &config).unwrap();
        let schema = Value::Object(schema.as_ref().clone());
        assert!(schema["properties"].get("target").is_some());
        assert!(
            schema["required"]
                .as_array()
                .is_none_or(|required| !required.iter().any(|field| field == "target"))
        );
    }

    #[test]
    fn published_viewport_schema_matches_runtime_bounds() {
        let config = McpConfig::default();
        let schema = operation_input_schema(BrowserOperationKind::SetViewport, &config).unwrap();
        let schema = Value::Object(schema.as_ref().clone());
        let override_variant = find_tagged_variant(&schema, "mode", "override").unwrap();
        let metrics = &override_variant["properties"]["metrics"];
        let properties = &metrics["properties"];
        for dimension in ["width", "height"] {
            assert_eq!(properties[dimension]["minimum"], 1);
            assert_eq!(properties[dimension]["maximum"], 10_000);
        }
        assert_eq!(properties["device_scale_factor"]["exclusiveMinimum"], 0.0);
        assert_eq!(properties["device_scale_factor"]["maximum"], 8.0);
        for mode in ["override", "preset", "clear"] {
            assert!(
                find_tagged_variant(&schema, "mode", mode).is_some(),
                "missing viewport mode {mode}"
            );
        }
        let preset = find_tagged_variant(&schema, "mode", "preset").unwrap();
        assert_eq!(
            preset["properties"]["preset"]["enum"],
            serde_json::json!([
                "responsive_small",
                "responsive_tablet",
                "responsive_desktop",
                "mobile_phone",
                "mobile_tablet"
            ])
        );
    }

    #[test]
    fn published_semantic_query_schema_is_bounded_and_complete() {
        let config = McpConfig::default();
        let schema = operation_input_schema(BrowserOperationKind::QueryPage, &config).unwrap();
        assert_eq!(schema["properties"]["max_matches"]["minimum"], 1);
        assert_eq!(schema["properties"]["max_matches"]["maximum"], 100);
        assert_eq!(schema["properties"]["max_matches"]["default"], 20);
        let encoded = serde_json::to_string(schema.as_ref()).unwrap();
        for value in ["role", "label", "text", "test_id", "exact", "contains"] {
            assert!(encoded.contains(&format!("\"{value}\"")), "missing {value}");
        }
        assert!(encoded.contains("scope"));
        assert!(encoded.contains("1024"));
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
