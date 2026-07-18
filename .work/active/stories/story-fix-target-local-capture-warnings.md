---
id: story-fix-target-local-capture-warnings
kind: story
stage: review
tags: [bug, browser, agent-ux]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Scope capture warnings to the operated page

## Symptom

A healthy Hacker News target returned `status: degraded` because a different Wikipedia target's
retained capture had failed. Closing the failed tab restored warning-free inspection.

## Root cause

The MCP response mapper appends every failed capture status in the browser session to every operation
response. It does not constrain those statuses to the operation result's target identity.

## Fix approach

Resolve the target identity already present in each page result projection and include capture-health
warnings only for that target. Browser-scoped results without one target retain the session-wide view,
and failed page operations use the target in their error context.

## Regression test

`crates/krometrail-mcp/src/response.rs` maps a successful screenshot for one target alongside a failed
capture status for another and asserts the response remains successful and warning-free. It failed as
degraded before the fix.

## Implementation notes

- Execution capability: host agent, high reasoning; this is a narrow response-contract correction
  whose target identity is already present in the projection.
- Files changed: MCP response mapping and regression tests.
- Confirmation: the new regression failed before the filter and passes afterward; all ten response
  mapping tests pass. Integrated workspace and live multi-target verification are deferred to the
  combined release pass.
- Adjacent issues parked: none.
