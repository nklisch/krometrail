---
id: feature-perf-store-ingestion-accounting
kind: feature
stage: review
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

## Perf Overview

The frame-append critical path does O(retained-frame) accounting and one
`wal_checkpoint(TRUNCATE)` on **every** frame, then commits the frame index under
`synchronous=FULL`. At 50 fps this makes per-frame cost grow with store size
(1.95 ms → 9.77 ms across 1k → 20k retained frames) and pushes the default btrfs
data dir under the arrival rate (47.3 fps ceiling vs 50 fps → steady-state
`IngestionQueueSaturated`). Eviction removes timeline rows one frame at a time
through an un-indexed full-table `SCAN` (253 ms/segment) while holding the store
mutation gate, and every interactive read serialises behind the same write cost.

The plan attacks four bottleneck clusters top-down on the optimization hierarchy:

1. **Durability alignment (I/O, level 2)** — `synchronous=NORMAL` + a WAL-size
   checkpoint policy that replaces the per-append checkpoint, aligning index
   durability with the segment payload durability window that already exists.
   This is the primary btrfs win and the highest-risk change; it is a prerequisite
   for the accounting rebuild (which stops checkpointing).
2. **Incremental usage accounting (algorithmic, level 1)** — a maintained
   in-memory budget total reconciled from SQL at natural barriers, plus removing
   the two `USE TEMP B-TREE FOR ORDER BY` retained-bounds sorts and the pinned-usage
   join from the append path. Flattens per-append accounting from O(retained frames)
   to O(1).
3. **Set-based eviction (algorithmic + I/O, level 1/2)** — one set-based
   `DELETE … WHERE kind='frame' AND payload_sort_key IN (…)` covered by a new
   `WHERE kind='frame'` partial index, replacing the per-frame full-table scan.
4. **Read decoupling (parallelism, level 5)** — a dedicated read-only connection
   pool under WAL so interactive reads run concurrently with the single writer;
   plus range-read `ORDER BY` alignment and a `frame_availability` min/max split
   that turn temp-b-tree sorts into index seeks.

Expected end state: flat ~2 ms append independent of store size, btrfs sustaining
50 fps with margin, eviction in low single-digit ms/segment, and interactive read
latency decoupled from write cadence.

## Profiling Summary

All figures from the feature Brief (release build, public APIs, 20k-frame
populated store; ingestion probe at 5k frames of 20 KB).

| Hot spot | Evidence | Root cause | Probe family |
|---|---|---|---|
| `append_frame` scales with store size | 1.95 ms @1k → 9.77 ms @20k retained; ~+0.40 ms / 1k frames; extrapolates ~42 ms @100k | `ensure_append_capacity`→`refresh_usage`→`refresh_index_usage` runs `wal_checkpoint(TRUNCATE)` + `usage_snapshot` (class sums + `retained_bounds` two full sorts) every append | On-CPU + I/O |
| btrfs per-frame durability | 2,864 µs ext4 (349 fps) vs **21,129 µs btrfs (47.3 fps)**; 4.13 fsyncs, 3 write txns/frame | WAL + `synchronous=FULL` fsyncs each of: checkpoint, usage upsert, frame index commit | Off-CPU / I/O (fsync) |
| Eviction | 253 ms/reclaimed segment (2.53 ms × ~190 frames) holding the mutation gate | `DELETE FROM timeline_observations WHERE kind='frame' AND payload_json=?` per frame is a full `SCAN`; no index covers `kind='frame'` | I/O + off-CPU (lock hold) |
| Interactive read contention | 4.8 ms mean / 7.3 ms p99 read-one-frame during ext4 ingest; ~21 ms+ on btrfs | Single `Mutex<Connection>` serialises reads behind per-frame write cost; WAL natively allows concurrent readers | Off-CPU / synchronization |
| Range reads (secondary) | 25.2 / 21.1 ms @20k rows | `ORDER BY capture_ordinal_be` while `frame_range_idx` leads `session_time_be` → `USE TEMP B-TREE`; eager per-row decode | On-CPU |
| `frame_availability` (secondary) | 1.41 ms | combined `min()/max()` in one query defeats index seeks | On-CPU |
| Per-frame copies (minor) | ~10–50 µs | `raw.clone()` (capture/pipeline.rs:~1140), `RawFrame::after_ack`, `frame.clone()` (recording.rs:2018) | Runtime idiom |

Negative finding (confirmed, do not touch): `observation_for_payload`
(range.rs:147) plans as SCAN but measures 0.014 ms/op — SQLite selects the
partial anchor indexes at bind time; not a bottleneck.

### Grounding confirmations from code

- `payload_sort_key` for a frame observation is exactly the frame-id bytes
  (`codec::id(value.as_uuid())`, timeline.rs:289–299), so a set-based delete keyed
  on `payload_sort_key` is sound.
- Recovery fully re-derives both the `frames` row and its `timeline_observations`
  frame observation from the durable segment record:
  `recovery::reconcile_segment` → `reconcile::upsert_recovered_frame_tx` →
  `index_frame_tx` (frames.rs:24) which inserts the row **and** calls
  `append_observation_tx` for the frame observation. This is the linchpin of the
  durability decision: anything derivable from a segment record is
  crash-reconstructable and needs no per-frame index fsync.
- The segment writer already flushes-only per append and promotes durability at
  seal/rotation (writer.rs:432–434). Per-frame index fsync is therefore a
  *stricter* guarantee than the payload it points at — an inversion recovery
  already has to reconcile away.
- `status()` (recording.rs:2298) already avoids the mutation gate and the
  checkpoint via `live_usage_snapshot`; only its `retained_bounds` sorts remain
  O(n) and are fixed by Optimization 2.

## Optimization Plan

### Optimization 1: Durability alignment — `synchronous=NORMAL` + WAL-size checkpoint policy
**Hierarchy Level**: I/O / service boundary (level 2)
**Probe Family**: Off-CPU / I/O (fsync count)
**Bottleneck**: Per-frame `wal_checkpoint(TRUNCATE)` + `synchronous=FULL` commit =
4.13 fsyncs/frame, dominating btrfs cost (21.1 ms/frame, negative headroom vs
50 fps arrival).
**Expected Metric Movement**: fsyncs/frame 4.13 → ~0 in steady state (fsync only
at checkpoint barriers and segment seals); btrfs per-frame 21.1 ms → ~2 ms;
sustains 50 fps with margin. No change on the algorithmic scaling (Opt 2 owns that).
**Why higher levels don't apply**: The work is inherently durability I/O, not
redundant computation — there is no cheaper *algorithm* for making bytes durable.
The lever is *how often* we force durability and *which layer* owns it, i.e. a
service-boundary/I/O change. It is sequenced first because Opt 2 removes the
per-append checkpoint and needs a replacement WAL-bounding owner to exist.
**Story**: `feature-perf-store-ingestion-accounting-opt-1`

#### Durability argument (the riskiest decision — treated with proportional rigor)

Chosen policy: **WAL journal mode + `synchronous=NORMAL`**, with WAL growth bounded
by an explicit checkpoint policy, and a checkpoint barrier at session flush/stop.

Crash-loss window under `NORMAL` + WAL:
- **Process crash / kill (no power loss):** SQLite `NORMAL` is fully durable — the
  WAL is on disk and replayed on next open. **Zero loss.** (`FULL` only adds
  protection against power/OS-level loss, not process crashes.)
- **Power loss / OS crash:** committed-but-unsynced WAL frames since the last
  checkpoint may roll back. SQLite recovers the WAL up to the last valid
  (checksummed) frame, so the database stays **consistent** — only the most recent
  transactions are lost, never corrupted.

Why this is safe and actually *better aligned* than the status quo:
- The segment writer already flushes-only per append and fsyncs at
  seal/rotation/flush. So the payload for a just-appended frame is **not** durable
  against power loss until its segment seals. Today's per-frame index fsync
  therefore protects a metadata row whose *payload* is not yet durable — an
  inversion. On power loss where the segment tail is lost, `recovery` already
  removes index rows not backed by a surviving segment record
  (`reconcile_segment` row-mismatch → `remove_frame_rows`). So the per-frame index
  fsync buys nothing recovery does not already provide.
- Under `NORMAL`, index and segment now share **one** power-loss window: "frames
  since the last checkpoint / segment fsync may be absent from both." Recovery
  reconciles the two to their common surviving set and **re-derives** any
  segment-backed frame row + frame timeline observation it is missing
  (`upsert_recovered_frame_tx`). This matches the documented contract exactly:
  "Metadata does not claim that a frame exists until its complete segment record
  is durable" — the status quo violated it in spirit; `NORMAL` honours it.

Pre-mortem — what could go wrong, and the mitigation:
1. **Non-segment-backed index records** (`capture_gaps`, `interactions`/operation
   evidence, `browser_events`) are index-only; recovery cannot rebuild them from a
   segment. Under `NORMAL` their power-loss tail (bounded by the checkpoint
   interval) can vanish. Accepted: these are best-effort loss/lifecycle evidence
   per SPEC, and on a power-loss tail the surrounding frames are gone too, so the
   evidence they annotate is gone with them. The checkpoint policy bounds the
   window; `flush`/stop checkpoints make a clean stop fully durable (preserves
   SPEC "Stopping a session flushes accepted frames and metadata before reporting
   completion").
2. **WAL grows unbounded** if capture runs long with no seal. Mitigation: the
   checkpoint policy (below) triggers on WAL page count regardless of appends.
3. **Open-time safety invariant** at index/mod.rs:104–111 currently *asserts*
   `synchronous == 2` (FULL) and fails startup otherwise. Must flip to expect
   `1` (NORMAL) with updated rationale; leaving it would make startup reject the
   new setting.
4. **No schema change** here — this is a PRAGMA + policy change only, so no
   version bump, no cache clear.

Checkpoint policy (bounds WAL growth; the sole checkpoint owner after Opt 2):
- Run `PRAGMA wal_checkpoint(TRUNCATE)` when the WAL exceeds a page threshold
  (default target a few MB, e.g. `KROMETRAIL`-internal constant ~2,000 pages),
  checked cheaply after mutations without a checkpoint on the common path.
- Also checkpoint at every segment seal/rotation (a durability barrier already
  exists there) and at session flush/stop.
- The checkpoint runs on the writer connection under the mutation gate; readers
  (Opt 4) are unaffected under WAL.

#### Implementation Units

##### Unit 1.1: Switch durability PRAGMA and open-time invariant
**File**: `crates/krometrail-store/src/index/mod.rs`

```rust
// open(): replace synchronous=FULL with NORMAL
connection.pragma_update(None, "synchronous", "NORMAL")?;
// verify: synchronous == 1 (NORMAL), foreign_keys == 1
if foreign_keys != 1 || synchronous != 1 { /* safety-settings error */ }
```

**Implementation Notes**:
- Update the doc/rationale comment at the invariant to state the NORMAL window and
  that recovery reconciles index from segments.

##### Unit 1.2: WAL checkpoint policy
**File**: `crates/krometrail-store/src/index/maintenance.rs` (new `checkpoint_if_wal_exceeds`), called from the writer/mutation paths.

```rust
/// Truncate the WAL when it exceeds `max_wal_pages`; O(1) size probe on the
/// common path, checkpoint only when over threshold. Returns bytes folded.
pub(crate) fn checkpoint_if_wal_exceeds(&self, max_wal_pages: u64) -> Result<()>;
/// Unconditional durability barrier for flush/stop and segment seal.
pub(crate) fn checkpoint_truncate(&self) -> Result<()>;
```

**Implementation Notes**:
- Probe WAL size with `PRAGMA wal_checkpoint`? No — read WAL frame count cheaply
  via the `-wal` file length / `pragma wal_autocheckpoint` is not used; instead
  gate on a maintained append counter (checkpoint every N appends **or** at seal),
  which avoids any per-append pragma. Simplest coherent form: checkpoint every
  `checkpoint_interval` appends and at every seal/flush.
- Remove the per-append checkpoint from the accounting path is done in Opt 2;
  this unit must land first so WAL is still bounded.

**Acceptance Criteria**:
- [ ] Crash-injection test: append N frames, simulate process kill (drop without
      flush), reopen, run `recover`, assert frame rows + frame observations for all
      *segment-durable* frames are present and index==segment.
- [ ] `synchronous` reads back as `1`; startup still rejects a tampered setting.
- [ ] WAL file length stays bounded under a sustained append loop with no seal.
- [ ] Existing store + recovery tests pass.

---

### Optimization 2: Incremental usage accounting — maintained budget total, no per-append checkpoint or bounds sort
**Hierarchy Level**: Algorithmic / data model (level 1)
**Probe Family**: On-CPU + I/O
**Bottleneck**: `refresh_usage` on every append (recording.rs:511) does a
checkpoint + `usage_snapshot`, whose `retained_bounds` runs two
`USE TEMP B-TREE FOR ORDER BY` full sorts (O(n log n)) plus a pinned-usage join
and class sums — 7.7–7.9 ms/op at 20k, ~80% of append cost, scaling with store size.
**Expected Metric Movement**: per-append accounting O(retained frames) → O(1);
append flat ~2 ms independent of store size (removes the +0.40 ms/1k slope);
eliminates the append-path checkpoint (compounds Opt 1's fsync win).
**Story**: `feature-perf-store-ingestion-accounting-opt-2` (depends_on opt-1)

**Decision — maintained aggregate vs SQL SUM:** choose a **maintained in-memory
budget total** (`UsageAccumulator`), not per-append SQL SUM. Rationale: appends
run at 50 fps for the whole session and segment count grows into the hundreds
across a 7-day retention window; an O(1) hot-path read beats an O(segments) sum,
and the checkpoint/bounds work is not needed for a budget decision at all.

**Invariant / never-drift story:** the SQL `usage` table + `segments` +
`deletion_objects` remain the sole durable truth (reconciled by `recovery`). The
in-memory total is **derived state**, never a persistence authority:
- **Startup (post-recovery):** initialise the accumulator from one full
  `usage_snapshot()`.
- **Every mutation** under the `mutations` gate applies the same byte delta to both
  the SQL rows (as today) and the accumulator.
- **Seal / reclaim / checkpoint barrier:** recompute from SQL and overwrite the
  accumulator; assert equality and log any non-zero drift (a drift is a bug
  signal, corrected toward SQL truth — fail toward truth).
- A crash can never leave it persistently drifted: it is rebuilt at startup.

The append path stops calling `refresh_usage` (full snapshot) and instead reads
`budget_total_bytes()` (accumulator + O(1) `PRAGMA page_count/freelist_count/
page_size` for the SQLite self-size class; WAL contribution is bounded by Opt 1's
policy and folded into the existing `open_overhead_limit`/slack rather than
measured per frame). `retained_bounds` and `pinned_usage` are **removed from the
append path** — they are status-only — and made O(log n) for status via Opt 2.3.

#### Implementation Units

##### Unit 2.1: `UsageAccumulator` and cheap budget total
**File**: `crates/krometrail-store/src/recording.rs` (new accumulator field on `RecordingStore`), `crates/krometrail-store/src/index/retention.rs` (helpers).

```rust
struct UsageAccumulator { total_bytes: std::sync::atomic::AtomicU64 }
// RecordingStore:
fn budget_total_bytes(&self) -> Result<u64>;      // accumulator + O(1) index page probe
fn reconcile_accumulator(&self) -> Result<()>;    // overwrite from full SQL snapshot; assert+log drift
```

**Implementation Notes**:
- Replace `refresh_usage()` calls on the append/reclaim decision paths
  (recording.rs:574, 595, 672, 679, 693, 804, 921, 1928) with `budget_total_bytes()`.
- Keep `current_status()` / `status()` on the full snapshot (now O(log n) bounds).
- Delete the per-append checkpoint: `refresh_index_usage` (maintenance.rs:35) is no
  longer on the hot path; its checkpoint responsibility now belongs solely to Opt 1's
  policy. Keep the index-page `usage` upsert only where a durable index-bytes figure
  is genuinely needed (seal/status), not per frame.

##### Unit 2.2: Reconcile hooks
**File**: `crates/krometrail-store/src/recording.rs`

**Implementation Notes**:
- Call `reconcile_accumulator()` post-recovery at store construction, after each
  reclaim batch, and at each seal/checkpoint barrier.

##### Unit 2.3: O(log n) retained bounds (status path)
**File**: `crates/krometrail-store/src/index/retention.rs` (`retained_bounds`, ~884)

```sql
-- oldest: drive from segment_created_idx, then the tied segment's min-time frame
SELECT f.session_id, f.target_id, f.session_time_be
  FROM frames f JOIN segments s USING(segment_id)
 WHERE s.created_unix_ms = (SELECT min(created_unix_ms) FROM segments)
 ORDER BY f.session_time_be ASC, f.frame_id ASC LIMIT 1;   -- newest: max()/DESC
```

**Implementation Notes**:
- Preserves the documented ordering authority (`created_unix_ms`, tie-break
  `session_time` then `frame_id`). `segment_created_idx(created_unix_ms, segment_id)`
  makes the min/max created_unix_ms an index seek; the equality filter bounds the
  join to the (usually one) tied segment; the small in-segment sort is
  ~frames-per-segment, not full-table. Eliminates both `USE TEMP B-TREE` sorts.

**Acceptance Criteria**:
- [ ] Benchmark: append latency is flat within noise across 1k/5k/20k retained
      frames (no size slope); ~2 ms/op.
- [ ] `retained_bounds` query plan shows no `USE TEMP B-TREE FOR ORDER BY`.
- [ ] Accumulator equals full SQL snapshot after every seal/reclaim in tests
      (drift == 0); status figures unchanged vs current within accepted WAL slack.
- [ ] Existing retention/budget tests pass.

---

### Optimization 3: Set-based eviction delete with `kind='frame'` partial index
**Hierarchy Level**: Algorithmic + I/O (level 1/2)
**Probe Family**: I/O + off-CPU (mutation-gate hold)
**Bottleneck**: `DELETE FROM timeline_observations WHERE kind='frame' AND
payload_json=?` per deleted frame is a full-table `SCAN` (deletion.rs:276–290;
maintenance.rs:129–145) — 253 ms/segment (2.53 ms × ~190 frames), O(frames ×
observations), held under the mutation gate stalling live capture.
**Expected Metric Movement**: eviction 253 ms/segment → low single-digit ms
(50–100×); mutation-gate hold during reclaim collapses accordingly, unblocking
interactive capture.
**Why higher levels don't apply**: this *is* the algorithmic fix — one set-based
statement over an index seek replaces an N×full-scan. No locality/idiom change
would rescue an O(N·rows) scan.
**Story**: `feature-perf-store-ingestion-accounting-opt-3`

**Decision — index coverage:** extend the existing partial-index family. `payload_sort_key`
for a frame is the frame-id bytes, so add a **non-unique partial index**:

```sql
CREATE INDEX timeline_frame_ref_idx ON timeline_observations(payload_sort_key) WHERE kind='frame';
```

This mirrors the existing `navigation_anchor_id_idx`/`marker_anchor_id_idx`
`WHERE kind=…` family and keeps the schema one-current-shape. The set-based delete:

```sql
DELETE FROM timeline_observations WHERE kind='frame' AND payload_sort_key IN (?,?,…);
```

**Decision — schema evolution (current-sql-schema pattern):** bootstrap-only does
**not** suffice. Adding the index to `CURRENT_SCHEMA_SQL` leaves existing
version-12 stores (opened as `Ready`, no schema writes) without the index. Bump
`CURRENT_SCHEMA_VERSION` 12 → 13 so a version-12 store is classified
`Incompatible` and the disposable recording cache is cleared and re-bootstrapped
with the index — exactly the sanctioned path (no runtime migration; cache is
disposable per Current Contract Discipline). Add `12` to the
`incompatible_versions_are_classified_without_mutation` test list, and add
`timeline_frame_ref_idx` to the schema catalog assertions.

#### Implementation Units

##### Unit 3.1: Partial index + version bump
**File**: `crates/krometrail-store/src/index/schema.rs`

**Implementation Notes**:
- Add the `CREATE INDEX timeline_frame_ref_idx …` line; `CURRENT_SCHEMA_VERSION = 13`;
  update the version comment; update `expected_indexes` and the incompatible-version
  test list.

##### Unit 3.2: Set-based deletes
**Files**: `crates/krometrail-store/src/index/maintenance.rs` (`remove_frame_rows`, 129–145), `crates/krometrail-store/src/index/deletion.rs` (276–296)

```rust
// Build the payload_sort_key list = codec::id(frame_id) bytes; one DELETE with
// an IN-list (chunked to SQLITE_MAX_VARIABLE_NUMBER). Frame rows already delete
// by segment_id in one statement in deletion.rs:293 — mirror that for the
// timeline rows instead of the per-frame loop.
```

**Implementation Notes**:
- `remove_frame_rows` already selects the frame ids first; delete their timeline
  rows in one chunked `IN` statement keyed on `payload_sort_key`, then the frames
  rows (already O(log n) by PK). Drop the per-id `payload_json` encode + loop.
- Session deletion (deletion.rs:322) already deletes all timeline rows by
  `session_id` in one statement — no change needed there; only the per-segment
  eviction loop changes.

**Acceptance Criteria**:
- [ ] Benchmark: eviction of a ~190-frame segment in low single-digit ms.
- [ ] Delete query plan uses `timeline_frame_ref_idx` (no `SCAN`).
- [ ] Empty in-memory DB bootstraps to v13 with the new index; a v12 DB is cleared
      and re-initialised; config/profiles/diagnostics untouched.
- [ ] Existing maintenance/deletion/recovery tests pass (adjust catalog assertions).

---

### Optimization 4: Read decoupling — dedicated read connection pool + read-path index alignment
**Hierarchy Level**: Parallelism (level 5)
**Probe Family**: Off-CPU / synchronization
**Bottleneck**: single `Mutex<Connection>` (index/mod.rs:39,136) serialises every
interactive read behind write cost — 4.8 ms mean / 7.3 ms p99 (ext4), ~21 ms+
(btrfs). WAL natively supports concurrent readers with one writer.
**Expected Metric Movement**: interactive read latency decoupled from write cadence
(p99 falls to the read's own cost, sub-ms for by-id); reads no longer queue behind
appends/eviction.
**Why higher levels don't apply**: the reads are already correct and cheap in
isolation; the loss is pure serialization against the writer. WAL provides
snapshot isolation, so ownership separation (a reader pool) is the right and
minimal fix — this is the single-writer-effect-reducer pattern: one serialized
writer, N concurrent readers.
**Story**: `feature-perf-store-ingestion-accounting-opt-4`

**Decision — which ops move, what the gate still protects:**
- **Move to the read pool** (SELECT-only ports): `frames_by_id`,
  `frames_in_range`, `frame_availability`, `frame_read_snapshots_*`, temporal
  range reads, browser-event queries, pin-state snapshot reads, and the status
  `live_usage_snapshot`.
- **Stays on the single writer connection under the `mutations` gate:** all
  mutating store ops (`append_frame`, `append_gap`, `append_operation_evidence`,
  `flush`), retention read-modify-write (`ensure_append_capacity` → append →
  reclaim as one atomic decision), the checkpoint policy, and eviction batches.
  The gate still guarantees write ordering and single-writer retention decisions;
  it no longer stands between a reader and the database.

#### Implementation Units

##### Unit 4.1: Read connection pool
**File**: `crates/krometrail-store/src/index/mod.rs`

```rust
pub struct SqliteIndex {
    connection: Mutex<Connection>,                 // writer (unchanged)
    read_pool: Vec<Mutex<Connection>>,             // read-only, SQLITE_OPEN_READ_ONLY, WAL
    // …
}
fn read_connection(&self) -> MutexGuard<'_, Connection>; // round-robin / first free
```

**Implementation Notes**:
- Open pool members read-only against the same file; size to the analysis
  concurrency config (small fixed, e.g. 2–4). Route the SELECT-only ports through
  `read_connection()`; keep writers on `connection()`.
- `query_browser_events`, `frames_*`, `frame_availability`, `usage_snapshot`
  (status) take the read path.

##### Unit 4.2: Range-read ORDER BY alignment (secondary — cheap & coherent)
**File**: `crates/krometrail-store/src/index/frames.rs` (~130, 168, 356, 451, 496)

```sql
-- was: ORDER BY f.capture_ordinal_be ASC, f.session_time_be ASC, f.frame_id ASC
-- to:  ORDER BY f.session_time_be ASC, f.capture_ordinal_be ASC, f.frame_id ASC
```

**Implementation Notes**:
- Within one (session,target) `session_time` and `capture_ordinal` are
  co-monotonic (both assigned in arrival order), so the ordering is identical while
  aligning with `frame_range_idx(session_id,target_id,session_time_be,
  capture_ordinal_be)`, removing `USE TEMP B-TREE FOR ORDER BY`. **Guard with a
  test** that asserts both orderings agree on a captured sequence before relying on
  co-monotonicity.

##### Unit 4.3: `frame_availability` min/max split (secondary)
**File**: `crates/krometrail-store/src/index/frames.rs` (520)

```sql
-- two seeks instead of combined min()/max():
SELECT session_time_be FROM frames WHERE session_id=?1 AND target_id=?2 ORDER BY session_time_be ASC  LIMIT 1;
SELECT session_time_be FROM frames WHERE session_id=?1 AND target_id=?2 ORDER BY session_time_be DESC LIMIT 1;
```

**Implementation Notes**:
- Each seek uses `frame_range_idx`; replaces the 1.41 ms combined scan.

##### Unit 4.4 (optional, minor): reduce per-frame payload copies
**Files**: `crates/krometrail-cdp/src/capture/pipeline.rs` (~1140 `raw.clone()`), `crates/krometrail-store/src/recording.rs` (2018 `frame.clone()`)

**Implementation Notes**:
- ~10–50 µs each, negligible against the 2 ms target. Include only if it falls out
  cleanly (e.g. pass owned `EncodedFrame` into `append_indexable` instead of
  clone-then-move); otherwise skip. Not a priority; do not contort ownership for it.

**Acceptance Criteria**:
- [ ] Benchmark: read-one-frame p99 during a sustained append loop is decoupled
      from write cadence (does not track the append cost).
- [ ] Range-read and `frame_availability` plans show index seeks, no temp b-tree.
- [ ] 4.2 guard test confirms identical frame ordering before/after.
- [ ] Existing read/temporal tests pass.

## Benchmarks

**Location**: `crates/krometrail-store/tests/perf_baseline.rs` (new; `#[ignore]`
probes — no bench harness exists, so these are wall-clock integration probes run
explicitly). The implementer finalises signatures against current store APIs when
landing Opt 1/2 and captures the baseline before any change.

**Run command**:
```bash
cargo test -p krometrail-store --release --test perf_baseline -- --ignored --nocapture
```
fsync counts are not measurable from inside the test; capture them out-of-band on
the ingestion probe with `strace -f -e trace=fsync,fdatasync -c` around a fixed
5k-frame append run on the btrfs data dir, before and after Opt 1.

**Probes** (each populates a store via the public `RecordingSink`, then times the
target op; report mean + p99 over a fixed iteration count, warmup discarded):

| Probe | Measures | Baseline | Target |
|---|---|---|---|
| `append_flat_vs_size` | `append_frame` mean at 1k / 5k / 20k retained | 1.95 / ~5 / 9.77 ms | flat ~2 ms, no size slope |
| `append_btrfs_steady` (data dir on btrfs) | full `append_frame` at 5k steady | 21,129 µs (47.3 fps) | ≤ ~2 ms (>50 fps margin) |
| `evict_segment_ms` | reclaim one ~190-frame segment | 253 ms | low single-digit ms |
| `read_one_frame_under_ingest` | `frames_by_id` p99 while an append loop runs | 4.8 ms mean / 7.3 ms p99 (ext4) | decoupled; ~sub-ms, not tracking append cost |
| `retained_bounds_plan` | `EXPLAIN QUERY PLAN` assertion | `USE TEMP B-TREE` ×2 | no temp b-tree |
| `frame_availability_ms` | availability query | 1.41 ms | index-seek, sub-ms |

**Counter targets** (out-of-band strace): fsyncs/frame 4.13 → ~0 steady state;
write transactions/frame 3 → 1.

## Implementation Order

1. **Opt 1 — durability alignment + checkpoint policy** (`opt-1`). Highest btrfs
   impact; prerequisite that gives Opt 2 a WAL-bounding owner. Riskiest — lands
   with crash-injection tests.
2. **Opt 2 — incremental accounting** (`opt-2`, depends_on opt-1). Removes the
   per-append checkpoint (now safe) and the O(n) bounds/pinned work; delivers the
   flat-append target.
3. **Opt 3 — set-based eviction** (`opt-3`, independent; carries the only schema
   change → v13). Can land in parallel with 1–2.
4. **Opt 4 — read decoupling + read-path index alignment** (`opt-4`, independent).
   Delivers interactive-read decoupling; secondary units fold in here.

## Risks

- **Durability (Opt 1) is the load-bearing risk.** `NORMAL` trades a bounded
  power-loss tail for the per-frame fsync. Argument above shows recovery already
  re-derives segment-backed frame rows + observations and reconciles the two
  layers to their common surviving set, so the *effective* frame-durability window
  is unchanged; only non-segment-backed records (gaps/interactions/events) lose a
  bounded, best-effort tail on power loss. **Needs host attention** if any external
  consumer depends on per-frame index durability stronger than segment payload
  durability — none is identified (agent-only tool, Current Contract Discipline),
  but confirm before merge.
- **Co-monotonicity assumption (Opt 4.2):** reordering range-read `ORDER BY`
  relies on `session_time` and `capture_ordinal` agreeing within a target. Guarded
  by a test; if it ever fails to hold, keep the current ORDER BY and instead steer
  the planner to the `UNIQUE(session_id,target_id,capture_ordinal_be)` index.
- **Schema v13 clears existing recording caches (Opt 3).** Expected and sanctioned
  (disposable cache), but it means any store carried across this release loses
  retained evidence on first open. Consistent with instance-scoped evidence
  semantics; note in the release summary.
- **Accumulator drift (Opt 2):** mitigated by SQL-truth reconciliation at every
  seal/reclaim + startup rebuild and a drift assertion; a latent delta-accounting
  bug would surface as a logged non-zero drift rather than silent budget error.
- **Benchmark noise:** btrfs figures are filesystem- and machine-specific; treat
  microbenchmarks as directional and validate the flat-append and fsync-count
  claims on the same btrfs `$HOME` data dir used for the baseline.

## Implementation notes

- Implemented Opt 1–4 in dependency order. Metadata remains WAL-backed with
  `synchronous=NORMAL`; the writer uses a 2,000-mutation checkpoint counter plus
  unconditional seal/rotation and flush/stop barriers. Segment payload durability
  was not weakened: recovery still re-derives both frame rows and frame timeline
  observations from surviving segment records.
- Added the derived `UsageAccumulator`, startup/seal/reclaim reconciliation and
  drift assertion, segment/artifact deltas, segment-first retained-bound seeks,
  the v13 `timeline_frame_ref_idx`, chunked frame-reference deletes, four
  read-only connections, aligned range reads, and split availability seeks.
- Added the ignored public-API scaffold at
  `crates/krometrail-store/tests/perf_baseline.rs`. Release probe results on this
  machine (before → final after) were:

  | Probe | Before | After |
  |---|---:|---:|
  | `append_flat_vs_size` retained=1k | 539.784 µs mean / 586.279 µs p99 | 120.586 µs / 144.21 µs |
  | retained=5k | 1.656144 ms / 1.792137 ms | 114.062 µs / 134.92 µs |
  | retained=20k | 6.652687 ms / 6.695759 ms | 123.878 µs / 143.489 µs |
  | `append_btrfs_steady` | 1.684116 ms / 1.728027 ms | 126.432 µs / 212.18 µs |
  | `read_one_frame_under_ingest` | 51.716 µs / 75.68 µs | 74.534 µs / 114.77 µs |
  | `frame_availability_ms` | 355.196 µs / 383.299 µs | 14.867 µs / 42.67 µs |

- The final query-plan probe reports `segment_created_idx`, `frame_range_idx`,
  and `timeline_frame_ref_idx` seeks with no temp sort. The range-read SQL omits
  the unreachable `frame_id` tie term so the existing index can provide that
  plan; capture ordinal is unique per session/target and the co-monotonic guard
  preserves the observable order.
- `evict_segment_ms` measured 184.84 µs before and 76.469 µs after, but the
  scaffold's default public budget does not force reclaim; these are harness smoke
  timings, not the designed ~190-frame eviction comparison. `strace` is not
  installed on this machine, so no fsync count is claimed or fabricated.
- Verification: `cargo fmt --all -- --check`; wire-enum schema check; locked
  workspace check, tests, and clippy with `-D warnings` all pass. The v12 schema
  assertions were rebased to v13 while preserving cache-reset and preservation
  behavior. No item stage fields were changed.
