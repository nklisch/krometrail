---
id: refactor-centralize-cdp-network-long-lived-classification
kind: story
stage: implementing
tags: [refactor, browser]
parent: null
depends_on: [epic-temporal-debugging-workflow-capture-and-browser-event-context]
release_binding: null
gate_origin: refactor-design
created: 2026-07-14
updated: 2026-07-14
---

# Centralize CDP network long-lived classification

## Brief

`crates/krometrail-cdp/src/events/normalize.rs` repeats the same semantic policy in three network normalization paths: `Network.requestWillBeSent` (around lines 417-421), orphan `Network.responseReceived` (around lines 491-497), and orphan `Network.loadingFailed` (around lines 562-568). Each independently decides that `NetworkResourceType::WebSocket` and `NetworkResourceType::EventSource` are long-lived. The resulting boolean controls both persistence metadata and network-quiet exclusion, so these copies can drift and make the finite-request policy harder to audit.

Extract one private helper in `normalize.rs` and route all three paths through it. Keep the core vocabulary, network wait behavior, event ordering, request-correlation limits, persistence shape, and all public contracts unchanged.

**Source lens**: missing abstraction / exact duplicate policy

**Rationale**: one local definition prevents drift in the long-lived exclusion rule used by both browser-event persistence and network-quiet fan-out, without introducing a core API or a second registry.

**Black-box classification**: pure refactor. Identical CDP inputs, normalized events, `NetworkActivity::long_lived()` values, wait outcomes, drop behavior, persistence rows, errors, and ordering must remain identical.

## Current State

The `WebSocket | EventSource` match appears once for a known request type and is repeated as two `Option` predicates for orphan response/failure paths. The orphan paths also independently re-normalize the raw resource type before applying the same policy.

## Target State

One private `is_long_lived(resource_type: Option<NetworkResourceType>) -> bool` helper in `events/normalize.rs` owns the exact `Some(WebSocket | EventSource)` rule. Known, response-created, and failure-created request contexts call that helper; no duplicate long-lived match remains.

## Acceptance Criteria

- [ ] The three current normalization paths use one private helper, with no change to the allowlisted resource-type mapping or the `NetworkActivity::long_lived()` contract.
- [ ] Redirect and out-of-order response/failure normalization, finite-request network quiet, long-lived exclusion, and generation-fenced routing retain their existing behavior.
- [ ] Existing focused CDP browser-event and waits/batches tests pass; no new test surface or public/API/schema/privacy policy is introduced.
- [ ] `cargo fmt --all -- --check`, `cargo check --workspace --all-targets --locked`, `cargo test --workspace --all-targets --locked`, and `cargo clippy --workspace --all-targets --locked -- -D warnings` pass.

## Risk and Rollback

**Risk**: Low. The helper is private and has no state; the only risk is accidentally changing the `Option` handling for orphan events or the two excluded resource types.

**Rollback**: Revert the implementation commit to restore the three local expressions. No migration, data rewrite, or compatibility step is required.

## Discovery Notes

- Scope: design `65d9f4c`; implementation commits `ea82451`, `64e7f48`, `f5e3056`, `1507e8b`, `8f866a2`; review-tree transition `52f225d`; current tree `5479a2f` where later progressive/refactor work changed shared store/session files. Focused on core browser events/privacy/context/ports, CDP events normalization/domain/pipeline/network/session/waits, v5 browser-event store retention/recovery/query, root composition, and their tests. Artifact implementation/refactors, progressive evidence, bundle work, and substrate-only changes were excluded.
- This story is deliberately blocked by the browser-event feature review so its shared CDP event files do not collide with the feature's review/remediation pass.
- Adjacent candidates rejected: removing the legacy external `ConsoleMessage`/`JavascriptException`/`NetworkLifecycle` timeline variants would change the public core enum and serialized timeline schema; unifying CDP pump/install functions would obscure distinct normalization/fan-out lifecycles; removing `selector_filter`'s infallible `Result` is too small to earn a standalone item; store projection/recovery helpers intentionally differ in insert-versus-repair semantics; manual store class/severity/clock encoders are persistence-boundary codecs, not safe policy deletion.
- No source, test, documentation, existing item, backlog item, or `.work/bin/work-view` was changed by discovery.
