---
id: epic-rust-cdp-capture-foundation-bounded-screencast-ingestion-real-chrome-fidelity
kind: story
stage: implementing
tags: [browser, testing]
parent: epic-rust-cdp-capture-foundation-bounded-screencast-ingestion
depends_on: [epic-rust-cdp-capture-foundation-bounded-screencast-ingestion-contract-remediation]
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-13
---

# Prove production capture fidelity with real Chrome

## Redesign note (2026-07-13)

The first implementation correctly stayed red after production Chrome disproved two assumptions. This story now depends on the contract remediation rather than directly on supervised wiring:

- `Page.screencastFrame.params.sessionId` is ack-only. Production Chrome 149 and both canonical final5 traces use constant token `1`; strict Chrome-token/source-sequence assertions are removed.
- Initial capture yielded zero frames because the initial visibility probe accepted only one cdpkit raw result shape. Remediation must resolve observed visibility before Ready.

This story validates Krometrail-owned `CaptureOrdinal`, not browser continuity. It must not modify production code to make the test pass.

## Scope

Finish the test-only, opt-in real-Chrome contract over the exact production connector, exact cdpkit 0.4.0 adapter, and existing `tests/fixtures/browser/cdp-transport-gate` fixture.

Keep all test-only sinks, deterministic ID source, observation helpers, and bounded wait logic inside `capture_real.rs`. Reuse the existing Chrome support helpers, `KROMETRAIL_REAL_CHROME_TESTS=1` gate, real-browser lock, unique profile guard, and CDP fault proxy without changing their ownership. Do not modify production code, fixture content, canonical final5 evidence, spike code, or support files.

The managed run proves fixed `SessionOrigin` precedes capture setup/first receipt, initial visibility is observed before Ready, and at least 30 non-empty JPEG frames reach the sink. Validate frame identity, target/session isolation, three clocks, Krometrail `CaptureOrdinal`, image/viewport metadata, scale, and shutdown cleanup.

A two-target run proves no cross-delivery or cross-target ordinal ownership. A capacity-one blocked sink proves acknowledgement continues ahead of bounded handoff and loss remains explicit. A proxy-sever run proves old-generation fencing, target identity preservation, higher attachment generation, continuous per-target Krometrail ordinal, and `BrowserDisconnected` evidence.

This is fidelity/lifecycle evidence, not a new performance qualification. Capture bounded ack-latency and cadence summaries as diagnostics only. Do not copy final5 thresholds or claim that Chrome's acknowledgement token detects missing frames. The dependent cross-platform feature owns Linux/macOS/high-DPI timing fidelity.

## Required file

- `crates/krometrail-cdp/tests/capture_real.rs`

This story owns only that test file. Production corrections belong to its remediation dependency.

## Acceptance criteria

- [ ] Initial visibility is observed from the production cdpkit raw result before session Ready; `Page.startScreencast` occurs only after Ready/Attached/Visible; managed Chrome yields at least 30 non-empty JPEG frames under a bounded timeout.
- [ ] The recorded `SessionOrigin` precedes subscriptions/start/first receipt. Frames have unique `FrameId`, expected `SessionId`/`TargetId`, strict Krometrail `CaptureOrdinal`, nondecreasing observed/session times (`>=`, equality allowed), session time not later than observed time, Chrome source time when supplied, coherent JPEG/viewport dimensions, positive device scale, and unchanged compressed bytes.
- [ ] No assertion treats CDP `params.sessionId` as a source sequence or requires it to change. If diagnostics inspect it, they only corroborate ack echo behavior and are not persisted as frame metadata.
- [ ] A two-page run proves cdpkit session scoping never crosses target identity, Krometrail ordinal ownership, queue status, or gap ownership.
- [ ] With capacity one and a blocked sink, acknowledgements and bounded ack-histogram samples continue before handoff; accepted depth remains bounded; dropped count and `IngestionQueueSaturated` become non-zero; no host-speed percentile is asserted.
- [ ] Releasing the sink lets accepted work drain; stopping before release returns/records bounded incomplete shutdown plus `CaptureStopped` without hanging or claiming flush.
- [ ] A real proxy sever cancels the old stream, records `BrowserDisconnected`, preserves exact `TargetId`, advances attachment generation, rejects old callbacks, and captures new frames whose Krometrail ordinal continues above the pre-disconnect maximum. No ordinal or token arithmetic claims to measure unknown browser-side loss.
- [ ] Explicit visibility opens/closes `TargetHidden` when Chrome emits it. If this platform emits no transition, record the limitation without inferring a visible-silence gap.
- [ ] Managed stop uses one aggregate deadline through capture stop/drain/flush, detach, `Browser.close`, and process termination, then leaves no profile-root process reference. Attached stop leaves external Chrome alive and responsive.
- [ ] All waits use explicit timeout plus condition predicates; no sleep establishes correctness, and no host path/endpoint/frame payload enters committed output.
- [ ] Status exposes bounded ack/cadence sample-count/p50/p95/p99/max diagnostics. The later cross-platform feature remains timing-fidelity authority.
- [ ] `KROMETRAIL_REAL_CHROME_TESTS=1 cargo test -p krometrail-cdp --test capture_real --locked -- --nocapture`, workspace fmt/check/test/clippy, no-default check, and cdpkit spike regression pass.

## Test adjustments after remediation

- Replace `assert_strict_sequence` with a helper that groups by target and asserts strict `capture_ordinal()` order.
- For reconnect, record the maximum pre-sever ordinal and require every restored frame ordinal to be greater while attachment generation separately advances.
- Retain all frame, clock, dimension, saturation, visibility, shutdown, attached-browser, and privacy assertions that do not depend on fabricated Chrome sequence semantics.
- Add no test asserting the acknowledgement token is always `1`; that observation corrects interpretation but is not a Krometrail compatibility guarantee.

## Execution

- Effective worker: highest.
- Depends on `...-contract-remediation`, which transitively depends on the completed wiring and engine.
- Review weight: standard at the parent feature; this production evidence informs feature-level review.

## Implementation history

- The first pass created `crates/krometrail-cdp/tests/capture_real.rs` only, with opt-in managed/attached Chrome, two-target/visibility, capacity-one saturation/incomplete-stop, and fault-proxy reconnect scenarios.
- It reused the existing lock, profile guard, endpoint/transport helpers, fixture bytes, and fault proxy. A local fixture server accommodates the committed fixture's absolute `/animation.js` path; a disposable headless wrapper keeps this test-only.
- Default-gated verification passed: workspace fmt/check/test/clippy, no-default check, spike regression, and repeated opt-in-disabled runs.
- Production verification remained red: Chrome 149 reproduced four zero-capture liveness failures because initial visibility did not accept the raw result shape. Independent live diagnostics plus canonical final5 Linux/macOS traces also invalidated the strict source-sequence assertion. The parent feature was bounced to drafting and no assertion was weakened.

## Handoff

After remediation lands, update only this test file, run the opt-in command against production Chrome, and record the observed evidence. The story remains `implementing` until all four real scenarios pass honestly; it must not advance on the existing default-gated verification alone.
