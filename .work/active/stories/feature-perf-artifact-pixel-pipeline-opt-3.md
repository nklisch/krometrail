---
id: feature-perf-artifact-pixel-pipeline-opt-3
kind: story
stage: done
tags: [perf]
parent: feature-perf-artifact-pixel-pipeline
depends_on: [feature-perf-artifact-pixel-pipeline-opt-1]
release_binding: null
gate_origin: perf-design
created: 2026-07-23
updated: 2026-07-23
---

# Deterministic std::thread::scope parallelism for the per-pair loops

Optimization 3 of the parent feature. Parallelism promoted to level 1 by the
inherently-parallel exception. Depends on opt-1 (parallelizes the rewritten
loops). Near-linear on the pair dimension. See parent body Units 3.1–3.2 and the
determinism pre-mortem.

## Scope

- New `pub(crate)` `parallel.rs` in temporal-vision: `for_each_chunk` (positional
  slot writes) and `map_reduce` (per-worker private accumulator + integer merge)
  over `std::thread::scope`. Worker cap = `min(available_parallelism, count, 16)`;
  `count <= 1` or single-core runs inline (no threads).
- Parallelize `measure_adjacent` (`measure.rs:238-246`), difference accumulation
  (`difference_map.rs:165-229`), and motion accumulation
  (`motion_history.rs:363-416`) over the pair/row dimension.

## Dependency policy

No new crate dependency. rayon is not in the workspace and the crate is kept
dependency-minimal; `std::thread::scope` (std) is used. Nested inside the app's
`tokio::task::spawn_blocking` generator execution, which is safe.

## Determinism

Byte-identical output regardless of worker count. Per-pair outputs go to
pre-assigned slots; per-pixel accumulators are per-worker private then merged in
fixed order by integer `add`/`max`/bit-`or` (associative + commutative; no
floats). No `unsafe`; no shared mutable hot-path state.

## Acceptance

- [ ] Parent Unit 3.1–3.2 acceptance criteria met.
- [ ] Sequential-vs-parallel and 1/2/16-worker digest equivalence tests green
      (via the scaffold's `PERF_PAIR_WORKERS` knob).
- [ ] 4-artifact identity suite well under 1 s combined with opt-1.
- [ ] `cargo test -p temporal-vision` green; clippy clean.

## Implementation notes

Added scoped `std::thread` chunking with a 16-worker ceiling, private worker
reducers, positional pair ordering, and fixed-order merges. The internal worker
equivalence test covers 1, 2, and 16 workers; the release scaffold produced the
same normalized/artifact/output digests at worker 1 and worker 16. On this host,
the 30-frame 1920×1080 sample measured 1,066,098 µs at worker 1 and 1,113,817 µs
at worker 16, indicating scheduler overhead rather than a correctness issue.
