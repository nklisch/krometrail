---
id: feature-perf-scout-adjudication
kind: feature
stage: drafting
tags: [perf, testing]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-20
updated: 2026-07-20
---

# Measure and adjudicate the perf-scout backlog

## Brief

Ten parked `perf-scout-*` items from a 2026-07-15 scout run, all speculative and
none measured:

- `perf-scout-batch-artifact-publication` (io)
- `perf-scout-bounded-generator-fanout` (parallelism)
- `perf-scout-bounded-parallel-decode` (parallelism, investigate-first)
- `perf-scout-characterize-png-policy` (runtime)
- `perf-scout-lazy-difference-accumulators` (memory)
- `perf-scout-opaque-row-normalization` (memory, investigate-first)
- `perf-scout-overlap-frame-cache` (caching)
- `perf-scout-profile-artifact-stages` (runtime, investigate-first)
- `perf-scout-raster-row-maps` (memory)
- `perf-scout-request-source-digests` (caching, investigate-first)
- `perf-scout-share-pair-classification` (algorithmic, investigate-first)

Six carry an explicit `investigate-first` tag. These are borrowed-pattern
hypotheses ("sparse matrices and bitmap engines", "HPC loop fusion", "database
dataloaders"), not diagnosed bottlenecks.

Implementing them blind would violate code economy and the project's own
discipline — several items say in their own text to revisit only if evaluation
shows the cost is material. The correct way to drain this cluster is to
**measure first, then adjudicate**: each item ends either as a demonstrated win
with a benchmark proving it, or closed with a recorded measurement showing it is
not material. Closing with evidence is a real terminal outcome, not a dodge.

`perf-scout-profile-artifact-stages` is the natural first unit: it is itself the
measurement infrastructure the rest depend on. Sequence it before the others and
let its numbers decide which remaining items are worth implementing.

## Simplification opportunity

The likeliest honest outcome is that most of these close unimplemented. Adding
parallelism, caching layers, and sparse representations to a local single-user
tool that is currently bounded by decode and disk would add concepts without
buying anything measurable. Resist implementing the whole list; let the profile
decide, and delete the items that the numbers do not justify.

## Acceptance

- Artifact-stage profiling exists and produces reproducible numbers on a real
  workload (the shakedown's 474-frame / 1673x1288 range is a good baseline).
- Every one of the ten items reaches a terminal state: implemented with a
  benchmark demonstrating the win, or closed with the measurement that refutes
  it.
- No speculative optimization is merged without a number attached.
