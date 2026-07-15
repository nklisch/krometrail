---
id: perf-temporal-share-pair-classification-opt-1-baseline-equivalence
kind: story
stage: done
tags: [perf, visual, testing]
parent: perf-temporal-share-pair-classification
depends_on: []
release_binding: null
gate_origin: perf-design
created: 2026-07-15
updated: 2026-07-15
---

# Establish Pair-Reuse Baseline and Equivalence Scaffold

## Purpose

Create the release-mode benchmark and instrumentation contract that makes the
adjacent-pair duplication measurable before any optimization is implemented.
This story is a measurement checkpoint only. It must not change normalization,
visual algorithms, service behavior, scheduler policy, Chrome, or model code.

## Evidence to preserve

The historical discovery run used Rust 1.85 release/locked dependencies on
Linux x86_64, Ryzen 7 7800X3D, retained 1920x1080 PNG frames, and a cold
storyboard + orientation + difference request. Its 60/120 wall references were
1,318.018/2,583.599 ms, but the normalization optimization landed afterward.
Run a five-repetition current-revision baseline before using those thresholds:
`B60_current` and `B120_current` are the authoritative values.

## Implementation units

### Unit 1: Browser-free temporal-vision benchmark

**File**: `crates/temporal-vision/tests/pair_classification_perf.rs`

Add an ignored release integration test using a deterministic opaque
1920x1080 synthetic sequence with a moving patch. Environment controls select
8/30/60/120 frames, identity/down-2 normalization, clean/masked/gapped input,
and storyboard+difference or storyboard+difference+motion consumers. Small
width/height overrides are for compile/smoke checks only.

The baseline path calls the current public generator APIs and prints one JSON
report containing:

- frame count, adjacent/measurable/gap pairs, storyboard baseline pair calls;
- `measure_adjacent` calls and direct pair pixel passes per consumer;
- expected classifier pixel-call count (`included_pixels * classified_passes`);
- wall time, process CPU/task-clock, cumulative allocation bytes, peak RSS;
- artifact and manifest digests for deterministic equality.

Use `perf stat` externally for cycles, instructions, cache misses, and branch
misses. If counters are denied, record the denial and retain the other metrics.
Run the same fixture twice and assert byte/value-identical normalized buffers,
artifacts, manifests, hashes, and accounting.

### Unit 2: Current-revision rebaseline record

Run each acceptance cell five times with cold artifact/source namespaces and
the existing production scheduler limits. Record the median or predeclared
trimmed mean and host/runtime/command details in the parent feature body. Keep
the historical numbers as context, but do not silently use them as the
post-normalization gate.

## Acceptance criteria

- [x] The locked release benchmark compiles and runs when explicitly ignored;
      it is not a default CI workload.
- [x] Accounting matches the source formulas: default storyboard+difference is
      `2M+B` classified pixel passes; adding motion is `4M+B`; gap pairs have
      metadata but zero classifier calls.
- [x] Reports cover 8/30/60/120 and identity/down-2. Masked and gapped runs
      prove that accounting follows the actual analysis domain and continuity
      boundaries.
- [x] Five current-revision cold repetitions establish `B60_current` and
      `B120_current` with wall, CPU/task-clock, allocations, RSS, and available
      counter evidence.
- [x] Repeated baseline outputs and hashes are exact; no cache hit is counted
      as a cold measurement.

## Implementation evidence

### Scaffold

Added `crates/temporal-vision/tests/pair_classification_perf.rs` and the
benchmark-only `libc` dev dependency. The ignored test builds one deterministic
opaque 1920x1080 moving-patch source sequence, normalizes it with identity or
down-2 processing, and calls the current public storyboard (including
orientation), difference-map, and optional motion-history generators. `clean`,
`masked`, and `gapped` evidence modes are all in the same fixture. Width and
height overrides are retained only for smoke/compile checks.

The test-only counting allocator, `getrusage` CPU time, and Linux RSS readings
measure only benchmark accounting. It reports wall time, process CPU, cumulative
allocation bytes, RSS/HWM, accounting, normalized/artifact/manifest/image/output
SHA-256 digests, and external `perf stat` status. `task_clock_us` remains null in
the JSON because task-clock is intentionally owned by the external counter
command rather than being approximated by process CPU time. Each measured run
is followed by an unmeasured duplicate run; normalized buffers, artifacts,
accounting, manifests, hashes, and bytes are compared directly.

The accepted cold command was run from a fresh process for each repetition:

```text
PERF_PAIR_FRAMES=60 PERF_PAIR_SCALE=down2 \\
  PERF_PAIR_EVIDENCE=clean PERF_PAIR_GENERATORS=storyboard-difference \\
  perf stat -e task-clock,cycles,instructions,cache-misses,branch-misses -- \\
  rustup run 1.85.0 cargo test -p temporal-vision --test pair_classification_perf \\
    --release --locked -- --ignored --exact baseline_pair_classification_profile --nocapture
```

The test remains `#[ignore]`; no Chrome, model, network, service, scheduler, or
artifact cache is involved.

### Current-revision cold baseline

Host/runtime: Linux x86_64, AMD Ryzen 7 7800X3D (8 cores/16 threads, 96 MiB
L3), `rustc 1.85.0 (4d91de4e4 2025-02-17)`, Cargo 1.85.0, locked release
profile. The authoritative clean storyboard+difference down-2 distributions
were five separate cold processes:

| baseline | wall ms (five runs; median) | process CPU ms (five runs; median) | normalization/generator ms (median) | allocations | peak RSS KiB (median; max) |
|---|---:|---:|---:|---:|---:|
| `B60_current` | `[1082.302, 1052.269, 1044.537, 1048.717, 1036.510]`; **1048.717** | `[1078.105, 1047.564, 1040.745, 1045.112, 1032.780]`; **1045.112** | 308.230 / 740.472 | 223,488,695 | 697,096; 697,320 |
| `B120_current` | `[2121.246, 2123.465, 2062.360, 2108.262, 2088.045]`; **2108.262** | `[2105.863, 2085.689, 2052.862, 2091.719, 2077.799]`; **2085.689** | 664.442 / 1423.600 | 410,162,461 | 1,357,328; 1,357,400 |

The five-run external `perf stat` medians (including Cargo/test startup, as
reported by the command) were B60 task-clock 2247.85 ms, cycles
9,870,639,974, instructions 42,106,490,794, cache-misses 2,909,840, and
branch-misses 2,832,025; B120 task-clock 4502.79 ms, cycles 19,418,922,641,
instructions 82,057,964,609, cache-misses 7,069,320, and branch-misses
3,348,946. All requested counters were permitted; no denial was hidden.

For clean runs, `A/M/G/P/B/Bm` are respectively `59/59/0/518400/40/40`
(B60) and `119/119/0/518400/80/80` (B120). The default path reports 158 and
318 classified passes, or 81,907,200 and 164,851,200 expected classifier pixel
calls. The normalized and output digests were:

| baseline | normalized digest | output digest | artifact digests (storyboard, orientation, difference) |
|---|---|---|---|
| B60 | `d235e074cd4ef04828ff614b2fdef65d05cc8fba720be1297a2cd091b01aab3e` | `8f7d3ad0feaf2d12d445e454b74dfe6ff8193ae75d3215810c8d9d3f4e598c5a` | `da3e56ccdaaa3d23ab0b63724686985759586f8632b8fc24a316b9c5ab9e9f5e`; `0166467c529d54a1618cbe0c75442eb0b47101326a7a1b26a2267dd8eeb12fb9`; `1b2e499d3f6b31d5e1799af2bc99e9a98b8d90df6c6a99be448539833510df83` |
| B120 | `3daa145cc263afdedcbda0bda35bd5e1678ac16232ad6cc46e0195ee1766c309` | `c43b6f16d0bfb754ef4ee4aa7691e96136fe73604f72f1e1df49d96bbe6d23cd` | `555b223404fba6c920e246e3afb79d80803d8aebfee031a3fa505fc00b467356`; `d08d0a44c50bc8fdcd3df69a219e03918fc018ce94ab8da8e91fb4a97d94d9f6`; `c68bceb7660cd221be21dc8a68ec4f9fd1d16a22a02dc8a44fee765adeaa8105` |

The corresponding first-run manifest digests are B60:
`15854ec45e900a9ac4a108893aaab08a6bc5ca308191822fd0e2c6e2daf4ecde`,
`321a865eb8a01041bb56a19bccf203c50447a7cf85b2cb049e854ada8832af8e`, and
`cba3b0a6dc6971be1a8551cddac7d284f0447570311b80438f08388f5f355fb5`; B120:
`6666c15bf91042c5fb6aa1d8ff0785027e08130e821b6d50005f072b052cd8c0`,
`2e3d49a8c5225e0a7cd04b04d5b785bfd63f566aab965f58a23cfa5ff17079ab`, and
`ca7589ab12a6d589be323796440438314b0ac59994e56745ebdee6e26b09d7af`.
Every one of the 48
required cells (4 frame counts x 2 scales x 3 evidence modes x 2 consumer
sets) also ran five separate cold processes, for 240 matrix runs. All 240
reported duplicate equality and exact normalized/output digests across their
five runs.

### Verification and disposition

- `cargo fmt --all -- --check`: passed.
- `cargo check --workspace --all-targets --locked`: passed.
- `cargo test --workspace --all-targets --locked`: passed.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.
- Ignored Rust 1.85 release smoke at 64x48 down-2: passed with direct buffer,
  artifact, manifest, hash, and accounting equality.
- No Chrome, models, normalization, production algorithms, scheduler, service,
  or `.work/bin/work-view` changes were made.

The historical 1318.018/2583.599 ms values remain context only. `B60_current`
and `B120_current` above are the authoritative post-normalization baselines;
the later optimization story owns any 20% candidate gate. No blocker remains
for the next child story.

## Non-goals

Do not add the candidate trace, alter classifier math, review normalization,
change service grouping, add parallelism, or modify `.work/bin/work-view`.

## Dependency and handoff

This story has no prerequisite and blocks the temporal context story. The
benchmark remains the shared before/after harness for all later stories; do not
create a second workload with different fixture or cache policy.
