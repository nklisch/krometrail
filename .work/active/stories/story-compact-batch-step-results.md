---
id: story-compact-batch-step-results
kind: story
stage: done
tags: [bug, agent-ux, performance]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Remove duplicate live observations from batch step results

## Symptom

A two-step batch produced roughly 13,000 response tokens because a state-changing step embedded its
full live page snapshot and the batch embedded another full final observation.

## Root cause

The MCP batch projector calls the standalone response projector for each child and copies its entire
`result`, including the standalone live observation. It then independently projects the required
batch-final observation, duplicating large accessibility state.

## Fix approach

Keep each step's status, timing, anchor, error, operation outcome/record, and optional requested step
screenshot, but remove the nested live `observation` from child results. The batch-final observation
remains the single authoritative current-state payload.

## Regression test

`crates/krometrail-mcp/src/response.rs` constructs a successful state-changing batch step and asserts
that its projected child result omits `observation` while the final observation remains available.

## Implementation notes

- Execution capability: host agent, high reasoning; this is a focused response-projection repair.
- Files changed: `crates/krometrail-mcp/src/response.rs`.
- Step status, timing, interaction anchor, record/outcome, errors, and requested step screenshots are
  unchanged; only the duplicate child live-observation field is removed.
- Regression confirmation: `cargo test -p krometrail-mcp
  degradation_wait_timeout_page_anchor_and_batch_failure_remain_distinct --locked` passes with a
  synthetic 403-node snapshot in both child and final evidence.
- Full workspace verification is deferred to the integrated patch pass.

## Review

Bounded inline review approved. The final observation remains the batch's single current-state
authority, while child-specific outcomes and anchors remain intact. Read-only child results are
unchanged because they do not carry the removed top-level field. No independent reviewer ran.
