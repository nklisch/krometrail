---
id: perf-temporal-normalize-opaque-row-major
kind: story
stage: done
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

- [x] Existing temporal-vision tests plus a multi-row, nontrivial rectangular
      opaque fixture compare the optimized full-domain kernel with the preserved
      general/reference kernel at identity and downscale factors 2, 4, and 8.
- [x] The factor-4 comparison also runs the storyboard/orientation PNG and
      manifest/hash path; buffers, PNG bytes, manifests, and output hashes match.
- [x] On the same host and Rust 1.85 release scaffold, five repetitions show
      lower normalization wall time at 30-frame identity and 120-frame down-2;
      the two-frame identity run has no greater than a 5% regression.
- [x] The committed production-policy scaffold is ignored by ordinary tests,
      accepts frame-count/repetition parameters, reports exact output digests,
      and performs no browser, network, or model work.
- [x] RSS methodology and repeated E2E distributions are recorded without
      turning a browser-free scaffold result into a cross-harness claim. The
      optimized path remains deterministic and adds no unbounded work, threads,
      or retained buffers.

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

- Execution capability: GPT-5.6 Luna inline review-fix implementation; the
  accepted comments remain within the existing normalization story.
- Review weight: standard bounded review already completed; the caller requested
  no re-review and host approval at `stage: review`.
- Files changed: `crates/temporal-vision/src/normalize.rs`,
  `crates/temporal-vision/tests/temporal_normalize_perf.rs`, and this story.
- Tests added: a 40x24 opaque rectangular, five-frame fixture compares every
  normalized RGB16 buffer with `normalize_frame_general` at identity and down-2,
  down-4, and down-8. The down-4 case also compares storyboard and orientation
  PNG bytes, manifests, and manifest output hashes. Alpha, crop, mask, upscale,
  and restricted-domain selection tests remain unchanged.
- Implementation: unchanged behavior boundary. `normalize_sequence` only permits
  the fast path for an unrequested crop and unrestricted domain. Identity uses
  direct packed row-major transfer-table conversion; opaque downscale uses the
  existing non-overlapping box average and round-half-up rule over direct source
  rows. General alpha, crop, upscale, restricted-domain, and excluded downscale
  inputs retain the old kernel and all provenance/cache identity strings.
- Reproducible benchmark scaffold: committed as the ignored integration test
  `crates/temporal-vision/tests/temporal_normalize_perf.rs`. It uses only
  in-memory opaque frames and the production storyboard + orientation +
  difference-map policy. `PERF_TEMPORAL_FRAMES`,
  `PERF_TEMPORAL_REPETITIONS`, `PERF_TEMPORAL_SCALE`, width, and height are
  parameterized; every repetition prints normalization time, E2E time, RSS/HWM
  readings, normalized digest, three manifest-plus-PNG digests, and a combined
  output digest. `#[ignore]` keeps it out of ordinary tests, and it never
  launches Chrome, opens a network connection, or calls a model.
- Benchmark command and harness: build/run with
  `rustup run 1.85.0 cargo test -p temporal-vision --release --locked
  --test temporal_normalize_perf -- --ignored --exact
  production_policy_release_profile --nocapture`, setting
  `PERF_TEMPORAL_FRAMES` and `PERF_TEMPORAL_REPETITIONS`. Baseline runs used the
  isolated parent-commit worktree with the byte-identical scaffold and separate
  release target directories; optimized runs used this worktree. All five-run
  output digests matched: for 120/down-2 the combined digest was
  `d8492bc099fe1042f78d6cb65afae9c9d03e49e4b160ae85ae5f3589a5e587ae`.
- Normalization distributions (ms, baseline -> optimized; five repetitions):
  `2/identity: [21.730, 21.591, 19.882, 19.679, 19.823] ->
  [10.108, 10.033, 7.660, 7.710, 7.766]`, mean `20.541 -> 8.655`
  (`57.86%` lower); `30/identity: [325.224, 323.899, 297.560, 298.858,
  300.455] -> [152.169, 140.248, 118.709, 119.078, 119.501]`, mean
  `309.199 -> 129.941` (`57.97%` lower); `120/down-2: [1,246.347,
  1,252.786, 1,189.855, 1,148.193, 1,181.338] -> [573.015, 555.667,
  475.773, 466.980, 490.406]`, mean `1,203.704 -> 512.368` (`57.43%`
  lower). These are the same-harness normalization-stage measurements.
- Production-policy E2E distributions (ms, including normalization and all
  three in-memory outputs) were `30/identity: [2,576.359, 2,598.935,
  2,543.180, 2,547.086, 2,579.035] -> [2,429.041, 2,412.906, 2,371.676,
  2,370.815, 2,391.060]`, mean `2,568.919 -> 2,395.100`; and `120/down-2:
  [3,478.484, 3,444.250, 3,361.152, 3,303.194, 3,350.348] -> [2,814.138,
  2,730.555, 2,637.618, 2,628.762, 2,659.850]`, mean `3,387.486 ->
  2,694.185`. The two-frame E2E distribution was `[330.086, 328.583, 323.697,
  322.981, 323.321] -> [350.242, 329.173, 314.146, 311.982, 312.149]`,
  mean `325.734 -> 323.538`; this is inconclusive. The prior same-harness
  real-RecordingStore one-run remains `1,965.824 -> 1,994.995` (`+1.5%`),
  within the 5% budget. No cross-harness 22.8% attribution is retained.
- RSS methodology and evidence: each scaffold run reads Linux `/proc/self/status`
  `VmHWM` (process high-water mark), with `VmRSS` also reported. The repeated
  distributions above used one process per variant with five repetitions; the
  120/down-2 maximum HWM was `1,380,356 -> 1,380,264 KiB` (`-0.0067%`). The
  earlier same-harness record is corrected explicitly: `1,393,504 -> 1,394,252
  KiB` is `+0.0537%`, not `+0.007%`. These measurements remain within budget and
  are not presented as cross-harness evidence.
- Verification: Rust 1.85.0 `cargo fmt --all -- --check`, full workspace
  `cargo check --workspace --all-targets --locked`, full workspace
  `cargo test --workspace --all-targets --locked` (706 passed, 0 failed, 2
  ignored), and full workspace clippy with `-D warnings` all pass. The focused
  ignored release smoke also passes with 4 frames, 1 repetition, 64x48,
  down-2, and matching normalized/artifact/output digests.
- Simplification: no dependencies, LUTs, threads, caches, or adjacent perf work
  were added. The only structural addition is the narrow committed ignored
  scaffold needed to make the evidence reproducible.
- Adjacent issues parked: none.

## Review decision

**Approved.** A fresh-context `openai-codex/gpt-5.6-luna` bounded standalone review found no correctness blocker. Its downscale-coverage and reproducibility comments were accepted and fixed in `66b9bc0`: identity/down-2/down-4/down-8 now compare against the reference kernel over rectangular multi-row fixtures with artifact/hash equality, and a committed ignored Rust 1.85 benchmark scaffold reproduces five-run distributions. The optimized normalization stage is about 58% faster at 30 and 120 frames; same-harness production-policy E2E improved about 6.8% and 20.5% respectively, while the two-frame result is explicitly inconclusive. Full gates pass. Per standard policy, no re-review was run. The story advances to `done`.
