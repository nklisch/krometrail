---
id: perf-temporal-overlap-frame-reuse
kind: feature
stage: drafting
tags: [perf, visual, storage, testing]
parent: null
depends_on: []
release_binding: null
gate_origin: perf-design
created: 2026-07-15
updated: 2026-07-15
---

# Reuse Decoded and Normalized Frames Across Overlapping Queries

## Discovery brief

Design bounded reuse for nearby temporal artifact queries that share retained
source frames. The current service reuses decoded/normalized work only inside one
single-flight request; distinct overlapping ranges generate independent work.
This crosses artifact scheduling, lifecycle/invalidation, memory accounting, and
possibly store source identity, so it remains a `[perf]` feature at
`stage: drafting` rather than an implementation story.

## Exact baseline evidence

- **Workload**: two concurrent production-policy requests over adjacent windows
  with `N-1` shared 1920x1080 PNG source frames. Each request generated the
  storyboard, orientation, and difference-map outputs. The windows were
  cache-isolated because their ordered source sets differed. No browser,
  network, or model was launched.
- **Build/host**: Rust 1.85.0 release build, locked dependencies, Linux x86_64,
  AMD Ryzen 7 7800X3D (8 cores/16 threads, 96 MiB L3). The scheduler used two
  blocking jobs and one generator permit per request to retain capture headroom.
- **Commands** (temporary scaffold, removed before commit):

  ```text
  rustup run 1.85.0 cargo test --release --no-run --locked
  PERF_DISCOVERY_FRAMES=60 PERF_DISCOVERY_MODE=overlap \
    /storage/cargo-target/release/deps/krometrail-f61aef4d711914da \
    artifacts::discovery_tests::perf_design_discovery --exact --ignored --nocapture
  ```

| window frames | shared frames | concurrent wall ms | concurrent CPU ms | allocations bytes | peak RSS delta | generated outputs |
|---:|---:|---:|---:|---:|---:|---:|
| 8 | 7 | 510.111 | 1,013.467 | 737,877,420 | 460,544 KiB | 6 |
| 30 | 29 | 1,633.762 | 3,229.373 | 2,067,902,810 | 1,363,244 KiB | 6 |
| 60 | 59 | 1,352.497 | 2,657.293 | 2,579,550,088 | 1,404,396 KiB | 6 |
| 120 | 119 | 4,984.012 | 4,970.494 | 5,086,900,044 | 1,386,892 KiB | 6 |

  For comparison, one cold request measured 493.052/1,538.006/1,318.018/
  2,583.599 ms wall at 8/30/60/120 frames. At 60 frames, overlapping queries
  nearly doubled CPU while the scheduler kept wall time near one request. At
  120 frames, the combined memory reservation serialized the two requests and
  wall time rose to 4,984.012 ms. The concurrent runs returned six generated
  outputs despite 7/29/59/119 shared source frames: there was no cross-query
  decoded or normalized-frame reuse.

- **All-hit validation context**: one exact all-hit query remained cheap but
  linear in source validation: 120 frames took 20.023 ms wall, of which the
  timed store lookup/revalidation path took 15.526 ms across three lookups. This
  is not the cold bottleneck and does not justify a broad persistent digest cache
  by itself.

## Required design questions

- Choose a byte-weighted, bounded lifetime and eviction policy that cannot retain
  deleted sessions, stale source payloads, or capture-critical memory. Define
  whether decoded pixels, normalized buffers, or both earn the memory cost.
- Key entries by every correctness input: exact source encoded digest and frame
  identity, decoder profile, visual epoch, crop/scale/background/normalization
  recipe, mask/region, algorithm/LUT version, and any measurement parameters.
- Preserve deletion/source revalidation fences and deterministic results. A cache
  hit may avoid work but cannot become authority for retained evidence.
- Benchmark concurrent adjacent windows at 30/60/120 frames and sequential
  sliding windows, with one/two request permits, cache hit rate, decode/normalize
  counts, wall/CPU, allocations, peak RSS, capture-headroom proxies, and exact
  artifact identity equality.

## Acceptance budget for the implementation design

- [ ] A proposed cache demonstrates a measured shared-frame hit rate and bounded
      byte accounting; no unbounded global or persistent intermediate CAS is
      accepted.
- [ ] On the discovery host, overlapping 60-frame windows reduce aggregate CPU
      by at least **25%** from 2,657.293 ms while keeping wall time no worse than
      the one-request 1,318.018 ms baseline; the 120-frame case must improve
      wall time by at least 25% from 4,984.012 ms without exceeding the existing
      single-request peak RSS budget by more than 8%.
- [ ] Decode and normalization calls for shared frames are reduced to one per
      cache key, while all generated manifests, PNG bytes, hashes, and source
      provenance remain identical to a cache-disabled run.
- [ ] Session deletion, source corruption, cache invalidation, cancellation,
      and concurrent publication tests prove no stale frame or artifact escapes.

## Scout disposition

- **Survives and is promoted**: `perf-scout-overlap-frame-cache`, because the
  adjacent-window workload measured duplicate work directly and showed a
  user-visible 120-frame wall penalty.
- **Rejected as a prerequisite**: persistent intermediate CAS; its lifecycle and
  retention complexity is disproportionate to this bounded local process.
- **Deferred**: request-scoped source-digest memoization remains a small warm
  validation optimization (15.526 ms at 120 frames) rather than a cold-path
  priority. Bounded parallel decode is also deferred until reuse/memory design
  establishes safe scheduler headroom.

## Discovery notes

This feature was emitted by `perf-design` discovery with `gate_origin: perf-design`.
No product cache or implementation has been added.
