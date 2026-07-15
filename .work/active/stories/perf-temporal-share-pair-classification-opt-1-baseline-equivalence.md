---
id: perf-temporal-share-pair-classification-opt-1-baseline-equivalence
kind: story
stage: implementing
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

- [ ] The locked release benchmark compiles and runs when explicitly ignored;
      it is not a default CI workload.
- [ ] Accounting matches the source formulas: default storyboard+difference is
      `2M+B` classified pixel passes; adding motion is `4M+B`; gap pairs have
      metadata but zero classifier calls.
- [ ] Reports cover 8/30/60/120 and identity/down-2. Masked and gapped runs
      prove that accounting follows the actual analysis domain and continuity
      boundaries.
- [ ] Five current-revision cold repetitions establish `B60_current` and
      `B120_current` with wall, CPU/task-clock, allocations, RSS, and available
      counter evidence.
- [ ] Repeated baseline outputs and hashes are exact; no cache hit is counted
      as a cold measurement.

## Non-goals

Do not add the candidate trace, alter classifier math, review normalization,
change service grouping, add parallelism, or modify `.work/bin/work-view`.

## Dependency and handoff

This story has no prerequisite and blocks the temporal context story. The
benchmark remains the shared before/after harness for all later stories; do not
create a second workload with different fixture or cache policy.
