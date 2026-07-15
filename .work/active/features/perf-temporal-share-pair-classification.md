---
id: perf-temporal-share-pair-classification
kind: feature
stage: drafting
tags: [perf, visual, testing]
parent: null
depends_on: []
release_binding: null
gate_origin: perf-design
created: 2026-07-15
updated: 2026-07-15
---

# Share Adjacent-Pair Classification Across Temporal Generators

## Discovery brief

Design a request-scoped, bounded way to reuse adjacent-pair classification and
measurements across storyboard selection, direct pair analysis, difference-map
accumulation, and any other generators that need the same normalized sequence.
This is a multi-site algorithm/data-model change, so it remains at
`stage: drafting` for a dedicated perf-design pass and benchmark design.

## Exact baseline evidence

- **Scope**: production temporal-vision kernels reached by cold multi-output
  artifact generation for retained 1920x1080 PNG frames. Three outputs were
  requested (storyboard, before/during/after orientation, difference map).
  No Chrome, network, or model was launched.
- **Build/host**: Rust 1.85.0 release build from locked dependencies on Linux
  x86_64, AMD Ryzen 7 7800X3D (8 cores/16 threads, 96 MiB L3).
- **Harness command** (temporary scaffold, removed before commit):

  ```text
  rustup run 1.85.0 cargo test --release --no-run --locked
  PERF_DISCOVERY_FRAMES=120 PERF_DISCOVERY_MODE=stage \
    /storage/cargo-target/release/deps/krometrail-f61aef4d711914da \
    artifacts::discovery_tests::perf_design_discovery --exact --ignored --nocapture
  ```

| frames | adjacent pairs | pair analysis ms | selection ms | difference accumulation + render + encode/hash ms | cold E2E wall ms |
|---:|---:|---:|---:|---:|---:|
| 2 | 1 | 14.502 | 28.999 | 97.605 | 170.771 |
| 8 | 7 | 101.385 | 194.290 | 177.763 | 493.052 |
| 30 | 29 | 380.570 | 779.127 | 499.621 | 1,538.006 |
| 60 | 59 | 194.837 | 397.635 | 246.818 | 1,318.018 |
| 120 | 119 | 394.082 | 802.166 | 465.979 | 2,583.599 |

  At 120 frames the standalone stage run spent 394.082 ms in
  `measure_adjacent`, 802.166 ms in storyboard selection, and 465.979 ms in
  the difference-map generator (which includes accumulation, raster rendering,
  PNG encoding, and output hashing). A sampled-cycle profile of the same Rust
  1.85 release run attributed 34.79% to
  `temporal_vision::measure::classify_pixel_change` and 24.48% to
  `temporal_vision::measure::measure_pair`; together those repeated comparison
  kernels were the dominant CPU evidence. The selection and difference paths
  independently classify the same normalized adjacent pairs.

## Required design questions

- Identify the smallest bounded trace or reusable measurement representation
  that preserves exact integer semantics, masks, declared gaps, tie ordering,
  changed-region bounds, timestamps, and deterministic manifests.
- Map every consumer and prove that a cache/trace hit cannot cross a visual epoch,
  normalization identity, measurement parameter, mask, or cancellation boundary.
- Decide whether selection needs the full measurement vector or a compact trace,
  and quantify retained bytes for 8/30/60/120 frames at identity and down-2
  analysis. Do not retain a speculative 100 MB-plus accumulator without a budget.
- Provide end-to-end and stage benchmarks, exact output/hash equality checks, and
  a fallback if trace construction costs more than the rescans it replaces.

## Acceptance budget for the implementation design

- [ ] A design benchmark reports pair-classification call/pass counts and wall,
      CPU, allocation, peak RSS, and cache-counter evidence for 8/30/60/120
      frames.
- [ ] The selected implementation must reduce cold 60- and 120-frame
      multi-output generation wall time by at least **20%** versus 1,318.018 ms
      and 2,583.599 ms baselines respectively, or the design records why the
      measured result does not justify implementation.
- [ ] Storyboard selections, difference accumulators, manifests, encoded bytes,
      and hashes remain byte identical across gaps, masks, tie timestamps, and
      identity/down-2 normalization.
- [ ] Peak RSS increases by no more than 8% at 120 frames and the existing
      scheduler/capture-headroom bounds remain intact.

## Scout disposition

- **Survives and is promoted**: `perf-scout-share-pair-classification`.
  Release profiling confirmed the repeated classifier/measurement kernels are
  the largest CPU family, so this is a higher-level algorithmic candidate than
  micro-optimizing the selector.
- **Deferred**: lazy difference accumulators and raster row maps remain
  profile-gated follow-ups; the current evidence does not separate their benefit
  from the duplicate pair work.
- **Rejected for this feature**: selector temporary cleanup, packed AoS
  accumulators, GPU residency, and persistent intermediate CAS; they either target
  small bounded work or add memory/lifecycle complexity before the duplicate scan
  is addressed.

## Discovery notes

This feature was emitted by `perf-design` discovery with `gate_origin: perf-design`.
It intentionally contains no implementation or committed benchmark scaffold.
