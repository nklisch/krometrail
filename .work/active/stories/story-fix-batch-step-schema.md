---
id: story-fix-batch-step-schema
kind: story
stage: review
tags: [bug, agent-ux, browser]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Publish agent-readable batch step schemas

## Symptom

The v1.0.4 `batch` MCP declaration renders `steps` as `Array<unknown | unknown | ...>`, so an
agent cannot construct a batch from the advertised tool contract.

## Root cause

The MCP schema publisher leaves the tagged batch-operation union under `items.oneOf`. The host tool
declaration projection recognizes the branches but does not materialize their concrete object shapes
for an array-item `oneOf`, even though each generated branch is valid and fully dereferenced.

## Fix approach

Publish this mutually exclusive tagged union as `items.anyOf` after filtering the generated registry
union. The per-operation `const` discriminator preserves exact validation while using the composition
shape the host declaration projection exposes.

## Regression test

`crates/krometrail-mcp/src/schema.rs` asserts that the published batch steps use `anyOf` and that every
branch exposes a concrete operation discriminator and object request schema. The test failed against
the v1.0.4 `oneOf` projection before the fix.

## Implementation notes

- Execution capability: host agent, high reasoning; the change is a small public-schema projection
  with release compatibility implications, best kept in one context.
- Files changed: MCP schema projection/test and the Krometrail skill's batch guidance.
- Confirmation: the regression test failed before the projection change; all seven MCP schema tests
  pass afterward. Workspace-wide verification and installed-plugin qualification are deferred to the
  integrated release pass so they run once across the complete patch.
- Adjacent issues parked: none.
