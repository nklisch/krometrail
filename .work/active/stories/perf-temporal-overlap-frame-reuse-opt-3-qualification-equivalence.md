---
id: perf-temporal-overlap-frame-reuse-opt-3-qualification-equivalence
kind: story
stage: done
tags: [perf, visual, storage, testing]
parent: perf-temporal-overlap-frame-reuse
depends_on: [perf-temporal-overlap-frame-reuse-opt-2-scheduler-service-integration]
release_binding: null
gate_origin: perf-design
created: 2026-07-15
updated: 2026-07-15
---

# Qualify overlap reuse and make the implementation decision

## Scope

Run the parent feature's ignored Rust 1.85 release benchmark after the shared
work integration and decide whether the mechanism earns its complexity. This
story is a hard qualification checkpoint, not permission to relax thresholds.

## Benchmark matrix

Run concurrent adjacent windows and sequential one-frame sliding controls at
30/60/120 frames with one and two request permits. Collect repeated wall and
process CPU samples, allocation bytes, RSS/HWM, scheduler reservations,
intermediate hit/decode/normalize counters, artifact-cache dispositions, and
capture-headroom proxies. Run `perf stat` for task-clock, cycles, instructions,
cache misses, and branch misses where permitted; record permission denial
verbatim.

## Exact equivalence

For the same deterministic source, IDs, request windows, and generator slots,
compare cache-disabled and reuse-enabled output by window and artifact kind.
Require byte equality for normalized buffers, manifest JSON, PNG bytes, output
hash, source frame IDs/order, visual epoch, normalization/mask/measurement
provenance, and combined artifact hash. Exercise clean, masked, gapped,
alpha/crop/scale, tie-timestamp, deletion, corruption, invalidation, and
cancellation cases.

## Acceptance decision

The two-permit 120 overlap result must achieve all of:

- wall <= 2,986.643 ms (25% below 3,982.190 ms);
- CPU <= 2,773.898 ms (30% below 3,962.712 ms);
- allocations <= 3,584,253,236 bytes (30% below 5,120,361,765 bytes);
- peak RSS <= 1,505,412 KiB (8% above the 1,393,900 KiB single-request
  reference);
- no capture-headroom, deletion, cancellation, or equivalence failure.

## Implementation notes

- Execution capability: inline feature-owner qualification and rollback; the final child owns the release matrix, exactness decision, and disposition of the two optimization commits.
- Review weight: standard default; this is a child-story checkpoint and does not enter review.
- Files changed: restored all optimization source files from `0112f10` and `1855fdb` to the low-risk scaffold revision `97c4ea0`; updated this story and the parent feature. The pre-existing modified `.work/bin/work-view` was preserved and not staged.
- Tests added/removed: no new permanent tests. Candidate-only service/fence tests were run before rollback; the ignored `src/artifacts/overlap_perf.rs` scaffold from `97c4ea0` was retained.
- Simplification: removed the bounded decoded/normalized intermediate registry, Arc-backed reuse path, shared scheduler reservations, and service integration rather than shipping a mechanism that missed hard gates. No persistent cache or follow-up optimization was added.
- Discrepancies from design: the candidate failed hard performance gates before a dedicated cache-disabled/reuse-enabled variant matrix could be accepted. Repeated candidate runs also exposed non-byte-identical manifest/artifact hashes in the 120-frame two-permit cell, so exactness is recorded as a failed gate rather than inferred from passing fence tests.
- Adjacent issues parked: none.

## Qualification evidence

Host/runtime: Linux x86_64, AMD Ryzen 7 7800X3D, 8 cores/16 threads, 96 MiB L3, Rust `1.85.0 (4d91de4e4 2025-02-17)`, locked release build. The candidate was measured after `1855fdb` and before rollback. Each cell used a fresh process/source store and five repetitions; 1920x1080 PNG inputs, explicit production down-2 analysis, storyboard + orientation + difference-map outputs, and no Chrome, model, network, or live evidence. Arrays below are repetition order 1..5. Wall/CPU are milliseconds; RSS is `VmHWM` delta KiB.

| mode | frames | permits | wall ms distribution | CPU ms distribution | allocations distribution (bytes) | peak RSS distribution (KiB) | intermediate hits D/N | decode/normalize frames | artifact cache/generated |
|---|---:|---:|---|---|---|---|---|---|---|
| concurrent adjacent | 30 | 1 | `[1135.210, 1089.741, 1078.726, 1088.657, 1084.512]` | `[1131.004, 1085.513, 1075.092, 1084.666, 1080.829]` | `[2018870098, 2018869780, 2018869780, 2018869780, 2018869780]` | `[371540, 0, 0, 12, 0]` | `0/0` | `60/60` | `0/6`
| concurrent adjacent | 60 | 1 | `[2213.356, 2195.597, 2198.011, 2197.365, 2177.491]` | `[2201.144, 2183.690, 2186.057, 2185.613, 2167.907]` | `[3965590250, 3965589932, 3965589932, 3965589932, 3965589932]` | `[707752, 544, 100, 676, 84]` | `0/0` | `120/120` | `0/6`
| concurrent adjacent | 120 | 1 | `[4585.152, 4606.413, 4396.177, 4519.250, 4348.859]` | `[4553.586, 4526.238, 4344.606, 4410.841, 4310.100]` | `[7858865315, 7858864997, 7858864997, 7858864997, 7858864997]` | `[1381168, 1508, 248, 0, 0]` | `0/0` | `240/240` | `0/6`
| concurrent adjacent | 30 | 2 | `[555.779, 549.685, 547.092, 550.148, 552.036]` | `[880.788, 872.500, 871.607, 873.641, 873.943]` | `[1104799484, 1104798944, 1104798944, 1104798944, 1104798944]` | `[419240, 0, 0, 0, 0]` | `29/29` | `31/31` | `0/6`
| concurrent adjacent | 60 | 2 | `[1073.885, 1064.452, 1060.554, 1069.390, 1067.807]` | `[1673.718, 1664.268, 1660.175, 1666.166, 1664.657]` | `[2105929020, 2105928448, 2105928544, 2105928544, 2105928480]` | `[758356, 2064, 0, 0, 0]` | `59/59` | `61/61` | `0/6`
| concurrent adjacent | 120 | 2 | `[2238.928, 2259.263, 2339.923, 2244.244, 2255.672]` | `[3387.118, 3353.483, 3481.406, 3374.655, 3391.158]` | `[4108023045, 4108022473, 4108022377, 4108022473, 4108022473]` | `[1411548, 0, 0, 0, 0]` | `119/119` | `121/121` | `0/6`
| sequential sliding | 30 | 1 | `[2171.744, 2169.935, 2176.465, 2172.304, 2073.447]` | `[2164.108, 2161.987, 2167.853, 2164.193, 2065.217]` | `[4037664522, 4037664204, 4037664204, 4037664204, 4037664204]` | `[370952, 0, 32, 2156, 0]` | `0/0` | `120/120` | `0/12`
| sequential sliding | 60 | 1 | `[4208.001, 4196.177, 4198.784, 4025.486, 3885.026]` | `[4192.056, 4180.496, 4183.280, 4010.523, 3871.661]` | `[7931034487, 7931034169, 7931034169, 7931034169, 7931034169]` | `[707672, 268, 2480, 212, 808]` | `0/0` | `240/240` | `0/12`
| sequential sliding | 120 | 1 | `[8716.916, 8895.506, 8844.382, 8862.461, 8411.917]` | `[8641.541, 8828.720, 8715.571, 8776.913, 8347.639]` | `[15717440275, 15717439957, 15717439957, 15717439957, 15717439957]` | `[1378012, 0, 0, 0, 0]` | `0/0` | `480/480` | `0/12`
| sequential sliding | 30 | 2 | `[2181.801, 2157.718, 2170.151, 2171.206, 2162.063]` | `[2173.266, 2149.789, 2161.741, 2162.817, 2154.010]` | `[4037664522, 4037664204, 4037664204, 4037664204, 4037664204]` | `[371300, 424, 0, 0, 44]` | `0/0` | `120/120` | `0/12`
| sequential sliding | 60 | 2 | `[4197.161, 4180.270, 4170.939, 3903.804, 3890.400]` | `[4182.312, 4165.363, 4156.077, 3890.378, 3876.583]` | `[7931034487, 7931034169, 7931034169, 7931034169, 7931034169]` | `[706472, 1980, 1924, 452, 0]` | `0/0` | `240/240` | `0/12`
| sequential sliding | 120 | 2 | `[8757.501, 8913.756, 8961.089, 9272.326, 9019.149]` | `[8654.352, 8795.787, 8782.503, 9046.944, 8806.238]` | `[15717440275, 15717439957, 15717439957, 15717439957, 15717439957]` | `[1377876, 0, 0, 0, 0]` | `0/0` | `480/480` | `0/12`

All concurrent two-permit cells reported the expected `N-1` decoded and normalized hits; one-permit concurrent cells were serialized controls with zero hits; completed sequential windows had zero hits. Scheduler evidence was constant in the candidate reports: per-request output reservation `201326592` bytes, concurrent total request-local reservation `402653184` bytes, unique shared intermediate reservation `353548800/695692800/1379980800` bytes for 30/60/120 frames, combined budget `2000000000`, scheduler memory capacity `1865782272`, capture reserve `134217728`, shared-work cap `1610612736`, blocking permits `4`, generator permits `1`. The capture-headroom proxy was explicitly browser-free: `compare request/cpu/memory permits; no CDP queue claim`.

The hard 120-frame two-permit gate was **rejected**. Across five repetitions, wall max was `2339.923 ms` (pass against `2986.643`), CPU min was `3353.483 ms` (fail against `2773.898`), allocation min was `4108022377` bytes (fail against `3584253236`), and RSS max was `1411548 KiB` (pass against `1505412`). No headroom proxy failure was observed, but CPU and allocation failures are material and sufficient to reject the mechanism.

External `perf stat` was permitted; no denial occurred. For the two-permit concurrent cells (one repetition, counters include the test process): 60 frames: task-clock `1829.75 ms`, cycles `7708654750`, instructions `33547876560`, cache-misses `6291971`, branch-misses `2637608`, elapsed `1.239348437 s`, user `1.683422 s`, sys `0.140845 s`; 120 frames: task-clock `3664.85 ms`, cycles `15466945492`, instructions `65689826249`, cache-misses `13294950`, branch-misses `4142378`, elapsed `2.521010987 s`, user `3.325353 s`, sys `0.319123 s`.

Candidate fence smoke passed before rollback: `qualification_tests` 3 passed/1 ignored (corruption regeneration, session deletion publication fence, and concurrent persistence); `service_tests` 6 passed (cancellation/deadline/partial failure, limits, cache/determinism, identical-flight sharing, and adjacent sharing). This is not an exactness pass. In the five repeated 120-frame two-permit reports, four repetitions had the same evidence signature but repetition 4 swapped generated difference-map artifact IDs and changed manifest SHA-256 values (`9922a596...`/`b89da0af...` to `8af96ff0...`/`a7b57928...`) while PNG bytes stayed equal. Therefore the required cache-disabled/reuse-enabled byte equality for normalized buffers, manifests, PNGs, hashes, source IDs/order, epoch, and provenance was not established; the candidate is rejected rather than silently treating the fence tests as proof.

## Verification and rollback disposition

- Rust 1.85.0 `cargo fmt --all -- --check`: passed.
- Rust 1.85.0 locked workspace `cargo check --workspace --all-targets`: passed.
- Rust 1.85.0 locked workspace `cargo test --workspace --all-targets`: passed.
- Rust 1.85.0 locked workspace `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- Rust 1.85.0 release benchmark smoke after rollback: 30-frame concurrent two-permit ignored profile passed; the retained scaffold reported zero intermediate hits and generated six outputs.
- The optimization source introduced by `0112f10` and `1855fdb` was restored exactly to `97c4ea0`; the ignored benchmark/design scaffold from `97c4ea0` remains. No Chrome, models, network, live evidence, other features, push, or `.work/bin/work-view` change was made.

**Decision: measured rejection.** The intermediate reuse mechanism is not shipped. The child is closed as a rejected qualification checkpoint, and no speculative follow-up optimization is proposed.

If any material condition fails, record the measured reason in the parent,
remove/reject the optimization stories as appropriate, and retain the simpler
no-intermediate-cache behavior and benchmark scaffold. Do not ship a cache that
wins CPU by starving capture or retaining deleted session pixels.
