---
id: perf-temporal-share-pair-classification-opt-3-service-scheduler-wiring
kind: story
stage: done
tags: [perf, visual, testing]
parent: perf-temporal-share-pair-classification
depends_on: [perf-temporal-share-pair-classification-opt-2-temporal-context]
release_binding: null
gate_origin: perf-design
created: 2026-07-15
updated: 2026-07-15
---

# Group Compatible Artifact Generators Under Existing Budgets

## Purpose

Wire the bounded temporal pair-analysis context into the production artifact
service without changing external artifact behavior, publication ordering,
cancellation, epoch isolation, or scheduler/capture headroom.

## Implementation units

### Unit 1: Group compatible pending slots

**Files**:

- `src/artifacts/service.rs`;
- `src/artifacts/generators.rs`;
- `src/artifacts/epoch.rs`.

Change `run_flight` grouping from `(epoch_index, generator_index)` to a
compatible analysis identity containing the exact epoch, normalized-sequence
identity, transformed mask/domain, measurement parameters, ordered frame
identity/timestamps, gaps, and algorithm/decoder identity. Storyboard,
orientation, difference-map, and compatible motion slots share one context;
different noise floors, masks, normalization recipes, epochs, or requests do
not. Display-only difference parameters share the core only after measurement
identity is proven equal.

Keep output IDs, result slots, publication sequence, cache keys, artifact
kinds, and partial-failure handling in their current deterministic order.
Orientation continues to consume the storyboard selection rather than measure
again. Single-output paths may keep the direct path when building a context
would add work.

### Unit 2: Extend scheduler accounting without a new cache

**Files**: `src/artifacts/scheduler.rs`, `src/artifacts/service.rs`.

Reserve the bounded trace plus exactly one working core per distinct requested
difference/motion consumer in the existing combined-request memory budget.
Reject before allocation with the existing resource-limit error. Do not count a
shared core once per output slot, do not create an unbounded semaphore, and do
not move any pixels into a global or persistent cache. Retain the existing
request, CPU, generator, decoded, normalized, output, deadline, and cancellation
permits.

### Unit 3: Preserve service failure fences and production evidence

Add/retain tests for epoch boundaries, equal timestamps, gaps, masks,
normalization identity, cache hits, source invalidation, session deletion,
flight cancellation, deadline expiry, partial failure, and deterministic
publication. Run the story-1 benchmark through the real
`TemporalVisionArtifactService`/`RecordingStore` production policy for cold
60/120-frame storyboard + orientation + difference outputs, with motion as a
separate cell.

## Acceptance criteria

- [ ] Compatible outputs make one adjacent traversal and report the expected
      pair-pass/classifier-call reduction; incompatible contexts never share.
- [ ] Stored artifact bytes, manifests, source IDs, hashes, selection roles,
      timestamps, gaps, masks, and cache dispositions are identical to a
      cache/context-disabled baseline.
- [ ] Cancellation and deadline errors cannot publish partial context results;
      last-waiter flight cancellation remains effective; session deletion and
      source revalidation fences remain effective.
- [ ] Scheduler memory reservations include the trace and distinct consumer
      cores, remain within configured limits, and preserve capture queue/gap
      headroom. No 100 MB-plus trace or persistent intermediate is introduced.
- [ ] After rebaselining the current normalization revision, cold E2E median is
      at least 20% faster at both 60 and 120 frames, or the parent feature
      records the measured miss and stops without adding lower-level or
      parallel work to this feature.
- [ ] Peak RSS is no more than 8% above the current 120-frame baseline and
      release stage/counter/CPU/allocation evidence is recorded.

## Non-goals

Do not modify normalization, Chrome/model execution, public artifact contracts,
`.work/bin/work-view`, persistent caches, GPU paths, or generator fan-out
parallelism. Do not absorb the concurrent normalization worker's changes.

## Dependency and handoff

Depends on `perf-temporal-share-pair-classification-opt-2-temporal-context`,
which depends on the release baseline/equivalence scaffold story. This is the
final implementation checkpoint before the parent feature review.

## Implementation notes

- Execution capability: inline feature-owner implementation; service, generator, scheduler, and production qualification share one lifecycle and publication boundary.
- Review weight: standard, project default; this child checkpoint advances directly to done after the integrated gates.
- Files changed: `src/artifacts/service.rs`, `src/artifacts/generators.rs`, `src/artifacts/service_tests.rs`, and the temporal-vision context/render adapters in `crates/temporal-vision/src/{pair_analysis,difference_map,motion_history,render,select,lib}.rs`.
- Architecture: `run_flight` derives one exact request-local identity from the ordered retained source fingerprint, epoch geometry, gaps, effective normalization, measurement parameters, decoder/adapter identity, and algorithm identity. Compatible pending slots share one opaque bounded context; generator IDs are reserved in original slot order and publication is deferred/sorted by that ordinal so IDs and publication order remain unchanged. Difference accumulation is one shared `Arc` core; motion allocates one core per distinct decay. Region filmstrips and the disabled-context test policy retain direct generator paths.
- Scheduler accounting: the existing combined-request semaphore now reserves one `80 * pair_count + 64` trace and one 48-byte-per-pixel difference core plus one 4-byte-per-pixel-plus-mask motion core per distinct consumer, before decode/context allocation. No persistent/global cache, semaphore, fan-out, normalization, or public artifact schema was added.
- Tests added/updated: compatible service baseline equivalence and incompatible measurement-identity equivalence compare output IDs, cache dispositions, manifests, stored bytes/hashes, source ordering, and artifact roles; existing epoch, gap, cancellation, deadline, partial failure, deletion, source invalidation, cache, and exact memory-boundary suites remain green. The ignored production benchmark now drives the real `TemporalVisionArtifactService` over `RecordingStore` and reports CPU, allocation, RSS, pair/call accounting, and external perf counters.
- Simplification: replaced generator-index-only grouping with one identity/group coordinator and removed the duplicate adjacent traversal at the service boundary without introducing another cache or scheduling policy.
- Discrepancies from design: the context is exposed as an opaque temporal-vision bridge because Rust crate privacy cannot pass the crate-private opt-2 type across the production adapter boundary; its fields and artifact contracts remain private. Publication is collected briefly and emitted in original ordinal order to preserve interleaved generator order.
- Adjacent issues parked: none. The measured target miss is intentionally not compensated with lower-level, normalization, or parallel work.
- Exactness evidence: `cargo test --workspace --all-targets --locked` passed with 710 tests and 4 ignored. Baseline/context service fixtures matched artifact IDs, cache dispositions, manifests, encoded bytes, hashes, source ordering, orientation roles, timestamps, gaps, and masks. `cargo clippy --workspace --all-targets --locked -- -D warnings` and formatting passed.
- Production benchmark policy: five cold release repetitions per cell, new real `RecordingStore`/service/temp source each run, 1920x1080 PNG source frames, down-2 normalized analysis, no Chrome/model/network, and generated cache dispositions all `Generated`. Context-disabled service medians were 1,262.124 ms (60) and 2,598.758 ms (120); context medians were 999.354 ms (60) and 2,049.344 ms (120), a 20.82%/21.14% reduction against that paired production policy.
- Acceptance disposition: against the authoritative supplied baselines B60=1,048.717 ms and B120=2,108.262 ms, the final context medians were 999.354 ms and 2,049.344 ms: 4.71% and 2.79% faster, below the required 20% target (thresholds 838.974 ms and 1,686.610 ms). This is an honest miss, so the optimization is left at the achieved bounded context and this feature adds no lower-level or parallel work.
- Distributions/counters: context cold wall distributions were B60 `[996.250, 997.345, 999.354, 1,000.497, 1,001.882]` ms and B120 `[2,043.220, 2,044.905, 2,049.344, 2,076.290, 2,077.391]` ms; motion cells were B60 `[1,025.907, 1,028.511, 1,030.963, 1,031.412, 1,036.479]` ms and B120 `[2,084.721, 2,101.841, 2,125.889, 2,141.215, 2,145.983]` ms. Peak RSS maxima were 722,060 KiB at 60 and 1,388,980 KiB at 120 versus 1,357,328 KiB supplied B120 (2.33% above). Allocated-byte deltas were 1,298,107,871 (60), 2,560,106,341 (120), 1,318,544,518 (60 motion), and 2,596,324,672 (120 motion). Representative external `perf stat` counters (including test startup) were 60: task-clock 1,255.34 ms, cycles 4,728,805,696, instructions 20,369,357,013, cache-misses 7,819,247, branch-misses 4,140,712; 120: 2,292.97 ms, 9,290,998,316, 39,531,123,085, 10,719,078, and 5,029,991.
- Pair reduction: clean no-motion context reports 99/199 classified passes and 51,321,600/103,161,600 classifier calls at 60/120, reducing 59/119 passes and 30,585,600/61,689,600 calls from the baseline formulas. Motion reports 177/357 pass and 91,756,800/185,068,800 call reductions.
