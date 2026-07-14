---
id: refactor-centralize-artifact-await-control
kind: feature
stage: review
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

## Refactor Overview

This is a one-step, behavior-preserving extraction from the current source tree.
The service and scheduler each currently implement the same caller-controlled
await policy, but the scheduler already owns the primitive that controls request
permits and blocking work. The smallest consolidation is to expose that existing
scheduler helper as `pub(crate)`, import it in the service, rename the three
service call sites to use it, and remove the service-local helper plus its now
unused imports.

The feature remains correctly tagged `[refactor]`: the change affects no public
contract, result ordering, cache identity, persistence, artifact bytes,
publication policy, or error meaning. It only removes duplicate policy code.

## Current-source Verification

The design is based on the current files, not only historical commit `622f9be`:

- `src/artifacts/service.rs:99-104` wraps `FrameSource::frames_by_id`,
  `:109-125` wraps planning, and `:206-212` wraps each artifact lookup with
  `caller_controlled`.
- `src/artifacts/service.rs:586-603` currently defines `caller_controlled` and
  its local `external_cancelled` waiter.
- `src/artifacts/scheduler.rs:112-123` uses `controlled` for the request
  permit; `:189-199` defines the equivalent caller-controlled select,
  `:213-218` defines its external-cancellation waiter, and `:220-222` supplies
  the equivalent deadline sleep.
- Both selects have the exact same branch order and outcomes:
  `future -> Ok(value)`, external cancellation -> `cancelled_error()`, then
  deadline -> `deadline_error()`. Both external waiters await the supplied
  `CancellationSignal` or use `pending()` for `None`. The only difference is
  scheduler `controlled` delegates the deadline conversion to its private
  `sleep_until` helper.
- The duplication originated with `13e6464`, but the post-artifact current
  tree still has it. The later lost-wakeup fixes `4ba4214` and `ace0b39` are
  specifically preserved by leaving `single_flight.rs` out of the write set.

This is caller control only. `controlled_work` in `scheduler.rs` remains the
separate shared-work policy over `&WorkCancellation`; it is used by the leader's
memory, generator, and blocking operations. The service's external context
cancellation must not be converted into that shared token by this refactor.

## Refactor Steps

### Step 1: Reuse the scheduler-owned caller await policy

**Priority**: High
**Risk**: Low to Medium
**Source Lens**: elimination / exact duplication / misplaced private responsibility
**Files**: `src/artifacts/service.rs`, `src/artifacts/scheduler.rs`
**Story**: `refactor-centralize-artifact-await-control-step-1`

**Current State**:

```rust
// service.rs
async fn caller_controlled<T>(
    future: impl std::future::Future<Output = T>,
    deadline: Instant,
    cancellation: Option<&Arc<dyn CancellationSignal>>,
) -> Result<T> {
    tokio::select! {
        value = future => Ok(value),
        () = external_cancelled(cancellation) => Err(cancelled_error()),
        () = tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)) => Err(deadline_error()),
    }
}

async fn external_cancelled(cancellation: Option<&Arc<dyn CancellationSignal>>) {
    match cancellation {
        Some(signal) => signal.cancelled().await,
        None => pending().await,
    }
}

// scheduler.rs
async fn controlled<T>(
    future: impl std::future::Future<Output = T>,
    deadline: Instant,
    cancellation: Option<&Arc<dyn CancellationSignal>>,
) -> Result<T> {
    tokio::select! {
        value = future => Ok(value),
        () = external_cancelled(cancellation) => Err(cancelled_error()),
        () = sleep_until(deadline) => Err(deadline_error()),
    }
}
```

**Target State**:

```rust
// scheduler.rs: the existing implementation is the sole caller-control helper.
pub(crate) async fn controlled<T>(
    future: impl std::future::Future<Output = T>,
    deadline: Instant,
    cancellation: Option<&Arc<dyn CancellationSignal>>,
) -> Result<T> {
    tokio::select! {
        value = future => Ok(value),
        () = external_cancelled(cancellation) => Err(cancelled_error()),
        () = sleep_until(deadline) => Err(deadline_error()),
    }
}

// service.rs: import `controlled` from `scheduler` and use it at the existing
// frame-read, planning, and cache-lookup call sites. Remove only the local
// `caller_controlled` and `external_cancelled` definitions, plus `pending` and
// the service-only `CancellationSignal` import.
```

**Implementation Notes**:

- Change only `scheduler::controlled` visibility to `pub(crate)`; do not alter
  its select branches, `sleep_until`, `external_cancelled`, or error helpers.
- Add `controlled` to the existing scheduler import in `service.rs`; rename
  exactly three `caller_controlled(...)` calls to `controlled(...)`.
- Remove `future::pending` and `CancellationSignal` from the service imports.
  Keep `Instant`, `Result`, `cancelled_error`, and `deadline_error`: they are
  still used by the service's deadline setup/error paths and other code.
- Do not route `waiter.wait(...)` through `controlled`. Its loop registers and
  enables `Notify::notified()` before checking the result, then selects on the
  notification, caller cancellation, and deadline. This is the lost-wakeup
  fix from `4ba4214` and must remain local to `single_flight.rs`.
- Do not replace `WorkCancellation` with the external caller signal. The
  single-flight `Flight` retains one shared token, and `FlightWaiter::Drop`
  cancels it only when the last waiter leaves. Scheduler `controlled_work`
  continues to observe that token for shared work.
- No new abstraction module, alias, test helper, public API, or compatibility
  shim is warranted. The existing scheduler helper has the right signature and
  already owns the related permit-await policy.

**Acceptance Criteria**:

- [ ] `controlled` is the only caller-controlled deadline/external-cancellation
      helper in the artifact root modules; `service.rs` has no duplicate
      `caller_controlled` or external waiter.
- [ ] The three service awaits use scheduler `controlled` without changing the
      future, deadline, cancellation argument, select ordering, or `Result` /
      error propagation.
- [ ] `controlled_work` and `WorkCancellation` remain unchanged in role and
      call sites; a caller cancellation still stops only that caller's await,
      while last-waiter cancellation still suppresses shared publication.
- [ ] `single_flight.rs` retains the notify-aware registration/`enable()` loop,
      local external-cancellation waiter, and last-waiter shared cancellation
      semantics; no helper import obscures or bypasses that loop.
- [ ] Existing scheduler, service, and single-flight deadline/cancellation
      coverage remains green; no new test is added solely for moving a private
      helper.
- [ ] `cargo fmt --all -- --check` passes.
- [ ] `cargo test --workspace --all-targets --locked` passes.
- [ ] `cargo clippy --workspace --all-targets --locked -- -D warnings` passes.
- [ ] A structural review confirms the implementation write set is limited to
      `src/artifacts/service.rs` and `src/artifacts/scheduler.rs` and that no
      source/test behavior outside this extraction changed.

**Purity / Risk**:

- **Purity**: pure refactor. `pub(crate)` changes internal visibility only;
  there is no external API change. The helper's branch order, cancellation
  waiter, deadline conversion, and error constructors are retained byte-for-
  byte in behavior.
- **Risk**: low for behavior, with medium attention required at the async
  boundary. The failure mode is an accidental import/call-site mismatch or
  accidentally applying caller control to shared `WorkCancellation` work;
  existing tests and the explicit source checks cover both.

**Rollback**: Revert the single implementation commit. That restores the
service-local helper/imports and leaves the scheduler, notify-aware single-flight
waiter, and shared-work token behavior otherwise unchanged.

## Alternatives Considered

1. **Leave both helpers in place** — rejected because the deadline and external
   cancellation policy can drift, and the service copy has no ownership reason
   to exist.
2. **Create a new common `await_control` module** — rejected as extra surface
   and another owner for a policy already adjacent to scheduler permit control.
3. **Generalize `single_flight::FlightWaiter::wait` through the helper** —
   rejected because its notify registration/`enable()` ordering is a distinct
   lost-wakeup invariant, and its shared `WorkCancellation` last-waiter policy
   must remain visible and independent.
4. **Use `controlled_work` for service calls** — rejected because that would
   conflate caller cancellation with cancellation of shared generation work and
   would change the single-flight contract.

## Tests and Quality Gates

No source or test edits are part of design. Implementation retains the existing
scheduler cancellation qualification, service deadline/caller-cancellation
qualification (`service_tests.rs:409-437`), single-flight lost-wakeup test,
and last-waiter-only shared-cancellation test. Verification is the workspace
format, locked all-target test, and locked all-target Clippy gates listed in
Step 1, plus a direct structural check that the service has three imports/call
sites of scheduler `controlled` and no local duplicate.

## Implementation Order

1. `refactor-centralize-artifact-await-control-step-1` — make scheduler's
   existing caller-control helper `pub(crate)`, consume it from the service,
   remove the duplicate service helper/imports, and run all gates.

This feature depends only on its already-done artifact-generation parent. The
one child story has `depends_on: []`; the parent edge supplies the feature's
existing sequencing without introducing a dependency cycle.

## Implementation summary

The single checkpoint landed in `9bf998c`. `scheduler::controlled` is the sole caller deadline/external-cancellation await policy for scheduler request permits and the service's frame-read, planning, and cache-lookup futures. Its implementation is unchanged apart from crate visibility; service duplicates were removed. Shared `WorkCancellation`, `controlled_work`, and the notify-aware single-flight loop remain untouched. Rust 1.85 locked format, full workspace tests, and Clippy with warnings denied passed. The feature is ready for standard review.
