---
id: feature-perf-artifact-pixel-pipeline-opt-4
kind: story
stage: implementing
tags: [perf]
parent: feature-perf-artifact-pixel-pipeline
depends_on: [feature-perf-artifact-pixel-pipeline-opt-1, feature-perf-artifact-pixel-pipeline-opt-3]
release_binding: null
gate_origin: perf-design
created: 2026-07-23
updated: 2026-07-23
---

# Shared adjacent-pair classification across generators

Optimization 4 of the parent feature. The headline dedup: compute the
adjacent-pair classification once per `(normalized-sequence identity,
noise_floor)` cohort and share it, collapsing `4M + B` toward `M + B` (~2.5–4×
on multi-artifact requests). Largest surface, highest risk; sequenced last and
validated as incremental once the suite is already under the deadline. See parent
body Units 4.1–4.3 and Decision 1.

## Scope

- `temporal_vision::SharedAdjacentAnalysis` (Unit 4.1): `comparisons:
  Box<[FrameComparison]>` (select) + `change_masks: Option<PairChangeMasks>`
  (bit-packed per measurable pair; ≈ 17 MB at 119×1.17 M, budget-gated) built in
  one fused, opt-3-parallelized traversal (`analyze_adjacent_pairs`).
- Optional `Option<&SharedAdjacentAnalysis>` input on `generate_storyboard` /
  `select_storyboard_frames`, `render_difference_map` / `accumulate`,
  `build_motion_history_plan` / `generate_motion_history` (Unit 4.2). Absent =
  today's behavior exactly. select reuses `comparisons`; motion applies decay to
  masked pixels (zero classify); difference skips the threshold classify on
  unchanged pixels and recomputes `weighted_square` only for changed ones.
- Cohort detection + threading in `src/artifacts/service.rs::run_flight`
  (`334-520`) and `generators::generate` (`generators.rs:177-325`) (Unit 4.3):
  build the shared analysis once per epoch cohort sharing normalization identity
  + `noise_floor`; fall back per-generator when alone or when masks exceed the
  normalized-bytes budget. Strictly additive — never regresses a lone request.

Rejected alternative (documented in parent): full reduction fusion of difference
+ motion into the shared pass; rejected for coupling three generators' reduction
logic and motion's decay into one function.

## Determinism

With and without the shared analysis, every artifact's `output_hash`, manifest,
and image bytes byte-identical. Perf scaffold asserts `classified_pixel_passes`
collapses from `4M+B` toward `M+B` for the storyboard-difference-motion set.

## Acceptance

- [ ] Parent Unit 4.1–4.3 acceptance criteria met.
- [ ] Single-generator requests unchanged; mixed noise_floor / normalization
      falls back per-generator (no incorrect sharing).
- [ ] `cargo test` (workspace) green; clippy clean.

## Host attention

If cohort detection in `run_flight` proves too invasive, opt-1+opt-2+opt-3
already meet the deadline and `<1 s` target, so this story can be reviewed and
landed on its own merits or deferred without losing the headline wall-time result.

## Implementation notes

Added `SharedAdjacentAnalysis` with comparisons and optional changed-pixel
masks, threaded it through storyboard, difference-map, and motion-history
generators, and built it once for matching epoch/normalization/noise cohorts in
`run_flight`. Shared-mask permits remain held while the cohort cache is live;
alone, mixed, over-budget, and analysis-failure paths fall back to independent
generation. Public equivalence tests compare bytes, manifests, and output hashes
with and without sharing. The release benchmark reports `M+B` (119 shared
measurable pairs plus 80 storyboard baseline pairs) instead of the previous
`4M+B` accounting.
