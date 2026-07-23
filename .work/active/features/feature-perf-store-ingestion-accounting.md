---
id: feature-perf-store-ingestion-accounting
kind: feature
stage: drafting
tags: [perf]
parent: null
depends_on: []
release_binding: null
gate_origin: perf-design
created: 2026-07-23
updated: 2026-07-23
---

# Store ingestion and retention accounting performance

## Brief

Profiling (release build, public APIs, 20k-frame populated store; separate
ingestion probe at 5k frames of 20 KB) found the frame write path and its
retention accounting dominate store cost, cap capture throughput below the
observed ~50 fps screencast cadence on btrfs (the default `$HOME` data-dir
filesystem on this machine), and inflate interactive read latency during
capture. Session-lifetime cost is quadratic in retained frames.

Measured evidence:

- `append_frame` grows linearly with retained frame count: 1.95 ms/frame at
  1k retained → 9.77 ms/frame at 20k (~+0.40 ms per 1,000 retained frames);
  extrapolates to ~42 ms/append (~24 fps ceiling) at 100k frames. Root:
  `ensure_append_capacity` (recording.rs:568) runs on every append →
  `refresh_usage` → `refresh_index_usage` (index/maintenance.rs:35) issues
  `PRAGMA wal_checkpoint(TRUNCATE)` per append, then `usage_snapshot`
  (index/retention.rs:777) does full Rust-side decode-and-sum scans of
  `usage`, `deletion_objects`, `segments`, and pinned segments, and
  `retained_bounds` (retention.rs:884) runs two `SCAN f … USE TEMP B-TREE
  FOR ORDER BY` full-table sorts. Accounting alone is 7.7–7.9 ms/op at 20k
  frames (~80% of append cost).
- Filesystem-dependent per-frame cost (5k-frame steady state): full
  `append_frame` 2,864 µs on ext4 (349 fps ceiling) but **21,129 µs on
  btrfs (47.3 fps ceiling — negative headroom vs 50 fps arrival, so
  `IngestionQueueSaturated` gaps are the steady-state outcome)**. Per frame:
  4.13 fsyncs, 3 SQLite write transactions (checkpoint + usage upsert +
  frame index insert) under WAL + `synchronous=FULL` (index/mod.rs:90-107).
  The segment layer correctly defers durability to seal/rotation (0 segment
  fsyncs per append; writer.rs:432-434 documents the promotion contract),
  so per-frame durable index commits are a stricter guarantee than the
  payload beneath them; recovery already reconciles the index from segments.
- Eviction removes timeline rows one frame at a time: per deleted frame,
  `DELETE FROM timeline_observations WHERE kind='frame' AND payload_json=?`
  (deletion.rs:276-290; same pattern maintenance.rs:129-145) is a full table
  `SCAN` — no index covers `kind='frame'` (partial indexes cover only the
  other kinds, and index `payload_sort_key`, not `payload_json`). Measured
  253 ms per reclaimed segment (2.53 ms × ~190 frames) while holding the
  store mutation lock, stalling live capture; O(frames_per_segment ×
  total_observations). Note `payload_sort_key` for frame observations IS the
  frame-id bytes (timeline.rs:289), enabling a set-based delete.
- Contention: the global `mutations` mutex (recording.rs:97,2013) and single
  `Mutex<Connection>` (index/mod.rs:39,136) serialize every interactive read
  behind per-frame write cost: 4.8 ms mean (p99 7.3 ms) to read one frame by
  id during ext4 ingestion; ~21 ms+ queueing on btrfs. WAL natively supports
  concurrent readers.
- Secondary (worthwhile only after the above): full-window range reads pay
  `USE TEMP B-TREE FOR ORDER BY` (frames ordered by `capture_ordinal_be`
  while `frame_range_idx` leads with `session_time_be`; timeline `CASE` in
  ORDER BY) plus eager per-row decode — 25.2 ms / 21.1 ms at 20k rows.
  `frame_availability` combined min()/max() defeats index seeks (1.41 ms).
  Minor: three per-frame payload copies (~10–50 µs) in
  capture/pipeline.rs:~1140 (`raw.clone()`), `RawFrame::after_ack`, and
  recording.rs:2018 (`frame.clone()`).
- Negative finding (do not "fix"): `observation_for_payload` (range.rs:147)
  plans as SCAN with a dummy bind but measures 0.014 ms/op — SQLite picks
  the partial anchor indexes at bind time; anchor lookup is not a bottleneck.
- Under budget pressure, appends stall up to 36.8 ms max (ext4) — above the
  19 ms frame cadence.

Proposed hierarchy levels: level 1 (incremental accounting aggregates;
indexed/maintained retained bounds), level 2 (checkpoint policy, group-commit
or `synchronous=NORMAL` durability alignment, set-based eviction delete with
`(kind, payload_sort_key)` index coverage), level 5 (read-connection
separation, narrower mutation gate). Probe families: I/O + off-CPU/locks.
Expected: flat ~1.5–2 ms appends independent of store size; 50–100× eviction
metadata removal; interactive reads decoupled from write cadence; btrfs
sustains 50 fps with margin.

Durability note for design: relaxing per-frame index durability (batching or
`synchronous=NORMAL`) must be argued against the existing recovery contract
(recovery reconciles index from segments; segment durability promotes at
seal/rotation/flush) — the design must state the crash-loss window and show
recovery covers it. Do not weaken segment payload durability.
