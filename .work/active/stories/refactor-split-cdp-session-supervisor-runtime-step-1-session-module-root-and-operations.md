---
id: refactor-split-cdp-session-supervisor-runtime-step-1-session-module-root-and-operations
kind: story
stage: done
tags: [refactor, browser]
parent: refactor-split-cdp-session-supervisor-runtime
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Convert `session.rs` into a module root and extract operation dispatch

## Checkpoint

Turn `crates/krometrail-cdp/src/session.rs` into `crates/krometrail-cdp/src/session/mod.rs`, create `crates/krometrail-cdp/src/session/operations.rs`, and move only the request-execution/page-result helpers there while keeping the crate-local `crate::session::{OperationExecutionContext, SessionShared, execute_operation}` surface intact for sibling modules.

## Files

- `crates/krometrail-cdp/src/session.rs` → `crates/krometrail-cdp/src/session/mod.rs`
- `crates/krometrail-cdp/src/session/operations.rs`
- `crates/krometrail-cdp/src/control/batch.rs` should need no semantic change because `session/mod.rs` re-exports the same crate-local items.

## Current → target

```rust
// current: crates/krometrail-cdp/src/session.rs
pub(crate) async fn execute_operation(/* ... */) -> Result<BrowserOperationResult>;
```

```rust
// target: crates/krometrail-cdp/src/session/mod.rs
mod operations;
pub(crate) use operations::{execute_operation, OperationExecutionContext};
```

```rust
// target: crates/krometrail-cdp/src/session/operations.rs
pub(crate) async fn execute_operation(/* unchanged signature */) -> Result<BrowserOperationResult>;
```

## Acceptance evidence

- The file-to-directory move lands atomically with the new `operations.rs` file.
- Batch dispatch, standalone page operations, and read-only execution still flow through the same logic and cancellation checks.
- Existing lifecycle/observation/interaction/waits-and-batches tests continue to pass without changing public or crate-local behavior.

## Risk

Medium: the Rust module-path move is structural but pervasive, and `control/batch.rs` depends on the crate-local import surface staying stable.

## Rollback

Collapse `session/mod.rs` and `session/operations.rs` back into one `session.rs` file and remove the submodule declaration if the module-root conversion causes churn.

## Implementation notes

- Execution capability: highest; selected by the autopilot caller because this private module move crosses the session supervisor's control, cancellation, and capture paths.
- Review weight: standard (caller); child checkpoint review is not applicable.
- Files changed: `crates/krometrail-cdp/src/session.rs` moved to `session/mod.rs`; request dispatch and page-result helpers moved to `session/operations.rs`.
- Tests added/removed: none; existing lifecycle, observation, interaction, and waits/batches integration suites remain the behavior contract.
- Simplification: isolated operation dispatch without new wrappers, traits, or caller imports.
- Discrepancies from design: none.
- Adjacent issues parked: none.
- Verification: `cargo check -p krometrail-cdp --all-targets --locked`; four focused integration suites (47 tests); package all-target Clippy with `-D warnings`; format check — all passed.