---
id: epic-temporal-debugging-workflow-mcp-investigation-surface-routing-session-and-cancellation
kind: story
stage: implementing
tags: [agent-ux, visual, browser]
parent: epic-temporal-debugging-workflow-mcp-investigation-surface
depends_on:
  - epic-temporal-debugging-workflow-mcp-investigation-surface-contracts-registries-and-resource-read
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Temporal MCP Routing, Session Geometry, and Cancellation

## Checkpoint

Extend the existing dynamic MCP adapter to route the primary bundle, progressive tools, and chronological browser-event detail over injected application ports. Preserve one session owner and make its current-geometry view available to current-reference region requests. This checkpoint owns dispatch and capability filtering, not result projection or resource protocol methods.

## Likely files

- `crates/krometrail-mcp/src/{config.rs,session.rs,registry.rs,schema.rs,server.rs}`
- `crates/krometrail-mcp/src/lib.rs`
- `crates/krometrail-core/src/ports/browser.rs` only for a required adapter correction
- focused MCP route/session tests

## Design

- Replace the prepublic browser-only `build_service(connector, config)` input with `McpDependencies { browser, temporal_debug_bundles, progressive_evidence, temporal_context }`. The root will update all call sites together; no compatibility constructor remains.
- Register `temporal_debug_bundle` from its definition, eight tool-exposed progressive operations, and `query_browser_events` from its context definition. Resource-only progressive operations are excluded from `tools/list` by metadata.
- Expose inner request objects. Each progressive route wraps arguments in the existing tagged `ProgressiveEvidenceRequest`; the bundle and event routes deserialize their exact domain request directly. Invalid input is rejected before an application call.
- Implement `CurrentReferenceGeometry` for `BrowserSessionOwner` by delegating to its active session. Pass that view only in `ProgressiveEvidenceContext`; retained historical operations work without an active session.
- Bridge rmcp's request cancellation token to the existing core `CancellationSignal`. Compute one absolute 30-second MCP deadline and use it for bundle/progressive calls and the read-only context call. Do not introduce MCP tasks, polling, resource subscriptions, or remote transport.

## Acceptance evidence

- [ ] Capability-filtered registration lists control tools unchanged, temporal-vision tools/resources separately, and browser-event detail independently; names and descriptions come only from registries.
- [ ] Valid bundle, progressive, event, and current-reference calls reach exactly one intended port with the exact request; malformed input reaches none.
- [ ] Registry/schema initialization fails closed on duplicate names, missing definitions, non-object schemas, or disabled capability leakage.
- [ ] Current-reference geometry delegates through the active owner and preserves existing stale/lifecycle errors; no CDP or storage type enters MCP/core contracts.
- [ ] Cancellation before dispatch and deadline expiry produce stable cancellation without cancelling another request or stopping the browser session.
- [ ] Exact rmcp 0.11.0 APIs compile; no future task or subscription semantics are used.

## Ordering constraints

Depends on the contract/registry checkpoint. Response projection must consume the final route request/result associations and cancellation context from this checkpoint.

## Out of scope

No response envelope extension, resource URI parser/templates, blob reads, inline image loading, root runtime composition, or full JSON-RPC qualification.
