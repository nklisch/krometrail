use rmcp::model::{CallToolResult, Content};
use serde_json::{Value, json};

use krometrail_core::KrometrailError;

pub(crate) fn visible_error(tool: &str, error: KrometrailError) -> CallToolResult {
    let summary = format!("{tool} failed: {}", error.message);
    let structured = json!({
        "tool": tool,
        "status": "failed",
        "result": {},
        "interaction": null,
        "warnings": [],
        "images": [],
        "error": error,
    });
    let mut result = CallToolResult::error(vec![Content::text(summary)]);
    result.structured_content = Some(structured);
    result
}

pub(crate) fn provisional_success(tool: &str, value: Value) -> CallToolResult {
    let structured = json!({
        "tool": tool,
        "status": "succeeded",
        "result": value,
        "interaction": null,
        "warnings": [],
        "images": [],
        "error": null,
    });
    let mut result = CallToolResult::success(vec![Content::text(format!("{tool} succeeded"))]);
    result.structured_content = Some(structured);
    result
}
