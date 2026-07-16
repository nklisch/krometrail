---
id: perf-temporal-share-pair-classification
kind: feature
stage: done
tags: [perf, visual, testing]
parent: null
depends_on: []
release_binding: 1.0.0
gate_origin: perf-design
created: 2026-07-15
updated: 2026-07-15
---

# Share Adjacent-Pair Classification Across Temporal Generators

## Discovery brief

Design a request-scoped, bounded way to reuse adjacent-pair classification and
measurements across storyboard selection, direct pair analysis, difference-map
accumulation, and any other generators that need the same normalized sequence.
This is a multi-site algorithm/data-model change. The dedicated perf-design
pass below advances the feature to `stage: implementing` with sequential
measurement and implementation checkpoints.

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

## Current-revision baseline record (opt-1)

The committed ignored scaffold is
`crates/temporal-vision/tests/pair_classification_perf.rs`. It ran under
`rustc 1.85.0 (4d91de4e4 2025-02-17)` / Cargo 1.85.0, locked release
dependencies, on Linux x86_64 with an AMD Ryzen 7 7800X3D (8 cores/16 threads,
96 MiB L3). Each repetition was a new process with a newly generated in-memory
source fixture, no artifact/source cache, and an unmeasured duplicate run that
compared normalized buffers, artifacts, manifests, bytes, accounting, and
hashes directly. The benchmark is ignored and does not launch Chrome, a model,
network, service, or scheduler.

The authoritative clean storyboard + orientation + difference, down-2 current
baselines are:

| baseline | wall ms (five cold runs; median) | process CPU ms (median) | normalization / generator ms (median) | allocations | peak RSS KiB (median; max) |
|---|---:|---:|---:|---:|---:|
| `B60_current` | `[1082.302, 1052.269, 1044.537, 1048.717, 1036.510]`; **1048.717** | `[1078.105, 1047.564, 1040.745, 1045.112, 1032.780]`; **1045.112** | 308.230 / 740.472 | 223,488,695 | 697,096; 697,320 |
| `B120_current` | `[2121.246, 2123.465, 2062.360, 2108.262, 2088.045]`; **2108.262** | `[2105.863, 2085.689, 2052.862, 2091.719, 2077.799]`; **2085.689** | 664.442 / 1423.600 | 410,162,461 | 1,357,328; 1,357,400 |

For these baselines, `A/M/G/P/B/Bm` are B60
`59/59/0/518400/40/40` and B120 `119/119/0/518400/80/80`; `Bm` excludes a
possible gap-boundary metadata call from classified passes. The default
storyboard + difference path therefore reports `2M+B` as 158 and 318
classified pixel passes, or 81,907,200 and 164,851,200 expected classifier
pixel calls. Adding motion reports `4M+B` (276 and 556 passes). A gapped cell
keeps its gap metadata call but contributes zero classifier calls.

The five-run external `perf stat` medians, with all requested events permitted,
were B60 task-clock 2247.85 ms, cycles 9,870,639,974, instructions
42,106,490,794, cache-misses 2,909,840, and branch-misses 2,832,025; B120
was task-clock 4502.79 ms, cycles 19,418,922,641, instructions 82,057,964,609,
cache-misses 7,069,320, and branch-misses 3,348,946. These counters include
Cargo/test startup because the prescribed external command wraps the locked
Cargo test; the benchmark JSON leaves `task_clock_us` null rather than
substituting process CPU time. No counter denial was hidden.

The baseline normalized/output digests are:

| baseline | normalized digest | combined artifact digest | output digest |
|---|---|---|---|
| B60 | `d235e074cd4ef04828ff614b2fdef65d05cc8fba720be1297a2cd091b01aab3e` | `da3e56ccdaaa3d23ab0b63724686985759586f8632b8fc24a316b9c5ab9e9f5e`, `0166467c529d54a1618cbe0c75442eb0b47101326a7a1b26a2267dd8eeb12fb9`, `1b2e499d3f6b31d5e1799af2bc99e9a98b8d90df6c6a99be448539833510df83` | `8f7d3ad0feaf2d12d445e454b74dfe6ff8193ae75d3215810c8d9d3f4e598c5a` |
| B120 | `3daa145cc263afdedcbda0bda35bd5e1678ac16232ad6cc46e0195ee1766c309` | `555b223404fba6c920e246e3afb79d80803d8aebfee031a3fa505fc00b467356`, `d08d0a44c50bc8fdcd3df69a219e03918fc018ce94ab8da8e91fb4a97d94d9f6`, `c68bceb7660cd221be21dc8a68ec4f9fd1d16a22a02dc8a44fee765adeaa8105` | `c43b6f16d0bfb754ef4ee4aa7691e96136fe73604f72f1e1df49d96bbe6d23cd` |

The first-run manifest digests are B60
`15854ec45e900a9ac4a108893aaab08a6bc5ca308191822fd0e2c6e2daf4ecde`,
`321a865eb8a01041bb56a19bccf203c50447a7cf85b2cb049e854ada8832af8e`,
`cba3b0a6dc6971be1a8551cddac7d284f0447570311b80438f08388f5f355fb5`; and
B120 `6666c15bf91042c5fb6aa1d8ff0785027e08130e821b6d50005f072b052cd8c0`,
`2e3d49a8c5225e0a7cd04b04d5b785bfd63f566aab965f58a23cfa5ff17079ab`,
`ca7589ab12a6d589be323796440438314b0ac59994e56745ebdee6e26b09d7af`.

All 48 required cells (8/30/60/120 x identity/down-2 x clean/masked/gapped x
storyboard+difference or storyboard+difference+motion) ran five separate cold
process repetitions: 240 matrix runs. Every duplicate comparison passed and
every cell repeated its normalized and output digest exactly. Masked included
pixels were 1,166,400 identity / 292,320 down-2; clean included pixels were
2,073,600 / 518,400. Gapped cells had `G=1` and zero classifier calls for the
gap pair. Clean baseline `B/Bm` was 6/6, 20/20, 40/40, 80/80 for 8/30/60/120;
gapped `B/Bm` was 3/2, 6/5, 11/10, 21/20. The matrix reported the exact
`2M+B` and `4M+B` formulas in every consumer mode.

This record supersedes the historical numbers as the implementation baseline;
the 20% candidate thresholds remain owned by opt-2/opt-3 and are not claimed
by this measurement-only story.

## Design decisions

### Scope and measurement posture

This is an algorithmic/data-model optimization, not a normalization, codec,
parallelism, browser, or model change. Direct reading of the complete
`temporal-vision` crate plus the service and scheduler was sufficient; no
exploratory worker was needed. The current call graph and the committed
benchmark contract below are the evidence boundary.

The recorded discovery numbers predate the opaque row-major normalization
optimization. They remain useful historical attribution, but are not the
implementation gate. Before implementation acceptance, rerun five cold
repetitions on the current revision with the same host, locked Rust 1.85
release build, source fixture, scheduler limits, and cache isolation. Let the
trimmed or median current measurements be `B60_current` and `B120_current`.
The required targets are `0.80 * B60_current` and `0.80 * B120_current`, not a
comparison against a stale pre-normalization number. The old anchors imply
1,054.414 ms and 2,066.879 ms respectively, but those values are informative
only until the rebaseline is recorded.

No benchmark may use an artifact-cache hit as a cold result. The production
measurement must retain the existing single-flight, deletion, publication,
epoch, and scheduler behavior and must report a separate warm-cache result.

### Verified current consumer call graph

For `N` frames, `A = N - 1` adjacent pairs, `G` adjacent pairs intersecting a
declared gap, `M = A - G` measurable adjacent pairs, `P` included analysis
pixels, and `B` baseline-to-later pair calls made by storyboard selection:

| Consumer | Current calls/passes | Classification work |
|---|---:|---:|
| `select_storyboard_frames` -> `measure_adjacent` | `A` pair calls | `M * P` pixel classifications |
| storyboard peak baseline (`measure_pair`) | `B` non-adjacent pair calls before the next continuity boundary | `B * P` classifications |
| `render_difference_map` -> `DifferenceAccumulators::accumulate` | `M` direct pair passes | `M * P` classifications |
| `build_motion_history_plan` -> `measure_adjacent` | `A` pair calls | `M * P` classifications |
| motion `accumulate_segment` | `M` direct pair passes | `M * P` classifications |
| orientation renderer | none | consumes the storyboard selection |

Therefore the default storyboard + orientation + difference path performs
`2M + B` full classified pixel passes and approximately `P * (2M + B)`
classifier calls. Adding motion makes it `4M + B`; it also currently classifies
those adjacent pairs once to build `FrameComparison`s and once again while
building the motion plan. Gap pairs still produce metadata outcomes but perform
zero pixel classifications. The baseline `B` work is non-adjacent and is not a
duplicate adjacent scan; retaining a quadratic all-pairs trace to remove it is
out of scope and rejected.

This accounting was verified against the source at:

- `crates/temporal-vision/src/measure.rs:184-239, 242-353`;
- `crates/temporal-vision/src/select.rs:354-371, 573-597`;
- `crates/temporal-vision/src/difference_map.rs:139-216`;
- `crates/temporal-vision/src/motion_history.rs:215-248, 365-408`;
- `src/artifacts/service.rs:300-466` and `src/artifacts/scheduler.rs`.

The service currently groups work by `(epoch_index, generator_index)` in
`run_flight`, so a storyboard and difference generator get separate generation
calls even when the normalized sequence is reused. The scheduler already
bounds requests, blocking jobs, generator permits, decoded bytes, normalized
bytes, and output bytes; the pair work must be added to that accounting rather
than introducing a second memory policy.

### Chosen representation: streaming fan-out plus a tiny aggregate trace

Choose a request-scoped `PairAnalysisContext`, built once per compatible
analysis group. It contains:

```rust
struct PairAnalysisContext<FrameId> {
    // Existing public comparison values, in source declaration order.
    comparisons: Box<[FrameComparison]>,
    // Optional consumer-local results built during the same pixel traversal.
    difference_core: Option<DifferenceAccumulatorCore>,
    motion_cores: Box<[MotionAccumulatorCore]>,
}
```

The exact names are implementation choices; the boundary is not. The builder
is one deterministic row-major pair/pixel traversal. For every measurable
adjacent pair it calls the canonical integer classifier once and fans the
unchanged result to the measurement aggregate, requested difference core(s),
and requested motion core(s). It emits a normal `FrameComparison` for the
selector. Gap pairs emit the same `GapBoundary { declared_gap_count }` and
elapsed nanoseconds as today, and no consumer receives pixel events for them.

The selected representation does **not** retain a per-pixel trace. The
replayable trace is only the existing `FrameComparison` vector; difference and
motion arrays are the output-local accumulators that those generators already
need, built in place while the source pixels are hot. This is smaller and safer
than a dynamic consumer trait, a persistent cache, or a bitmap-plus-magnitude
trace. It also avoids retaining a 100 MB-plus intermediate trace.

The classifier result must preserve the existing checked `u128` weighted
square and exact threshold comparison. If an internal event carries channel
deltas or luminance, it must use the same values and rounding order as the
current kernel; it must not replace the `u128` intermediate with an unchecked
narrower value. Difference accumulation keeps the current checked `u64`
conversion and `u128` weighted-time sum. Measurement vectors retain exact
changed/compared counts, bounds, rounded integer means, and integer square-root
behavior.

The existing public `measure_pair`, `measure_adjacent`, and
`select_storyboard_frames` contracts remain available and unchanged. The new
path is an internal overload/adapter used by the multi-output service and by
generator functions that accept a precomputed context. Single-output callers
may continue using the existing path; no cache is worthwhile when there is no
second consumer.

### Compatibility key and lifetime

A context is valid only for one request/single-flight flight and one visual
epoch. Its compatibility key includes:

- epoch index and exact ordered source frame IDs, timestamps, dimensions,
  viewport/device-scale epoch identity, and declared gaps;
- the exact normalized sequence identity, including normalization recipe,
  normalized dimensions, transformed analysis mask bytes/domain, and ordered
  normalization steps;
- `MeasurementParameters`, including `noise_floor`;
- the analysis mask identity and pixel count even when it is represented as an
  unrestricted domain;
- the measurement algorithm version and decoder/adapter identity where the
  service constructs the key.

A cache/context hit must never cross a geometry epoch, normalization identity,
mask, measurement parameter, cancellation boundary, or request. The service
must group pending slots by `(epoch, normalization identity, measurement
identity)` rather than by generator index. Different noise floors or
normalization recipes form separate contexts. Different difference display
parameters (frequency mode, palette, background, repeated-change separation)
share the same difference core only when their measurement identity is equal;
they are applied after accumulation. Different motion decay parameters receive
separate bounded motion cores fed by the one traversal.

The context is dropped on cancellation, deadline, error, or flight completion;
it is never placed in `SingleFlight`, the artifact cache, SQLite, a segment, or
a process-global map. Publication still performs its existing cancellation and
source/deletion fences. No partially built context can publish an artifact.

### Exact retained-byte budget

`FrameComparison` is 80 bytes on the current Rust layout. The trace payload is
therefore `80 * (N - 1)` bytes plus at most 64 bytes of fixed context metadata;
identity and down-2 have the same trace size because no pixels are retained in
it.

| frames | pairs | trace payload | trace allocation budget (payload + 64 B) | identity | down-2 |
|---:|---:|---:|---:|---:|---:|
| 8 | 7 | 560 B | 624 B | 624 B | 624 B |
| 30 | 29 | 2,320 B | 2,384 B | 2,384 B | 2,384 B |
| 60 | 59 | 4,720 B | 4,784 B | 4,784 B | 4,784 B |
| 120 | 119 | 9,520 B | 9,584 B | 9,584 B | 9,584 B |

For clarity, these are trace bytes, not the already-bounded working memory of
the requested generators. At 1920x1080, one existing difference accumulator
is 99,532,800 bytes (94.92 MiB), and one motion core is 8,812,800 bytes (8.40
MiB). At down-2 those values are 24,883,200 bytes (23.73 MiB) and 2,203,200
bytes (2.10 MiB). Normalized RGB16 retention is `6 * pixels * N`: identity is
94.92/355.96/711.91/1,423.83 MiB for 8/30/60/120 frames, while down-2 is
23.73/88.99/177.98/355.96 MiB. These existing buffers remain subject to the
scheduler's configured limits; the selected pair trace never grows with
pixels, and no design may retain a 100 MB-plus trace or a persistent
intermediate CAS.

The scheduler reservation must include the small trace and the exact number of
requested consumer cores in the existing combined-request budget. It must not
silently double-count a shared accumulator or undercount a second motion/diff
core. On a reservation failure, return the existing resource-limit error before
building the context.

### Determinism, manifests, and ordering

The coordinator consumes pairs and pixels in source declaration order. It does
not parallelize consumers or use unordered maps to choose a winner. Selector
logic continues to use its current lexicographic `PeakMetrics` ordering,
strict replacement behavior, earlier-index tie behavior, segment boundaries,
marker/gap anchors, and `usize::MAX - index` tie key. Timestamps remain source
frame timestamps plus each comparison's checked elapsed nanoseconds.

The context is an internal execution optimization, not a new visual
transformation. Existing algorithm names/versions, normalization and
threshold provenance, selected source-frame IDs, gap lists, changed bounds,
manifest parameter maps, encoded PNG bytes, and output hashes must remain byte
identical between the baseline and reuse paths. No context ID or cache-hit
metadata is added to an artifact manifest.

## Profiling summary and skipped probes

The release sampled profile is strong enough to select the algorithmic fix:
`classify_pixel_change` and `measure_pair` together dominate the measured CPU
family, and the source call graph proves the same adjacent pair is traversed by
multiple consumers. The selector's baseline comparisons are intentionally
left as a separate non-adjacent pass.

The discovery report did not contain hardware-counter values, allocation
profiles, or peak-RSS distributions for this feature. The benchmark contract
below collects `task-clock`, cycles, instructions, cache misses, and branch
misses with `perf stat`, cumulative allocation bytes in the benchmark process,
and peak RSS. If hardware counters are unavailable due host permissions, record
that fact and retain task-clock/CPU, wall, allocation, and RSS evidence; do not
invent cache results. No I/O, off-CPU, NUMA, or parallelism probe is needed for
the chosen level, but service runs must still record capture-headroom proxies.

## Optimization plan

### Optimization 1: Establish release-mode call-count and equivalence baseline

**Hierarchy Level**: Algorithmic / Data Model (measurement scaffold)

**Probe Family**: Workload baseline, on-CPU, memory allocation, microarchitecture

**Bottleneck**: The current source has duplicate adjacent-pair scans, but the
post-normalization current revision needs a clean baseline before an improvement
percentage can be trusted.

**Expected Metric Movement**: No product movement; this unit produces reliable
`A`, `M`, `B`, pair-pass, classifier-call, wall, CPU, allocation, RSS, and
counter evidence plus exact artifact digests.

**Story**: `perf-temporal-share-pair-classification-opt-1-baseline-equivalence`

#### Implementation units

##### Unit 1.1: Benchmark fixture and accounting report

**File**: `crates/temporal-vision/tests/pair_classification_perf.rs`

The authorized scaffold is an ignored release integration benchmark using a
browser-free 1920x1080 synthetic sequence with a moving patch. It accepts
`PERF_PAIR_FRAMES=8|30|60|120`, `PERF_PAIR_SCALE=identity|down2`,
`PERF_PAIR_EVIDENCE=clean|masked|gapped`, and
`PERF_PAIR_GENERATORS=storyboard-difference|storyboard-difference-motion`.
It runs the current public generators, prints JSON with the verified consumer
call/pass accounting, wall time, allocation bytes, RSS high-water delta, and
artifact/manifest digests, and runs twice to assert deterministic equality.
It is not a production runtime dependency and is not run in CI by default.

Run one cell as:

```text
PERF_PAIR_FRAMES=60 PERF_PAIR_SCALE=identity \
  PERF_PAIR_GENERATORS=storyboard-difference \
  perf stat -e task-clock,cycles,instructions,cache-misses,branch-misses \
  cargo test -p temporal-vision --test pair_classification_perf --release --locked -- \
    --ignored --exact --nocapture baseline_pair_classification_profile
```

Repeat for both scales, all four frame counts, and the motion set. A small
`PERF_PAIR_WIDTH/HEIGHT` override is permitted only for compile/smoke checks;
it is not acceptance evidence. The implementation pass adds the candidate
variant to this same scaffold rather than creating a second workload.

**Acceptance criteria:**

- [ ] Current-revision five-run cold baselines are recorded for 8/30/60/120,
      identity/down-2, with `A`, `M`, `B`, pair-pass, and classifier-call
      accounting matching the source formulas.
- [ ] Wall, process CPU/task-clock, cumulative allocation, peak RSS, and
      available hardware-counter results are recorded for every acceptance cell.
- [ ] Baseline output bytes, manifests, hashes, and normalized buffers repeat
      exactly; no benchmark result is accepted from a cache hit.

### Optimization 2: Build one bounded temporal-vision pair context

**Hierarchy Level**: Algorithmic / Data Model

**Probe Family**: On-CPU and memory allocation

**Bottleneck**: `measure_adjacent`, difference accumulation, and motion-history
accumulation independently execute the same classifier over the same normalized
adjacent pair.

**Expected Metric Movement**: The shared default storyboard + difference path
removes one `M * P` adjacent pixel pass; the motion path removes two. The
non-adjacent selector baseline work remains. The context allocation is at most
9,584 bytes at 120 frames and does not scale with image pixels.

**Why higher levels do not apply**: This removes work before data-layout,
runtime, or parallelism changes; no thread or SIMD complexity is justified.

**Story**: `perf-temporal-share-pair-classification-opt-2-temporal-context`

#### Implementation units

##### Unit 2.1: Canonical pair event and context builder

**Files**:

- `crates/temporal-vision/src/measure.rs`
- `crates/temporal-vision/src/pair_analysis.rs` (new, if a separate home keeps
  the measurement module cohesive)
- `crates/temporal-vision/src/select.rs`
- `crates/temporal-vision/src/difference_map.rs`
- `crates/temporal-vision/src/motion_history.rs`

Define one crate-private context builder with an explicit request-local
consumer plan. Keep `measure_pair` as the public direct-pair authority and
make the context use the same threshold/classifier and checked aggregate
helpers. Split difference-map accumulation from its render/data wrapper so a
prebuilt core can be rendered without rescanning. Split motion-plan creation
similarly, preserving per-segment reset, saturating `u16` accumulation, max
composition, outline, measured/gap counts, and range metadata.

**Acceptance criteria:**

- [ ] One context build produces the same `FrameComparison` values as
      `measure_adjacent` for clean, masked, gapped, equal-timestamp, threshold,
      and identity/down-2 sequences.
- [ ] Difference and motion cores receive exactly the same changed decisions,
      weighted magnitudes, later-frame offsets, mask exclusions, gap behavior,
      segment ranks, and checked overflow handling as their baseline scans.
- [ ] Storyboard selected frames, reasons, visual moments, changed bounds,
      role indices, tie ordering, manifests, encoded bytes, and hashes are
      byte/value identical to baseline.
- [ ] The context's retained trace allocation stays within the table above and
      never stores a per-pixel bitmap, magnitude trace, normalized pixels, or
      persistent artifact intermediate.
- [ ] Cancellation/deadline checkpoints prevent publication of partial results;
      errors drop the context and preserve the existing explicit error code.

### Optimization 3: Group compatible service generators under scheduler budgets

**Hierarchy Level**: Algorithmic / Data Model at the service boundary

**Probe Family**: Workload baseline, memory allocation, off-CPU/cancellation,
cache isolation, and capture-headroom observation

**Bottleneck**: `run_flight` currently groups by generator index, so compatible
storyboard, difference-map, and motion-history generators cannot share one
pair traversal even though normalized buffers are reused.

**Expected Metric Movement**: Cold 60/120-frame multi-output E2E wall time must
improve by at least 20% against the current-revision five-run baseline after
normalization rebaseline. Pair-pass and classifier-call counts must fall by the
formula above. Peak RSS must be no more than 8% above the current baseline, and
capture-ingestion queue/gap/headroom measurements must not regress.

**Why higher levels do not apply**: The dominant duplication is at the service
work grouping and visual algorithm boundary; parallel generator fan-out would
add memory and scheduler contention before this work is removed.

**Story**: `perf-temporal-share-pair-classification-opt-3-service-scheduler-wiring`

#### Implementation units

##### Unit 3.1: Compatible-slot grouping and generator adapters

**Files**:

- `src/artifacts/service.rs`
- `src/artifacts/generators.rs`
- `src/artifacts/scheduler.rs`
- `src/artifacts/epoch.rs`

Group pending slots by epoch and exact analysis identity, construct one context
per group, and let each requested output consume its appropriate prebuilt core.
Keep result slots and publication order in the existing deterministic ordinal
order. Keep one generator permit for the grouped blocking job unless measured
scheduler evidence proves a narrower safe reservation. Account trace bytes and
each distinct consumer core in the existing memory reservation; do not create a
second unbounded semaphore or a global cache.

**Acceptance criteria:**

- [ ] Compatible storyboard/orientation/difference and optional motion outputs
      share one adjacent traversal; incompatible epochs, masks, normalization
      identities, measurement parameters, and request/flight lifetimes do not.
- [ ] Epoch partitioning preserves image/viewport/device-scale boundaries,
      timestamps, declared gaps, markers, frame IDs, and reference-frame
      validation.
- [ ] Existing single-flight, cancellation, deadline, session-deletion,
      source-revalidation, publication, cache-hit, and partial-failure tests
      remain explicit and deterministic.
- [ ] Current scheduler limits reject an over-budget grouped context before
      allocation and preserve capture headroom; no 100 MB-plus trace is
      retained.
- [ ] Cold E2E at 60 and 120 frames reaches the rebaselined 20% wall target or
      the feature body records the measured result and a justified decision not
      to ship the optimization.

## Benchmarks and acceptance thresholds

**Scaffold location to be created by story 1**:
`crates/temporal-vision/tests/pair_classification_perf.rs`. The design pass
validated its intended baseline shape with a small local smoke fixture and
removed the temporary file under the concurrent-worker rule. Story 1 recreates
it as the authorized benchmark scaffold; no benchmark file is part of this
uncommitted design-only change.

**Stage command**:

```text
for n in 8 30 60 120; do
  for scale in identity down2; do
    PERF_PAIR_FRAMES=$n PERF_PAIR_SCALE=$scale \
      PERF_PAIR_GENERATORS=storyboard-difference \
      perf stat -e task-clock,cycles,instructions,cache-misses,branch-misses \
      cargo test -p temporal-vision --test pair_classification_perf --release --locked -- \
        --ignored --exact --nocapture baseline_pair_classification_profile
  done
done
```

Run the motion generator set separately. The production-policy run must use
the existing real `RecordingStore`, `TemporalVisionArtifactService`,
`ArtifactWorkLimits`, and cache-isolated source/artifact namespace; it must not
launch Chrome or a paid model. Record stage timings for decode, normalization,
pair context, selection, difference/motion core, rendering, encoding/hash, and
publication, plus E2E wall/CPU, allocations, RSS, scheduler memory permits,
blocking-job utilization, queue latency, and dropped/gap counts. Candidate and
baseline must use the same artifact IDs only for comparison purposes and must
compare the resulting stored bytes/manifests/hashes independently of IDs.

**Baseline targets**:

- Historical reference: 1,318.018 ms at 60 and 2,583.599 ms at 120.
- Authoritative implementation baseline: five cold current-revision repetitions
  after the normalization optimization, reported as `B60_current` and
  `B120_current`.

**Candidate thresholds**:

- Median cold E2E `<= 0.80 * B60_current` and `<= 0.80 * B120_current`.
- Exact normalized buffers, selection plans, difference/motion cores,
  manifests, encoded PNG bytes, and output hashes equal the baseline across
  clean, gap, mask, equal-timestamp, threshold-boundary, identity, and down-2
  fixtures.
- Peak RSS `<= 1.08 *` the current 120-frame baseline; no increase in decoded
  or normalized scheduler reservations beyond the exact shared-core budget;
  no capture queue starvation, unrepresented cancellation, or new gap.
- The benchmark reports pair-classifier calls/passes reduced according to the
  selected consumer set. A speedup without call reduction is not evidence that
  this feature succeeded.
- If the candidate misses 20% at either 60 or 120, do not compensate by adding
  parallel generator fan-out, a persistent cache, a large trace, or a
  normalization change. Record the measured result and stop this feature; any
  remaining bottleneck becomes separately scoped work.

## Implementation order

1. `perf-temporal-share-pair-classification-opt-1-baseline-equivalence`
2. `perf-temporal-share-pair-classification-opt-2-temporal-context` (depends on 1)
3. `perf-temporal-share-pair-classification-opt-3-service-scheduler-wiring`
   (depends on 2)

The feature remains one implementation/review boundary. The child stories are
sequential measurement and integration checkpoints, not separate parallel
owners.

## Rejected alternatives

- **Full per-pixel weighted-square trace**: even the smallest dense `u64`
  payload is 110.8 MiB at 8 identity frames and approximately 1.85 GiB at 120
  identity frames before metadata; it violates the bounded trace requirement.
- **Changed bitmap plus sparse magnitudes**: dense motion makes it larger than
  the dense weighted-square representation, while sparse encodings add branch,
  indexing, and deterministic-order complexity. It also remains a retained
  pixel trace.
- **Aggregate `FrameComparison` cache only**: it is small but cannot feed
  difference or motion per-pixel accumulators, so those consumers would still
  rescan and the measured duplication would remain.
- **Persistent intermediate CAS or process-global trace cache**: lifecycle,
  deletion, retention, source-integrity, and cancellation complexity is
  disproportionate to one local request and risks retaining browser evidence.
- **Lazy difference accumulators, packed accumulator layouts, GPU residency,
  normalization changes, and parallel generator fan-out**: profile-gated or
  explicitly deferred; they do not earn priority before duplicate pair work
  is removed and would make memory/capture behavior harder to prove.

## Blockers

No design blocker remains. The implementation gate has one required evidence
step: rebaseline current cold 60/120 production E2E after the normalization
optimization before claiming the 20% threshold. Hardware counters may be
unavailable under host permissions; that degrades only the microarchitecture
sub-probe if task-clock, CPU, allocation, RSS, call-count, exact-output, and
capture-headroom evidence remains. A concurrent normalization review-fix
worker owns its files and story; this design does not stage, reset, or modify
those changes.

## Discovery notes

This feature was emitted by `perf-design` discovery with `gate_origin: perf-design`.
The design pass is direct-read only and does not alter temporal-vision,
normalization, Chrome, model, scheduler, or live-feature code. The benchmark
scaffold is specified above and is intentionally deferred to the first child
story so the host can commit this feature/story design without shared-index
contamination.

## Integrated implementation evidence

All three child checkpoints are complete: opt-1 baseline/equivalence, opt-2
bounded temporal context, and opt-3 production service/scheduler wiring. The
production path now groups only exact compatible request identities, reserves
trace and distinct cores through the existing combined-request budget, shares
orientation selection, preserves IDs/publication order and all cache/source
fences, and leaves region/direct paths and failure isolation intact. The
service qualification uses the real `TemporalVisionArtifactService` and
`RecordingStore`; no Chrome, model, network, persistent cache, new semaphore,
normalization, or fan-out was introduced.

The final five cold Rust 1.85 release repetitions per 1920x1080 PNG/down-2
production context cell were:

- 60 frames: `[996.250, 997.345, 999.354, 1,000.497, 1,001.882]` ms,
  median `999.354` ms; context-disabled production policy median `1,262.124`
  ms.
- 120 frames: `[2,043.220, 2,044.905, 2,049.344, 2,076.290, 2,077.391]` ms,
  median `2,049.344` ms; context-disabled production policy median
  `2,598.758` ms.
- Motion cells: 60 median `1,030.963` ms and 120 median `2,125.889` ms.

Against the authoritative supplied baselines `B60=1,048.717` ms and
`B120=2,108.262` ms, the candidate is only 4.71% and 2.79% faster, not the
required 20% (required medians `838.974` ms and `1,686.610` ms). This is an
honest target miss. The feature therefore stops here without lower-level,
parallel, normalization, or cache work; the paired production-policy result
still demonstrates 20.82%/21.14% improvement and the expected classifier
reduction, but does not replace the authoritative gate.

Peak RSS was at most `1,388,980 KiB` at 120 frames versus supplied B120
`1,357,328 KiB` (2.33%); allocated-byte deltas were recorded for clean and
motion cells. Representative external `perf stat` evidence recorded task
clock/cycles/instructions/cache-misses/branch-misses for both frame sizes;
the exact distributions, counters, reservations, cache dispositions, output
bytes/manifests/hashes, and gate results live in child opt-3's implementation
notes. `cargo fmt --all -- --check`, full locked workspace check/test, and
full locked workspace clippy with `-D warnings` pass.

## Review and rollback disposition

The accepted STANDARD feature review completed as one independent pass. The
optimization hypothesis failed its authoritative gate: the context candidate
was only 4.71% faster at 60 frames and 2.79% faster at 120 frames, against the
required at-least-20% improvement. The paired context-disabled production
comparison is retained as evidence, but it is not the acceptance baseline.

The low-risk opt-1 baseline/equivalence scaffold from `518c5c3` is retained,
including `crates/temporal-vision/tests/pair_classification_perf.rs`. Production
and pure pair-context implementation from `c54ddc3` and `269caba` was
mechanically removed, restoring the code to the pre-context baseline. The
review blockers were eliminated by deletion rather than patched: same-decay
motion single-consumption, the public storyboard context source/anchor
validation gap, missing production cancellation checkpoints, and stale motion
benchmark accounting no longer exist in the retained code path. The opt-2 and
opt-3 bodies remain unchanged as experimental implementation evidence, with
rollback dispositions appended to their records.

No lower-level, normalization, cache, parallelism, Chrome, model, network, or
other feature work was introduced. The feature is closed as a measured negative
result. Standard policy requires no re-review after this verified rollback.
