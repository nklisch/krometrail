---
id: gate-tests-mcp-successful-mutation-projections
kind: story
stage: done
tags: [testing]
parent: null
depends_on: []
release_binding: 1.1.0
gate_origin: tests
created: 2026-07-18
updated: 2026-07-18
---

# Round-trip mutation projection defaults and expansion through MCP

## Priority
Medium

## Value evidence

Item: `epic-agent-browser-ergonomics-response-projections`

This release changes omitted response preferences to compact/no-inline and preserves explicit legacy/full/inline expansion. Unit tests cover projector parts, but successful mutation request splitting, routing, result projection, and MCP content serialization are not protected together.

## Gap type
e2e-seam

## Suggested test

Drive one successful live mutation through in-memory JSON-RPC using omitted, legacy, full, and full+inline response requests. Assert identical operation outcome/interaction/warnings/resources, bounded compact default, exact expansion differences, and one inline image only when requested.

## Test location
`crates/krometrail-mcp/src/server.rs`

## Acceptance criteria

- An in-memory JSON-RPC test drives a successful state-changing browser tool through request splitting, routing, response projection, and MCP serialization.
- Omitted response preferences produce compact snapshot/page-state detail and no inline image.
- Explicit legacy/full/inline variants expand presentation as requested.
- Every variant preserves the same successful operation outcome, interaction anchor, warnings, and resource identities.
- The test asserts MCP content shape as well as structured content and fails if mutation dispatch is skipped or duplicated.

## Implementation plan

- Extend the protocol fake with one deterministic successful mutation result containing full observation data and an encoded screenshot.
- Call the mutation through in-memory JSON-RPC for default, legacy, full, and inline projections.
- Compare authoritative envelope fields across variants and assert only the intended presentation differences.

## Implementation notes

- Extended the in-memory protocol session with a deterministic successful click containing page state, a 121-node snapshot, and a PNG screenshot.
- Exercised default, explicit legacy, full, and full-plus-inline projection requests through JSON-RPC and the live MCP service.
- Asserted four total dispatches, successful MCP envelopes, invariant records/anchors/warnings/resources, compact versus full node counts, and inline images only for the requested variants in both MCP content and structured metadata.

## Validation

- `cargo test -p krometrail-mcp successful_mutation_roundtrip_preserves_semantics_across_response_projections --locked -- --nocapture`
- `cargo test -p krometrail-mcp --locked`
- `cargo test --workspace --all-targets --locked`
- `cargo check --workspace --all-targets --locked`
- `cargo clippy --workspace --all-targets --locked -- -D warnings`

## Review

- Verdict: pass; the JSON-RPC seam covers dispatch, projection, MCP serialization, semantic invariants, and presentation-only expansion.
- Effective implementation size: small. Effective review weight: standard bounded inline standalone-story review.
- Review tightened the compact default assertion to the concrete 96-node response bound; the focused test passed afterward.
