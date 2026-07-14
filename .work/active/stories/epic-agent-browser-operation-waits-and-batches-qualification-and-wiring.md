---
id: epic-agent-browser-operation-waits-and-batches-qualification-and-wiring
kind: story
stage: implementing
tags: [browser, agent-ux, testing]
parent: epic-agent-browser-operation-waits-and-batches
depends_on: [epic-agent-browser-operation-waits-and-batches-batch-coordinator]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Wait and Batch Qualification and Wiring

## Checkpoint

Integrate the wait and batch routes through the production connector/supervisor, add a real-Chrome fixture and end-to-end qualification, and close the feature's full verification boundary. This story owns verification and composition wiring only; it does not register MCP tools, add storage, or add temporal query behavior.

## Files

- `crates/krometrail-cdp/tests/waits_and_batches.rs` (new) — real-browser and scripted integration tests.
- `tests/fixtures/browser/waits-and-batches/` (new or existing fixture extension) — delayed state/text, navigation, finite requests, and long-lived connection scenarios.
- `crates/krometrail-cdp/src/session.rs`, `crates/krometrail-cdp/src/control/mod.rs`, and composition exports — only final additive wiring and qualification fixes.

## Acceptance evidence

- Real Chrome demonstrates elapsed, text present/absent, element attached/visible/enabled/editable/checked state, navigation readiness/URL, boolean page condition, and explicitly requested network quiet. The network test demonstrates finite request completion plus the documented long-lived/pre-subscription limitations; no implicit network-idle assertion is added to navigation or interactions.
- Real Chrome demonstrates ordered navigation/interaction/wait batches, default stop, opt-in continuation, skipped steps, optional per-step screenshots, one final live observation, stale-reference failure, cancellation/deadline, and explicit degraded observation.
- Assertions inspect browser state and returned typed contracts: operation kinds, target identity, monotonic timings, child anchors, parent-batch correlation, statuses/outcomes, screenshot cardinality, final observation, stable errors, and no cross-target execution.
- Linux real-Chrome qualification passes. The existing macOS CI path runs deterministic and real-Chrome qualification where available; unavailable evidence is recorded honestly rather than weakened or fabricated.
- `cargo fmt --all -- --check`, `cargo check --workspace --all-targets --locked`, `cargo test --workspace --all-targets --locked`, and `cargo clippy --workspace --all-targets --locked -- -D warnings` pass. The existing browser lifecycle/page-observation/verified-interaction qualifications remain green.
- The story records exact verification commands, browser/platform evidence, any bounded environment limitation, and changed files before advancing directly to `done`.

## Implementation notes

Use the existing standalone browser fixture/test support and Chrome connector. Do not add MCP registration, SQLite/timeline writes, or a second fixture runtime. A real-browser failure is evidence to diagnose, not a reason to weaken an assertion.
