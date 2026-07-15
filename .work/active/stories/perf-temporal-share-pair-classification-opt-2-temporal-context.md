---
id: perf-temporal-share-pair-classification-opt-2-temporal-context
kind: story
stage: implementing
tags: [perf, visual, testing]
parent: perf-temporal-share-pair-classification
depends_on: [perf-temporal-share-pair-classification-opt-1-baseline-equivalence]
release_binding: null
gate_origin: perf-design
created: 2026-07-15
updated: 2026-07-15
---

# Build One Bounded Temporal Pair-Analysis Context

## Purpose

Replace duplicate adjacent-pair classifier traversals with one deterministic
row-major traversal that fans the canonical integer result into the requested
selection, difference, and motion consumers. This story owns the
browser-independent `temporal-vision` computation only; service grouping and
scheduler wiring are the next story.

## Chosen shape

Introduce a crate-private request-local pair-analysis context. It retains the
existing ordered `Box<[FrameComparison]>` (80 bytes per current record) and
optional consumer-local cores. It does not retain normalized pixels, a
per-pixel bitmap, weighted magnitudes, or a persistent intermediate cache.
Difference and motion cores are built in place while the one pixel traversal is
hot, then renderers consume those cores without rescanning.

At 120 frames the trace payload is 9,520 bytes and the bounded context budget
is 9,584 bytes. Identity/down-2 use the same trace budget. Existing output
working memory remains separately bounded: one difference core is 99,532,800
bytes at 1920x1080 and one motion core is 8,812,800 bytes; these are not trace
bytes and must be accounted by the caller.

## Implementation units

### Unit 1: Canonical pair event and builder

**Files**:

- `crates/temporal-vision/src/measure.rs`;
- `crates/temporal-vision/src/pair_analysis.rs` (new if it is the clearest
  cohesive home);
- `crates/temporal-vision/src/select.rs`;
- `crates/temporal-vision/src/difference_map.rs`;
- `crates/temporal-vision/src/motion_history.rs`.

Use the existing checked classifier and aggregate semantics. A measurable pair
emits one `FrameComparison`; every included pixel is classified once and fans
out unchanged `weighted_square`, changed state, channel/luminance deltas, and
coordinates to the selected cores. A gap emits the existing elapsed metadata
and `GapBoundary` outcome and performs no pixel work. Preserve `u128`
intermediates, checked conversions, changed bounds, exact rational counts,
round-half-up means, integer square root, later-frame timestamps/offsets,
per-segment reset, saturating motion weights, and four-connectivity outline.

Split accumulation from rendering so a difference map can consume an existing
core. Split motion plan construction similarly. Keep public `measure_pair`,
`measure_adjacent`, and selector signatures unchanged; add only an internal
precomputed path or adapter.

### Unit 2: Exact regression coverage

Add focused tests for clean, masked, gapped, equal-timestamp, threshold-floor,
identity, and down-2 sequences. Compare the context path with the old direct
path for:

- all `FrameComparison` values and visual summary moments;
- selection IDs, reasons, omitted anchors, role indices, and tie ordering;
- difference/motion core values and changed bounds;
- manifests, encoded PNG bytes, output hashes, and normalization steps.

Cancellation checkpoints may be generic crate callbacks, but the visual crate
must remain browser/runtime independent. A failed checkpoint drops the partial
context and cannot return a publishable artifact.

## Compatibility invariants

The context is valid only for one exact normalized sequence, ordered frame
identity/timestamps, geometry epoch, transformed analysis mask, gap set, and
`MeasurementParameters`. It must never be reused across a visual epoch,
normalization identity, mask, measurement parameter, request, or cancellation
boundary. Selector baseline comparisons remain explicit non-adjacent work;
do not introduce an O(N²) trace.

## Acceptance criteria

- [ ] One context traversal replaces adjacent classifier passes for every
      requested consumer while leaving non-adjacent selector baseline calls
      unchanged.
- [ ] Direct and context paths are byte/value identical for all required gaps,
      masks, timestamps, changed bounds, threshold boundaries, tie ordering,
      manifests, PNGs, hashes, and identity/down-2 normalization cases.
- [ ] Current trace allocation is at most `80 * (N - 1) + 64` bytes and never
      reaches 100 MB; consumer cores retain their existing explicit limits.
- [ ] Context construction checks cancellation/deadline boundaries, preserves
      checked overflow errors, and has no publication or global-cache side
      effects.
- [ ] The shared benchmark from story 1 reports the predicted reduction in
      pair passes/classifier calls before any service integration is claimed.

## Non-goals

Do not change normalization, algorithm versions, public artifact schemas,
Chrome/models, scheduler grouping, persistent caches, GPU paths, or parallel
execution.

## Dependency and handoff

Depends on `perf-temporal-share-pair-classification-opt-1-baseline-equivalence`.
The service/scheduler wiring story may consume the context only after these
pure-kernel exactness tests pass.
