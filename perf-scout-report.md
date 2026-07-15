# Perf-Scout Report

**Generated**: 2026-07-15
**Scope**: cold temporal artifact generation from retained PNGs
**Files considered**: 38
**Workload shape**: CPU-heavy image/data pipeline with durable storage publication

> ⚠️ Everything below is a candidate optimization: a speculative, unvalidated hypothesis. Nothing here is a measured speedup. The current 3.174 s cold observation came from `cargo test` at `opt-level=0` and is invalid for attributing cost. Validate in optimized builds before acting.

## Stack & shape profile

- Rust 2024, Rust 1.85 MSRV; browser-independent `temporal-vision` kernels plus Tokio application orchestration and SQLite/file storage.
- Retained PNG/JPEG bytes flow through store reads, decode, normalization, pairwise measurement, storyboard selection, difference accumulation, rasterization, PNG encoding/SHA-256, and atomic publication.
- Realistic priority is 8/30/60/120 1080p frames and overlapping nearby queries, not the two-frame diagnostic alone.
- Likely scaling surfaces are `frame_count × pixels`, repeated pair scans, serial decode, dense per-pixel accumulators, full-frame memory traffic, codec work, source validation, and multi-output publication.
- Already present: durable final-artifact cache, exact-request single-flight, request-local decode/normalization reuse, bounded scheduler/semaphores, deterministic integer algorithms, cache/source/deletion fences.

## Lens opportunity density

| Lens | Relevance | Opportunity | Ideas (Investigate / Worth / Long-shot) |
|---|---:|---:|---:|
| Algorithmic & data structures | most | 9/10 | 2 / 3 / 2 |
| Memory & data locality | most | 8/10 | 2 / 3 / 1 |
| Parallelism & vectorization | most | 8/10 | 2 / 3 / 2 |
| GPU & accelerators | relevant | 3/10 | 0 / 0 / 3 |
| Caching & memoization | most | 8/10 | 1 / 3 / 2 |
| I/O & batching | relevant | 7/10 | 1 / 4 / 2 |
| Compiler, runtime & language | most | 8/10 | 2 / 3 / 1 |
| Distributed systems | skipped | — | — |
| Game-engine & realtime | skipped | — | — |
| Database & storage internals | folded into I/O/caching | — | — |
| Approximation & precomputation | skipped | — | — |

## Top 5 to investigate first

1. **Optimized multi-frame stage profiling and cache isolation** — `src/app/live_evaluation/latency.rs:99` · benchmark engineering · High/Likely/Low.
2. **Request-scoped cross-generator pair-classification trace** — `crates/temporal-vision/src/measure.rs:242` · HPC loop fusion / materialized views · High/Likely/Medium.
3. **Request-scoped source-digest memoization and batched validation** — `crates/krometrail-store/src/recording.rs:331` · database dataloaders/content-addressed storage · High/Likely/Medium.
4. **Bounded concurrent retained-frame decode** — `src/artifacts/epoch.rs:191` · parallel ingestion pipelines · High/Likely/Medium.
5. **Opaque row-major normalization fast path** — `crates/temporal-vision/src/normalize.rs:484` · data-oriented image kernels · High/Likely/Low–Medium.

## Cross-model peer pass

- **Reviewer**: `zai/glm-5.2`, xhigh, fresh-context adversarial pass.
- **Pruned/relegated**: 16 generic, niche, premature, or likely-negative candidates, including persistent intermediate CAS, retained 100 MB accumulators, io_uring, AoS accumulator layout, selector micro-optimizations, full 64 KiB inverse LUT, and GPU work before profiling.
- **Added or materially reframed**: 10 angles. Four landed in the first investigation tier: cross-generator pair reuse, request-scoped digest reuse, scheduler-owned parallel decode, and honest cache-isolated stage profiling.
- **Independent correction**: the peer found cache-hit source hashing and cross-generator pair rescans were under-ranked, and confirmed the `cargo test` number cannot attribute hot stages.
- **Follow-up scout**: none; no missed whole lens warranted another wave.

## Ideas by priority

### Investigate-first

#### Optimized multi-frame stage profiling and cache isolation
- **Lens**: compiler-runtime · **Borrowed from**: benchmark engineering / roofline analysis
- **Location**: `src/app/live_evaluation/latency.rs:99`
- **Leverage / Applicability / Cost**: High / Likely / Low
- **Idea**: Add optimized benchmark/release execution and stage timers for store read, decode, normalize, pair analysis, selection, render, encode/hash, and publish. Measure 2/8/30/60/120 frames and isolate all-cold, mixed, and all-warm cache namespaces.
- **Why it might help**: it prevents optimizing an `opt-level=0` artifact or the wrong stage and supplies the baseline for every later decision.
- **Validate by**: p50/p95 wall time, CPU, allocations/RSS, bytes hashed/read/written, stage counts, and capture-ingestion latency under load.
- **Risk**: instrumentation can perturb timings; keep counters bounded and benchmark-only.
- **Source**: peer-glm5.2; also surfaced by runtime, GPU, and I/O scouts.
- **Parked**: `perf-scout-profile-artifact-stages`

#### Reuse one pair-classification trace across generators
- **Lens**: algorithmic · **Borrowed from**: HPC loop fusion and database materialized views
- **Location**: `crates/temporal-vision/src/measure.rs:242`
- **Leverage / Applicability / Cost**: High / Likely / Medium
- **Idea**: Build a bounded request-scoped adjacent-pair classification trace once and feed storyboard measurement, difference accumulation, and motion history instead of rescanning the same normalized frame pair independently.
- **Why it might help**: multi-output 120-frame bundles can perform hundreds of full-frame pair scans; eliminating duplicate passes attacks the dominant `frames × pixels` scaling directly.
- **Validate by**: pair-scan counters, CPU/cache misses, peak memory, and exact selection/accumulator/manifest/PNG equality for 8/30/60/120 frames with gaps and masks.
- **Risk**: trace storage, gaps, masks, tie ordering, and exact integer semantics must remain bounded and identical.
- **Source**: peer-glm5.2; also surfaced by algorithmic and memory scouts.
- **Parked**: `perf-scout-share-pair-classification`

#### Memoize source digests and batch validation within one request
- **Lens**: caching · **Borrowed from**: database dataloaders and request-scoped caches
- **Location**: `crates/krometrail-store/src/recording.rs:331`
- **Leverage / Applicability / Cost**: High / Likely / Medium
- **Idea**: Reuse validated `(frame_id, encoded SHA-256)` proofs and bulk artifact lookups within a single multi-output request while retaining every deletion fence and final publication revalidation.
- **Why it might help**: a 60-frame three-output lookup can currently hash source payloads roughly 180 times; request scope may collapse that to 60 without trusting persisted checksums indefinitely.
- **Validate by**: instrument hash calls/bytes, SQLite statements, source reads, warm/cold p95, corruption injection, deletion races, and output identity.
- **Risk**: memoization must not outlive the request or weaken integrity/deletion checks.
- **Source**: peer-glm5.2; also surfaced by caching, I/O, memory, and algorithmic scouts.
- **Parked**: `perf-scout-request-source-digests`

#### Decode retained frames concurrently under scheduler control
- **Lens**: parallelism · **Borrowed from**: bounded parallel ingestion pipelines
- **Location**: `src/artifacts/epoch.rs:191`
- **Leverage / Applicability / Cost**: High / Likely for 30+ frames / Medium
- **Idea**: Replace serial decode with indexed bounded parallel decode using a scheduler-owned pool sized to retain capture-ingestion headroom; collect output in source order.
- **Why it might help**: independent frame decodes otherwise add serially with frame count before any visual analysis starts.
- **Validate by**: 8/30/60/120-frame decode and end-to-end latency, peak decoded RSS, CPU utilization, and capture queue latency at 1/2/4 workers.
- **Risk**: nested pools and oversubscription can starve capture; never use an unconstrained global Rayon pool inside `spawn_blocking`.
- **Source**: peer-glm5.2; also surfaced by parallelism scout.
- **Parked**: `perf-scout-bounded-parallel-decode`

#### Specialize normalization for opaque row-major input
- **Lens**: memory-locality · **Borrowed from**: data-oriented image kernels
- **Location**: `crates/temporal-vision/src/normalize.rs:484`
- **Leverage / Applicability / Cost**: High / Likely / Low–Medium
- **Idea**: Hoist scale mode out of the pixel loop, write pre-sized row slices, and use a predictable in-loop `alpha == 255` path while retaining exact general-alpha handling.
- **Why it might help**: Chrome screenshots are commonly opaque; this may remove repeated scale dispatch, blend arithmetic, coordinate/index work, and per-pixel `Vec` extension bookkeeping.
- **Validate by**: stage benchmark and byte-identical normalized buffers/artifacts across alpha, crop, mask, identity, and downscale cases.
- **Risk**: preserve linear-light compositing, checked bounds, rounding, and provenance.
- **Source**: peer-glm5.2; also surfaced by memory, runtime, and parallelism scouts.
- **Parked**: `perf-scout-opaque-row-normalization`

### Worth-a-look

#### Lazily allocate sparse-change difference accumulators
- **Lens**: memory-locality · **Borrowed from**: sparse matrices and bitmap engines
- **Location**: `crates/temporal-vision/src/difference_map.rs:125`
- **Leverage / Applicability / Cost**: High / Plausible / Medium
- **Idea**: Keep comparable counts dense but allocate change/timing arrays lazily or by active tiles, with a density threshold for dense fallback.
- **Why it might help**: static/sparse 1080p inputs currently allocate and zero roughly 100 MB of accumulator arrays even when no pixels change.
- **Validate by**: 0/1/10/100% change density, allocation/zeroed bytes, RSS/cache misses, and exact output equivalence.
- **Risk**: deterministic iteration, limits, mask semantics, and dense crossover can negate benefits.
- **Source**: scout; surfaced by memory and algorithmic lenses; peer deferred until profiling but retained because validation is straightforward.
- **Parked**: `perf-scout-lazy-difference-accumulators`

#### Fan out independent artifact generators within bounded budgets
- **Lens**: parallelism · **Borrowed from**: pipeline fan-out/fan-in
- **Location**: `src/artifacts/service.rs:392`
- **Leverage / Applicability / Cost**: High / Plausible / Medium–High
- **Idea**: After shared decode/normalization, run independent generator groups concurrently under the existing per-request semaphore and restore deterministic result/publication order.
- **Why it might help**: storyboard and difference-map CPU work currently runs serially despite independent outputs.
- **Validate by**: stage times and end-to-end latency at 1/2 generators while monitoring peak memory, CPU, capture queue depth, cancellation, and exact artifacts.
- **Risk**: oversubscription, memory spikes, artifact ID/order nondeterminism, and duplicate inner parallelism.
- **Source**: scout-parallelism.
- **Parked**: `perf-scout-bounded-generator-fanout`

#### Characterize deterministic PNG compression/filter policy
- **Lens**: compiler-runtime · **Borrowed from**: codec engineering
- **Location**: `crates/temporal-vision/src/encode.rs:26`
- **Leverage / Applicability / Cost**: High / Plausible / Medium
- **Idea**: Benchmark `Best/Default/Fast` compression and deterministic filters separately on flat, text-heavy, sparse-change, and noisy outputs. Adopt only a measured policy and version the encoding identity/cache.
- **Why it might help**: every cold output uses `Best + NoFilter`; a different deterministic policy might reduce CPU and possibly bytes for synthetic canvases.
- **Validate by**: encode-only and end-to-end latency, output bytes, disk budget, decode pixel equality, hash/version updates.
- **Risk**: encoded bytes, hashes, artifact descriptors, and cache identity change.
- **Source**: runtime and I/O scouts; peer retained as profile-gated.
- **Parked**: `perf-scout-characterize-png-policy`

#### Batch durable publication for multi-output requests
- **Lens**: io-batching · **Borrowed from**: storage-engine group commit
- **Location**: `crates/krometrail-store/src/artifacts/files.rs:135`
- **Leverage / Applicability / Cost**: High / Plausible / Low–Medium
- **Idea**: Stage and sync all artifact temp files, rename them as a batch, sync the directory once, then finalize metadata with exact per-artifact receipts.
- **Why it might help**: multi-output requests currently serialize repeated file sync/rename/directory-sync work.
- **Validate by**: syscall/fsync counts, publication latency, failpoints, restart recovery, deletion races, and identity equality.
- **Risk**: partial-batch recovery and durability ordering must remain exact.
- **Source**: peer-glm5.2; also surfaced by I/O scout.
- **Parked**: `perf-scout-batch-artifact-publication`

#### Cache decoded/normalized frames across overlapping queries
- **Lens**: caching · **Borrowed from**: immutable image-operation caches
- **Location**: `src/artifacts/decode.rs:36`
- **Leverage / Applicability / Cost**: High / Plausible / High
- **Idea**: Add a bounded byte-weighted cache for decoded pixels and possibly normalized buffers keyed by source digest, decoder profile, geometry, normalization recipe, and algorithm/LUT version.
- **Why it might help**: sliding or nearby range queries share most frames but current reuse ends with one flight.
- **Validate by**: overlapping-window hit rates, decode/normalize call counts, p95 latency, resident bytes, source deletion/session lifecycle, and exact outputs.
- **Risk**: 1080p normalized buffers are large; stale-session retention and missing key fields can violate privacy/correctness or starve capture.
- **Source**: scout-caching; reinforced by operator priority and peer G9.
- **Parked**: `perf-scout-overlap-frame-cache`

#### Precompute raster coordinate maps and row writes
- **Lens**: memory-locality · **Borrowed from**: software rasterizers
- **Location**: `crates/temporal-vision/src/render/canvas.rs:102`
- **Leverage / Applicability / Cost**: Medium–High / Plausible / Low
- **Idea**: Precompute x/y source maps once per geometry and write validated destination row slices directly for tiles/panels.
- **Why it might help**: removes repeated integer division, source-index reconstruction, bounds checks, and per-pixel destination arithmetic.
- **Validate by**: render-only stage benchmark and exact pixels/PNG hashes across layouts/aspect ratios.
- **Risk**: contain-fit rounding and overlapping annotation regions must remain exact.
- **Source**: algorithmic/memory/runtime scouts; peer retained after profiling.
- **Parked**: `perf-scout-raster-row-maps`

### Long-shots / report-only

- **Persistent GPU-resident pipeline** — `crates/temporal-vision/src/measure.rs:242` — potentially useful only for long sequences after CPU/profile work; high transfer, memory, portability, and determinism cost.
- **Persistent intermediate CAS** — `src/artifacts/cache.rs:62` — excessive lifecycle/retention complexity for a local single-process product.
- **io_uring source reads** — `crates/krometrail-store/src/recording.rs:286` — likely negative at page-cache-resident low queue depth.
- **Retained reusable difference accumulators** — `difference_map.rs:137` — roughly 100 MB per 1080p range; conflicts with memory/capture budgets.
- **Full 64 KiB inverse-transfer LUT** — `normalize.rs:693` — may exceed L1D and regress the hot raster path.
- **Subset-aware single-flight** — `single_flight.rs:36` — cancellation semantics and low measured concurrency make it premature.

### Pruned by peer

- Packed AoS change accumulator — likely expands 48-byte SoA to 64+ bytes due `u128` alignment.
- Incremental storyboard fill scoring — bounded tile/frame counts make it tiny beside full-image passes.
- Marker/gap merge optimization — microsecond-scale bounded metadata work.
- Bit-parallel masks — default unrestricted path has no mask.
- Selector temporary-allocation cleanup — wrong scale.
- GPU decode/PNG encode — transfer/platform/determinism cost with no evidence.
- Blocking-task phase fusion — scheduler overhead is unlikely to explain seconds.

## Lenses skipped

| Lens | Why |
|---|---|
| Distributed systems | single-process local pipeline; no remote coordination in scope |
| Game-engine & realtime | relevant concepts were covered through memory/parallel/raster lenses; no extra scout needed |
| Database & storage internals | storage ideas were covered by I/O/caching and cross-model review |
| Approximation | evidence artifacts require exact deterministic semantics; approximate output is out of scope |

## Backlog parking summary

| Metric | Count |
|---|---:|
| Ideas selected to park | 11 |
| Duplicates skipped | 0 |
| Opt-out | false |
| Substrate present | true |

Parked ideas are investigation candidates, not release blockers. Hand the top profiling item to `perf-design` first; do not implement the deck directly.

## Scout gaps

- Memory, algorithmic, and GPU scouts could not perform requested web searches because their delegated harness lacked web tools; their code-grounded scans still completed.
- All seven selected lenses returned usable results.

## Next steps

1. Run `perf-design` on `perf-scout-profile-artifact-stages` to establish release-mode stage attribution across 2/8/30/60/120 frames.
2. Promote only measured candidates into `[perf]` work.
3. Re-run end-to-end latency and capture-starvation qualification after each accepted optimization.
