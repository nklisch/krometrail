---
id: epic-agent-browser-operation-mcp-control-surface-response-mapping
kind: story
stage: done
tags: [browser, agent-ux]
parent: epic-agent-browser-operation-mcp-control-surface
depends_on: [epic-agent-browser-operation-mcp-control-surface-registry-and-session]
release_binding: 1.0.0
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

## Implementation notes

- Execution capability: highest from the autopilot caller for stable public structured/error/image semantics.
- Review weight: `standard` from the autopilot caller; child checkpoint advances directly to done.
- Files changed: MCP response projector and registry output wiring; core JSON-schema derives for interaction anchors and screenshot metadata; MCP dependency/lock metadata.
- Tests added: bounded visible errors, PNG/JPEG byte separation, degradation, wait timeout, page anchor/failure, partial batch failure, and common output-schema assertions.
- Simplification: one exhaustive `BrowserOperationResult` projection owns all result-family translation; repeated page/interaction/live-observation helpers avoid route or capability duplication.
- Discrepancies from design: image MIME is derived from the authoritative encoded-byte signature because the existing result contract retains screenshot target metadata but not the requested encoding field. Unsupported signatures fail internally rather than being mislabeled.
- Adjacent issues parked: none.

## Completion evidence

- `PATH=/home/nathan/.cargo/bin:$PATH cargo +1.85.0 test -p krometrail-mcp --locked` passed all 8 focused MCP tests.
- Tool results are constructed from bounded success/error content and then assigned `structured_content` directly; full JSON is not duplicated into text, and encoded bytes occur only in image blocks.
