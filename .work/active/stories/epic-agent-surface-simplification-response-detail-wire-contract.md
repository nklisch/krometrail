---
id: epic-agent-surface-simplification-response-detail-wire-contract
kind: story
stage: implementing
tags: [agent-ux, browser]
parent: epic-agent-surface-simplification-response-detail
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Collapse the response wire contract

Replace the per-part response projection matrix with the only supported request shape: optional `response.detail` (`concise`, `expanded`, `full`) and optional boolean `response.inline_images`. Omission is concise/no-inline. Route `browser_status` through this common shape and remove its top-level detail request.

Delete diagnostic suppression from the request schema and server. Failed and degraded structured results always receive privacy-bounded correlation/log identity; successful results remain quiet. Remove the projected-route discovery set and duplicate request parsing used only to decide whether diagnostics could be hidden.

## Acceptance evidence

- Generated schemas expose a closed two-field response object and reject all removed fields/values at the precise input path.
- Omitted and empty response objects decode to the same default.
- Browser status accepts the shared response object and rejects the removed top-level detail field.
- Failed/degraded responses always include diagnostics, successful responses do not, and JSON-RPC failures retain their existing bounded diagnostic data.
- Obsolete enums, compatibility constructors, diagnostic parsing, and `*_projected` wrapper names are absent rather than deprecated.

## Ordering

This establishes the public types and parsing used by the projection checkpoint. It has no sibling dependency.
