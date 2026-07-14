---
id: refactor-centralize-artifact-await-control-step-1
kind: story
stage: done
tags: [refactor, visual, storage]
parent: refactor-centralize-artifact-await-control
depends_on: []
release_binding: null
gate_origin: refactor-design
created: 2026-07-14
updated: 2026-07-14
---

# Reuse the scheduler-owned caller await policy

## Checkpoint

Remove the exact duplicate caller-controlled deadline and external-cancellation
await policy from `src/artifacts/service.rs` and reuse the existing scheduler
implementation. This is an internal behavior-preserving extraction: the
scheduler helper becomes `pub(crate)`, while its select policy and deadline
conversion remain unchanged.

## Current State

`service.rs:586-603` owns `caller_controlled` and a local
`external_cancelled` waiter. Its three callers are the frame read at
`:99-104`, planning at `:109-125`, and cache lookup at `:206-212`.

`scheduler.rs:189-199` already owns the same generic caller select for the
request permit at `:112-123`; its external waiter is at `:213-218` and its
`sleep_until` wrapper is at `:220-222`. Both policies select in this order:

1. underlying future succeeds;
2. external `CancellationSignal` completes and returns `cancelled_error()`;
3. the deadline elapses and returns `deadline_error()`.

The only source-level difference is that the service spells out the Tokio
sleep conversion while the scheduler calls its equivalent private wrapper.
The duplicate originated in `13e6464`; this story is grounded in the current
tree after artifact completion and after `4ba4214`/`ace0b39`.

## Target State

- `scheduler::controlled` is `pub(crate)` with its body, branch order,
  `external_cancelled`, `sleep_until`, and error helpers unchanged.
- `service.rs` imports `controlled` from `scheduler` and uses it at its three
  existing caller-controlled awaits.
- `service.rs` removes its local `caller_controlled` and
  `external_cancelled` functions, `future::pending`, and its now-unused
  `CancellationSignal` import.
- `controlled_work` remains the scheduler-owned helper for `&WorkCancellation`
  shared generation work; it is not substituted for caller control.
- `single_flight.rs` remains untouched. Its waiter still pins and enables
  `Notify::notified()` before the result check, selects on notification versus
  caller cancellation/deadline, and drops the shared `Flight` waiter so only
  the last waiter cancels shared work.

## Implementation Notes

1. Change only the visibility of `controlled` in
   `src/artifacts/scheduler.rs`.
2. Add it to the existing scheduler import in `service.rs` and rename exactly
   the three `caller_controlled` call sites.
3. Remove only the service-local duplicate functions and imports made unused
   by that removal.
4. Do not route `FlightWaiter::wait` through the helper. Its notify-aware loop
   is a separate lost-wakeup invariant, and its shared `WorkCancellation`
   semantics must remain explicit.

## Acceptance Evidence

- [ ] `service.rs` has no `caller_controlled` or duplicate external waiter;
      all three service call sites use scheduler `controlled`.
- [ ] Scheduler `controlled` is the sole caller-control implementation and its
      future, external cancellation, deadline branch order and errors are
      unchanged.
- [ ] `controlled_work`, `WorkCancellation`, and single-flight's Notify
      registration/enable loop and last-waiter cancellation behavior are
      unchanged in role.
- [ ] No public API, cache identity, manifest, artifact output, publication,
      retention, or persistence behavior changes.
- [ ] `cargo fmt --all -- --check` passes.
- [ ] `cargo test --workspace --all-targets --locked` passes, including the
      existing service deadline/cancellation and single-flight wakeup tests.
- [ ] `cargo clippy --workspace --all-targets --locked -- -D warnings` passes.

## Purity, Risk, and Rollback

**Purity**: behavior-preserving internal visibility/import consolidation. The
external caller's deadline and cancellation policy remains identical, and
shared-work cancellation remains owned by the single-flight token.

**Risk**: Low to medium. The only meaningful risk is accidentally changing the
async branch policy or confusing caller cancellation with `WorkCancellation`.
The exact current-source comparison and existing cancellation tests constrain
both risks.

**Rollback**: revert the one refactor commit to restore the service-local
helper and imports. No source outside the two artifact root modules and no
single-flight implementation needs to be reverted.

## Alternatives Rejected

- A new shared helper module adds ownership and surface without reducing
  concepts; the existing scheduler helper is the shortest owner.
- Reusing `controlled_work` would change shared-flight cancellation semantics.
- Generalizing `single_flight::wait` would risk the `Notify::enable()` lost-wakeup
  fix from `4ba4214` and would hide the last-waiter `WorkCancellation` policy.

## Implementation record

- Execution capability: baseline inline ownership; one exact private-policy consolidation across two files.
- Scheduler `controlled` is now crate-visible and serves request permits plus the service's frame read, planning, and cache lookup awaits.
- The scheduler select body, external cancellation waiter, deadline conversion, branch order, and error constructors are unchanged. Service-local duplicates and their unused imports were removed.
- `controlled_work`, `WorkCancellation`, and `single_flight.rs` were not modified; shared-work and notify-registration semantics remain explicit.
- Rust 1.85 locked format, full workspace all-target tests, and Clippy with warnings denied passed.

## Verification Gates

The implementation is a single atomic structural step. Run the workspace
format, locked all-target test, and locked all-target Clippy gates. Also perform
a direct source check for exactly three service uses of scheduler `controlled`,
absence of the service duplicate, unchanged `single_flight.rs`, and the
restricted two-file implementation write set. No test is added solely for
moving a private helper.
