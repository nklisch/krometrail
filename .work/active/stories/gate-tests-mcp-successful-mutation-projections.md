---
id: gate-tests-mcp-successful-mutation-projections
kind: story
stage: implementing
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
