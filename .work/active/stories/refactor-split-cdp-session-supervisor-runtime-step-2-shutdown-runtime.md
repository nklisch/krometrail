---
id: refactor-split-cdp-session-supervisor-runtime-step-2-shutdown-runtime
kind: story
stage: implementing
tags: [refactor, browser]
parent: refactor-split-cdp-session-supervisor-runtime
depends_on: [refactor-split-cdp-session-supervisor-runtime-step-1-session-module-root-and-operations]
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Extract aggregate shutdown budgeting and terminal cleanup

## Checkpoint

Move the shutdown deadline/budget types and `perform_shutdown`/`finish_state` into `crates/krometrail-cdp/src/session/shutdown.rs` without changing capture flush order, detach ordering, managed-browser close behavior, process termination deadlines, or terminal state publication.

## Files

- `crates/krometrail-cdp/src/session/mod.rs`
- `crates/krometrail-cdp/src/session/shutdown.rs`
- Shutdown-focused unit tests move only if they otherwise require test-only re-exports.

## Current → target

```rust
// current: crates/krometrail-cdp/src/session.rs
enum ShutdownPhase { /* ... */ }
struct ShutdownDeadline { /* ... */ }
async fn perform_shutdown(/* ... */) -> Result<()>;
fn finish_state(/* ... */);
```

```rust
// target: crates/krometrail-cdp/src/session/shutdown.rs
pub(super) enum ShutdownPhase { /* unchanged */ }
pub(super) struct ShutdownDeadline { /* unchanged */ }
pub(super) async fn perform_shutdown(/* unchanged signature */) -> Result<()>;
pub(super) fn finish_state(/* unchanged signature */);
```

## Acceptance evidence

- The shutdown path still uses one absolute deadline across capture, detach, browser close, process termination, and completion.
- `flush_capture: false` for reconnect exhaustion remains intact.
- Managed versus attached ownership still controls whether `Browser.close` is attempted.

## Risk

Medium: the code is structurally separable, but shutdown ordering is load-bearing for capture durability and bounded cleanup.

## Rollback

Move the shutdown types and functions back into `session/mod.rs`; do not leave split ownership of shutdown policy.