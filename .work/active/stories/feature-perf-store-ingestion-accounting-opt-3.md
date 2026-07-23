---
id: feature-perf-store-ingestion-accounting-opt-3
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

# Set-based eviction delete with a kind='frame' partial index

Optimization 3 of the parent feature. Independent of opt-1/opt-2 and carries the
only schema change (v12 → v13). See the parent feature body for profiling data.

## Scope

- Add a partial index `timeline_frame_ref_idx ON timeline_observations(payload_sort_key) WHERE kind='frame'`
  (mirrors the existing `navigation_anchor_id_idx`/`marker_anchor_id_idx` family).
  `payload_sort_key` for a frame IS the frame-id bytes, so it covers the delete.
- Replace the per-frame `DELETE … WHERE kind='frame' AND payload_json=?` full-scan
  loop with one chunked set-based `DELETE … WHERE kind='frame' AND payload_sort_key IN (…)`.
- Bump `CURRENT_SCHEMA_VERSION` 12 → 13. Bootstrap-only does NOT suffice: an existing
  v12 store opens as `Ready` without schema writes and would lack the index. The bump
  classifies v12 as `Incompatible`, so the disposable recording cache is cleared and
  re-bootstrapped with the index — the sanctioned current-sql-schema path (no runtime
  migration; cache is disposable per Current Contract Discipline).

## Files

- `crates/krometrail-store/src/index/schema.rs` — add the index line;
  `CURRENT_SCHEMA_VERSION = 13`; update the version comment; add `timeline_frame_ref_idx`
  to `expected_indexes`; add `12` to the incompatible-version test list.
- `crates/krometrail-store/src/index/maintenance.rs` — `remove_frame_rows` (129–145):
  set-based timeline delete (chunk to `SQLITE_MAX_VARIABLE_NUMBER`).
- `crates/krometrail-store/src/index/deletion.rs` — per-segment eviction loop
  (276–290): same set-based delete. Session deletion (322) already deletes by
  `session_id` in one statement — unchanged.

## Acceptance criteria

- [ ] `evict_segment_ms` probe: ~190-frame segment reclaimed in low single-digit ms
      (from 253 ms).
- [ ] Delete `EXPLAIN QUERY PLAN` uses `timeline_frame_ref_idx` (no `SCAN`).
- [ ] Empty in-memory DB bootstraps to v13 with the new index; a v12 DB is cleared
      and re-initialised; config/profiles/diagnostics untouched.
- [ ] Existing maintenance/deletion/recovery/schema-catalog tests pass (assertions
      updated).
