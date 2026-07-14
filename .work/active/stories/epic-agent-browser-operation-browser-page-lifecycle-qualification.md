---
id: epic-agent-browser-operation-browser-page-lifecycle-qualification
kind: story
stage: done
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

## Implementation notes

- Execution capability: highest; qualification covers public contracts, serialized state, protocol ordering, cancellation, ownership, and real renderer behavior.
- Review weight: standard (caller); child checkpoints do not self-review.
- Files changed: shared `ScriptedCdp`, lifecycle fixture/helper, deterministic lifecycle suite, focused existing test migrations, and final formatting.
- Tests: 282 default workspace tests across 27 suites; 10 page-lifecycle tests with real Chrome enabled, including complete managed start/status/create/select/new- and same-document navigation/reload/back/forward/reference invalidation/unselected and selected close/stop, plus named-profile reopen persistence. Existing profile, compatibility, supervision, capture, and observation suites remain green.
- Gates: formatting, locked workspace check/test/clippy with `-D warnings`, and locked no-default CDP all-target compilation passed.
- Simplification: qualification extends the shared scripted transport and browser fixture helpers; no second fake protocol or runtime was introduced.
- Discrepancies from design: Electron lifecycle remains environment-gated by `KROMETRAIL_ELECTRON_ENDPOINT`; deterministic compatibility tests prove classification and Node-inspector rejection when no endpoint is available.
- Adjacent issues parked: none.
