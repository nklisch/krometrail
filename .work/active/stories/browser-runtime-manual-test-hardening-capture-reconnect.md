---
id: browser-runtime-manual-test-hardening-capture-reconnect
kind: story
stage: done
tags: [browser, testing]
parent: browser-runtime-manual-test-hardening
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Recover capture after terminal acknowledgement failure

Preserve one-shot acknowledgement truth while routing terminal acknowledgement failure through generation-fenced session reconnect. Acceptance is exact one-command/one-gap accounting followed by successful capture on the rebuilt attachment generation.

## Implementation notes

- Execution capability: host agent, high reasoning; the change crosses capture failure accounting and supervised reconnect but stays inside one existing lifecycle boundary.
- Review weight: standard (workflow default); child story checkpoints do not receive a separate review.
- Files changed: `crates/krometrail-cdp/src/capture/mod.rs`, `crates/krometrail-cdp/src/capture/pipeline.rs`, `crates/krometrail-cdp/src/capture/tests.rs`, `crates/krometrail-cdp/src/session/mod.rs`, and `crates/krometrail-cdp/tests/session_supervision.rs`.
- Tests added/changed: acknowledgement deadline coverage now proves one generation-fenced failure notification; scripted supervision proves one failed token command, one acknowledgement gap, reconnect, and later persistence on attachment generation two.
- Simplification: both frame-stream closure and terminal capture failure use one session-observer connection-loss helper; no acknowledgement retry or second reconnect authority was added.
- Discrepancies from design: the existing observer is synchronous, so the implementation adds a terminal failure callback rather than the sketched async failure signature.
- Adjacent issues parked: none.
- Tooling deviation: `.work/bin/work-view` is an x86-64 Linux binary unavailable on this macOS host; dependency readiness was verified directly from item frontmatter (`depends_on: []`).

## Verification

- `cargo test -p krometrail-cdp --lib capture::tests::acknowledgement_beyond_an_explicit_short_deadline_fails_once_with_one_gap --locked`
- `cargo test -p krometrail-cdp --test session_supervision failed_capture_acknowledgement_reconnects_without_retrying_the_token --features cdpkit-transport --locked`
