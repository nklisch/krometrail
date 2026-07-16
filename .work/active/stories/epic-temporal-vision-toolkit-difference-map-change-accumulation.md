---
id: epic-temporal-vision-toolkit-difference-map-change-accumulation
kind: story
stage: done
tags: [visual]
parent: epic-temporal-vision-toolkit-difference-map
depends_on: [epic-temporal-vision-toolkit-difference-map-rendering-foundation]
release_binding: 1.0.0
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Change-Model Extraction and Bounded Accumulation

## Checkpoint

Make the thresholded change test and the gap rule canonical by extracting them from `measure.rs` as `pub(crate)` helpers, then build the bounded per-pixel accumulator that folds every measurable adjacent pair into frequency, magnitude-weighted timing, and repeated-change state. This is the load-bearing unit of the feature: if it is correct and bounded, the panel rendering above it is mechanical.

## Files

- `crates/temporal-vision/src/measure.rs` (extract two `pub(crate)` helpers; refactor `measure_pixels`/`measure_pair` to call them — behavior-preserving)
- `crates/temporal-vision/src/difference_map.rs` (new domain types and accumulation kernel)
- `crates/temporal-vision/src/lib.rs` (add `mod difference_map;` and explicit exports for `FrequencyMode`, `TimePalette`, `DifferenceMapParameters`, `DifferenceMapLimits`)

## Extracted helpers (behavior-preserving)

```rust
// measure.rs — public API unchanged
pub(crate) struct PixelChange { pub changed: bool, pub weighted_square: u128 }

pub(crate) fn classify_pixel_change(
    before: &[u16; 3],
    after: &[u16; 3],
    parameters: MeasurementParameters,
) -> Result<PixelChange>;

pub(crate) fn intersecting_gap_count(
    gap_ranges: &[TimeRange],
    earlier: Timestamp,
    later: Timestamp,
) -> usize;
```

`classify_pixel_change` is the exact weighted-square test from `measure_pixels` (`weighted_square = 13_933·dr² + 46_871·dg² + 4_732·db²`, changed iff `> noise_floor² · 65_536`). `intersecting_gap_count` is the exact rule from `measure_pair`. The existing `measure_pixels`/`measure_pair` bodies call these helpers; their outputs and the existing `analysis.rs` tests must not change.

## New domain surface (exact signatures)

```rust
stable_registry! {
    pub enum FrequencyMode { Count => "count", Magnitude => "magnitude", NormalizedFrequency => "normalized_frequency" }
}
stable_registry! {
    pub enum TimePalette { Spectral => "spectral" }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DifferenceMapLimits { max_accumulator_bytes: NonZeroUsize, max_output_bytes: NonZeroUsize }
impl DifferenceMapLimits {
    pub const fn new(max_accumulator_bytes: NonZeroUsize, max_output_bytes: NonZeroUsize) -> Self;
}
impl Default for DifferenceMapLimits; // 256 MiB / 256 MiB

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DifferenceMapParameters {
    reference_frame_index: usize,
    frequency_mode: FrequencyMode,
    time_palette: TimePalette,
    repeated_change_separation: Option<Timestamp>,
    measurement: MeasurementParameters,
    background: Rgb8,
    limits: DifferenceMapLimits,
}
impl DifferenceMapParameters {
    pub const fn new(
        reference_frame_index: usize,
        frequency_mode: FrequencyMode,
        time_palette: TimePalette,
        repeated_change_separation: Option<Timestamp>,
        measurement: MeasurementParameters,
        background: Rgb8,
        limits: DifferenceMapLimits,
    ) -> Self;
    // read-only accessors
}
```

## Accumulation kernel (crate-private)

```rust
const ACCUMULATOR_BYTES_PER_PIXEL: usize = 48;

pub(crate) struct DifferenceAccumulators {
    dimensions: PixelDimensions,
    analysis_mask: Option<BinaryMask>,
    change_count: Box<[u32]>,          // above-threshold events
    comparable_count: Box<[u32]>,      // in-domain, non-gap events
    magnitude_sum: Box<[u64]>,         // Σ weighted_square (also the timing weight)
    weighted_time_sum: Box<[u128]>,    // Σ later_offset · weighted_square
    first_change_offset: Box<[u64]>,   // ns offset from range start
    last_change_offset: Box<[u64]>,    // ns offset from range start
}
impl DifferenceAccumulators {
    pub(crate) fn accumulate<F: Clone + Eq>(
        normalized: &NormalizedSequence<F>,
        measurement: MeasurementParameters,
        limits: DifferenceMapLimits,
    ) -> Result<Self>;
}

pub(crate) struct DifferenceMapData { /* accumulators + range/effective-separation/maxima */ }
impl DifferenceMapData {
    pub(crate) fn frequency_value(&self, pixel: usize) -> Option<u32>;
    pub(crate) fn is_repeated_change(&self, pixel: usize) -> bool;
    pub(crate) fn timing_offset(&self, pixel: usize) -> Option<u64>;
}
```

## Implementation notes

- Allocate the six arrays sized to `normalized.dimensions().pixel_count()` only after `pixel_count · 48 ≤ limits.max_accumulator_bytes`; otherwise `ResourceLimitExceeded`.
- For each adjacent pair `(i, i+1)`: if `intersecting_gap_count(...) > 0` skip the pair entirely. Otherwise compute `later_offset = later.timestamp() − range_start` (checked). For each in-domain pixel, call `classify_pixel_change`; if `changed`, increment `change_count`, add `weighted_square` (checked `u64`), add `later_offset · weighted_square` (checked `u128`), update first/last offset. Always increment `comparable_count` for in-domain pixels in a measurable pair.
- `range_duration_ns = last_frame.timestamp() − range_start`. `effective_separation_ns = repeated_change_separation.map(|s| s.as_nanos()).unwrap_or_else(|| max(1, range_duration_ns / 4))`.
- `is_repeated_change(pixel)` = `change_count ≥ 2 && last_offset − first_offset ≥ effective_separation_ns`.
- `timing_offset(pixel)` = `weighted_time_sum / magnitude_sum` (None when `change_count == 0`). Range-relative offsets keep `weighted_time_sum` bounded by range duration.
- Image-wide `max_change_count` and `max_magnitude` are computed in a final pass for `Count`/`Magnitude` scaling.

## Acceptance evidence

- `measure.rs` public outputs and the existing `analysis.rs` tests are unchanged after the helper extraction.
- A shared table-driven test pins `classify_pixel_change` to the existing kernel at below-floor, at-floor, one-over, and far-over deltas.
- `intersecting_gap_count` matches `measure_pair`'s `GapBoundary` decision on the same sequences.
- Masked-out pixels never increment any counter; `comparable_count` is zero for fully excluded pixels.
- Adversarial inputs fail with `ResourceLimitExceeded` instead of wrapping; accumulator allocation is rejected up front when `pixel_count · 48` exceeds the configured limit.

## Ordering constraints

Depends on `rendering-foundation` for `lib.rs` coherence only; the accumulation kernel itself does not import rendering code. Coordinate the `measure.rs` edit with `epic-temporal-vision-toolkit-normalization-and-measurements`: sequence this story after that feature reaches `done`, or have the orchestrator sequence the shared edit so the two features never hold conflicting `measure.rs` state.

## Implementation notes

- Execution capability: raised/high; integer overflow, gap semantics, and per-pixel memory bounds are load-bearing evidence contracts.
- Review weight: standard (autopilot caller).
- Files changed: `crates/temporal-vision/src/measure.rs`, `crates/temporal-vision/src/difference_map.rs`, `crates/temporal-vision/src/lib.rs`.
- Tests added/removed: exact accumulation/gap/repeated-change/allocation-bound regression added; all 34 package tests pass.
- Simplification: extracted the existing threshold and gap predicates once and retained one struct-of-arrays accumulator with no alternate change model.
- Discrepancies from design: none material; the internal generic does not require unnecessary `Clone + Eq` bounds.
- Adjacent issues parked: none.
