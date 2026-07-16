---
id: epic-temporal-vision-toolkit-normalization-and-measurements-direct-measurements
kind: story
stage: done
tags: [visual]
parent: epic-temporal-vision-toolkit-normalization-and-measurements
depends_on: [epic-temporal-vision-toolkit-normalization-and-measurements-normalized-sequence]
release_binding: 1.0.0
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Compute Exact Gap-Aware Visual-Change Measurements

## Checkpoint

Implement `crates/temporal-vision/src/measure.rs` and expose `MeasurementParameters`, exact measurement/output types, `measure_pair`, and `measure_adjacent` from `src/lib.rs`.

Use one deterministic per-pixel kernel over normalized linear RGB16. For absolute channel deltas `dr/dg/db`, compute `13_933*dr² + 46_871*dg² + 4_732*db²`; the weights sum to 65,536. A pixel is changed only when this value is strictly greater than `noise_floor² * 65_536`. The default floor is 512. Pixels at or below the floor contribute zero to every visual-change aggregate. Excluded transformed-mask pixels contribute to neither counts nor aggregates.

Publish a `MeasurementVector` containing checked absolute RGB-channel difference sum, exact changed/compared counts as `ChangedPixelProportion`, minimal half-open changed-region bounds, mean Rec.709 linear-luminance difference, mean per-channel color difference, and integer perceptual frame distance. Means use round-half-up integer division; perceptual distance is floor square root of the mean weighted square. Accumulate in checked `u128` and expose only bounded integer/rational output—no serialized floating-point values.

Every `FrameComparison` carries earlier/later indices and elapsed nanoseconds. `measure_pair` requires ordered valid indices. If any declared closed gap intersects the closed pair interval, return `ComparisonOutcome::GapBoundary` with its nonzero gap count and compute no pixel metrics. `measure_adjacent` uses the same pair function for every frame after the first, preserving zero elapsed time for timestamp ties.

`MeasurementParameters::provenance_step` returns `NormalizationKind::Thresholding`, version `weighted-linear-rgb-v1`, with the exact floor, comparison rule, integer weights, and below-floor-zeroing behavior. Artifact features append it to `NormalizedSequence::normalization_steps()` rather than reconstructing the parameters.

## Acceptance evidence

- Identical frames report exact zero values, zero changed count, nonzero compared count, and no changed bounds.
- A hand-computed tiny pair matches exact absolute sum, rational proportion, bounds, luminance/color means, and integer RMS distance.
- Threshold equality is unchanged and one-over is retained; masked pixels never affect output.
- Invalid/reversed indices fail; timestamp ties return zero elapsed time without reordering.
- Any intersecting gap returns an explicit boundary and never presents unseen time as measured stability.
- Output is direct and descriptive: no NaN/infinity/negative zero, inferred motion, defect label, or diagnosis.

## Ordering

Depends on `epic-temporal-vision-toolkit-normalization-and-measurements-normalized-sequence`, which owns the pixels, geometry, analysis mask, gap ranges, and base provenance.

## Implementation notes

- Execution capability: raised/high, selected by the autopilot caller because every artifact selection and rendering algorithm consumes these metrics.
- Review weight: standard (caller); child stories close on verification and the parent feature remains the review boundary.
- Files changed: `crates/temporal-vision/src/measure.rs` and `src/lib.rs`.
- Tests added: exact identity/threshold behavior and full-range integer-square-root boundaries; the public contract checkpoint adds integrated hand-computed vectors and gap/mask cases.
- Verification: focused library and existing contract tests pass with clippy warnings denied.
- Simplification: one checked integer kernel serves arbitrary and adjacent comparisons; no floating metrics, diagnostics, inferred motion, or alternate threshold implementation.
- Discrepancies from design: none.
- Adjacent issues parked: none.
