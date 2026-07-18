---
id: story-fix-batch-direct-target-inheritance
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

# Make targetless batch steps inherit the outer direct target

## Symptom

A batch admitted against an explicit Wikipedia target failed its first targetless step when another
page was logically selected, reporting `target_failed` because the step resolved against the selected
GitHub page instead of the outer target.

## Root cause

`BatchRequest` validates target compatibility but retains each omitted step target as
`PageSelection::Selected`. At execution, `child_resolves_to` resolves that value against mutable
session selection rather than binding it to the already admitted outer batch target.

## Fix approach

Before each batch step is checked or dispatched, inherit the admitted outer target only when the
step uses `Selected`. Preserve explicit matching targets and the existing rejection of contradictory
explicit targets/references.

## Regression test

`crates/krometrail-core/src/browser/batch.rs` constructs an explicit-target batch with a targetless
step and proves admission binds that step to the outer target before execution.

## Implementation notes

- Execution capability: host agent, high reasoning; this is a focused domain-boundary correction.
- Files changed: `crates/krometrail-core/src/browser/operation.rs` and `batch.rs`.
- The admission constructor now binds omitted child selections to an explicit outer target before
  compatibility validation, so execution cannot drift with later logical selection.
- Regression confirmation: `cargo test -p krometrail-core
  explicit_batch_target_is_inherited_by_targetless_steps --locked` passes.
- Full workspace verification is deferred to the integrated patch pass.
