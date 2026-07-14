---
id: bug-prevent-artifact-notify-lost-wakeups
kind: story
stage: done
tags: [bug, visual, storage]
parent: null
depends_on: []
release_binding: null
gate_origin: review
gate_finding: artifact-generation-I-1
created: 2026-07-14
updated: 2026-07-14
---

# Prevent lost wakeups in artifact coordination

## Symptom

An artifact single-flight waiter can sleep until its request deadline after the leader has already published the result, and session deletion can theoretically hang while draining active artifact publications. Work cancellation has the same race. Each path creates an unpolled `Notify::notified()` future, checks shared state, and relies on `notify_waiters()`; a concurrent notification between the check and the future's first poll leaves no stored permit.

## Root cause

`tokio::sync::Notify::notify_waiters()` wakes only futures already registered with the `Notify`. The loops in `src/artifacts/single_flight.rs`, `src/artifacts/epoch.rs`, and `crates/krometrail-store/src/artifacts/mod.rs` construct `Notified` before checking state but do not pin and `enable()` it before that check. On a multi-threaded runtime, completion/cancellation/drop can therefore occur after the state check but before registration.

## Fix approach

Keep the existing state predicates and multi-waiter `notify_waiters()` behavior, but pin each `Notified` future and call `enable()` before checking shared state. This is Tokio's documented no-lost-wakeup pattern and preserves all result, cancellation, waiter-count, deletion, and deadline semantics. Do not replace multi-waiter broadcasts with `notify_one()`.

## Regression test

Focused multi-threaded coordination tests beside single-flight, work cancellation, and publication drain verify that each path can transition after a waiter has armed its notification but before it awaits, then completes within a short timeout. Because the original failing instruction window depends on cross-thread scheduling between a state check and first poll, these are the closest deterministic guards; existing concurrent single-flight and session-deletion qualification remain black-box coverage.

## Implementation notes

- Execution capability: baseline inline ownership; the defect is one synchronization pattern repeated at three tightly scoped sites.
- Files changed: `src/artifacts/{single_flight.rs,epoch.rs}`, `crates/krometrail-store/src/artifacts/mod.rs`, and this story.
- Each loop now pins `Notified` and calls `enable()` before its shared-state predicate. Existing `notify_waiters()` broadcasts remain intact for multiple artifact waiters/cancellation listeners.
- Regression tests cover completion, cancellation, and publication drop after registration but before awaiting, using Tokio's multi-threaded runtime and bounded timeouts.
- Confirmation: the new tests pass; the full locked Rust 1.85 workspace test suite passes; format and workspace Clippy with warnings denied pass. The originally reviewed lost-permit window is removed at all three sites.
- Parked separately: `idea-artifact-error-context` captures the review's lower-risk context-propagation nit.

## Review (2026-07-14)

**Verdict**: Approve

**Blockers**: none
**Important**: none
**Nits**: none

**Evidence**: Bounded standalone-story review inspected commit `4ba4214`, confirmed every reviewed state-check/notification loop registers before checking state, retained broadcast semantics for multiple listeners, and found no changes to result, timeout, cancellation, waiter-count, publication, or deletion contracts. The three focused tests and full Rust 1.85 workspace gate passed. No independent reviewer ran, as required for a standalone fix story.
