---
id: epic-durable-browser-memory-recovery
kind: feature
stage: done
tags: [storage, browser]
parent: epic-durable-browser-memory
depends_on:
  - epic-durable-browser-memory-segment-format
  - epic-durable-browser-memory-sqlite-index
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Crash Recovery, Open-Segment Sealing, and Index Reconciliation

## Brief

Own the startup-time consistency capability of the recording store: after a crash or unclean shutdown, locate unsealed segments, scan complete frame records, truncate incomplete trailing data, seal recoverable segments, and reconcile the SQLite frame index and usage accounting against what the segment files actually contain. The invariant this feature enforces is the SPEC's "metadata does not claim that a frame exists until its complete segment record is durable" — after recovery, every frame the index claims exists is backed by a complete, durable segment record.

Recovery runs once at store open, before retention or capture begin. It treats the SQLite index as reconcilable metadata and the segment files as the byte-level authority for what was actually persisted. Pins live in SQLite (WAL-durable) and are trusted across recovery; recovery does not reconstruct them. It reports the open-segment evidence required by the evaluation.

This feature does not own the segment byte format, the SQLite schema, runtime eviction, or range resolution. It is the startup consistency pass that makes the store safe to use after a crash.

## Epic context

- Parent epic: `epic-durable-browser-memory`
- Position in epic: consumer of the segment-format and SQLite-index features; runs at store open, before retention. Independent of the retention and range-resolution features at the dependency-graph level.
- Design decisions inherited: recovery-before-retention startup ordering; the recoverable-record layout (length-prefix plus checksum) is owned by the segment-format feature and consumed here; pins are trusted across recovery because they are SQLite metadata, not frame payloads; the index is the reconcilable metadata authority and the segment files are the byte-level authority.

## Simplification opportunity

- Reuse the same primitive helpers (`remove_frame_rows`, `remove_segment`, `update_usage`, `remove_usage`) supplied by the SQLite-index feature for dangling-row and stale-usage removal. Recovery's reconciliation composes those primitives with a different predicate rather than duplicating index-mutation logic.
- Trust the segment file's per-record checksum and length-prefix as the recovery authority. Do not maintain a parallel recovery journal; the sealed-footer + per-record CRC format is already a recoverable record format by design, and idempotent operations make a crash during recovery self-healing.
- The open-segment evidence (how many open segments were found and sealed) is a reported measurement, not a hard failure: at most one open segment per active target may exist while recording, and recovery reports what it observed rather than refusing to open.

## Foundation references

- `docs/VISION.md` — Local-First Operation
- `docs/SPEC.md` — Disk Budget and Retention (stopping a session flushes accepted frames and metadata before reporting completion), Errors and Degraded Operation (an unrecoverable browser connection ends the session after flushing accepted data)
- `docs/ARCHITECTURE.md` — Recording Store (Crash Recovery), Frame Ingestion (ack-then-handoff ordering), Failure Isolation (process shutdown waits for bounded flushing and then reports incomplete work)
- `docs/EVALUATION.md` — Storage and Retention Evaluation (crash recovery restores complete records and removes incomplete trailing writes; deletion removes all data belonging to the selected session; disk accounting tolerates at most one open segment beyond the configured budget while recording and reports the bound)

## Scope and honest non-goals

**In scope:**

- Store-open recovery routine: locate every unsealed segment in the data directory, scan its complete frame records, truncate incomplete trailing bytes, seal the segment with a sealed footer.
- Frame-index reconciliation: insert index rows for durable frames missing from the index; remove index rows whose backing segment record is incomplete or absent; preserve the `(segment_id, byte_offset)` addressing contract from the segment-format feature.
- Usage-accounting reconciliation: recompute `segment`-class usage from the reconciled segments and index state so retention's status surface and eviction decisions start from a correct number.
- Pin preservation: pins in SQLite are trusted across recovery; no pin reconstruction pass.
- The open-segment evidence measurement and its status-surface report.
- The write-order guarantee and its test: a frame's segment record is durable before its index row is committed. A crash between the two leaves an **orphan payload** — a complete segment record with no index row — which recovery repairs by **inserting** the missing row. The opposite inconsistency — a **dangling index row** whose backing record is missing or corrupt — arises from missing or corrupt segment files (e.g. a truncated open tail, external file deletion, or bit rot) and is repaired by **removing** the row. Recovery owns the cross-layer crash-mid-write integration test that proves both directions.

**Non-goals:**

- The segment byte format and writer — owned by `epic-durable-browser-memory-segment-format`. Recovery consumes the format; it does not define it.
- The SQLite schema, migrations, and removal helpers — owned by `epic-durable-browser-memory-sqlite-index`. Recovery calls those helpers.
- Runtime eviction, paused-budget state, budget-tolerance computation, and session deletion — owned by `epic-durable-browser-memory-retention`. Recovery produces a consistent starting state; retention operates on it afterward and interprets the open-segment evidence against the configured budget.
- Range resolution — owned by `epic-durable-browser-memory-range-resolution`.
- Reconstructing pins, interaction records, or browser events not already in SQLite. WAL-durable metadata is the authority; recovery does not second-guess it.

## Notes for the design pass

- The recoverable-record layout (length-prefix + checksum per record, sealed footer) is the contract this feature depends on. The segment-format feature's `scan_complete_records` already returns absolute file offsets and a `Trailing::{Clean, Incomplete{at}, Corrupt{at}}` classification by reading only the length+checksum prefix — exactly what recovery needs to find the last complete record without parsing payload contents.
- The write-order test (segment-record durable before index commit) is co-owned with the segment-format feature. The two features agreed that recovery owns the single integration test exercising a real crash-mid-write aftermath; this feature's `## Handoff to downstream features` records that.
- Recovery must be idempotent: running it twice on an already-recovered store is a no-op (all segments already sealed, index already reconciled). Idempotence plus the `.open`/`.kts` filename distinction plus per-segment SQLite transactions give crash-during-recovery safety without a recovery journal.
- Map recovery failures to the existing `ErrorCode::PersistenceFailed` (operational failures: IO, SQL) or `ErrorCode::ShutdownIncomplete` (recovery cannot make the store usable: the segments directory is unreadable or the index is inconsistent after migration). Quarantining an isolated corrupt segment is NOT a shutdown — recovery isolates the damage, removes the dangling rows, and continues.

## Execution policy and grounding

- **Driver:** direct design under an active agile-workflow autopilot `--all` goal; no user questions, no subagents, no peeragent, no push. All probes were local (`read`, `grep`, `find`, `work-view`).
- **Effective worker capability:** highest/raised. Recovery owns the cross-resource crash invariant (asymmetric failure direction), idempotence under crash-during-recovery, the seal-and-reconcile boundary that touches both the segment byte format and the SQLite index, and the cross-layer fault-injection evidence. Getting the asymmetric direction or idempotence wrong loses data silently and lies about what the store contains — the highest-consequence correctness surface in the epic.
- **Effective review weight:** `standard` (project default). No design-time advisory review runs (the caller prohibited subdelegation); feature-level implementation review remains required after implementation.
- **Dispatch rationale:** direct reads covered the feature brief, the parent epic and its decomposition-risk notes, all five foundation docs, `.agents/rules/agile-workflow.md`, the principles skill, the four sibling feature briefs (segment-format done body + review remediation, sqlite-index done body, retention drafting brief, range-resolution drafting brief), and the implemented store surface: `crates/krometrail-store/src/{lib,recording}.rs`, `segments/{mod,writer,scanner,header,footer,record}.rs`, `index/{mod,segments,frames,maintenance,migrations,codec,timeline,schema_v1}.rs`, the composition root (`src/app.rs`), the core address and error contracts (`crates/krometrail-core/src/recording/address.rs`, `error.rs`), and the queue state via `work-view --scope all` (confirming both dependencies are `done` and that retention + range-resolution are the parallel in-flight write sets to preserve).
- **UI surface:** none. This is a local startup consistency pass.
- **Rolling Foundation:** additive only, with one in-place correction to this item's own brief wording (the previous "orphan index row" phrasing inverted the asymmetric failure direction). No standing assertion in VISION/SPEC/ARCHITECTURE/EVALUATION is contradicted. The design concretizes `docs/ARCHITECTURE.md § Crash Recovery` (the five-step startup pass) and `docs/EVALUATION.md § Storage and Retention Evaluation` (crash recovery restores complete records and removes incomplete trailing writes); it does not change what those documents claim.

## Design decisions

### 1. Recovery lives in `krometrail-store::recovery`; recovery-specific SQL lives in `krometrail-store::index::reconcile`

`ARCHITECTURE.md` places recovery as a sibling to `segments`, `index`, `retention`, and `artifacts`. Recovery spans two stores (it scans segment files and mutates the SQLite index), so it is a top-level module that composes both:

```rust
// crates/krometrail-store/src/recovery.rs (new)

/// Counts from one recovery pass, returned to the composition root for logging.
/// All fields are zero on a no-op second run (idempotence proof).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RecoveryReport {
    pub open_segments_sealed: u64,    // `.open` files discovered and sealed
    pub segments_repaired: u64,       // sealed segments whose tail/footer was repaired in place
    pub segments_quarantined: u64,    // header-corrupt segments renamed to `.corrupt`
    pub segments_removed: u64,        // index-referenced segments whose file was absent
    pub bytes_truncated: u64,         // torn/corrupt trailing bytes removed before sealing
    pub frames_recovered: u64,        // orphan payloads → missing index rows inserted
    pub frames_removed: u64,          // dangling index rows removed
    pub usage_rows_reconciled: u64,   // segment-class usage rows upserted or removed
}

/// Runs once at store open, before retention or capture begins. Treats the
/// SQLite index as reconcilable metadata and the segment files as the
/// byte-level authority. Idempotent: a second call is a no-op.
pub fn recover(index: &SqliteIndex) -> krometrail_core::Result<RecoveryReport>;
```

`recover` reads the segments directory from `index.segments_directory()` (already `pub(crate)`). The module is declared in `lib.rs` (`pub mod recovery;`) and `RecoveryReport` / `recover` are re-exported.

Recovery-specific SQL — enumerating segment registrations, listing indexed offsets for one segment, idempotent frame-row insertion, and listing segment-class usage keys — lives in a new sibling `index/reconcile.rs` so SQL stays inside the `index` module (matching the sqlite-index feature's "SQL private to `krometrail-store::index`" boundary). It is declared with one additive line in `index/mod.rs`:

```rust
// crates/krometrail-store/src/index/mod.rs (one line added)
pub(crate) mod reconcile;
```

That is the only change to an existing index file. All other index access reuses already-`pub(crate)` helpers: `SqliteIndex::connection`, `index::segments::register_segment_tx`, `index::frames::index_frame_tx`, and `index::maintenance::{remove_frame_rows, remove_segment, update_usage, remove_usage, UsageEntry, UsageClass}`. No maintenance API is widened and no helper signature changes — retention and range-resolution's parallel work is untouched.

### 2. The asymmetric failure direction is the load-bearing invariant

The write order at runtime is **segment append (returns `FrameAddress`) → SQLite index commit**. A crash can fall in three places:

| Crash point | Segment file | SQLite index | Direction | Recovery action |
|---|---|---|---|---|
| After append, before commit | complete record present | row absent | **orphan payload** | **insert** missing row |
| After commit, before per-frame `sync_data` of an unsealed tail (power loss) | record absent/truncated | row present | **dangling row** | **remove** row |
| External deletion / bit rot / unsynced directory entry | file absent/corrupt | row present | **dangling row** | **remove** row (and segment if no records survive) |

The previous brief wording ("a crash between the two always leaves an orphan index row recovered by removal") inverted the common case. The common process-crash case is an **orphan payload** — the segment record was flushed to the OS page cache (durable against process crash) before the address returned, but the index transaction never committed. Recovery repairs it by **inserting** the missing row from the decoded record. Dangling index rows — the opposite — arise when the segment file is missing or corrupt relative to the index, and recovery **removes** them.

Recovery therefore owns **both** SQL directions and the integration test that proves each. This is the unit with the highest silent-failure risk: inserting when it should remove (or vice-versa) either fabricates frames that are not durable or drops durable frames the index already promised.

### 3. Discovery, sealing, reconciliation, usage — four phases, idempotent

```text
Phase A — Discover
  read_dir(segments_directory)
  classify each entry by extension:
    .open   → open segment candidates (validate filename stem parses as the segment UUID)
    .kts    → sealed segment candidates
    .corrupt → already quarantined; skip
    other/none → not a segment (e.g. residual write probes); skip

Phase B — Seal open segments (filesystem only; no SQLite)
  for each .open file:
    read bytes
    scan_complete_records(bytes)
      Err  → header unreadable → rename to <id>.corrupt, sync dir; remember id for Phase C removal
      Ok   → determine truncate point from Trailing
             truncate file to truncate point
             append fresh SealedFooter (counts + first/last session time from decoded
                 first/last complete records, or header defaults for a 0-record segment)
             flush + sync_data
             rename .open → .kts
             sync directory
  (a 0-record open segment — only a header — is sealed as record_count=0 and registered;
   retention evicts it first. It is not special-cased away.)

Phase C — Reconcile (filesystem reads + short SQLite transactions; no txn spans file I/O)
  segment set = (.kts files on disk) ∪ (segment ids referenced by the index)
  for each segment id in the union:
    if file exists:
      read bytes; scan
        header Err  → rename to .corrupt, sync dir; remove all index rows + registration
        Ok         → if Trailing != Clean: truncate at Trailing.at, append fresh footer,
                     sync_data, sync dir  (sealed-segment repair in place)
                     valid_offsets  = { scan.records[*].byte_offset }
                     indexed_offsets = SELECT (frame_id, byte_offset_be) FROM frames
                                       WHERE segment_id = ?   (one read, no txn)
                     dangling = indexed_offsets − valid_offsets
                     missing  = valid_offsets − indexed_offsets
                     remove dangling (policy in §4)
                     decode each missing record (read_frame_at) OUTSIDE any txn
                     one Immediate transaction:
                       register_segment_tx(reconciled Sealed registration)   // FK target
                       for each missing frame: upsert_recovered_frame_tx(...)  // SELECT guard
                       commit
    else (index references a segment with no file):
      remove_frame_rows(segment_id, None)   // all rows, own txn
      remove_segment(segment_id)            // cascade clears pin_segments
      remove_usage(Segment, segment_id key)

Phase D — Usage reconciliation (segment class only)
  live_segments = SELECT segment_id, file_bytes_be, session_id FROM segments
  usage_keys    = SELECT object_key FROM usage WHERE class = 'segment'
  for each live segment: update_usage(Segment, segment_id key, file_bytes)   // own txn each
  for each usage key not in live set: remove_usage(Segment, key)
```

### 4. Dangling-row removal policy

After computing `valid_offsets` and `indexed_offsets` for a segment:

- **Damaged tail (Trailing was Incomplete/Corrupt and was repaired):** every dangling offset is necessarily `>=` the repair truncate point (the scan stopped there). Recovery removes them with one call to the existing `remove_frame_rows(segment_id, Some(truncate_point))`. This is safe because everything at or above the truncate point was discarded.
- **Clean, properly-footed segment with stray dangling offsets** (an anomaly indicating index/segment disagreement that the scan did not attribute to a torn tail): recovery treats the segment file as the byte-level authority and rebuilds that segment's frame index from scratch — `remove_frame_rows(segment_id, None)` followed by re-inserting every complete record via the same idempotent path. Conservative; fires only on the anomalous case.
- **Whole-segment removal** (file missing or fatally corrupt): `remove_frame_rows(segment_id, None)` then `remove_segment(segment_id)`.

`remove_frame_rows(segment_id, None)` removes paired generic frame timeline observations in the same transaction (already implemented and tested by the sqlite-index feature), so timeline and frame rows stay paired. No new maintenance primitive is needed.

### 5. Missing-row insertion is idempotent via a SELECT guard over `index_frame_tx`

For each `valid_offset` not in `indexed_offsets`, recovery decodes the record (`read_frame_at(&bytes, address)`) into an `EncodedFrame`, then inside the per-segment transaction calls:

```rust
// crates/krometrail-store/src/index/reconcile.rs (new)
pub(crate) fn upsert_recovered_frame_tx(
    transaction: &rusqlite::Transaction<'_>,
    frame: &EncodedFrame,
    commit: &FrameWriteCommit,
) -> krometrail_core::Result<bool>;  // true iff a row was inserted
```

`upsert_recovered_frame_tx` runs `SELECT 1 FROM frames WHERE frame_id = ?` first; if present it returns `Ok(false)` (idempotent no-op), otherwise it calls the existing `index_frame_tx(transaction, frame, commit)` (which registers the segment, inserts the frame row, and appends the `ObservationKind::Frame` timeline observation) and returns `Ok(true)`. The `commit.active_segment` is the reconciled sealed `SegmentRegistration` and `commit.sealed_segment` is `None` — recovery registered the segment itself, so `index_frame_tx`'s internal upsert is a no-op repeat of the same row.

The hot path's raw `INSERT` is preserved unchanged (it must fail loudly on a real duplicate-ordinal bug); only recovery's path is guarded. No change to `index/frames.rs`.

### 6. Crash-during-recovery safety without a journal

Every recovery operation is independently idempotent, so a crash at any point leaves a state the next run completes:

- **Crash during Phase B (after sealing some `.open` files, before Phase C):** those segments are now `.kts`; the next run's discovery sees them as sealed and Phase C reconciles them (inserts their missing rows).
- **Crash during Phase C (after registering/inserting some segments, before others):** already-reconciled segments re-scan to `missing = ∅`, `dangling = ∅` → no-op; not-yet-reconciled segments reconcile now.
- **Crash mid-transaction:** SQLite `synchronous=FULL` + Immediate transactions mean a committed transaction is durable; an uncommitted one rolled back. The next run redoes it idempotently.

No recovery journal, no two-phase marker, no `PROGRESS.md`. The `.open`/`.kts` filename distinction is the durable seal marker; idempotent SQL is the reconcile marker.

### 7. Transactions and file ordering

Filesystem mutations (truncate, append footer, `sync_data`, rename, sync directory) happen **outside** any SQLite transaction. Per reconciled segment, recovery:

1. reads the file and scans (in-memory, no SQL);
2. decodes any missing records (filesystem-derived `&[u8]`, no SQL);
3. reads indexed offsets (one short connection use, no transaction needed — recovery is the sole writer at startup);
4. removes dangling rows via `remove_frame_rows` (its own Immediate transaction);
5. opens **one** Immediate transaction to upsert the sealed registration and insert all missing frame rows, then commits.

No SQLite transaction spans a filesystem read or write, matching the sqlite-index feature's lock-ordering rule (file reads release the SQLite mutex) and avoiding any re-entrant lock on the single connection.

### 8. Quarantine for fatal corruption; `PersistenceFailed` vs `ShutdownIncomplete`

A segment whose **header** CRC/magic/version fails (`scan_complete_records` returns `Err`, or `SegmentHeader::decode` fails) cannot be trusted at any record boundary. Recovery renames it `<id>.corrupt` in the segments directory, syncs the directory, and removes all of its index rows + registration + usage row. The `.corrupt` extension is skipped by future discovery. The store remains usable; the corrupt segment's data is honestly gone and the index no longer claims it.

Error mapping at the boundary:

- **`PersistenceFailed`** — an operational failure during recovery (a file read/write error, a rename failure, a SQL error). Propagated; the store does not open; the operator retries. The next recovery attempt is idempotent and completes.
- **`ShutdownIncomplete`** — recovery cannot make the store usable at all: the segments directory cannot be enumerated, or the index is missing required tables after migration (the migration step already catches the latter; this is the belt-and-braces path). Reserved for "give up on a corrupted store."
- **Quarantine is neither.** Isolating one corrupt segment and continuing returns `Ok(report)` with `segments_quarantined > 0`. Only the operator-facing report exposes the loss; the store opens.

### 9. Pins are trusted; usage reconciliation is segment-class only

Recovery does not read or write the `pins` or `pin_segments` tables. When recovery removes a segment (quarantine or missing file), `remove_segment`'s `ON DELETE CASCADE` clears that segment's `pin_segments` rows automatically; the `pins` row itself survives and simply protects nothing until retention reports the gap. WAL-durable metadata is the authority.

Usage reconciliation covers only the `segment` class — the class recovery has direct authority over (it just reconciled which segments survive and their file sizes). `index`, `browser_event`, and `artifact` usage are retention's authority (and the browser-event/artifact owning features); retention recomputes them on its first status query. Recovery's `update_usage(Segment, ...)` per surviving segment and `remove_usage(Segment, ...)` per stale key bring the segment usage table to ground truth so retention starts from a correct number.

### 10. Open-segment-beyond-budget tolerance: recovery reports evidence, retention interprets

The evaluation tolerates at most one open segment per active target beyond the configured budget while recording. Recovery is a startup pass — after it runs, **zero** open segments remain (all were sealed). The budget-tolerance comparison (count and size versus the configured budget) is retention's, because retention owns the budget number. Recovery's contribution is the raw evidence in `RecoveryReport.open_segments_sealed` (how many open segments existed at startup) plus the reconciled segment usage rows retention reads. This honors the parent epic's mitigation ("the bound is a reported measurement owned by retention, consumed unchanged by recovery's status report") without overclaiming a budget interpretation recovery cannot make.

### 11. Composition-root wiring

`recover` runs in `src/app.rs::open_storage` after `SqliteIndex::open` and `SegmentWriter::open` and before `IndexedRecordingSink::new`:

```rust
fn open_storage(data_directory: &Path) -> Result<StorageDependencies> {
    let segments_directory = data_directory.join("segments");
    let index = Arc::new(SqliteIndex::open(IndexStoreConfig { /* ... */ })?);
    let segments = Arc::new(SegmentWriter::open(SegmentStoreConfig { /* ... */ })?);
    let report = krometrail_store::recovery::recover(index.as_ref())?;
    tracing::info!(
        open_segments_sealed = report.open_segments_sealed,
        segments_repaired = report.segments_repaired,
        segments_quarantined = report.segments_quarantined,
        frames_recovered = report.frames_recovered,
        frames_removed = report.frames_removed,
        "recording store recovery complete"
    );
    Ok(StorageDependencies {
        recording: Arc::new(IndexedRecordingSink::new(segments, Arc::clone(&index))),
        /* timeline, catalog, gaps, frames unchanged */
    })
}
```

Recovery operates directly on the segments directory and the index. The just-opened `SegmentWriter` worker is idle (no appends yet) and holds no open files, so there is no file-handle conflict; capture-path appends after recovery create fresh `.open` files.

## Architectural choice: where recovery composes the two stores

Three options were weighed.

### Option A — top-level `recovery` module composing `index` + `segments` (chosen)

Recovery is a sibling module that reads segment files via the `segments` codec/scanner/path helpers and mutates the index via `pub(crate)` helpers, with its own SQL isolated in `index::reconcile`. Matches `ARCHITECTURE.md`'s module layout, keeps SQL inside `index`, and avoids widening any existing maintenance API.

### Option B — recovery as a method on `SqliteIndex`

Collapses recovery into the index adapter. Rejected: recovery's primary work is filesystem scanning/truncation/sealing of segment files, which is not the index adapter's concern, and bundling it there would conflate the byte-level authority (segments) with the metadata authority (index) inside one type.

### Option C — recovery inside `SegmentWriter`

Rejected for the mirror reason: recovery mutates the SQLite index heavily; the segment writer owns only frame-byte writes. Putting index reconciliation inside the writer inverts the dependency direction the architecture settled (segments does not depend on index).

**Choice:** Option A. It is the smallest reversible architecture that preserves the segment/index separation, isolates recovery-specific SQL, and touches no existing maintenance API surface.

## Trickiest unit

The **seal-and-reconcile boundary** is the unit with the most novel risk. Recovery must, after an arbitrary torn or corrupt tail, produce (a) a sealed segment whose footer validates, (b) an index whose rows exactly match the surviving complete records, and (c) do both idempotently so a crash during recovery itself is recoverable without a journal. The two reconciliation directions require opposite SQL operations on the same `(segment_id, byte_offset)` rows — insert for orphan payloads, remove for dangling rows — and misclassifying a segment as one when it is the other either fabricates non-durable frames or silently drops durable ones. The fault-injection story exists to force every realistic aftermath (orphan payload, truncated tail, missing file, corrupt header, crash-during-recovery, idempotent re-run) through the real segment writer + real index + real filesystem before the boundary is trusted.

## Implementation units

Two child stories, linear dependency chain.

### Unit 1: Recovery engine, reconcile helpers, and composition wiring

**Story:** `epic-durable-browser-memory-recovery-engine`

**Depends on:** `[]` (within-feature; the feature's own `depends_on` already gates on segment-format + sqlite-index, both `done`).

**Files:**
- `crates/krometrail-store/src/recovery.rs` (new) — `RecoveryReport`, `recover`, the four-phase orchestrator, private seal/repair/classify helpers, quarantine, the `QUARANTINED_SEGMENT_EXTENSION = "corrupt"` constant, and in-module unit tests for the pure classify/seal-decision logic.
- `crates/krometrail-store/src/index/reconcile.rs` (new) — `StoredSegment`, `IndexedFrame`, `list_segments_tx`, `indexed_offsets_tx`, `upsert_recovered_frame_tx`, `list_segment_usage_keys_tx`.
- `crates/krometrail-store/src/index/mod.rs` (one additive line) — `pub(crate) mod reconcile;`.
- `crates/krometrail-store/src/lib.rs` (extend) — `pub mod recovery;` and `pub use recovery::{RecoveryReport, recover};`.
- `src/app.rs` (extend) — call `recover(index.as_ref())?` in `open_storage` after both stores open and before `IndexedRecordingSink::new`; log the report via `tracing::info!`.

**Acceptance criteria:**
- [ ] `recover(&SqliteIndex) -> Result<RecoveryReport>` runs the four phases; a second call on an already-recovered store returns a report with every field zero and mutates nothing (idempotence proof).
- [ ] An open segment with a complete record followed by a torn tail is truncated at the torn record's offset, sealed with a footer whose `record_count`/`total_payload`/first-last session times match the surviving records, renamed `.open`→`.kts`, and directory-synced.
- [ ] An orphan payload (complete segment record, no index row) is recovered by inserting the missing frame row + timeline observation; a dangling index row (record absent/corrupt) is removed. Both directions verified.
- [ ] A header-corrupt segment is renamed to `.corrupt`, its index rows + registration + usage removed, and `recover` still returns `Ok` with `segments_quarantined == 1`.
- [ ] An index-referenced segment whose `.kts` file is absent has its rows + registration removed (`remove_frame_rows(.., None)` then `remove_segment`).
- [ ] Segment-class usage rows match the reconciled `segments` table after recovery; stale usage keys are removed.
- [ ] Pins and `pin_segments` rows for surviving segments are unchanged across recovery; a removed segment's `pin_segments` rows cascade-clear while its `pins` row survives.
- [ ] Filesystem mutations (truncate/seal/rename/sync) happen outside any SQLite transaction; per segment, missing-record decode happens before the insertion transaction opens.
- [ ] Operational failures map to `PersistenceFailed`; the segments-directory-unreadable path maps to `ShutdownIncomplete`. No new `ErrorCode` variant is introduced.
- [ ] `cargo fmt --all --check`, `cargo check --workspace --all-targets --locked`, `cargo test --workspace --all-targets --locked`, `cargo clippy --workspace --all-targets --locked -- -D warnings` pass; isolated `doctor` still reports a browser and creates `index.sqlite3` + `segments/`.

### Unit 2: Cross-layer fault-injection qualification

**Story:** `epic-durable-browser-memory-recovery-fault-injection`

**Depends on:** `epic-durable-browser-memory-recovery-engine`

**Files:**
- `crates/krometrail-store/tests/recovery.rs` (new) — integration tests using the real `SegmentWriter` + real `SqliteIndex` + real `IndexedRecordingSink` against `tempfile::TempDir`, simulating crash aftermaths by direct file manipulation (the honest way to test recovery without root power-loss simulation).

**Acceptance criteria (each test simulates a realistic crash aftermath and proves recovery restores consistency):**
- [ ] **Orphan payload (record-before-index):** append a frame's segment record via `SegmentWriter::append_indexable` (bypassing the index commit), then `recover` → the missing frame row is inserted and the frame is queryable through `FrameSource`.
- [ ] **Dangling tail (truncated open segment):** index N frames through `IndexedRecordingSink` (open segment), then truncate the `.open` file mid-last-record, then `recover` → the tail is removed, the segment is sealed, and the dangling index row for the truncated frame is removed; earlier frames stay queryable.
- [ ] **Idempotence:** after any recovery, `recover` again returns an all-zero report and the index is byte-for-byte unchanged.
- [ ] **Crash-during-recovery (sealed-but-unreconciled):** create an orphan payload, then manually seal the `.open` file (rename + append footer) to simulate a previous recovery that sealed but crashed before inserting rows, then `recover` → the sealed segment is reconciled and the missing row inserted.
- [ ] **Fatal corruption quarantine:** corrupt a sealed segment's header bytes, then `recover` → the file is renamed `.corrupt`, its index rows are gone, and `recover` returns `Ok` with `segments_quarantined == 1`.
- [ ] **Missing file (dangling segment):** flush a session, delete the `.kts` file, then `recover` → that segment's frame rows + registration are removed and other sessions are untouched.
- [ ] **Pins trusted:** insert a pin + `pin_segments` row for a surviving segment, then `recover` → both rows survive unchanged.
- [ ] **Usage reconciliation:** flush a session, delete its segment usage row, then `recover` → the usage row is restored to match the segment's file size.
- [ ] **Empty open segment:** write a `.open` file containing only a valid header, then `recover` → it is sealed as `record_count=0`, registered, and indexed with zero frames.
- [ ] **End-to-end reopen:** index frames via `IndexedRecordingSink`, drop the sink without flushing, re-open the index, `recover`, and assert every frame is queryable; a second `recover` is a no-op.
- [ ] **Open-segment report:** write to two targets without flushing, then `recover` → `report.open_segments_sealed == 2`.
- [ ] **Asymmetric invariant, both directions in one suite:** a fixture that produces an orphan payload on one segment AND a dangling row on another, then `recover` → the orphan is inserted and the dangling row removed in the same pass.
- [ ] Locked workspace fmt/check/test/clippy gates pass.

## Implementation order

```text
Unit 1 (recovery-engine)        depends_on: []
   │   recovery.rs + index/reconcile.rs + one-line index/mod.rs + lib.rs + app.rs wiring
   │   pure classify/seal-decision logic covered by in-module unit tests
   ▼
Unit 2 (fault-injection)         depends_on: [Unit 1]
       realistic crash-aftermath integration suite across segments + index + recovery
```

Linear chain. Unit 1 delivers the engine and the composition hook with deterministic in-module evidence; Unit 2 crosses the segment-format and sqlite-index features with the real crash-mid-write aftermaths the segment-format handoff named. Splitting them keeps the seal-and-reconcile boundary reviewable on deterministic evidence before the cross-layer fault suite layers on filesystem manipulation.

## Simplification and elimination

- **No recovery journal.** The `.open`/`.kts` filename distinction plus idempotent SQL make a crash during recovery self-healing. A separate journal would duplicate authority and add its own crash-consistency problem.
- **No new maintenance primitive.** Dangling-tail removal reuses `remove_frame_rows(segment_id, Some(truncate_point))`; anomalous clean-segment rebuild reuses `remove_frame_rows(segment_id, None)`; whole-segment removal composes both with `remove_segment`. Retention's parallel work is not disturbed.
- **No second scanner.** Recovery reuses the segment-format feature's `scan_complete_records` (absolute offsets, `Trailing` classification, no payload decode) and `read_frame_at` for the rare missing-record decode.
- **No codec duplication.** Recovery-specific SQL lives in `index::reconcile`, which has the same in-`index` access to the private codec that `frames`/`segments`/`timeline` already enjoy. One additive `pub(crate) mod reconcile;` line in `index/mod.rs`; codec itself stays private to `index`.
- **Segment usage only.** Recovery reconciles the one usage class it owns; `index`/`browser_event`/`artifact` usage stay with retention and their owning features. No god-usage pass.
- **Quarantine by extension, not a sidecar table.** `.corrupt` files are skipped by discovery and discoverable by the operator; no quarantine manifest to keep consistent.

## Testing

### Deterministic, in-module (Unit 1)

- **Tail classification:** given a synthetic segment byte buffer, `classify_tail` returns `Clean` for a valid footer, `AppendFooter` for clean records with no footer, `TruncateAndAppendFooter{at}` for `Trailing::Incomplete`/`Corrupt`. Header-corrupt buffers route to quarantine.
- **Footer-input derivation:** for a buffer with N complete records, derived `record_count`/`total_payload`/`first_session_time`/`last_session_time`/`sealed_observed` match the records (and header defaults for a 0-record buffer).
- **Filename parsing:** discovery parses `<uuid>.open`/`.kts`/`.corrupt`, rejects non-UUID stems, and skips non-segment files.

### Real filesystem, temp dir (Unit 1 + Unit 2)

- **Seal + reconcile:** the Unit 1 acceptance criteria, exercised through the real `SqliteIndex` and a temp segments directory.
- **Cross-layer fault injection (Unit 2):** every realistic crash aftermath in the Unit 2 acceptance list, using direct file manipulation to simulate what a crash leaves behind (orphan payloads, truncated tails, missing files, corrupt headers, sealed-but-unreconciled segments). No power-loss simulation — the tests honestly exercise the observable aftermaths.

### Boundary

- The `core_ports_have_no_runtime_or_transport_types` source-scanner test still passes (recovery adds no core types).
- `cargo check -p krometrail-core` is unaffected (recovery is store-local).
- Isolated `doctor` still reports a browser and creates `index.sqlite3` + `segments/` after recovery runs at startup.

## Risks

- **The asymmetric failure direction is the highest-reach decision.** Inserting when removal is correct fabricates non-durable frames; removing when insertion is correct drops durable frames the index already promised. Mitigation: the Unit 2 fault suite forces both directions through real crash aftermaths; the design states the direction table explicitly so the implementor does not invert it.
- **Crash-during-recovery without a journal.** Idempotence carries the weight a journal would. Mitigation: every operation is independently idempotent (sealing renames `.open`→`.kts`; inserts are SELECT-guarded; removals target absent rows); the Unit 2 "crash-during-recovery" and "idempotence" tests prove a second run completes consistently.
- **Sealed-segment in-place repair touches "immutable" files.** A sealed segment is immutable during normal operation, but recovery is the startup consistency pass and may repair a damaged footer/tail in place. The alternative (leaving a corrupt sealed segment) is worse. Mitigation: repair only fires when `Trailing != Clean`; intact sealed segments are never rewritten; the `bytes_truncated`/`segments_repaired` fields report every repair.
- **Per-segment-transation dangling removal is O(rows) on the anomalous rebuild path.** Acceptable: dangling rows are crash artifacts (rare), recovery is a startup pass, and the rebuild path fires only on the anomalous clean-segment-with-stray-dangling case. The common damaged-tail case is one `remove_frame_rows(.., Some(truncate_point))` call.
- **Loading each segment file fully into memory to scan.** The scanner takes `&[u8]`; the default rotation caps segment size at 128 MiB, so worst case is one 128 MiB allocation during recovery of one segment. Acceptable for a startup pass; a future streaming scan primitive (segment-format's territory) would reduce peak memory without changing recovery's contract.
- **One additive line in `index/mod.rs` overlaps the parallel retention/range-resolution write sets.** Purely additive (`pub(crate) mod reconcile;`); merges cleanly; does not modify any existing helper. Recovery otherwise lives in new files (`recovery.rs`, `index/reconcile.rs`) plus one composition-root call, so parallel work is preserved.

## Handoff to downstream features

- **`epic-durable-browser-memory-retention`:** consumes the reconciled store. Reads `RecoveryReport.open_segments_sealed` and the reconciled `usage` table to compute the open-segment-beyond-budget tolerance (recovery reports evidence; retention interprets against the configured budget). Operates only on sealed segments; recovery guarantees zero open segments remain at startup.
- **`epic-durable-browser-memory-range-resolution`:** unaffected. Resolution reads the reconciled index; recovery only makes that index honest.
- **Future artifact/browser-event features:** recovery does not touch `artifacts` or `browser_event` usage; those features and retention own their reconciliation.

## Notes

- The asymmetric failure direction (§2), the four-phase idempotent algorithm (§3), and the crash-during-recovery-via-idempotence settlement (§6) are the load-bearing decisions. Everything else (quarantine extension, segment-only usage, composition logging) is tunable within the constraints above.

## Implementation summary

- Execution capability: highest (autopilot caller), selected because recovery owns the cross-filesystem/SQLite durability invariant, corruption isolation, and startup safety.
- Review weight: standard (caller/project default). Implementation is complete and the feature is intentionally left at `stage: review`; no self-approval was performed.
- Dispatch: one cohesive feature owner carried both child checkpoints linearly. The engine checkpoint committed as `b8bcd46`; fault-injection and final idempotence hardening committed as `e370189`.
- Child checkpoints: `epic-durable-browser-memory-recovery-engine` and `epic-durable-browser-memory-recovery-fault-injection` advanced directly to `done` with focused evidence.
- Production files: new `krometrail-store::recovery` four-phase startup pass; new private `index::reconcile` SQL seam; store exports/module declarations; and root `open_storage` recovery/logging before the indexed recording sink is made available.
- Recovery behavior: validates UUID publications; skips quarantined/non-segment files; seals `.open` segments; repairs torn, CRC-corrupt, semantically undecodable, or footer-damaged tails; file-syncs and directory-syncs publication changes; quarantines header-invalid files; reconciles the disk/index union in both directions; recomputes segment usage; trusts surviving pins; and returns raw open-segment evidence without adding retention policy.
- Record-before-index direction: complete orphan payloads with absent frame IDs insert frame and timeline rows; dangling or mismatched claims are removed. Duplicate-ID orphan records are stably ignored without rewriting metadata on every startup, preserving true all-zero/no-mutation idempotence.
- Tests: 13 real-filesystem recovery cases use the real `SegmentWriter`, `SqliteIndex`, and `IndexedRecordingSink`, plus two deterministic module tests. Coverage includes both asymmetric directions, torn open tails, damaged sealed footers, crash-after-seal-before-reconcile, quarantine, missing files, pin trust/cascade, usage restore/stale removal, empty segments, multi-target reopen/open count, duplicate-ID idempotence, and stable error mapping.
- Verification: exact committed state `e370189` passed locked workspace format, check, 338 tests, and Clippy with warnings denied in an isolated worktree. Isolated `doctor` found one browser and created both `index.sqlite3` and `segments/` with recovery in startup. The shared working tree's check/test also passed, but its Clippy was temporarily blocked by an unrelated in-flight browser-interaction `too_many_arguments` warning; that browser work was preserved and excluded from this feature's commit/gate.
- Simplification: no recovery journal, retention behavior, second segment scanner, parallel usage authority, or new core error/port was introduced. Existing scanner/decoder, frame-index transaction, segment registration, and maintenance primitives remain the authorities.
- Discrepancies from design: logical SQLite row snapshots replace a raw database-file byte comparison for idempotence because WAL/page bytes can change without logical mutation. The implementation also carries the original torn-tail boundary across Phase B sealing into Phase C so only dangling tail rows are removed, and treats a CRC-valid but semantically undecodable record as the start of a corrupt tail.
- Foundation alignment: no standing foundation assertion became false or contradictory; this implementation concretizes the existing startup recovery and record-before-index claims.
- Adjacent issues parked: none.
- Blockers: none.

## Review (2026-07-14)

**Verdict**: Approve with comments

**Blockers**: none
**Important**: none
**Nits**: Synthesized sealed filenames depend on the current discovery invariant; recovery performs
a bounded second discovery; and report docs could clarify torn open-segment counting.
**Rejected**: none

**Notes**: Standard-weight fresh-context review verified publication discovery, tail scan/truncate/
seal/sync, quarantine, orphan-payload insertion, dangling-row removal, identity/address checks,
usage reconciliation, pin trust, idempotence and crash-during-recovery behavior, startup ordering,
open-segment reporting, source-safe errors, and realistic fault injection. Store tests, format,
check, and Clippy were green; the implementation's isolated locked workspace run passed 338 tests.
No material current-cycle risk remains.
