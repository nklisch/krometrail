---
id: epic-rust-cdp-capture-foundation-bounded-screencast-ingestion-real-chrome-fidelity
kind: story
stage: done
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

- [x] Initial visibility is observed from the production cdpkit raw result before session Ready/start; managed Chrome yielded 30 non-empty JPEG frames under the bounded capture timeout. The first-frame diagnostic was 780x437 JPEG/viewport at scale 1.
- [x] The recorded `SessionOrigin` precedes first receipt. Frames had unique `FrameId`, expected `SessionId`/`TargetId`, strict per-target Krometrail `CaptureOrdinal`, nondecreasing observed/session times (`>=`, equality allowed), session time not later than observed time, 30/30 Chrome source timestamps in the managed run, valid JPEG headers, positive viewport/device scale, and unchanged non-empty encoded payloads.
- [x] No assertion treats CDP `params.sessionId` as a source sequence or requires it to change. The test persists only Krometrail frame metadata and uses no CDP acknowledgement-token continuity assertion.
- [x] A three-page run (initial page plus two sequentially created pages) produced 15 target-owned frames, exactly 5 per target, with zero gaps and no cross-delivery; strict ordinals, capture status/queue bounds, frame identities, and gap ownership were checked per target.
- [x] With capacity one and a blocked sink, the observed diagnostics were `received=4`, `acknowledged=4`, `accepted=2`, `dropped=2`, `queue_depth=1`, `ack_samples=4`, and `cadence_samples=3`; `IngestionQueueSaturated` carried a positive missing estimate. No host-speed percentile was asserted; the fixed-bucket p99 may exceed exact max.
- [x] Releasing the sink drained accepted work before a successful managed stop. A separate blocked run stopped before release with bounded `ShutdownIncomplete` and `CaptureStopped`; the test makes no claim that incomplete shutdown flushed.
- [x] A real proxy sever produced `BrowserDisconnected`, preserved the exact `TargetId`, advanced attachment generation from 1 to 2, fenced old callbacks, and captured 8 restored frames above the pre-sever maximum ordinal 20. No ordinal or token arithmetic claims to measure unknown browser-side loss.
- [x] Explicit visibility handling remains optional: the real Chrome run emitted no visibility transition, so the test recorded that limitation and inferred no hidden-silence gap. If Chrome emits a transition, the test requires a target-owned `TargetHidden` gap and a same-target visible/capturing recovery.
- [x] Managed stop exercised the connector's managed ownership and cleanup; attached stop left external Chrome responsive. Each real run ended with zero matching test processes or profile-data references; an intermittent empty test-root shell caused by test-only drop order is parked separately.
- [x] All test condition waits use explicit deadlines and predicates; no sleep establishes correctness, and diagnostics contain no host path, endpoint, or frame payload.
- [x] Status diagnostics exposed bounded ack/cadence sample counts and p50/p95/p99/max values. The later cross-platform feature remains timing-fidelity authority.
- [x] The opt-in capture suite passed four times end-to-end (`5 passed`, 13.65s, 13.51s, 13.62s, and final 13.52s), with zero matching process/profile references after each; default-gated capture_real passed; workspace fmt/check/test/clippy, no-default check, and cdpkit spike regression passed, including 76 spike-library tests.

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
- Contract remediation landed in production separately. This completion pass stayed test-only: it removed the pre-activation managed attach workaround, uses true managed `Launch` ownership for cleanup, keeps attached ownership for the multi-target/proxy cases, and adds only test-side visibility activation/probe helpers.
- The test now groups all fidelity and ordinal assertions by target, records the pre-sever maximum ordinal, requires restored ordinals above it plus a higher attachment generation, and treats the CDP screencast token as ack-only. It also corrected the test's invalid `p99 <= max` diagnostic assertion because production histograms expose bucket upper bounds.
- Real Chrome evidence after remediation: managed capture 30 frames/30 source timestamps at 780x437 and scale 1; two-target isolation passed with 15 frames (5 per target), zero gaps, and no visibility event on this headless Chrome; saturation passed with 4 received/acknowledged, 2 accepted, 2 dropped, queue depth 1, and 4 ack/3 cadence samples; proxy reconnect passed with old generation 1, restored generation 2, pre-sever maximum 20, and 8 restored frames. Four full opt-in suite runs passed 5/5, with zero matching process/profile references after each.
- Default-gated capture_real, workspace fmt/check/test/clippy, no-default check, and the 76-test cdpkit spike regression all passed.

## Handoff

After remediation, only `crates/krometrail-cdp/tests/capture_real.rs` and this story changed. The opt-in scenarios pass honestly; complete the remaining repository gates, then advance this story to `review` with the concrete command results and cleanup observations above.

## Review (2026-07-13)

**Verdict:** Approve

**Blockers:** none
**Important:** none
**Nits:** one test-only drop-order path can leave an empty known-prefix root shell after profile/process cleanup; parked as `idea-clean-real-chrome-test-root-drop-order`.

**Notes:** Fresh-context three-round review ran the opt-in production Chrome suite three more times (all 5/5), independently verified 30-frame fidelity, three-target isolation, exact saturation accounting, generation 1→2 reconnect with continuous ordinals, attached/managed ownership, explicit visibility limitation, and zero process/profile references. Workspace gates pass with 156 tests; no material finding remains.
