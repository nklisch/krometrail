---
id: refactor-remove-temporal-debug-bundle-dead-scaffolding
kind: story
stage: implementing
tags: [refactor, visual, agent-ux]
parent: null
depends_on: [epic-temporal-debugging-workflow-temporal-debug-bundle]
release_binding: null
gate_origin: refactor-design
created: 2026-07-14
updated: 2026-07-14
---

# Remove unused temporal debug-bundle scaffolding

## Brief

The completed temporal bundle leaves several pre-service helpers and lint
allowances that have no callers in the current tree. `src/debug_bundle/mod.rs:13`
blanket-allows dead code and unused imports; `BundleMarkerEvidence` at lines
58-65 duplicates the live `markers::MarkerEvidence` at
`src/debug_bundle/markers.rs:17-25`; `effective_policy_from_outcomes` at lines
91-101 is not used because the service explicitly extracts focus and builds the
policy at `src/debug_bundle/service.rs:190-206`. The header lookup helper at
`src/debug_bundle/header.rs:143-153` has no caller, and
`src/debug_bundle/service.rs:439-442` retains an obsolete underscore import.

Delete this dead scaffolding and narrow/remove the blanket allowances after
verifying the remaining production items are live. This is elimination of
superseded local implementation scaffolding, not a new abstraction or contract
change.

**Source lens**: elimination first / dead weight

**Rationale**: removes an unused duplicate input shape, an unused policy path,
an unused lookup helper, and a stale lint-suppression seam that otherwise make
future bundle changes harder to audit. The implementation path remains exactly
as it is.

**Black-box classification**: pure refactor. No public type, serialization,
marker assembly, policy construction, header content, artifact generation,
context query, error, or MCP behavior changes.

## Current State

- `BundleMarkerEvidence` is defined but has zero workspace callers; the service
  uses `MarkerEvidence` directly.
- `effective_policy_from_outcomes` is defined but has zero workspace callers;
  the service's explicit two-step path is authoritative.
- `epoch_summary` is defined with `#[allow(dead_code)]` but has zero workspace
  callers.
- `service.rs` imports `cancelled_error as _` and `deadline_error as _` after
  already importing/using `cancelled_error`; neither underscore binding is
  required by the implementation.
- The module-level `#![allow(dead_code, unused_imports)]` hides this drift.

## Target State

- Delete the unused `BundleMarkerEvidence`, `effective_policy_from_outcomes`,
and `epoch_summary` items and their now-unused imports.
- Remove the obsolete service import and the module-level blanket allowance (or
  retain only a narrowly justified lint allowance if the compiler proves one is
  still required).
- Keep `MarkerEvidence`, `build_effective_policy`, `compose_header`, and the
  live service wiring unchanged.

## Acceptance Criteria

- [ ] Workspace search finds no references to the deleted helpers or duplicate evidence type.
- [ ] `src/debug_bundle` compiles without a blanket dead-code/unused-import suppression; any remaining allowance is narrow and justified by a live boundary.
- [ ] Existing bundle policy, marker, focus, header, service, root-composition, and MCP integration tests pass unchanged.
- [ ] No serialized shape, evidence policy, timing, error, privacy, or resource behavior changes.
- [ ] `cargo fmt --all -- --check`, locked workspace check/test, and Clippy with `-D warnings` pass.

## Risk and Rollback

**Risk**: Low. All candidates are private and have zero production callers; the
main risk is deleting a re-export or import still required by a live module.

**Rollback**: Revert the deletion commit to restore the helpers and lint
allowance. No data, schema, migration, or compatibility rollback is needed.

## Discovery Notes

- **Scope**: source and tests touched by commits `6b5776b` through `245fb1f`,
  with direct verification focused on `src/debug_bundle/{mod,header,service}.rs`.
- **Dispatch**: direct-read only; no exploratory agent or peer review was used
  because the candidate callers were fully resolvable with local grep/read.
- **Project conventions**: no `.agents/skills/refactor-conventions/` catalog is
  present; the built-in elimination/dead-weight lenses and prepublic clean-design
  policy were applied.
- `.work/bin/work-view` and all current epic/feature stages were preserved.
