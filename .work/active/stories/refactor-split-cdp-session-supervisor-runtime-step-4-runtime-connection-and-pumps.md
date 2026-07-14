---
id: refactor-split-cdp-session-supervisor-runtime-step-4-runtime-connection-and-pumps
kind: story
stage: done
tags: [refactor, browser]
parent: refactor-split-cdp-session-supervisor-runtime
depends_on: [refactor-split-cdp-session-supervisor-runtime-step-3-reconnect-transaction]
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Extract steady-state connection/runtime plumbing and leave a thin module root

## Checkpoint

Move connection bootstrap, reducer-effect application, the supervisor loop, target-event pumps, process watch, visibility/session-restore helpers, and target parsers into `crates/krometrail-cdp/src/session/runtime.rs`, leaving `session/mod.rs` as the connector/session composition root plus shared state and stable error mapping.

## Files

- `crates/krometrail-cdp/src/session/mod.rs`
- `crates/krometrail-cdp/src/session/runtime.rs`
- Session-local helper tests move only where adjacency is worth more than re-export churn.

## Current → target

```rust
// current: crates/krometrail-cdp/src/session.rs
async fn setup_connection(/* ... */) -> std::result::Result<ConnectionResources, CompatibilityProbeError>;
async fn apply_effects(/* ... */) -> Result<()>;
async fn run_supervisor(/* ... */);
async fn watch_process(/* ... */);
async fn pump_events(/* ... */);
```

```rust
// target: crates/krometrail-cdp/src/session/runtime.rs
pub(super) async fn setup_connection(/* unchanged signature */) -> std::result::Result<ConnectionResources, CompatibilityProbeError>;
pub(super) async fn apply_effects(/* unchanged signature */) -> Result<()>;
pub(super) async fn run_supervisor(/* unchanged signature */);
```

## Acceptance evidence

- Initial attach still restores Page/Runtime/Accessibility exactly once before the visibility probe.
- Event pumps still gate transport inputs by connection generation and feed the same reducer inputs.
- The managed-process watch still reports death through the same supervisor command path.
- `session/mod.rs` becomes materially smaller without introducing a new abstraction layer or changing public exports.

## Risk

Medium: this is the biggest remaining file move, but it is mostly mechanical once operations, shutdown, and reconnect have already been isolated.

## Rollback

Move the runtime helpers back into `session/mod.rs` and remove `runtime.rs` rather than leaving a half-split steady-state loop.

## Implementation notes

- Execution capability: highest; selected by the autopilot caller because the move spans bootstrap, reducer effects, generation-routed pumps, process watch, and the sole steady-state writer.
- Review weight: standard (caller); child checkpoint review is not applicable.
- Files changed: connection setup, domain restoration, visibility/target parsing, `apply_effects`, `run_supervisor`, process death signaling/watch, and event pumps moved from `session/mod.rs` to private `session/runtime.rs`.
- Tests added/removed: none; module-root tests continue to exercise helper seams through private parent visibility.
- Simplification: left `session/mod.rs` as connector/session composition and shared stable error mapping; no new wrapper or policy layer.
- Discrepancies from design: tests stayed consolidated with their existing shared fixtures; the established crate-local visibility error is explicitly re-exported (with a narrow unused-import allowance) to preserve the import surface.
- Adjacent issues parked: none.
- Verification: package all-target check; package all-target test (213 tests across 18 suites); package all-target Clippy with `-D warnings`; format check — all passed.