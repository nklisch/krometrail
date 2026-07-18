---
id: story-fix-bound-page-target-state
kind: story
stage: done
tags: [bug]
parent: null
depends_on: []
release_binding: 1.1.0
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Bound supervised page-target state

## Symptom

Repeated page creation and closure retains every terminal logical target in the supervisor map, allowing unbounded state growth, while the declared 128-page bound is not enforced for simultaneously live pages.

## Root cause

The target reducer allocates every newly observed logical page directly into `targets_by_key`; `MAX_KNOWN_PAGE_TARGETS` only documents the domain boundary and is not applied at the single-writer state transition.

## Fix approach

Preflight authoritative snapshots and individual logical-target creation against the live-page limit before mutation. When a legal creation needs storage, prune terminal entries as a deterministic class while retaining the monotonic sequence high-water mark and immutable resolved opener identity.

## Regression test

`crates/krometrail-cdp/tests/target_reducer.rs` covers more than 10,000 open/close cycles, atomic rejection of a 129th live page, cursor monotonicity, and opener/key-reuse behavior after terminal pruning.

## Implementation notes

- Execution capability: focused direct repair; the defect is isolated to the deterministic target reducer and its existing integration harness.
- Files changed: `crates/krometrail-cdp/src/targets/reducer.rs` and `crates/krometrail-cdp/tests/target_reducer.rs`.
- The reducer preflights individual creation, initial inventory, and reconnect inventory before mutation. Legal creation at storage capacity removes all terminal targets and stale session-key mappings while leaving `next_page_sequence` untouched.
- Confirmation: the new regressions failed before the repair and pass afterward; all reducer unit tests, the full target-reducer integration suite, CDP all-target check, and warning-denied CDP clippy pass.
- Adjacent issues parked: none.

## Bounded inline review — 2026-07-18

- Verdict: approved.
- The limit failure precedes pruning, ID/revision allocation, sequence allocation, and effect publication; a direct reducer test confirms state and effects are unchanged for both a 129th creation and an oversized authoritative snapshot.
- Terminal reclamation is deterministic by lifecycle class and cleans the auxiliary session-key index. Resolved popup opener identity remains immutable, raw key reuse allocates a new target identity, and the cursor remains a monotonic high-water mark across reclamation.
