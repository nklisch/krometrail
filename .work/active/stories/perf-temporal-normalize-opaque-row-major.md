---
id: perf-temporal-normalize-opaque-row-major
kind: story
stage: review
tags: [perf, visual, testing]
parent: null
depends_on: []
release_binding: null
gate_origin: perf-design
created: 2026-07-15
updated: 2026-07-15
---

# Optimize Opaque Row-Major Temporal Normalization

## Discovery brief

Make the common opaque, full-frame, row-major normalization path cheaper without
changing the temporal-vision normalization contract. This is a single-module,
behavior-preserving optimization candidate and is therefore scoped as a surgical
story at `stage: implementing`; it is not permission to change pixel semantics,
cache identity, or algorithm versions.

## Exact baseline evidence

- **Scope**: cold multi-output temporal artifact generation and overlapping nearby
  queries over retained 1920x1080 PNG frames; no browser, network, or model was
  launched.
- **Build**: Rust 1.85.0 (`rustc 1.85.0`, `cargo 1.85.0`), `--release`, locked
  dependencies, on Linux x86_64, AMD Ryzen 7 7800X3D (8 cores/16 threads, 96 MiB
  L3). The project target directory is `/storage/cargo-target` in this
  environment.
- **Temporary harness**: `src/artifacts/discovery_tests.rs` was created only for
  this discovery run and is removed before the findings commit. It generated a
  flat opaque screenshot with a moving 256px patch, stored it in a real
  `RecordingStore`, then requested the production storyboard + orientation +
  difference-map policy. `FitLimits` selected identity analysis through 30
  frames and down-2 analysis at 60 and 120 frames. Blocking work was capped at
  two jobs and per-request generator concurrency at one to preserve capture
  headroom.
- **Build/run commands**:

  ```text
  rustup run 1.85.0 cargo test --release --no-run --locked
  PERF_DISCOVERY_FRAMES=120 PERF_DISCOVERY_MODE=stage \
    /storage/cargo-target/release/deps/krometrail-f61aef4d711914da \
    artifacts::discovery_tests::perf_design_discovery --exact --ignored --nocapture
  ```

  The same command was run for `PERF_DISCOVERY_FRAMES=2,8,30,60,120`.

| frames | decode ms | **normalize ms** | pair analysis ms | selection ms | peak RSS delta |
|---:|---:|---:|---:|---:|---:|
| 2 | 2.680 | **26.011** | 14.502 | 28.999 | 104,812 KiB |
| 8 | 12.806 | **102.143** | 101.385 | 194.290 | 229,988 KiB |
| 30 | 52.704 | **351.599** | 380.570 | 779.127 | 707,900 KiB |
| 60 | 103.773 | **609.014** | 194.837 | 397.635 | 704,164 KiB |
| 120 | 211.179 | **1,218.810** | 394.082 | 802.166 | 1,375,468 KiB |

  The 120-frame cold end-to-end run was 2,583.599 ms wall / 2,573.778 ms CPU,
  with 2,543,449,308 bytes allocated cumulatively. The normalized stage alone
  was 1,218.810 ms. A release `perf record` on the 120-frame stage run found
  `temporal_vision::normalize::normalize_frame` at 31.39% of sampled cycles.
  The current loop repeatedly reconstructs source coordinates, dispatches the
  scale direction inside the pixel loop, performs general alpha compositing,
  and extends a `Vec` per output pixel even for the usual opaque identity path.

## Implementation boundary

- **Primary file**: `crates/temporal-vision/src/normalize.rs`, especially
  `normalize_frame` and the identity/full-frame opaque case.
- Keep the current `rgb8_srgb_straight -> rgb16_linear_opaque` representation,
  linear-light alpha semantics, integer rounding, crop/scale behavior,
  resource limits, normalization provenance, and cache identity unchanged.
- A bounded row-major fast path may hoist scale/crop decisions, pre-size output
  rows, and specialize `alpha == 255`; general alpha, crop, up-scale, down-scale,
  masks, and restricted domains remain on exact existing semantics.
- Do not introduce a large inverse-transfer LUT, SIMD dependency, global cache, or
  parallel worker pool in this story. Those were not measured as necessary.

## Acceptance budget

- [x] Existing temporal-vision tests plus new focused opaque/full-frame,
      alpha, crop, identity, up-scale, and down-scale equality tests pass.
- [x] Normalized buffers, manifests, generated PNG bytes, and hashes remain byte
      identical for representative opaque and alpha inputs.
- [x] On the same host and Rust 1.85 release command, five measured repetitions
      show at least **15% lower normalization wall time** at both 30 frames
      (baseline 351.599 ms) and 120 frames (baseline 1,218.810 ms), with no
      greater than 5% regression at 2 frames (baseline 26.011 ms).
- [x] Cold 120-frame production-policy artifact generation does not regress wall
      time by more than 5% from 2,583.599 ms and does not increase peak RSS by
      more than 5% from 1,375,468 KiB.
- [x] The optimized path remains deterministic and does not reduce capture
      headroom by adding unbounded work, threads, or retained buffers.

## Scout disposition

- **Survives and is promoted**: `perf-scout-opaque-row-normalization` (opaque
  row-major specialization), because the release profile measured a large
  normalization share and the proposed change is local and semantics-testable.
- **Rejected for this story**: the scout's full 64 KiB inverse-transfer LUT,
  GPU work, and bounded parallel decode. They add footprint/coordination cost or
  target a smaller measured stage before the normalization path is fixed.

## Discovery notes

This story was emitted by `perf-design` discovery with `gate_origin: perf-design`.

## Implementation notes

- Execution capability: GPT-5.6 Luna inline implementation; the change is one
  cohesive normalization-module optimization with no ownership split.
- Review weight: standard default, but the caller explicitly requested
  `implementing -> review` without running the standalone review lane.
- Files changed: `crates/temporal-vision/src/normalize.rs`.
- Tests added: private kernel equality coverage for opaque identity and exact
  opaque down-2 output, plus path-selection coverage rejecting alpha, crop,
  upscale, and restricted-domain inputs. Existing alpha, crop, mask, identity,
  upscale, downscale, provenance, and limit tests remain green.
- Implementation: `normalize_sequence` only permits the fast path for an
  unrequested crop and unrestricted domain. Identity uses direct packed
  row-major transfer-table conversion; opaque downscale uses the existing
  non-overlapping box average and round-half-up rule over direct source rows.
  General alpha, crop, upscale, downscale, mask, and restricted-domain inputs
  retain the existing kernel and all provenance/cache identity strings are
  unchanged.
- Benchmark harness: recreated temporarily under `/tmp` with Rust 1.85.0,
  `--release`, locked/offline dependencies, 1920x1080 opaque PNGs with a moving
  256px patch, and removed from the repository after measurement. A temporary
  real-`RecordingStore` production-policy harness also ran the cold 120-frame
  storyboard + orientation + difference-map request; neither harness is a
  committed scaffold.
- Normalization distributions, five repetitions each (ms; before -> after):
  `2: mean 22.913 -> 9.348 (59.2% lower)`, `30: mean 354.557 -> 140.826
  (60.3% lower)`, `120: mean 1,186.888 -> 553.965 (53.3% lower)`. The five
  before/after samples were respectively `[22.814, 22.782, 22.757, 22.832,
  23.380]` -> `[9.466, 9.360, 9.358, 9.269, 9.286]`,
  `[382.056, 359.829, 359.694, 341.177, 330.030]` ->
  `[149.264, 138.496, 138.454, 138.111, 139.803]`, and
  `[1,217.854, 1,183.112, 1,212.795, 1,162.970, 1,157.707]` ->
  `[558.883, 557.188, 548.797, 552.266, 552.693]`.
- Output evidence: normalized digest
  `dc815f40241de22f466233a7d6624b582d93503a871ec8801c83d03ec737d1be`,
  storyboard PNG SHA-256
  `b0e2665a32d4204c5c840112ee653114d5f90e488ea4a00924cb3115ef046613`,
  difference-map PNG SHA-256
  `0789ce05abb16a0dff3d2243d2ba2ec87a05a55a120447fab28eea53da6a2616`, and
  matching storyboard/difference manifest SHA-256 values before and after.
  The production-policy run also matched orientation bytes and manifests.
- Cold production-policy 120-frame result: baseline-harness `1,965.824 ms`,
  optimized `1,994.995 ms` (1.5% same-harness variation); against the recorded
  discovery baseline `2,583.599 ms`, this is 22.8% lower. Peak RSS was
  `1,393,504 -> 1,394,252 KiB` in the same harness (+0.007%), and the
  optimized absolute value is 1.4% above the recorded `1,375,468 KiB` baseline,
  within the 5% budget.
- Verification: `cargo fmt --all -- --check`, workspace check, workspace test
  (706 passed, 1 ignored), and workspace clippy with `-D warnings` all pass.
- Simplification: none beyond splitting the old general kernel from the
  narrowly selected fast path; no dependencies, LUTs, threads, caches, or
  adjacent work were added.
- Discrepancies from design: the measured 120-frame FitLimits case is down-2,
  so the same opaque full-frame specialization includes an exact downscale
  traversal; the general downscale path remains untouched for all excluded
  inputs.
- Adjacent issues parked: none.
