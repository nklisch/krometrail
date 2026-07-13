---
id: epic-agent-browser-operation-page-observation-qualification
kind: story
stage: implementing
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
