---
id: story-fix-batch-schema-rendering
kind: story
stage: review
tags: [bug, agent-ux]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Publish a batch step shape Codex can render

## Symptom

The installed v1.0.5 tool declaration still rendered `batch.steps` as nineteen `unknown` union
branches even though the MCP JSON Schema contained concrete `items.anyOf` branches.

## Root cause

The host declaration renderer does not materialize a large composed union nested under array items.
Changing `oneOf` to `anyOf` preserved valid JSON Schema but only moved the unsupported composition;
the schema-level test never asserted the actual renderer-compatible shape.

## Fix approach

Project the registry-filtered union into one flat step object: a required operation enum derived from
the admitted registry and a required request object. Runtime deserialization remains the exact tagged
`BrowserOperationRequest` authority, while the shipped skill points each request to the corresponding
standalone tool arguments.

## Regression test

`crates/krometrail-mcp/src/schema.rs` asserts that batch array items are a single object with no
`oneOf`/`anyOf`, the exact admitted operation enum, and a required object request.

## Implementation notes

- Execution capability: host agent, high reasoning; this is a narrow public-schema compatibility
  repair grounded in the installed Codex declaration.
- Files changed: `crates/krometrail-mcp/src/schema.rs`.
- The admitted operation names still derive from the registry, and exact request validation remains
  owned by `BrowserOperationRequest` deserialization.
- Regression confirmation: `cargo test -p krometrail-mcp
  batch_schema_is_filtered_from_the_generated_complete_union --locked` passes.
- Installed-host declaration verification will run after the next plugin refresh; the unit test now
  guards the flat renderer-compatible shape rather than assuming nested composition support.
