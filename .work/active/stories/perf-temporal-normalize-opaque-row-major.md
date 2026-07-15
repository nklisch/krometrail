---
id: perf-temporal-normalize-opaque-row-major
kind: story
stage: implementing
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

- [ ] Existing temporal-vision tests plus new focused opaque/full-frame,
      alpha, crop, identity, up-scale, and down-scale equality tests pass.
- [ ] Normalized buffers, manifests, generated PNG bytes, and hashes remain byte
      identical for representative opaque and alpha inputs.
- [ ] On the same host and Rust 1.85 release command, five measured repetitions
      show at least **15% lower normalization wall time** at both 30 frames
      (baseline 351.599 ms) and 120 frames (baseline 1,218.810 ms), with no
      greater than 5% regression at 2 frames (baseline 26.011 ms).
- [ ] Cold 120-frame production-policy artifact generation does not regress wall
      time by more than 5% from 2,583.599 ms and does not increase peak RSS by
      more than 5% from 1,375,468 KiB.
- [ ] The optimized path remains deterministic and does not reduce capture
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
No product optimization or committed benchmark scaffold has been applied.
