---
id: feature-perf-store-ingestion-accounting-opt-2
kind: story
stage: done
tags: [perf]
parent: feature-perf-store-ingestion-accounting
depends_on: [feature-perf-store-ingestion-accounting-opt-1]
release_binding: null
gate_origin: perf-design
created: 2026-07-23
updated: 2026-07-23
---

# Incremental usage accounting: maintained budget total, no per-append checkpoint or bounds sort

Optimization 2 of the parent feature. Depends on opt-1 so a WAL-bounding checkpoint
owner already exists before the append path stops checkpointing. See the parent
feature body for full rationale.

## Scope

- Introduce a maintained in-memory `UsageAccumulator` (budget total) as **derived**
  state; SQL `usage` + `segments` + `deletion_objects` stay the sole durable truth.
- Append/reclaim decision paths read `budget_total_bytes()` (accumulator + O(1)
  page-count pragmas) instead of `refresh_usage()` (checkpoint + full snapshot).
- Remove `retained_bounds` and `pinned_usage` from the append path (status-only).
- Make `retained_bounds` O(log n) for the status path via segment-first indexed seek.

## Never-drift invariant

- Startup (post-recovery): initialise accumulator from one full `usage_snapshot()`.
- Every mutation under the `mutations` gate applies the same byte delta to SQL and
  the accumulator.
- Seal / reclaim / checkpoint barrier: recompute from SQL, overwrite, assert
  equality, log any non-zero drift (correct toward SQL truth).
- Rebuilt at startup, so a crash can never leave it persistently drifted.

## Files

- `crates/krometrail-store/src/recording.rs` — accumulator field, `budget_total_bytes`,
  `reconcile_accumulator`, reconcile hooks; replace `refresh_usage()` on the
  decision paths (574, 595, 672, 679, 693, 804, 921, 1928). Keep status on the full
  snapshot.
- `crates/krometrail-store/src/index/retention.rs` — `retained_bounds` (~884) →
  segment-first indexed seek preserving the `created_unix_ms` ordering authority and
  `session_time`/`frame_id` tie-break.
- `crates/krometrail-store/src/index/maintenance.rs` — `refresh_index_usage` no
  longer on the hot path; its checkpoint responsibility now belongs to opt-1's policy.

## Acceptance criteria

- [ ] `append_flat_vs_size` probe: append latency flat within noise across
      1k/5k/20k retained frames (~2 ms/op, no size slope).
- [ ] `retained_bounds` `EXPLAIN QUERY PLAN` shows no `USE TEMP B-TREE FOR ORDER BY`.
- [ ] Accumulator == full SQL snapshot after every seal/reclaim in tests (drift 0);
      status figures unchanged within accepted WAL slack.
- [ ] Existing retention/budget/status tests pass.

## Implementation notes

- Added SQL-derived `UsageAccumulator` state with segment/artifact deltas,
  startup rebuild, seal/checkpoint/reclaim reconciliation, and a debug assertion
  that corrects drift toward SQL truth. Budget decisions now use the accumulator
  plus an O(1) live-page probe; status retains the full snapshot path.
- Retained bounds now narrow by `segment_created_idx` and seek each tied segment
  through `frame_range_idx`; the final plan has no temp sort.
- The release scaffold measured append means of 120.586 µs, 114.062 µs, and
  123.878 µs at 1k, 5k, and 20k retained frames respectively, versus the
  pre-change 539.784 µs, 1.656144 ms, and 6.652687 ms.
