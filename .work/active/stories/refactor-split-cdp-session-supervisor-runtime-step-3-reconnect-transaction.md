---
id: refactor-split-cdp-session-supervisor-runtime-step-3-reconnect-transaction
kind: story
stage: done
tags: [refactor, browser]
parent: refactor-split-cdp-session-supervisor-runtime
depends_on: [refactor-split-cdp-session-supervisor-runtime-step-2-shutdown-runtime]
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Extract reconnect transaction control without changing policy

## Checkpoint

Move the reconnect-attempt helpers and `reconnect_loop_transactional` into `crates/krometrail-cdp/src/session/reconnect.rs` while preserving bounded target restoration, no-replay reconnect behavior, reconnect interruption handling, and reconnect-exhausted shutdown semantics.

## Files

- `crates/krometrail-cdp/src/session/mod.rs`
- `crates/krometrail-cdp/src/session/reconnect.rs`

## Current → target

```rust
// current: crates/krometrail-cdp/src/session.rs
struct AttemptCancellation { /* ... */ }
struct AttemptControl { /* ... */ }
async fn reconnect_loop_transactional(/* ... */) -> bool;
```

```rust
// target: crates/krometrail-cdp/src/session/reconnect.rs
struct AttemptCancellation { /* unchanged */ }
struct AttemptControl { /* unchanged */ }
pub(super) async fn reconnect_loop_transactional(/* unchanged signature */) -> bool;
```

## Acceptance evidence

- Reconnect backoff, per-attempt timeout, attach concurrency, target cap, cancellation, and process-death interruption remain unchanged.
- Operations received during reconnect still fail explicitly and are never replayed.
- Existing reconnect unit tests continue to cover target caps, concurrency limits, timeout/cancellation cutoffs, and process death before commit.

## Risk

High: reconnect is the highest-risk slice because it couples target restoration, reducer state reconstruction, and shutdown interruption behavior.

## Rollback

Re-inline the reconnect helpers into `session/mod.rs` as one block if any extraction subtly changes reconnect ordering or interruption semantics.

## Implementation notes

- Execution capability: highest; selected by the autopilot caller because reconnect combines bounded restoration, interruption, cancellation, and terminal cleanup.
- Review weight: standard (caller); child checkpoint review is not applicable.
- Files changed: the complete reconnect attempt/transaction block moved from `session/mod.rs` to private `session/reconnect.rs`.
- Tests added/removed: none; existing reconnect fixtures remain in the module-root test suite with narrow `pub(super)` access to their established seams.
- Simplification: grouped backoff, endpoint refresh, target restoration, staged effects, and exhausted handling without a new policy abstraction.
- Discrepancies from design: reconnect tests stayed in `session/mod.rs` to retain shared transport and endpoint fixtures.
- Adjacent issues parked: none.
- Verification: package all-target check; 93 library tests (including cap/concurrency/deadline/cancellation/process-death cases); session-supervision and capture suites (16 tests); package all-target Clippy with `-D warnings`; format check — all passed.