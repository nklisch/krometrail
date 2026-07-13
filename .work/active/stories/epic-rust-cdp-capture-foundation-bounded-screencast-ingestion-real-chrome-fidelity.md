---
id: epic-rust-cdp-capture-foundation-bounded-screencast-ingestion-real-chrome-fidelity
kind: story
stage: implementing
tags: [browser, testing]
parent: epic-rust-cdp-capture-foundation-bounded-screencast-ingestion
depends_on: [epic-rust-cdp-capture-foundation-bounded-screencast-ingestion-supervised-wiring]
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Prove production capture fidelity with real Chrome

## Scope

Implement Unit 3 of the parent design as a test-only, opt-in real-Chrome contract over the exact production connector, exact cdpkit 0.4.0 adapter, and existing `tests/fixtures/browser/cdp-transport-gate` fixture.

Create all test-only sinks, deterministic ID source, observation helpers, and bounded wait logic inside `capture_real.rs`. Reuse the existing `tests/support/chrome.rs` helpers, `KROMETRAIL_REAL_CHROME_TESTS=1` gate, real-browser lock, unique temporary profile guard, and CDP fault proxy without changing their files. Do not modify production code, fixture content, final5 evidence, or support ownership.

The normal run launches managed Chrome, waits for session/target readiness, captures at least 30 non-empty JPEG frames, and validates frame identity, session/target isolation, clocks, source sequence, encoded header dimensions, viewport metadata, positive scale, and shutdown cleanup. A two-target run proves no cross-delivery. A capacity-one blocked sink proves ack remains prompt while bounded handoff drops explicitly. A proxy-sever cycle proves generation fencing and disconnect-gap closure.

This is fidelity/lifecycle evidence, not a new performance qualification. Use generous explicit timeouts, condition-based event/status waits, and no correctness sleeps. Do not copy final5 p99 thresholds or weaken its unchanged gate; final5 remains transport-selection evidence, while this story verifies production wiring. The dependent cross-platform feature owns Linux/macOS/high-DPI CI qualification.

## Required files

- `crates/krometrail-cdp/tests/capture_real.rs` (new)

This story exclusively owns one new test file and can leave all production files untouched.

## Acceptance criteria

- [ ] `Page.startScreencast` is observed only after Ready/Attached state, and managed real Chrome yields at least 30 non-empty JPEG frames under a bounded timeout.
- [ ] Frames have unique `FrameId`, one expected `SessionId`/`TargetId`, strictly nondecreasing observed/session times, session time not later than observed time, Chrome source time when supplied, increasing source sequence within the generation, correct JPEG header dimensions, coherent viewport metadata, positive device scale, and unchanged compressed bytes at the sink.
- [ ] A two-page run proves session-scoped cdpkit delivery never crosses target identity, source sequence tracking, queue depth/status, or gap ownership.
- [ ] With capacity one and a deliberately blocked sink, the reader keeps acknowledging promptly, accepted depth remains bounded, dropped count becomes non-zero, and an `IngestionQueueSaturated` gap with a non-zero exact estimate is observable.
- [ ] Releasing the sink lets accepted work drain; stopping before release instead returns/records bounded incomplete shutdown plus `CaptureStopped` without hanging or claiming flush.
- [ ] One real fault-proxy sever cancels the old stream, records `BrowserDisconnected`, preserves the exact target's `TargetId`, advances attachment generation, rejects old callbacks, and captures new-generation frames without cross-generation sequence comparison.
- [ ] Explicit visibility evidence opens/closes `TargetHidden` when Chrome emits it; if the platform does not emit the event for the fixture, the test records that limitation and does not infer a false visible-silence gap.
- [ ] Managed stop leaves no process command line referencing the unique profile root and allows guard cleanup; attached stop leaves external Chrome alive and responsive.
- [ ] All waits have explicit timeouts and condition predicates; no sleep establishes correctness and no host path/endpoint/frame payload is written to logs or committed output.
- [ ] `KROMETRAIL_REAL_CHROME_TESTS=1 cargo test -p krometrail-cdp --test capture_real -- --nocapture`, workspace format/check/test/clippy, no-default production check, and cdpkit spike regression pass.

## Execution

- Effective worker: `highest`.
- Depends on complete supervised wiring and has no shared-file conflict with either earlier story.
- Review weight: `standard` at the parent feature; this story's evidence informs the feature-level independent review.
