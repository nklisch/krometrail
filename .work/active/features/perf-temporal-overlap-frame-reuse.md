---
id: perf-temporal-overlap-frame-reuse
kind: feature
stage: done
tags: [perf, visual, storage, testing]
parent: null
depends_on: []
release_binding: 1.0.0
gate_origin: perf-design
created: 2026-07-15
updated: 2026-07-15
---

# Reuse Decoded and Normalized Frames Across Overlapping Queries

## Discovery brief

Design bounded reuse for nearby temporal artifact queries that share retained
source frames. The current service reuses decoded/normalized work only inside
one exact-key single-flight request; distinct overlapping ranges generate
independent work. This crosses artifact scheduling, lifecycle/invalidation,
memory accounting, and the temporal-vision storage representation, so it
remains a `[perf]` feature rather than an implementation story.

## Design decisions

- **Measured disposition: viable, but only as an in-flight request-batch
  work-flight.** Current post-normalization, post-pair-rollback measurements
  show 119 duplicate decode and 119 duplicate normalization frame operations
  for two 120-frame adjacent requests. The work is material enough to justify
  one bounded design pass. A persistent intermediate CAS or a process-wide TTL
  cache is rejected: it would add deletion/session lifecycle and memory pressure
  without evidence that completed queries need to remain reusable.
- **Lifetime:** an intermediate entry is retained only while at least one
  generation request in the shared work batch owns it. In-flight leaders and
  waiters share the entry; the last batch lease drops the pixels and their byte
  reservation. There is no durable artifact-store row, filesystem object, or
  completed-query TTL. Sequential sliding windows are a deliberate no-reuse
  control unless a later measurement proves a short-lived lease safe.
- **Data retained:** decoded RGBA8 frame pixels and normalized linear RGB16
  pixels, both behind immutable `Arc` storage. Sharing only normalized output
  would leave duplicate decode work and did not meet the target; sharing only
  decoded output would leave the measured normalization pass duplicated.
- **Memory policy:** byte-weighted admission, not an item-count limit. The
  shared entry owns the scheduler permits for its unique pixel bytes. A second
  request that joins an entry does not reserve those bytes again. Entries that
  cannot be admitted are not retained after their current waiter set completes
  and are recomputed on a later request. In-flight entries are never evicted;
  completed entries can be dropped from the lookup table when the byte budget is
  reached, and are recomputed rather than growing memory.
- **Budget:** the implementation must expose a bounded reuse-byte limit and
  charge it against the existing artifact memory budget. For the qualification
  policy, use the smaller of 1.5 GiB and the validated combined-request budget
  after two request-local output reservations and a 128 MiB capture reserve.
  The 120-frame down-2 overlap needs about 1,379,980,800 unique decoded plus
  normalized bytes for 121 source frames, so it fits this cell; identity 1080p
  normalization needs about 2,509,056,000 bytes for the same 121 frames and
  must be rejected or downscaled rather than violating the bound. The bound is
  deliberately not a second unaccounted memory pool.
- **Scheduler shape:** retain one permit for each unique shared pixel entry and
  keep output/publication reservations request-local. Preserve the existing
  request, blocking-job, and generator permits. The one-permit cell must remain
  a valid capture-headroom control; the two-permit cell is the overlap target.
- **No pair-context work:** the rolled-back pair-classification optimization is
  not revived or combined with this feature. Current pair scans remain the
  post-rollback baseline.

## Exact current baseline evidence

All measurements below are browser-free and use Rust 1.85.0 release builds with
locked dependencies on the current tree after the normalization fast path and
pair-context rollback. Host: Linux x86_64, AMD Ryzen 7 7800X3D, 8 cores/16
threads, 96 MiB L3. The overlap scaffold uses 1920x1080 PNG source frames,
explicit production-policy down-2 analysis, storyboard + orientation +
difference-map outputs, one generator permit per request, a 2 GiB benchmark
combined-memory ceiling, and a fresh recording store per process.

### Retained normalization benchmark

`crates/temporal-vision/tests/temporal_normalize_perf.rs`, two repetitions per
cell:

| frames | normalization wall ms | end-to-end wall ms |
|---:|---:|---:|
| 30 | 147.426 / 135.781 | 790.609 / 731.698 |
| 60 | 282.938 / 278.978 | 1,438.863 / 1,405.473 |
| 120 | 576.009 / 587.583 | 2,814.910 / 2,780.932 |

The normalized digest and all three artifact digests were equal across both
repetitions in every cell. Identity-scale 60/120 runs correctly hit the
benchmark's default processing-byte limit; the overlap qualification therefore
uses the retained down-2 production policy and treats identity as a memory
admission case, not as a silently substituted result.

Command:

```text
PERF_TEMPORAL_FRAMES=<30|60|120> PERF_TEMPORAL_SCALE=down2 PERF_TEMPORAL_REPETITIONS=2 \
  rustup run 1.85.0 cargo test -p temporal-vision --release --locked \
  --test temporal_normalize_perf -- --ignored --exact production_policy_release_profile --nocapture
```

### Post-rollback pair baseline

`crates/temporal-vision/tests/pair_classification_perf.rs`, clean
storyboard + difference mode, two repetitions:

| frames | wall ms | CPU ms | allocations bytes | classified pixel passes |
|---:|---:|---:|---:|---:|
| 30 | 599.080 / 531.204 | 596.730 / 529.269 | 130,144,855 | 78 |
| 60 | 1,043.175 / 996.615 | 1,039.378 / 993.143 | 223,488,695 | 158 |
| 120 | 2,086.502 / 1,946.789 | 2,048.041 / 1,939.199 | 410,162,461 | 318 |

The 120-frame run reports 164,851,200 classifier pixel calls and the
`2M+B` formula. This confirms that pair-context sharing remains rolled back and
is not being credited to this design.

### Current production-service overlap and sliding baselines

The ignored scaffold at `src/artifacts/overlap_perf.rs` records exact
manifest/output/artifact SHA-256 values, source IDs, cache dispositions,
allocation bytes, process CPU, RSS/HWM, decode/normalize call counters, and
scheduler headroom fields. Current intermediate hit counters are zero.
Values are one fresh-process repetition; RSS is `VmHWM` delta and is naturally
noisy, so the acceptance gate requires repeated candidate runs.

Concurrent adjacent windows share N-1 source frames:

| frames | permits | wall ms | CPU ms | allocations bytes | peak RSS delta KiB | decoded frames | normalized frames |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 30 | 1 | 1,055.854 | 1,052.231 | 1,334,247,098 | 369,288 | 60 | 60 |
| 30 | 2 | 552.743 | 1,077.649 | 1,334,247,288 | 716,588 | 60 | 60 |
| 60 | 1 | 1,965.616 | 1,958.659 | 2,596,340,462 | 699,132 | 120 | 120 |
| 60 | 2 | 1,123.346 | 2,177.631 | 2,596,340,524 | 1,392,108 | 120 | 120 |
| 120 | 1 | 4,202.090 | 4,177.414 | 5,120,361,575 | 1,375,184 | 240 | 240 |
| 120 | 2 | 3,982.190 | 3,962.712 | 5,120,361,765 | 1,384,888 | 240 | 240 |

Sequential one-frame sliding controls use four windows and intentionally do not
reuse completed work:

| frames | permits | wall ms | CPU ms | allocations bytes | peak RSS delta KiB | decoded frames | normalized frames |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 30 | 1 | 2,147.128 | 2,138.316 | 2,668,485,522 | 369,228 | 120 | 120 |
| 30 | 2 | 2,107.295 | 2,099.041 | 2,668,485,522 | 369,328 | 120 | 120 |
| 60 | 1 | 3,888.260 | 3,874.204 | 5,192,670,335 | 702,636 | 240 | 240 |
| 60 | 2 | 3,896.484 | 3,882.317 | 5,192,670,335 | 702,948 | 240 | 240 |
| 120 | 1 | 8,485.594 | 8,436.470 | 10,240,705,979 | 1,375,092 | 480 | 480 |
| 120 | 2 | 8,519.253 | 8,461.026 | 10,240,705,979 | 1,375,392 | 480 | 480 |

A single 120-frame request was also measured at 2,164.372 ms wall,
2,137.243 ms CPU, 2,560,183,465 allocated bytes, and 1,393,900 KiB peak HWM.
This is the conservative RSS reference for the acceptance budget. Current
artifact-cache hits are zero for all distinct windows; the durable final
artifact cache is not a source of intermediate reuse.

The 120-frame two-permit cell was additionally run under
`perf stat`: 18,012,248,771 cycles, 79,152,917,773 instructions, 13,983,329
cache misses, and 4,139,380 branch misses over 4.334 seconds task-clock. These
counters are evidence for the CPU-bound workload, not a claimed cache miss
optimization; the candidate must rerun the same command and compare counters.

## Perf overview

The measured bottleneck is redundant source-frame work across distinct artifact
requests, at hierarchy level **algorithmic/data model** with CPU, allocation, and
workload-baseline probes. The current service:

- reads and validates each requested encoded frame in
  `src/artifacts/service.rs::generate_inner`;
- uses `src/artifacts/single_flight.rs::SingleFlight` only for the exact ordered
  artifact-key vector;
- reserves the full decoded + normalized + output estimate in
  `src/artifacts/scheduler.rs::ArtifactScheduler` per request;
- decodes each visual epoch once per flight in
  `src/artifacts/epoch.rs::decode_plan`;
- normalizes once per generator identity only within that flight in
  `src/artifacts/service.rs::run_flight`;
- drops all intermediate maps when the flight completes.

Thus two 120-frame windows perform 240 decodes and 240 normalized frame
operations even though 119 exact source frames are shared. Four sequential
windows perform 480 of each. The durable cache in `src/artifacts/cache.rs` and
`krometrail-store` correctly caches final artifacts, but its ordered source key
must remain cache-isolated for different ranges and must not be expanded into an
intermediate persistent store.

## Optimization plan

### Optimization 1: request-lifetime shared decoded/normalized work flight

**Hierarchy Level**: Algorithmic / Data Model

**Probe Family**: Workload baseline, on-CPU time, allocation/memory, off-CPU
scheduler behavior, and microarchitecture counters

**Bottleneck**: Distinct overlapping ranges have different final artifact keys,
so the current exact-key `SingleFlight` cannot share their decoded or normalized
source work. At 120 frames the two-permit baseline performs 240 decode calls and
240 normalized frames, allocates 5,120,361,765 bytes, and takes 3,982.190 ms
wall. The 119 shared frames represent 49.6% of the two-request frame-work
accesses; the expected shared decoded + down-2 normalized storage for 121 unique
frames is 1,379,980,800 bytes versus 2,737,152,000 bytes when each request owns
its copy.

**Expected Metric Movement**:

- one decode and one normalization operation per exact shared frame key;
- 119 decoded-frame hits and 119 normalized-frame hits in the 120 overlap cell;
- aggregate CPU and allocation reduction of at least 30%;
- two-permit 120 overlap wall reduction of at least 25%;
- no more than 8% HWM increase over the 1,393,900 KiB single-request reference;
- no capture-headroom regression or unbounded scheduler queue growth;
- sequential sliding controls may remain at zero hits because the selected
  lifetime ends with the prior request.

**Why higher levels do not apply**: This removes whole image decode and
normalization work. I/O batching cannot remove the CPU work, locality tuning is
smaller than eliminating duplicate frames, and parallelism would increase
memory/capture pressure before the duplicate work is removed.

**Story**: `perf-temporal-overlap-frame-reuse-opt-1-shared-work-flight`

#### Implementation Units

##### Unit 1.1: exact intermediate identity and shared entries

**Files**: `src/artifacts/cache.rs`, `src/artifacts/single_flight.rs`,
`src/artifacts/epoch.rs`, `src/artifacts/decode.rs`

Define typed internal keys rather than reusing the final artifact key:

```text
DecodedFrameKey {
  session_id, target_id,
  frame_id, capture_ordinal, session_time,
  source_format, image_dimensions, viewport_dimensions, device_scale_bits,
  encoded_sha256,
  visual_epoch_hash,
  decoder_profile,
  decoder_algorithm_version,
}

NormalizedFrameKey {
  decoded_key,
  visual_epoch_hash,
  source_crop, effective_scale, background,
  normalization_recipe_version, transfer_lut_version,
  mask_or_region_digest,
  normalization_algorithm_version,
}
```

The source hash is computed from the exact retained encoded bytes already loaded
by `SourceFingerprint::from_frame`. The visual epoch identity includes ordered
geometry and scale. Every key includes the session/target scope even though the
frame ID and source hash are also present. Measurement parameters remain in the
final artifact key and in any future analysis-work key; they cannot change a
normalized pixel buffer, so they must not be silently omitted from a later
measurement cache.

Add a `WorkBatchLease` registry alongside, not inside, the durable artifact
cache. Entries hold immutable `Arc` pixel storage and the byte permits for the
unique storage. The registry uses weak cleanup after the last lease; it has no
filesystem or SQLite representation. Per-entry state is `InFlight`, `Ready`, or
`Failed`; failures and cancellations are never retained.

**Implementation Notes**:

- Preserve source order and visual-epoch partitioning; a hit supplies one exact
  frame, never an inferred frame or a cross-epoch substitute.
- A waiter can cancel without cancelling a leader while another waiter remains;
  when the last waiter drops, the shared work cancellation token is triggered,
  matching the existing `SingleFlight` behavior.
- Allocate artifact IDs in deterministic request slot order before shared work
  begins, so baseline and reuse runs can compare manifests byte-for-byte.

##### Unit 1.2: Arc-backed temporal-vision representations

**Files**: `crates/temporal-vision/src/frame.rs`,
`crates/temporal-vision/src/normalize.rs`, `src/artifacts/generators.rs`

Provide the narrowest internal/public-neutral constructor needed to assemble a
sequence from already validated immutable decoded frames and normalized frame
buffers without copying pixels. `OwnedFrame`/normalized frame payloads may use
`Arc<[u8]>`/`Arc<[u16]>` storage while retaining the existing read-only accessors
and deterministic `FrameSequence`/`NormalizedSequence` contracts.

The constructor must recompute or validate sequence-level dimensions, crop,
mask, analysis pixel count, gap ranges, and normalization provenance. It must
reject a frame whose ID, timestamp, dimensions, or normalized dimensions do not
match the requested epoch. No caller may inject a normalized frame without the
same normalization recipe and algorithm version.

##### Unit 1.3: service and scheduler integration

**Files**: `src/artifacts/service.rs`, `src/artifacts/scheduler.rs`,
`src/artifacts/epoch.rs`

Pass one batch lease through `run_flight`. Decode and normalize through
per-frame work flights, assemble each request's source and normalized sequence in
source order, then run the existing generators and publication path unchanged.
Split the existing combined memory reservation into:

1. unique shared intermediate bytes owned by work entries; and
2. request-local output, manifests, and temporary publication bytes.

The shared byte budget is acquired before publication of a `Ready` entry and is
released with the last `Arc`/batch lease. If the budget cannot admit an entry,
keep sharing only among current in-flight waiters and allow later work to
recompute. The request must fail explicitly on an impossible single-request
reservation; it must not silently exceed the limit.

Do not add a global Rayon pool, a persistent cache, or pair-analysis context.
Keep one/two request permits and one generator permit as explicit benchmark
cells, and record when a cell is memory-serialized rather than claiming
parallel capture safety.

##### Unit 1.4: deletion, invalidation, and cancellation fences

**Files**: `crates/krometrail-store/src/recording.rs`,
`crates/krometrail-store/src/artifacts/mod.rs`,
`src/artifacts/service.rs`, tests under `src/artifacts/` and
`crates/krometrail-store/tests/`

Intermediate entries are never authority. Every request still obtains the exact
source frames and passes current `validate_and_plan`; a source hash, metadata,
visual epoch, or deletion mismatch is a miss/failure. Final artifact lookup and
publication continue through `lookup_artifact`, `validate_source_payloads`,
`PublicationRegistry`, and the existing artifact cache lock.

The request-lifetime registry has no completed-session retention. A session
being deleted can therefore leave only transient in-flight buffers owned by an
active request; source reads reject the deleted session, cancellation stops
unpublished work, publication rechecks the deletion fence, and the batch lease
drops all intermediate bytes before the generation task completes. Add tests
for deletion during decode, deletion during normalization, source corruption
between two overlapping requests, invalidated durable artifacts, waiter
cancellation, leader cancellation, and concurrent publication.

## Benchmarks

**Location**: `src/artifacts/overlap_perf.rs` (ignored browser-free release
scaffold), with test-only call counters in `src/artifacts/perf_counters.rs` and
hooks at `decode_frame`/`generators::normalize`.

**Build**:

```text
rustup run 1.85.0 cargo test -p krometrail --release --locked --no-run
```

**Concurrent adjacent cells** (run every frames/permit combination):

```text
PERF_OVERLAP_FRAMES=<30|60|120> PERF_OVERLAP_MODE=concurrent \
PERF_OVERLAP_REQUEST_PERMITS=<1|2> PERF_OVERLAP_REPETITIONS=5 \
  rustup run 1.85.0 cargo test -p krometrail --release --locked \
  artifacts::overlap_perf::overlap_and_sliding_release_profile \
  -- --ignored --exact --nocapture
```

**Sequential sliding controls**:

```text
PERF_OVERLAP_FRAMES=<30|60|120> PERF_OVERLAP_MODE=sequential \
PERF_OVERLAP_REQUEST_PERMITS=<1|2> PERF_OVERLAP_SLIDING_WINDOWS=4 \
PERF_OVERLAP_REPETITIONS=5 \
  rustup run 1.85.0 cargo test -p krometrail --release --locked \
  artifacts::overlap_perf::overlap_and_sliding_release_profile \
  -- --ignored --exact --nocapture
```

**Hardware counters** (repeat at least for 60 and 120 two-permit overlap):

```text
PERF_OVERLAP_FRAMES=120 PERF_OVERLAP_MODE=concurrent \
PERF_OVERLAP_REQUEST_PERMITS=2 PERF_OVERLAP_REPETITIONS=1 \
perf stat -e task-clock,cycles,instructions,cache-misses,branch-misses \
  /storage/cargo-target/release/deps/krometrail-<current-test-hash> \
  artifacts::overlap_perf::overlap_and_sliding_release_profile \
  --ignored --exact --nocapture
```

The scaffold must report, per repetition and per request/window:

- request mode, frame count, shared-frame count, one/two request permits, and
  sliding count;
- intermediate decoded-frame hit count, normalized-frame hit count, decode
  calls/frames, normalization calls/frames, and durable artifact hits;
- wall time, process CPU time, allocations, current RSS, peak RSS/HWM;
- exact source frame IDs, ordered manifest SHA-256, PNG/output SHA-256, and
  combined manifest+PNG artifact SHA-256;
- unique shared-byte reservation, request-local reservation, blocking permits,
  generator permits, and a clearly labeled browser-free capture-headroom proxy;
- external hardware-counter status without synthesizing missing values.

Candidate equivalence runs must execute the same windows twice with the same
source fixture and deterministic ID source: one cache-disabled path and one
reuse-enabled path. For each `(window, generator kind, output slot)`, require
exact equality of manifest JSON bytes, encoded PNG bytes, output hash, source
frame IDs/order, visual epoch, normalization/mask/measurement provenance, and
combined artifact hash. A differing opaque artifact ID is not an acceptable
shortcut; reserve IDs deterministically or compare the full canonical manifest
with the same ID allocation transcript.

## Acceptance criteria

- [ ] The two-permit 120-frame overlap cell reaches at least **25% lower wall
      time**, **30% lower process CPU**, and **30% lower allocated bytes** than
      the current 3,982.190 ms / 3,962.712 ms / 5,120,361,765-byte baseline:
      wall <= 2,986.643 ms, CPU <= 2,773.898 ms, allocations <=
      3,584,253,236 bytes.
- [ ] Peak RSS remains <= **1,505,412 KiB**, eight percent above the measured
      1,393,900 KiB single-request 120-frame reference, and no candidate cell
      exceeds its validated shared/request-local byte budget.
- [ ] The two-permit 30/60/120 concurrent runs report the expected exact
      shared-frame hit counts (N-1 where admission permits) and reduce each
      shared decode/normalization operation to one execution per key. One-permit
      cells remain bounded controls and do not claim sharing when requests are
      serialized.
- [ ] Sequential sliding results remain explicit: either they show measured
      benefit only if a separately bounded short-lived lease is added, or the
      report records zero completed-request hits and the feature does not add a
      persistent cache merely to improve this control.
- [ ] Cache-disabled and reuse-enabled outputs are byte-identical at normalized
      buffer, manifest, PNG, output-hash, source-provenance, and artifact-hash
      levels for clean, masked, gapped, alpha, crop/scale, visual-epoch, and
      tie-timestamp fixtures.
- [ ] Deletion, source corruption, durable-artifact invalidation, cancellation,
      concurrent publication, cache-budget admission, and capture-headroom
      tests pass. No intermediate bytes survive a completed request batch or
      deleted session.
- [ ] If any target threshold or fence fails, do not implement/ship the reuse
      mechanism. Record the measured disposition in this feature and close the
      child stories as rejected; retain the benchmark scaffold and current
      simpler behavior.

## Implementation order

1. `perf-temporal-overlap-frame-reuse-opt-1-shared-work-flight` — introduce
   exact keys, Arc-backed representations, the request-lifetime work-flight
   primitive, and its unit/fence tests.
2. `perf-temporal-overlap-frame-reuse-opt-2-scheduler-service-integration` —
   integrate shared entries with byte permits, epoch assembly, generator
   requests, cancellation, deletion, and publication revalidation.
3. `perf-temporal-overlap-frame-reuse-opt-3-qualification-equivalence` — run
   all release benchmark cells, hardware-counter comparisons, exact artifact
   equivalence, and the acceptance decision. This is the final implementation
   checkpoint, not permission to lower the thresholds.

## Profiling and design notes

No Chrome, network, model, live evidence, push, or persistent intermediate CAS
was used. `.work/bin/work-view` was pre-existing modified working-tree state
and was not edited. The pair-context optimization remains rolled back at
`fcaa5ec`; this feature intentionally designs only cross-query decoded and
normalized work reuse.

## Integrated qualification and rollback disposition

All three child checkpoints reached a terminal implementation decision. The
final qualification ran the real `TemporalVisionArtifactService` and
`RecordingStore` candidate from `0112f10` + `1855fdb` in fresh Rust 1.85 release
processes across concurrent adjacent and sequential one-frame sliding windows,
30/60/120 frames, one/two request permits, and five repetitions per cell. The
full distributions, reservations, call counters, cache dispositions, exactness
observations, and perf-stat output are recorded in
`perf-temporal-overlap-frame-reuse-opt-3-qualification-equivalence.md`.

The hard two-permit 120-frame gate was rejected: wall max `2339.923 ms` passed
`2986.643 ms`, and RSS max `1411548 KiB` passed `1505412 KiB`, but CPU min
`3353.483 ms` failed `2773.898 ms` and allocation min `4108022377` bytes failed
`3584253236` bytes. Concurrent two-permit hits were the expected 119 decoded
and 119 normalized frames; one-permit controls and sequential completed windows
had zero hits. Scheduler accounting remained explicit with a 128 MiB capture
reserve, 1.5 GiB shared-work cap, 4 blocking permits, and 1 generator permit;
the capture-headroom field remained a browser-free proxy and made no CDP claim.
External `perf stat` was permitted for the 60- and 120-frame two-permit cells;
all requested task-clock, cycle, instruction, cache-miss, and branch-miss values
are retained in the child with no denial.

The candidate also failed the exactness checkpoint. Repeated 120-frame
concurrent two-permit runs produced two artifact-evidence signatures: one run
swapped the generated difference-map artifact IDs and changed manifest and
combined hashes while PNG bytes remained equal. The required dedicated
cache-disabled/reuse-enabled equality across normalized buffers, manifests,
PNG bytes, source order, epochs, and provenance was therefore not claimed.
Candidate deletion, corruption/invalidation, cancellation, publication, and
headroom fence smoke passed, but those results cannot override the failed hard
performance and exactness gates.

The optimization source from `0112f10` and `1855fdb` was mechanically restored
to `97c4ea0`. This removes the intermediate work registry, Arc-backed reuse,
shared scheduler accounting, and service integration. The low-risk ignored
benchmark/design scaffold from `97c4ea0` remains, including its no-reuse
control. The child is closed as a measured rejection; no lower-level,
parallel, persistent-cache, or other speculative follow-up was introduced.
Rust 1.85 fmt, locked workspace check/test/clippy, and the retained release
benchmark smoke passed after rollback. `.work/bin/work-view` remains the
pre-existing modified working-tree file and was not staged.

## Review decision

**Approved as a measured rejection.** An independent GPT-5.5 standard integrated review verified that `4b6858c` restores all product code exactly to the pre-optimization `97c4ea0` state, leaving only the ignored benchmark and substrate evidence. It independently confirmed the hard-gate arithmetic, Rust 1.85 gates, and rollback smoke. The observed manifest/hash variation came from concurrent opaque artifact-ID allocation in the benchmark comparison while PNG bytes matched; it does not establish a product stale-pixel defect, but it correctly prevented the candidate from satisfying its stricter exactness gate. The review's sole advisory was accepted: the retained scaffold now reports expected decode/normalize calls per frame rather than per request. No material blocker remains. Per standard policy, no re-review was run; the feature advances to `done` with no reuse mechanism shipped.
