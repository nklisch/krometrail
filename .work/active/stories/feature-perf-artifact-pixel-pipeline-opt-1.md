---
id: feature-perf-artifact-pixel-pipeline-opt-1
kind: story
stage: done
tags: [perf]
parent: feature-perf-artifact-pixel-pipeline
depends_on: []
release_binding: null
gate_origin: perf-design
created: 2026-07-23
updated: 2026-07-23
---

# Inner-loop scalar rewrite: hoisted-u64, row-based, changed-only

Optimization 1 of the parent feature. Highest single-threaded win (3–6× per
pass), no public API change, byte-identical output. Prerequisite for opt-3
(which parallelizes these rewritten loops). See the parent feature body for the
overflow proof, profiling table, and full unit specs (Units 1.1–1.4).

## Scope

- New `PixelClassifier` in `measure.rs`: hoist `noise_floor² · WEIGHT_SUM` once
  per pair; classify per pixel in unchecked `u64` (exact by the documented
  `≤ 2.815e14 < 2^63` bound; `debug_assert!` + comment record the invariant).
  `WEIGHT_SUM`/channel weights become `u64` constants (values unchanged).
- Row-based rewrite of `measure_pixels` (`measure.rs:248-360`),
  `DifferenceAccumulators::accumulate` (`difference_map.rs:165-229`), and
  `accumulate_segment` (`motion_history.rs:363-416`): outer `y` / inner `x`
  loops (no per-pixel `div`/`mod`/`try_from`), per-row analysis-mask slices,
  coordinate/luminance/bounds computed **only for changed pixels**.
- Aggregate sums (`weighted_square_sum`, `absolute_sum`, `luminance_sum`,
  difference `weighted_time`) stay `u128` checked — they can reach ~3.3e20. Only
  the per-pixel classify + single-pixel `weighted_square` move to `u64`.

## Determinism

Byte-identical `MeasurementVector`, `DifferenceAccumulators`, `MotionHistoryPlan`,
and every artifact `output_hash`. Guards: existing
`identity_and_threshold_boundary_are_exact`,
`accumulation_is_exact_gap_aware_repeated_and_bounded`,
`accumulation_saturates_resets_at_gaps_and_respects_the_mask`, plus a new
full-u16-range classifier equivalence test and the perf scaffold's
`assert_equivalent` / `duplicate_run_equal`.

## Acceptance

- [ ] All parent Unit 1.1–1.4 acceptance criteria met.
- [ ] `cargo test -p temporal-vision` green; `cargo clippy -- -D warnings` clean.
- [ ] Benchmark shows per adjacent-pair pass 980 ms → 150–300 ms at identity.

## Implementation notes

Implemented the u64 classifier and all three row-based reducers. The full-u16
reference test, threshold-boundary test, masked/gapped tests, artifact digest
guards, and shared-vs-independent reducer assertions pass. The 120-frame
identity release scaffold measured 2,075,270 µs for normalization plus all
generators at one worker, with the shared accounting reduced to `M+B`.
