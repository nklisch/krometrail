---
id: epic-agent-browser-operation-browser-page-lifecycle-qualification
kind: story
stage: implementing
tags: [browser, agent-ux]
parent: epic-agent-browser-operation-browser-page-lifecycle
depends_on: [epic-agent-browser-operation-browser-page-lifecycle-navigation-observations]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Qualify browser and page lifecycle end to end

## Checkpoint

Implement Unit 5 of the parent design. Extend shared core/reducer/control test modules and `ScriptedCdp`; do not introduce another fake transport stack. Protect coherent status, profile semantics, exact selected-key transitions, command scope/JSON, synchronous target reconciliation plus duplicate events, activate-before-select, close fallback, navigation completion/history bounds, snapshot invalidation, interaction outcomes, cancellation, and non-replay deterministically without sleeps.

Add a dependency-free two-page lifecycle fixture. Through the production connector, opt-in real Chrome must prove temporary managed launch, initial status/selection, multiple distinct pages, create/select, new- and same-document navigation, reload, back/forward, stale references, selected/unselected close and fallback, stop, and process/profile cleanup. Retain focused named-profile persistence, attached-browser survival, capability-probed Electron renderer, and Node-inspector rejection tests.

Run the complete Rust quality gate. This story is verification of the parent feature, not a new runtime or implementation-worker split.

## Required files

- `crates/krometrail-core/src/browser/control.rs` tests
- `crates/krometrail-cdp/src/control/tests.rs`
- `crates/krometrail-cdp/tests/page_lifecycle.rs`
- `crates/krometrail-cdp/tests/support/scripted_cdp.rs`
- `crates/krometrail-cdp/tests/support/chrome.rs`
- `crates/krometrail-cdp/tests/support/mod.rs`
- `crates/krometrail-cdp/tests/chrome_session_real.rs`
- `crates/krometrail-cdp/tests/profile_ownership.rs`
- `tests/fixtures/browser/page-lifecycle/index.html`
- `tests/fixtures/browser/page-lifecycle/second.html`
- `tests/fixtures/browser/README.md`

## Acceptance evidence

- [ ] Default deterministic tests cover public contracts, reducer invariants, exact command routing, mutation ordering/idempotence, cancellation, and errors without browser timing.
- [ ] Opt-in real Chrome proves the complete managed start/status/page/navigation/history/close/stop workflow and exact target/reference behavior.
- [ ] Named reusable, temporary, external attach, capable Electron renderer, Node inspector, and ownership-correct stop retain focused evidence.
- [ ] Fixture files remain standalone documented browser targets and add no Krometrail runtime.
- [ ] Formatting, workspace check/test/clippy with locked dependencies, and no-default CDP compilation pass.

## Ordering

Final checkpoint after all contract, lifecycle, selected-target, and navigation work. Green verification advances this child directly to done and makes the parent eligible for feature-level review.
