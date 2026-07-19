---
id: story-fix-mcp-boolean-result-schema
kind: story
stage: review
tags: [bug]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-19
updated: 2026-07-19
---

# Claude Code rejects tools/list: outputSchema publishes boolean `result` subschema

## Symptom

The krometrail Claude Code plugin connects but no tools load. `claude mcp list`
reports `plugin:krometrail:krometrail — ! Connected · tools fetch failed`.
Reproduced with the MCP TypeScript SDK client: `listTools()` throws a ZodError
with `Invalid input` at `tools[*].outputSchema.properties.result` for all 49
published tools, discarding the entire tool list atomically.

## Root cause

`ToolResponse.result` is declared as `serde_json::Value`
(`crates/krometrail-mcp/src/response.rs`), and schemars renders `Value` as the
boolean JSON Schema `true` ("anything allowed"). That is valid JSON Schema
draft 2020-12, but the MCP TypeScript SDK's zod validation only accepts
object-form subschemas inside `properties`, so every consumer built on that SDK
(Claude Code included) rejects the whole `tools/list` response. Every tool's
output schema is built from the one `tool_response_schema()` helper, so the
single boolean subschema fans out to all tools.

## Fix approach

Publish an object-form permissive schema for the `result` field via
`#[schemars(schema_with = ...)]` — an annotation-only object schema is
semantically identical to `true` (accepts any payload, which is correct: the
result shape varies by tool and detail level) while remaining compatible with
strict MCP clients. No wire behavior changes; only the advertised schema form.

## Regression test

`crates/krometrail-mcp/src/schema.rs` —
`response_schema_subschemas_are_object_form`: for both video-role variants of
`tool_response_schema`, asserts `properties.result` is an object and walks the
full schema asserting every subschema position (`properties`, combinators,
`items`, `not`, etc.) is object-form, never boolean.

## Implementation notes

- Files changed: `crates/krometrail-mcp/src/response.rs` (annotated
  `ToolResponse.result` with `#[schemars(schema_with = "tool_result_subschema")]`
  emitting an annotation-only object schema), `crates/krometrail-mcp/src/schema.rs`
  (regression test).
- Test added: `schema::tests::response_schema_subschemas_are_object_form` —
  failed with `result subschema must be an object, got true` before the fix,
  passes after.
- Verified end-to-end with the MCP TypeScript SDK client against the built
  binary: `listTools()` succeeds (50 tools) where it previously threw a ZodError
  rejecting the whole tool list.
- Reconciled onto origin/main after the v1.2.1 release landed mid-fix: the
  release touched the same files but did not address this bug, so the released
  v1.2.1 plugin still fails in Claude Code. The patch re-applied cleanly; full
  gate (fmt, workspace tests, clippy -D warnings) re-run green on the
  reconciled tree. Shipping this to plugin users requires a follow-up release.
- No checked-in canonical schema artifacts capture the MCP tool schemas
  (they are generated at serve time), so no artifact regeneration was needed.
- Parked for separate consideration: `idea-mcp-signal-shutdown` (server ignores
  SIGINT/SIGTERM; hosts escalate to SIGKILL on every shutdown).
