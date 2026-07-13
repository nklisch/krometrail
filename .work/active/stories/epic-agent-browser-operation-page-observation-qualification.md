---
id: epic-agent-browser-operation-page-observation-qualification
kind: story
stage: done
tags: [browser, agent-ux, testing]
parent: epic-agent-browser-operation-page-observation
depends_on: [epic-agent-browser-operation-page-observation-screenshots-live-observation]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Qualify structured page observation

## Checkpoint

Implement Unit 5 of the parent design. Extend the existing scripted transport and Chrome support instead of introducing duplicate fakes. Add one dependency-free page-observation target fixture and a production-connector integration test that exercises the complete operation port.

Default deterministic tests protect exact flat-session routing, actor completion in lifecycle failures, additive/malformed AX responses, generation/document/attachment invalidation, actionability failure, clip conversion, encoded-image rejection, read-only evaluation bounds, and live-observation partial failure without sleeps.

Opt-in real Chrome coverage under `KROMETRAIL_REAL_CHROME_TESTS=1` verifies fresh inspection, compact AX nodes/references, snapshot refresh and backing-node replacement, every screenshot scope, scroll conversion, shadow DOM, same-origin iframe behavior, default/forced scale reporting, and successful/degraded live observation. Record what Chrome actually reports: a host that ignores forced scale does not prove high DPI and must not fail through a fabricated expectation.

## Required files

- `crates/krometrail-cdp/tests/page_observation.rs`
- `crates/krometrail-cdp/tests/support/scripted_cdp.rs`
- `crates/krometrail-cdp/tests/support/chrome.rs`
- `crates/krometrail-cdp/tests/support/mod.rs`
- `tests/fixtures/browser/page-observation/index.html`
- `tests/fixtures/browser/README.md`

## Acceptance evidence

- [ ] Default tests cover stable operation/reference/error/provenance seams deterministically without requiring Chrome.
- [ ] Production Chrome proves actual AX-to-DOM resolution, dynamic replacement, scrolling, shadow/iframe behavior, screenshot dimensions/scale, and live observations on Linux.
- [ ] High-DPI/macOS claims remain limited to measured evidence; unavailable display behavior is reported/deferred honestly.
- [ ] The fixture is standalone target content with no product-runtime dependency.
- [ ] Workspace format/check/test/clippy locked gates and `krometrail-cdp --no-default-features --all-targets --locked` pass.

## Ordering

Depends on `epic-agent-browser-operation-page-observation-screenshots-live-observation`. This is the integrated acceptance checkpoint, not a separate test-team assignment.

## Implementation notes

- Extended the shared scripted CDP transport with ordered per-method responses/failures, complete command parameter capture, and an opt-in held-open event stream. Existing compatibility and named-event tests retain their original defaults; page-observation qualification does not introduce a second fake transport.
- Added production-connector deterministic coverage for exact flat-session routing, fresh inspection/history/layout, bounded side-effect-free evaluation, additive AX fields, generation replacement, loader invalidation, hidden-node actionability refusal, viewport-to-document clip conversion, malformed image/AX rejection, partial live observation, and prompt terminal-actor completion.
- Added the standalone dependency-free `page-observation` fixture with known geometry, a tall scrollable page, disabled/hidden/inert controls, repeated backing-node replacement, open shadow DOM, and same-origin iframe content. It remains target content and imports no Krometrail runtime.
- Added opt-in real Chrome coverage for fresh inspection/evaluation, actual AX-to-DOM references, shadow/iframe observation, viewport/full-page/reference/selector/viewport-region/document-region screenshots, measured dimensions and device scale, real backing-node replacement invalidation, snapshot refresh, and complete live observation.
- Added a forced-scale real-browser case that records the observed scale instead of assuming the flag worked. This Linux host honored the request and reported scale `2`; the test accepts and reports an honest positive measured value when another host ignores it. No macOS runner was available in this endpoint, so no macOS or Retina claim was invented.
- Real Chrome can transiently reject a capture while switching surface size between screenshot variants. The qualification performs at most one retry only for the contract's explicit retry-safe `screenshot_failed` outcome, then still requires a valid payload and provenance; persistent failures remain test failures.

## Verification

- `cargo test -p krometrail-cdp --test page_observation --locked -- --nocapture` — 8 default tests passed.
- `KROMETRAIL_REAL_CHROME_TESTS=1 cargo test -p krometrail-cdp --test page_observation --locked -- --nocapture` — 8 tests passed on Linux, including default and forced-scale real Chrome cases.
- `cargo test -p krometrail-cdp --all-targets --locked` — 153 tests passed across 15 suites.
- `cargo check -p krometrail-cdp --no-default-features --all-targets --locked` passed.
- `cargo fmt --all -- --check` passed.
- `cargo check --workspace --all-targets --locked` passed.
- `cargo test --workspace --all-targets --locked` — 221 tests passed across 22 suites.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` passed with no findings.
