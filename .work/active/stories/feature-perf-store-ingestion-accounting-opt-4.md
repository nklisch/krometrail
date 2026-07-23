---
id: feature-perf-store-ingestion-accounting-opt-4
kind: story
stage: implementing
tags: [perf]
parent: feature-perf-store-ingestion-accounting
depends_on: []
release_binding: null
gate_origin: perf-design
created: 2026-07-23
updated: 2026-07-23
---

# Read decoupling: dedicated read connection pool + read-path index alignment

Optimization 4 of the parent feature. Independent of the other optimizations. See
the parent feature body for profiling data.

## Scope

- Add a read-only connection pool to `SqliteIndex` (opened `SQLITE_OPEN_READ_ONLY`,
  WAL) so interactive reads run concurrently with the single writer. Single-writer-
  effect-reducer pattern: one serialized writer, N concurrent readers.
- Route SELECT-only ports to the read pool: `frames_by_id`, `frames_in_range`,
  `frame_availability`, `frame_read_snapshots_*`, temporal range reads, browser-event
  queries, pin-state snapshot reads, and status `live_usage_snapshot`.
- The `mutations` gate + writer connection still own all writes, retention
  read-modify-write, the checkpoint policy, and eviction — it no longer stands
  between a reader and the database.
- Secondary (cheap, coherent): align range-read `ORDER BY` with `frame_range_idx`
  and split `frame_availability` combined min/max into two index seeks.

## Files

- `crates/krometrail-store/src/index/mod.rs` — `read_pool: Vec<Mutex<Connection>>`,
  `read_connection()`; route read ports.
- `crates/krometrail-store/src/index/frames.rs` — range-read `ORDER BY`
  `session_time_be, capture_ordinal_be, frame_id` (~130,168,356,451,496);
  `frame_availability` (520) split min/max.
- Optional/minor: reduce per-frame copies in `crates/krometrail-cdp/src/capture/pipeline.rs`
  (~1140) and `crates/krometrail-store/src/recording.rs` (2018) only if it falls out
  cleanly; otherwise skip.

## Acceptance criteria

- [ ] `read_one_frame_under_ingest` probe: p99 decoupled from write cadence (does not
      track append cost).
- [ ] Range-read and `frame_availability` `EXPLAIN QUERY PLAN` show index seeks, no
      temp b-tree.
- [ ] Co-monotonicity guard test confirms identical frame ordering before/after the
      `ORDER BY` change.
- [ ] Existing read/temporal/browser-event tests pass.
