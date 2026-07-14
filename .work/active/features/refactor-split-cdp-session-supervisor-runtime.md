---
id: refactor-split-cdp-session-supervisor-runtime
kind: feature
stage: drafting
tags: [refactor, browser]
parent: null
depends_on: []
release_binding: null
gate_origin: refactor-design
created: 2026-07-14
updated: 2026-07-14
---

# Split the CDP session supervisor runtime into focused modules

## Brief

`crates/krometrail-cdp/src/session.rs` is now 3,626 lines and mixes at least four distinct responsibilities in one file: connector/bootstrap at `session.rs:1-220`, steady-state supervisor execution at `session.rs:971-1170`, reconnect orchestration at `session.rs:2108-2346`, and shutdown/process/event-pump machinery at `session.rs:2397-2668`. The page-lifecycle, waits-and-batches, MCP cancellation, and durable-memory capture work all extended this file, so edits in one slice now force readers through unrelated runtime code and increase the chance that reconnect/shutdown/session-edge changes drift together.

Extract the production session implementation into focused private modules under `crates/krometrail-cdp/src/session/` while keeping `ProductionBrowserConnector` and the feature-gated `crate::session` public surface unchanged. Preserve reducer ownership, single-writer supervision, exact reconnect/shutdown semantics, and existing session-focused tests.

**Source lens**: code smell / missing abstraction / elimination-first god-module split

**Rationale**: reduces coordination cost in the highest-churn browser-control adapter without changing behavior, and makes future reconnect/shutdown/session fixes auditable in smaller modules.

**Black-box classification**: pure refactor. Connector/session exports, runtime behavior, stable errors, logging fields, cancellation semantics, reconnect behavior, capture shutdown ordering, and test outcomes remain unchanged.

## Acceptance criteria

- [ ] `crates/krometrail-cdp/src/session.rs` becomes a focused module root (`session/mod.rs` or equivalent) that re-exports the same public entry points while moving reconnect, shutdown, and event-pump/process-watch helpers into private submodules.
- [ ] Reducer/application behavior remains identical: single-writer supervision, request cancellation, reconnect attempt handling, shutdown deadlines, managed/attached ownership, and capture sequencing are unchanged.
- [ ] Existing tests continue to exercise the same seams; no coverage is deleted, and session-focused tests move only as needed to match the new module layout.
- [ ] `cargo fmt --all -- --check`, `cargo test --workspace --all-targets --locked`, and `cargo clippy --workspace --all-targets --locked -- -D warnings` pass.

## Scope notes

- Keep this refactor inside `crates/krometrail-cdp/src/session*` and directly adjacent tests/helpers only.
- Do not redesign reconnect policy, shutdown policy, process-death detection semantics, or target reduction inputs in the same change.
- Prefer a small number of cohesive submodules over another layer of adapter indirection.
