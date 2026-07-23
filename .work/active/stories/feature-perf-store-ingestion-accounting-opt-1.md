---
id: feature-perf-store-ingestion-accounting-opt-1
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

# Durability alignment: synchronous=NORMAL + WAL checkpoint policy

Optimization 1 of the parent feature. This is the highest-risk change and the
prerequisite for opt-2 (which removes the per-append checkpoint). See the parent
feature body for the full durability argument, pre-mortem, and profiling data.

## Scope

- Switch metadata index durability from WAL+`synchronous=FULL` to WAL+`synchronous=NORMAL`.
- Replace the per-append `wal_checkpoint(TRUNCATE)` (owned today by the accounting
  path) with an explicit checkpoint policy that bounds WAL growth: checkpoint every
  N appends / at every segment seal/rotation / at session flush/stop.
- Flip the open-time safety invariant to expect `synchronous == 1` (NORMAL) with
  updated rationale.

No schema change; no version bump; no cache clear.

## Files

- `crates/krometrail-store/src/index/mod.rs` — PRAGMA + invariant (Unit 1.1).
- `crates/krometrail-store/src/index/maintenance.rs` — `checkpoint_if_wal_exceeds`
  / `checkpoint_truncate` helpers (Unit 1.2), called from writer/flush/seal paths.

## Durability window (must hold)

- Process crash/kill: zero loss (NORMAL WAL replays on reopen).
- Power/OS crash: bounded tail since last checkpoint may roll back; DB stays
  consistent. Recovery re-derives every segment-backed frame row + frame timeline
  observation (`reconcile_segment` → `upsert_recovered_frame_tx` → `index_frame_tx`)
  and reconciles index to the surviving segment records. Non-segment-backed records
  (gaps/interactions/browser_events) may lose a bounded best-effort tail — accepted.
- `flush`/stop performs a `checkpoint_truncate` barrier so a clean stop is fully
  durable (preserves SPEC "Stopping a session flushes accepted frames and metadata
  before reporting completion").

## Acceptance criteria

- [ ] Crash-injection test: append N frames, drop the store without flush, reopen,
      `recover`, assert all segment-durable frames' rows + frame observations are
      present and index==segment.
- [ ] `synchronous` reads back as `1`; startup still rejects a tampered value.
- [ ] WAL file length stays bounded under a sustained no-seal append loop.
- [ ] Existing store + recovery + retention tests pass.
- [ ] Out-of-band strace on the btrfs data dir shows fsyncs/frame drop from 4.13
      toward ~0 in steady state.

## Implementation notes

- Switched the SQLite writer to WAL + `synchronous=NORMAL`, added the maintained
  2,000-mutation checkpoint policy, and kept unconditional checkpoint barriers at
  segment seal/rotation and session flush/stop. The writer/mutation gate remains
  the sole owner of checkpoints.
- Existing recovery, retention, and segment durability tests pass unchanged in
  protective intent. `strace` was attempted but is unavailable on this machine,
  so fsync counts were not asserted.
