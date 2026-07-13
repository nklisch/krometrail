---
id: epic-agent-browser-operation-page-observation-core-contracts
kind: story
stage: implementing
tags: [browser, agent-ux]
parent: epic-agent-browser-operation-page-observation
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Establish core page-observation contracts

## Checkpoint

Implement Unit 1 of the parent design. Add infrastructure-free structured snapshot/reference, page state, CSS geometry, screenshot request/metadata/payload, bounded read-only evaluation, and partial live-observation contracts to `krometrail-core`. Add the single macro-backed browser operation registry and extend `BrowserSessionPort` with the object-safe `execute` method.

The operation declaration must be the one growing source for kind, request/result association, stable name, mutability, evidence policy, and exhaustive tests. Seed only `inspect_page`, `snapshot_page`, `take_screenshot`, `evaluate_page`, and `observe_live`; later lifecycle/interaction work extends the same declaration. Do not add MCP schemas, CDP values, browser commands, or a default unavailable `execute` implementation.

Add stable, target-contextual `stale_reference`, `reference_not_actionable`, `page_observation_failed`, `screenshot_failed`, and `evaluation_failed` errors with the exact retry/recovery semantics from the parent. Add only workspace `serde_json` to core for explicit by-value evaluation results.

## Required files

- `Cargo.toml`
- `Cargo.lock`
- `crates/krometrail-core/Cargo.toml`
- `crates/krometrail-core/src/browser/operation.rs`
- `crates/krometrail-core/src/browser/observation.rs`
- `crates/krometrail-core/src/browser/mod.rs`
- `crates/krometrail-core/src/ports/browser.rs`
- `crates/krometrail-core/src/ports/mod.rs`
- `crates/krometrail-core/src/error.rs`
- `crates/krometrail-core/src/lib.rs`
- all deliberate fake `BrowserSessionPort` implementations needed to preserve compilation

## Acceptance evidence

- [ ] The one operation declaration generates every initial variant, result pairing, stable name, mutability/evidence definition, and exhaustive association test.
- [ ] Snapshot topology, reference scope, CSS geometry, timing, screenshot quality/payload, evaluation value, and live-part invariants reject malformed direct and deserialized values.
- [ ] Screenshot bytes remain an in-process `Arc<[u8]>` payload and are not serialized as a JSON byte array.
- [ ] Core has no Tokio, CDP, cdpkit, WebSocket, MCP, filesystem, or process dependency.
- [ ] Existing core tests and no-default CDP compilation pass after all port fakes implement the new method explicitly.

## Ordering

This is the first checkpoint. It creates the contracts consumed by every later checkpoint; it is not a standalone worker split from the feature.
