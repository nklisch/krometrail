---
id: epic-agent-browser-ergonomics-temporal-range-handles-followups
kind: story
stage: done
tags: [agent-ux, visual]
parent: epic-agent-browser-ergonomics-temporal-range-handles
depends_on: [epic-agent-browser-ergonomics-temporal-range-handles-authority]
release_binding: 1.1.0
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Handle-enabled temporal follow-ups

## Checkpoint

Publish a bundle range handle, accept exactly one handle-or-range across artifact, region, source-list/fetch, browser-event, pinning, and video routes, normalize to the existing validated range contracts, echo the handle in responses, and teach agents to reuse it.

## Acceptance evidence

- Schema-wide tests prove exclusive additive input without changing legacy range requests or tool limits.
- Stdio tests prove bundle-to-follow-up reuse, exact range forwarding, compact-projection preservation, and no dispatch after invalidation.
- Skill guidance states lifetime, restart, provenance, and fallback semantics accurately.

## Ordering

Depends on `epic-agent-browser-ergonomics-temporal-range-handles-authority`; all routes consume the one injected authority.

## Implementation notes

- Added one fail-closed schema adapter for the exact designed follow-up set. Each published tool
  remains a closed object, keeps all prior non-range properties and limits, and requires exactly one
  root `range` or `range_handle`; exact-ID resource reads remain unchanged.
- Bundle success registers and publishes the exact resolved range. Handle inputs are availability-
  checked and replaced with that range before the existing typed decoder and service; full-range
  inputs are decoded first and then registered for a deduplicated response echo.
- The optional common response-envelope handle is outside result projection, so compact responses
  preserve it without adding handles to manifests, resources, or persisted evidence.
- One in-memory MCP flow covers bundle → source-frame list → browser events → pin state, exact range
  forwarding, compact handle preservation, both-field rejection, and restart/unknown-handle recovery
  before follow-up dispatch. Schema tests cover every named route, optional video, and unchanged exact
  resource retrieval.
- Updated plugin guidance with reuse, browser-stop lifetime, MCP-restart invalidation, full-range
  fallback, and provenance boundaries.

## Verification

- `cargo test -p krometrail-mcp --locked` — 56 passed.
- `cargo clippy -p krometrail-mcp --all-targets --locked -- -D warnings` — passed.
