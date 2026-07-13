---
id: epic-rust-cdp-capture-foundation-bounded-screencast-ingestion-real-chrome-fidelity
kind: story
stage: review
tags: [browser, testing]
parent: epic-rust-cdp-capture-foundation-bounded-screencast-ingestion
depends_on: [epic-rust-cdp-capture-foundation-bounded-screencast-ingestion-supervised-wiring]
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-13
---

# Prove production capture fidelity with real Chrome

## Scope

Implement Unit 3 of the parent design as a test-only, opt-in real-Chrome contract over the exact production connector, exact cdpkit 0.4.0 adapter, and existing `tests/fixtures/browser/cdp-transport-gate` fixture.

Create all test-only sinks, deterministic ID source, observation helpers, and bounded wait logic inside `capture_real.rs`. Reuse the existing `tests/support/chrome.rs` helpers, `KROMETRAIL_REAL_CHROME_TESTS=1` gate, real-browser lock, unique temporary profile guard, and CDP fault proxy without changing their files. Do not modify production code, fixture content, final5 evidence, or support ownership.

The normal run launches managed Chrome, proves the fixed `SessionOrigin` was sampled before capture subscriptions/start/first receipt, waits for session/target readiness, and captures at least 30 non-empty JPEG frames. It validates frame identity, session/target isolation, nondecreasing clocks, strictly increasing official CDP frame numbers preserved as `source_sequence`, encoded header dimensions, viewport metadata, positive scale, and shutdown cleanup. The strictly increasing assertion is required real-browser provenance: the scripted candidate trace's constant fixture value is not Chrome evidence and is not used to reject the source-sequence contract. A two-target run proves no cross-delivery. A capacity-one blocked sink proves ack remains structurally ahead of saturated handoff while bounded histograms/counters continue. A proxy-sever cycle proves generation fencing and disconnect-gap closure.

This is fidelity/lifecycle evidence, not a new performance qualification. Record the bounded acknowledgement-latency and frame-cadence sample-count/p50/p95/p99/max status summaries, but apply only generous liveness deadlines and structural assertions. Do not copy final5 p99 thresholds or weaken its unchanged gate. Final5 remains transport-selection evidence; the dependent cross-platform feature owns the Linux/macOS/high-DPI timing-fidelity matrix.

## Required files

- `crates/krometrail-cdp/tests/capture_real.rs` (new)

This story exclusively owns one new test file and can leave all production files untouched.

## Acceptance criteria

- [ ] The recorded `SessionOrigin` happens-before frame subscriptions, `Page.startScreencast`, and first receipt; start is observed only after Ready/Attached state; managed real Chrome yields at least 30 non-empty JPEG frames under a bounded timeout.
- [ ] Frames have unique `FrameId`, one expected `SessionId`/`TargetId`, nondecreasing observed/session times (`next >= previous`, equality allowed), session time not later than observed time, Chrome source time when supplied, strictly increasing official frame numbers preserved as `source_sequence` within the generation, correct JPEG header dimensions, coherent viewport metadata, positive device scale, and unchanged compressed bytes at the sink. This live assertion—not a scripted fixture—grounds discontinuity handling.
- [ ] A two-page run proves session-scoped cdpkit delivery never crosses target identity, source sequence tracking, queue depth/status, or gap ownership.
- [ ] With capacity one and a deliberately blocked sink, the reader continues completing acknowledgements and accumulating bounded ack-histogram samples before handoff, accepted depth remains bounded, dropped count becomes non-zero, and an `IngestionQueueSaturated` gap with a non-zero exact estimate is observable. No host-speed percentile threshold is asserted.
- [ ] Releasing the sink lets accepted work drain; stopping before release instead returns/records bounded incomplete shutdown plus `CaptureStopped` without hanging or claiming flush.
- [ ] One real fault-proxy sever cancels the old stream, records `BrowserDisconnected`, preserves the exact target's `TargetId`, advances attachment generation, rejects old callbacks, and captures new-generation frames without cross-generation sequence comparison.
- [ ] Explicit visibility evidence opens/closes `TargetHidden` when Chrome emits it; if the platform does not emit the event for the fixture, the test records that limitation and does not infer a false visible-silence gap.
- [ ] Managed stop uses one aggregate deadline across capture stop/drain/flush, detach, `Browser.close`, and process termination; it leaves no process command line referencing the unique profile root and allows guard cleanup. Attached stop uses the same aggregate budget but leaves external Chrome alive and responsive.
- [ ] All waits have explicit timeouts and condition predicates; no sleep establishes correctness and no host path/endpoint/frame payload is written to logs or committed output.
- [ ] Status reports bounded ack/cadence sample-count/p50/p95/p99/max summaries for the run. They are captured as diagnostics only; the later cross-platform feature remains the timing-fidelity authority.
- [ ] `KROMETRAIL_REAL_CHROME_TESTS=1 cargo test -p krometrail-cdp --test capture_real -- --nocapture`, workspace format/check/test/clippy, `cargo check -p krometrail-cdp --no-default-features --all-targets --locked`, and cdpkit spike regression pass.

## Execution

- Effective worker: `highest`.
- Depends on complete supervised wiring and has no shared-file conflict with either earlier story.
- Review weight: `standard` at the parent feature; this story's evidence informs the feature-level independent review.

## Implementation notes

- Execution capability: `highest`; the story is a single test-only ownership surface, so the real-browser harness, bounded sinks, clocks, IDs, and evidence helpers remain together in `crates/krometrail-cdp/tests/capture_real.rs`.
- Review weight: `standard` from the parent feature; explicitly staged at `stage: review` as requested.
- Files changed: `crates/krometrail-cdp/tests/capture_real.rs` only. Production, support, fixture, spike, storage, temporal-vision, and foundation files are untouched.
- Tests added: opt-in managed/attached real-Chrome fidelity, two-target isolation and visibility evidence, capacity-one saturation/incomplete stop, and fault-proxy reconnect/generation tests; bounded JPEG header validation and in-memory/blocking sink assertions are local to the integration file.
- Simplification: reused the existing Chrome lock, profile-root guard, endpoint/transport helpers, fixture bytes, and fault proxy; no persistent sink, image dependency, action surface, visual analysis, or product timing threshold was added.
- Discrepancies from design: the fixture is served from its existing committed bytes by a local test-only HTTP server because its script uses an absolute `/animation.js` path; a disposable headless Chrome wrapper is used only when supported by the host so the existing animation produces a sustained real screencast without changing the fixture or launcher. No production claim or architecture document required updating.
- Adjacent issues parked: none.

## Verification notes

- Passed: `cargo fmt --all -- --check`, workspace `cargo check --workspace --all-targets --locked`, workspace `cargo test --workspace --all-targets --locked`, workspace clippy with `-D warnings`, `cargo check -p krometrail-cdp --no-default-features --all-targets --locked`, and `cargo test -p krometrail-cdp --features cdp-spike-cdpkit --test cdpkit_transport_contract --locked`.
- Passed repeatedly with the opt-in gate disabled: `cargo test -p krometrail-cdp --test capture_real -- --nocapture` (three runs); the existing Electron test reports its explicit endpoint skip.
- Review blocker: with `KROMETRAIL_REAL_CHROME_TESTS=1`, the installed Chrome 149 run against the unchanged supervised wiring leaves the target without a usable initial visibility transition, so the new live gate cannot claim its required 30-frame evidence. A separate local diagnostic (not landed) confirmed that once capture was forced past that wiring issue, Chrome's observed constant frame number is rejected by the strict live assertion; the scripted constant is never accepted. This is recorded for the review lane rather than weakened or converted into a pass.
