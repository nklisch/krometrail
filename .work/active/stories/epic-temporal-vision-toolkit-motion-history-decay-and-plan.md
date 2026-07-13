---
id: epic-temporal-vision-toolkit-motion-history-decay-and-plan
kind: story
stage: implementing
tags: [visual]
parent: epic-temporal-vision-toolkit-motion-history
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Motion-History Decay Model, Gap-Aware Accumulation Kernel, and Plan

## Scope

Implement `MotionDecay`, `MotionHistoryParameters`, and `build_motion_history_plan` in
`crates/temporal-vision/src/motion_history.rs`, plus the additive `mod motion_history;`
and `pub use` exports in `src/lib.rs`. This story is the deterministic accumulation
kernel — render- and PNG-free — so it can be implemented and verified independently of
the shared render seam.

## Contracts to consume unchanged

- `FrameSequence`, `NormalizedSequence`, `MeasurementParameters`, `measure_adjacent`,
  `BinaryMask`, `PixelDimensions`, `PixelRect`, `TimeRange`, `Timestamp`, `Rgb8`,
  `ErrorCode`, `VisionError`, `Result`.
- The `pub(crate)` per-pixel change classifier `classify_pixel_change(parameters, before,
  after) -> bool` extracted verbatim from `measure_pixels` by the sibling difference-map
  feature (canonical weighted-square threshold). If motion-history lands first, expose the
  same helper additively in `measure.rs` in that canonical form.
- `NormalizedSequence::analysis_mask()` for domain restriction, exactly as `measure_pixels`
  uses it.

## Implementation notes

- Validate source/normalized alignment (frame count, ID order, timestamp order, dimensions)
  and `reference_frame_index < frame_count` before any allocation.
- Call `measure_adjacent(normalized, measurement)` once; group consecutive `Measured`
  pairs into continuity segments; each `GapBoundary` ends the current segment.
- Bound memory before allocation: the composite `u16` accumulator, the reused per-segment
  `u16` accumulator, and the two `ever_changed`/`outline` bit masks must fit
  `RenderLimits::max_canvas_bytes`; fail `ResourceLimitExceeded` otherwise.
- For each segment in source-pair order: rank `r = (segment_pair_count − 1) − k`;
  weight `decay.weight_at(r)`; skip zero weights; for each in-domain pixel apply
  `classify_pixel_change` and `segment_accum[p] = segment_accum[p].saturating_add(weight)`,
  setting `ever_changed[p]`.
- Composite per-pixel maximum into the `accumulation` buffer; clear `segment_accum` per segment.
- Outline = 4-connectivity boundary of `ever_changed`: a set pixel with at least one in-bounds
  unset 4-neighbor.
- All arithmetic checked/saturating `u16`/`u32`/`u128`; overflow is `ResourceLimitExceeded`,
  never wrap. No `f32`/`f64`, no diagnostic label, no inferred-motion claim.

## Acceptance evidence

- Source/normalized mismatch and out-of-range reference index fail before allocation.
- Decay curve is exact: rank 0 → `peak_intensity`; rank `half_life_ranks` → half;
  rank `live_window` and beyond → zero.
- Repeated within-segment change saturates at `u16::MAX`; cross-segment composite is the
  per-pixel maximum, never a cross-gap sum.
- `GapBoundary` starts a new segment and contributes no weight; gap count equals
  `source.gaps().len()`.
- Analysis-mask exclusion removes pixels from accumulation, `ever_changed`, and outline;
  `changed_pixel_count` matches `ever_changed` set bits.
- Outline is exactly the 4-connectivity boundary (isolated changed pixel is its own outline).
- Tiny `RenderLimits` fail explicitly; no arithmetic wraps.
- Identical input produces an identical serializable plan across runs; the plan carries no
  float and no inferred claim.

## Out of scope

Rendering, PNG encoding, manifest construction, and integration tests — those belong to
`epic-temporal-vision-toolkit-motion-history-rendering` and
`epic-temporal-vision-toolkit-motion-history-public-contract-tests`.
