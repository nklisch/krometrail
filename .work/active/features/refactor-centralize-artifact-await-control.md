---
id: refactor-centralize-artifact-await-control
kind: feature
stage: drafting
tags: [refactor, visual, storage]
parent: null
depends_on: [epic-temporal-debugging-workflow-artifact-generation-and-cache]
release_binding: null
gate_origin: refactor-design
created: 2026-07-14
updated: 2026-07-14
---

# Centralize artifact await cancellation control

## Brief

The artifact service defines `caller_controlled` and its `external_cancelled`
helper in `src/artifacts/service.rs:586-605`, while
`src/artifacts/scheduler.rs:189-223` already contains the same generic deadline /
external-cancellation select and cancellation waiter. The service uses its copy
for frame reads, planning, and cache lookups (`service.rs:99-109,206`), while the
scheduler copy owns the same policy for permits and blocking jobs. This exact
duplication was introduced by `13e6464` and is present in the final artifact tree
at `622f9be`.

Make the scheduler's existing private await-control utility the single internal
implementation used by the service, preserving the current select ordering,
deadline errors, cancellation errors, and caller-controlled versus shared-work
cancellation distinction. Do not merge it with single-flight notification
waiting: `src/artifacts/single_flight.rs:103-140` has a different wakeup loop and
must retain its notify semantics.

**Source lens**: elimination first / exact duplication / misplaced private
responsibility

**Rationale**: removes one duplicate deadline-and-cancellation policy and leaves
that policy beside the scheduler primitives it controls, without changing any
cancellation or timeout contract.

**Black-box classification**: pure refactor. External cancellation, deadline
selection, scheduler permits, single-flight behavior, publication suppression,
error codes/messages, and public ports remain unchanged.

## Acceptance criteria

- [ ] The service's frame-read, planning, and artifact-lookup awaits reuse one
  scheduler-owned caller-control helper.
- [ ] The duplicate service helper and duplicate external-cancellation waiter are
  removed without altering select branches, deadline precedence, or returned
  errors.
- [ ] Single-flight's notify-aware wait loop remains independent and unchanged in
  behavior; no shared helper swallows its notification wakeup.
- [ ] No public API, cache identity, manifest, output, retention, or persistence
  behavior changes.
- [ ] `cargo fmt --all -- --check` passes.
- [ ] `cargo test --workspace --all-targets --locked` passes.
- [ ] `cargo clippy --workspace --all-targets --locked -- -D warnings` passes.

## Implementation notes

- **Evidence**: `13e6464` added both root modules; final commit-tree
  verification uses `622f9be:src/artifacts/service.rs:99-109,586-605` and
  `622f9be:src/artifacts/scheduler.rs:189-223`.
- **Files**: `src/artifacts/service.rs`, `src/artifacts/scheduler.rs`; this
  feature file.
- **Tests**: retain scheduler cancellation/deadline coverage, service
  cancellation/deadline qualification, and single-flight waiter coverage; add no
  test solely for moving a private helper.
- **Ordering**: this feature depends on the artifact feature reaching its review
  decision before implementation. Its write set is limited to artifact root
  modules and does not overlap active browser-event or progressive-evidence work.

## Risk and rollback

**Risk**: Low to medium. The logic is duplicated exactly, but visibility and
imports must be adjusted without accidentally routing shared `WorkCancellation`
through the caller-controlled path.

**Rollback**: Revert the refactor commit to restore the service-local helper and
its imports. The artifact feature and single-flight implementation remain intact.
