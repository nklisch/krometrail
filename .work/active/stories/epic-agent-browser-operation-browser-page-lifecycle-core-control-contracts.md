---
id: epic-agent-browser-operation-browser-page-lifecycle-core-control-contracts
kind: story
stage: implementing
tags: [browser, agent-ux]
parent: epic-agent-browser-operation-browser-page-lifecycle
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Establish lifecycle and page-operation contracts

## Checkpoint

Implement Unit 1 of the parent design. Add infrastructure-free profile persistence, coherent browser/page status, selected/direct page addressing, validated interaction anchors and outcomes, and lifecycle/navigation request/result values to `krometrail-core`.

Extend the existing macro-backed `BROWSER_OPERATION_REGISTRY`; do not create another operation taxonomy. The declaration must generate kind, stable name, request/result association, mutability, evidence policy, and browser/page scope for existing observation plus list/create/select/close/navigate/reload/back/forward operations. Migrate observation requests to shared `PageSelection` with direct-target convenience constructors.

Replace split `BrowserSessionPort` status getters with one object-safe `status()` snapshot while retaining session origin, subscribe, execute, and stop. Add only the stable navigation error required by the parent. Keep Tokio, CDP, process, endpoint, and filesystem implementation types out of core.

## Required files

- `crates/krometrail-core/src/browser/control.rs`
- `crates/krometrail-core/src/browser/operation.rs`
- `crates/krometrail-core/src/browser/observation.rs`
- `crates/krometrail-core/src/browser/session.rs`
- `crates/krometrail-core/src/browser/target.rs`
- `crates/krometrail-core/src/browser/mod.rs`
- `crates/krometrail-core/src/ports/browser.rs`
- `crates/krometrail-core/src/ports/mod.rs`
- `crates/krometrail-core/src/error.rs`
- `crates/krometrail-core/src/lib.rs`
- deliberate `BrowserSessionPort` fakes needed to preserve compilation

## Acceptance evidence

- [ ] Default/named/temporary/external profile status and selected/direct page values round-trip and reject malformed inputs.
- [ ] One registry exhaustively covers operation association, stable names, mutability, evidence, and browser/page scope.
- [ ] Interaction timing and state-changing-kind invariants reject invalid direct and deserialized values; post-anchor failures retain structured context.
- [ ] `BrowserStatus` is one coherent serializable contract and split component getters no longer invite torn status reads.
- [ ] Core remains runtime, transport, process, endpoint, and filesystem-implementation independent.

## Ordering

This is the first checkpoint. It defines the shared contracts used by lifecycle/profile status, reducer selection, navigation, later rich interactions, batching, and MCP. It is not a separate worker assignment.
