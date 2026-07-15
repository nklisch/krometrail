---
id: epic-prove-temporal-advantage-live-capture-and-system-qualification-duration-capture-timing-and-movement
kind: story
stage: implementing
tags: [testing, visual]
parent: epic-prove-temporal-advantage-live-capture-and-system-qualification
depends_on: [epic-prove-temporal-advantage-live-capture-and-system-qualification-opt-in-harness-and-live-run-contract]
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Qualify duration capture, clocks, gaps, and movement

## Checkpoint

Run the canonical temporal benchmark matrix through the real production capture path after the
opt-in/runtime contract exists. Cover every registered case, duration `16|33|50|100|200 ms`, and
capture repetition. Do not maintain a second case/duration list or infer evidence from intended
fixture timing.

## Exact implementation

Add the orchestration and measurement code under `src/app/live_evaluation/` (with the test-only
module declared by `src/app.rs`) and fixture observation tests under
`crates/krometrail-cdp/tests/temporal_benchmark_live.rs` or the shared qualification support
surface. Consume `temporal_evaluation::canonical_matrix`/`CaptureTrial` and the committed fixture
hashes. For each trial, navigate to the exact `temporal-benchmark/index.html?case=...&duration_ms=...`
URL, reset through the existing structured control, click `#run` through the production operation
port, and wait for the fixture's observable running/settled condition. Resolve one interaction-
anchored `SourceInterval` with a bounded window covering the lead-in, active interval, reversal or
correction, and final settle.

Build the interval only from production `FrameSource`, `CaptureGapStore`, and timeline/control
records. Preserve source timestamp, frame observed time, normalized session time, capture ordinals,
source IDs, exact frame hashes, retention availability, and declared gap IDs without ordinal gap
inference or repair. Add per-duration metrics for eligible trials, observed fixture-state trials,
retained frame counts, source-time samples, and gap coverage. A complete measured rate below the
applicable EVALUATION criterion is `fail`; missing/corrupt/retention-interrupted evidence is
`inconclusive` with its existing failure code.

Implement `TemporalFixtureObservation` as a test-only, dependency-free-from-the-product helper
that decodes actual retained frame bytes and applies committed, geometry-synchronized pixel
predicates. Cover baseline/changed/final reachability, flicker, layout shift, teleport, reversal,
and stable motion. Return `Unknown` on image decode, viewport/scale, geometry, or predicate
uncertainty. Never modify frames, render a replacement, expose labels to the browser, or claim
that a state was captured when no retained frame proves it. Add fixture/definition drift tests
that fail before any browser launch.

Record the actual viewport and device scale in the capture summary. The canonical profile requires
800x450 and scale one; wrong observed metrics block the profile and do not get normalized. Do not
add high-DPI setup or a high-DPI threshold here; preserve that evidence gap for the platform
feature.

## Acceptance evidence

- [ ] Scripted tests prove matrix ordering, interval-window construction, source/observed/session
      clock separation, declared gap propagation, and exact manifest row identities.
- [ ] Real-run code uses the one production capture session/recording authority and observable
      fixture barriers, not sleeps or intended-duration assumptions.
- [ ] All canonical cases/durations/repetitions are represented; movement reversal, teleport,
      flicker/layout, and stable cases have explicit state observations.
- [ ] Pixel observation returns unknown on decode/scale/geometry mismatch and cannot turn missing
      source frames or declared gaps into passing evidence.
- [ ] A wrong viewport/device scale produces an honest blocked result; no high-DPI claim is made.
- [ ] Tests use no browser unless the feature-specific opt-in is present; design/ordinary
      verification does not launch Chrome.

## Ordering

This checkpoint depends on the opt-in harness and manifest contract. Control reliability must wait
for its interaction-anchor and settled-capture data, so the next child depends on this one.
