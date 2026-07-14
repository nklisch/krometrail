---
id: epic-agent-browser-operation-mcp-control-surface-response-mapping
kind: story
stage: implementing
tags: [browser, agent-ux]
parent: epic-agent-browser-operation-mcp-control-surface
depends_on: [epic-agent-browser-operation-mcp-control-surface-registry-and-session]
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# MCP Structured Response Mapping

## Checkpoint

Implement the one stable structured/text/error/image translator for lifecycle and all operation-result families. Keep image bytes out of JSON, preserve domain outcomes and anchors, and return caller-visible tool failures without inventing unreadable resource references.

## Likely files

- `crates/krometrail-mcp/src/response.rs`
- `crates/krometrail-mcp/src/registry.rs`
- focused response tests

## Acceptance evidence

- All tools advertise and return one `ToolResponse` output envelope with tool name, succeeded/degraded/failed status, result JSON, optional interaction anchor, stable warnings/error, and image metadata.
- The exhaustive `BrowserOperationResult` match contains no route names, schemas, or capability membership; repeated page/interaction variants share family helpers, so it is translation rather than a second registry.
- Domain `Err`, invalid arguments, no-active-session, page-operation failure, wait timeout, and failed/stopped/cancelled/timed-out batch outcomes return `Ok(CallToolResult::error(...))` with caller-visible concise text and structured stable errors.
- Successful mutations with unavailable observation parts remain successful-but-degraded and retain warnings; no screenshot or success is fabricated.
- PNG/JPEG bytes appear only in correctly typed MCP image blocks. Structured JSON and logs contain metadata but no base64. Default standalone actions emit at most their post-action image; explicit screenshots and requested batch step screenshots preserve caller intent; batch also emits its final observation image when available.
- Read-only inspection/evaluation/list/status/wait results do not add screenshots. No `ResourceLink` or URI is returned because this feature has no durable readable resource implementation.
- Representative family tests cover structured success, stable error, anchor, degradation, image mapping, wait timeout, and partial batch failure without one low-value test per operation.

## Out of scope

No image resize/transcode/crop/analysis, artifact persistence, temporal resources, interaction-resource persistence, raw source errors, or duplicate full JSON text content.
