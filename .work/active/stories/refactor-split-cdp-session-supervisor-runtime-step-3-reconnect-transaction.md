---
id: refactor-split-cdp-session-supervisor-runtime-step-3-reconnect-transaction
kind: story
stage: implementing
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