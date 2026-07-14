---
id: epic-durable-browser-memory-retention
kind: feature
stage: review
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

# Disk-Budget Accounting, Pinning, Eviction, and Session Deletion

## Brief

Own the data-removal capability of the recording store: one user-configured global disk budget applied across the complete Krometrail data directory (active sessions, retained sessions, indexes, browser events, generated artifacts), with segment-granular pinning, oldest-unpinned-first eviction, an explicit paused-budget state when only pinned data remains, and session-scoped deletion. This feature keeps total storage bounded and makes stopped sessions queryable under the same budget without requiring an explicit archive action.

Eviction operates on sealed segments: it computes total current usage, identifies the oldest unpinned segments across all sessions, deletes candidates in chronological order together with their associated index rows and unprotected artifacts, updates usage accounting, and stops when usage is within budget. When no unpinned data can satisfy the budget, the recorder enters a paused-budget state that is reported clearly through the status surface; pinned evidence is never deleted to make room. Session deletion removes every segment, index row, artifact, and event belonging to one session id.

This feature does not own the segment byte format, open-segment recovery, or range resolution. It is the runtime data-removal authority that consumes the established segment writer/scanner and extends the existing SQLite migration and maintenance registry rather than creating a second schema, scanner, recovery pass, or resolver.

## Epic context

- Parent epic: `epic-durable-browser-memory`
- Position in epic: consumer of the completed segment-format feature and implementation-complete SQLite-index feature; produces the bounded-storage and operational-status contracts required by SPEC.
- Inherited decisions: one budget for the complete data directory; stopped sessions stay queryable and are ordinary eviction candidates; pinning protects every intersecting segment; protected evidence is never deleted to satisfy the budget; metadata/index mutation remains inside the existing `SqliteIndex` adapter.

## Scope and honest non-goals

**In scope:** non-zero global budget/default 10 GB; authoritative classed usage; segment-range pin/unpin; deterministic global eviction; provenance-safe artifact removal; one-open-segment cleanup tolerance; paused-budget capture/resume; status composition; retryable crash-safe physical/metadata deletion; complete session deletion; bounded cancellation/concurrency behavior.

**Non-goals:** a new segment format or scanner; startup tail recovery/reconciliation; natural-anchor range resolution; artifact generation; new browser-event payload contracts; sub-segment pinning; a second database schema or a second store facade.

## Execution policy and grounding

- **Driver:** active autopilot `--all`; no questions, subagents, peeragent, or push.
- **Worker capability:** highest, caller-selected. Budget enforcement spans a durability-critical SQLite/filesystem boundary and changes live capture state.
- **Review weight:** standard, caller-selected. Design-time advisory review is skipped by explicit caller policy; feature review remains required after implementation.
- **Dispatch rationale:** direct-read only. Grounding covered this item and parent, completed segment-format design/remediation and code, implementation-complete SQLite-index design/code/tests, all five foundation documents, `docs/agents.md`, project rules/conventions, core recording/error/browser-status contracts, CDP capture transitions, and root composition.
- **Parallel-work safety:** only this feature and its new stories are changed. Existing uncommitted browser-control, temporal-vision, and `.work/bin/work-view` edits are preserved untouched.
- **UI surface:** none. The feature changes domain/status contracts and local persistence behavior only.
- **Rolling Foundation:** additive. The design implements existing SPEC/ARCHITECTURE/EVALUATION assertions and does not replace or contradict them.

## Design decisions

### 1. One `RecordingStore` coordinates writes and removal

Evolve `IndexedRecordingSink` into `krometrail_store::RecordingStore`. It owns the existing `SegmentWriter`, `SqliteIndex`, one shared async mutation gate, the retention policy, a blocking removal worker, and a generation-based budget-availability notification. It implements both `RecordingSink` and the new core `RetentionStore` port. `SqliteIndex` remains the timeline/catalog/gap/frame adapter; no SQL moves into core and no second metadata authority appears.

A wrapper around `IndexedRecordingSink` was rejected because two nested mutation locks would leave pin/delete/evict racing the append→index sequence. Putting retention inside `SegmentWriter` was rejected because it would mix SQL policy, artifacts, and session metadata into the frame payload primitive. The chosen coordinator is the smallest place that can serialize cross-resource mutations honestly.

### 2. Default budget is exact decimal 10 GB and global

`DiskBudgetBytes` remains the validated non-zero domain value. Add:

```rust
// crates/krometrail-core/src/recording/session.rs
pub const DEFAULT_DISK_BUDGET_BYTES: u64 = 10_000_000_000;

impl Default for DiskBudgetBytes {
    fn default() -> Self {
        Self::new(DEFAULT_DISK_BUDGET_BYTES).expect("default disk budget is non-zero")
    }
}
```

Decimal bytes match user-facing disk-capacity conventions. Root reads `KROMETRAIL_DISK_BUDGET_BYTES`, rejects empty/non-numeric/zero values before opening storage, and otherwise uses the default. The existing per-session `RecordingSession::disk_budget` remains a snapshot of the global configuration used when that session began; it is not a second enforceable quota.

### 3. Usage is one classed ledger, refreshed at every decision boundary

The existing `usage` table remains the single managed-object ledger. Segment registration upserts exact open/sealed file bytes in the same transaction as frame/segment metadata. Artifact and future browser-event writers must upsert their exact file bytes before publication; session deletion and eviction use the same rows. The SQLite component is refreshed from `index.sqlite3`, `index.sqlite3-wal`, and `index.sqlite3-shm` physical lengths before status/enforcement and stored under reserved `UsageClass::Index` keys. A second refresh after the accounting write absorbs WAL growth; one SQLite page/WAL frame is reported as bounded accounting slack rather than hidden.

```rust
// crates/krometrail-core/src/recording/retention.rs (new)
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct StorageUsage {
    pub segment_bytes: u64,
    pub index_bytes: u64,
    pub browser_event_bytes: u64,
    pub artifact_bytes: u64,
    pub pending_deletion_bytes: u64, // subset still physically present, not double-counted
    pub open_segment_bytes: u64,     // subset of segment_bytes
    pub accounting_slack_bytes: u64,
}

impl StorageUsage {
    pub fn total_bytes(&self) -> Result<u64>;
}
```

All summation uses checked `u64` in Rust over the established big-endian values. Directory walking is not the steady-state authority. It is used only to refresh the three known SQLite files and by the sibling recovery feature to reconcile external/orphan drift.

### 4. Pinning is range-addressed but segment-granular

```rust
// crates/krometrail-core/src/recording/retention.rs
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RetentionRange {
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub range: SessionRange,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PinChange {
    pub request: RetentionRange,
    pub protected_segments: Vec<SegmentId>,
    pub pinned_usage_bytes: u64,
}
```

`pin_range` seals/registers the target's current open segment under the shared mutation gate, then inserts one exact `(session,target,start,end)` pin and every sealed segment satisfying `segment.start <= range.end && segment.end >= range.start` in one immediate transaction. No intersecting segment is a `NotFound` result. Repeating the same pin is idempotent and returns the current protected set. Overlapping pins are independent; a segment stays protected while any `pin_segments` row references it.

`unpin_range` removes the exact range pin, is idempotent, returns the segments whose protection was recomputed, immediately enforces the budget, and wakes paused capture only after usage is actually satisfiable. It never interprets natural anchors; callers must use the sibling range resolver first.

### 5. Global age gets one durable ordering key

Session-relative times cannot be compared across sessions. Extend the existing migration registry with schema v2 rather than abusing `SessionTime`, UUID order, file mtime, or introducing a second schema. V2 adds a non-null-after-backfill `segments.retention_sequence INTEGER`, a singleton next-sequence row, a unique index, and insert/update guards. Existing rows are backfilled in current SQLite row insertion order inside the migration; new segments allocate once on first registration and preserve the value across open→sealed upserts. Overflow is `PersistenceFailed`.

Oldest unpinned selection is exactly:

```sql
ORDER BY segments.retention_sequence ASC, segments.segment_id ASC
```

The sequence means first durable local registration, which is the only cross-session chronological fact the store controls. Segment `start_time_be`/`end_time_be` remain range semantics inside a session, not global age.

### 6. Artifact safety is provenance-complete

Artifacts are derived caches, never authoritative source evidence and never implicitly pinned. Before deleting any frame, retention selects every artifact referencing **any** frame in the candidate segment. Those artifact files and rows are removed with the segment; retaining a mixed-source artifact after even one source frame disappears would leave a false reproducibility claim. Artifacts may also be pruned independently, oldest `(range start, artifact_id)` first, when they alone prevent the budget from fitting; deleting a derived cache does not delete source evidence.

An artifact remains only when every `artifact_frames.frame_id` still exists. The existing foreign key is the final guard: the removal transaction deletes affected artifacts before frame rows. No manifest JSON is parsed for retention and no provenance resolver is duplicated.

### 7. Eviction is deterministic and accounts for one open-segment tolerance

`RecordingStore::enforce_budget` runs under the shared mutation gate:

1. refresh usage;
2. if over budget, request the existing writer worker to seal/register all open segments before selecting candidates;
3. remove regenerable unprotected artifacts if that alone restores the budget;
4. repeatedly select the oldest sealed segment with no `pin_segments` reference, stage its dependent artifacts and segment file, remove metadata, and finish physical deletion;
5. stop at or below budget;
6. if no removable object can satisfy the budget, persist/publish `PausedBudget` and return status without touching pinned segments.

A serialized append may temporarily create or grow one current open segment before enforcement can seal it. After a bounded cleanup return, usage is within budget or the only reported overage is `open_segment_overhead_bytes <= open_segment_overhead_limit_bytes` (the configured rotation size plus fixed header/footer allowance). If multiple targets have open files when cleanup starts, all are sealed before returning; cleanup never excuses N open-segment overruns. The status reports both count and bytes so evaluation can prove the bound.

Before an append, the coordinator attempts cleanup against current usage plus the encoded record size. If protected/non-removable usage leaves no room, it returns `ErrorCode::BudgetExhausted` before calling `SegmentWriter`. After a successful append/index transaction, exact usage replaces the estimate and enforcement runs again. The returned address can therefore never name a frame deleted as part of the same append call.

### 8. Paused budget is a capture state, not a fatal persistence failure

Add `PausedBudget => "paused_budget"` to the existing `CaptureStreamState` registry. `RecordingStore` returns a source-safe `BudgetExhausted` error with retry `AfterRecovery` and recovery text “unpin or delete retained evidence, or increase the disk budget.” Add `RetentionStore::wait_until_recording_allowed`; it uses a watch generation plus a status recheck, so notifications cannot be lost.

The CDP worker handles only `BudgetExhausted` specially: it records one `PersistenceRejected` gap with detail `"disk budget paused capture"`, transitions `Capturing -> PausedBudget`, continues immediate CDP acknowledgement while bounded handoff records saturation gaps, and waits for either budget availability or stream shutdown. On availability it persists pending gaps, transitions `PausedBudget -> Capturing` (or `Hidden` if visibility changed), and continues. Other persistence errors remain terminal. A shutdown `Notify` makes the wait cancellation-aware; stopping a paused stream cannot hang until the flush deadline.

### 9. Status is composed without duplicating capture counters

```rust
// crates/krometrail-core/src/recording/retention.rs
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordingBudgetState { Available, PausedBudget }

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RetainedPoint {
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub session_time: SessionTime,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RetentionStatus {
    pub configured_budget: DiskBudgetBytes,
    pub usage: StorageUsage,
    pub pinned_usage_bytes: u64,
    pub oldest_retained: Option<RetainedPoint>,
    pub newest_retained: Option<RetainedPoint>,
    pub budget_state: RecordingBudgetState,
    pub eviction_blocked: bool,
    pub recording_blocked: bool,
    pub open_segment_count: u64,
    pub open_segment_overhead_bytes: u64,
    pub open_segment_overhead_limit_bytes: u64,
}

impl RetentionStatus {
    #[allow(clippy::too_many_arguments)]
    pub fn new(/* fields above */) -> Result<Self>;
}
```

Validated deserialization enforces: pinned/open bytes do not exceed relevant usage; blocked booleans equal `PausedBudget`; retained endpoints are both absent or coherently ordered by the store query; and reported overage does not exceed the open-segment limit unless paused.

`BrowserStatus` gains `pub retention: RetentionStatus`. Its existing `capture: Vec<TargetCaptureStatus>` remains the single source for frame cadence plus recorded/dropped counters. Thus the SPEC status surface is one response without re-counting live capture statistics in SQLite: budget/current/pinned/range/blocking fields come from `retention`, while cadence and `CaptureStatistics::{persisted_frames,dropped_frames}` come from `capture`.

### 10. Deletion is journaled across SQLite and the filesystem

Schema v2 also adds `deletion_batches` and `deletion_objects` to the existing migration array. Each object stores kind, typed object key, root-relative validated path, byte length, and usage key. There is no second migration runner.

Every eviction/session-delete batch follows one retryable protocol:

1. **Prepare transaction:** record the batch and exact segment/artifact objects while metadata is still queryable.
2. **Stage files:** the dedicated blocking removal worker atomically renames live files into `<data_dir>/.trash/<batch-id>/`, then syncs source and trash directories on Linux/macOS. Missing live + existing staged is idempotent; missing both is recorded and still reconciled.
3. **Metadata transaction:** delete affected artifact rows first, then frame/timeline rows, segment rows, pins as applicable, session-scoped gaps/timeline/targets/session rows for explicit deletion, and mark the batch `metadata_removed`. Usage rows remain while staged bytes still physically exist.
4. **Finalize files:** unlink staged files and sync the trash directory.
5. **Finalize transaction:** remove usage rows and the deletion journal, then remove the empty batch directory.

`RecordingStore::open` resumes all prepared/metadata-removed batches before accepting capture or serving status. Crash at any boundary therefore converges forward: metadata never claims a permanently deleted live file after startup recovery, and physical bytes remain accounted until unlink succeeds. Runtime filesystem errors return `PersistenceFailed`, keep the batch retryable, and do not claim freed usage. This is deletion-journal recovery only; it does not scan/truncate segment records or reconcile orphan writes owned by `epic-durable-browser-memory-recovery`.

### 11. Session deletion uses the same removal primitive and wins over pinning

`delete_session(session_id)` marks the session as deleting under the shared gate, seals/registers all its open segments, plans every segment and artifact file, then removes all rows for that session: artifact links/manifests, frame timeline/frame indexes, segment pins/pin ranges, capture gaps, all generic timeline observations (including interactions/markers/browser-event references), targets, session catalog, and every session-owned usage entry. Explicit deletion is user authority and removes pinned data; ordinary eviction never does.

Future appends/gaps for a deleting/deleted session fail source-safely with `NotFound`, preventing capture from recreating data after deletion. The CDP stream consequently stops through its existing non-budget persistence-failure path. The success report is returned only after staged files are unlinked and usage is cleared:

```rust
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SessionDeletion {
    pub session_id: SessionId,
    pub removed_segments: u64,
    pub removed_frames: u64,
    pub removed_artifacts: u64,
    pub removed_bytes: u64,
}
```

### 12. Core port surface

```rust
// crates/krometrail-core/src/ports/retention.rs (new)
pub trait RetentionStore: Send + Sync {
    fn pin_range(&self, request: RetentionRange)
        -> PortFuture<'_, Result<PinChange>>;
    fn unpin_range(&self, request: RetentionRange)
        -> PortFuture<'_, Result<PinChange>>;
    fn enforce_budget(&self)
        -> PortFuture<'_, Result<RetentionStatus>>;
    fn status(&self)
        -> PortFuture<'_, Result<RetentionStatus>>;
    fn delete_session(&self, session_id: SessionId)
        -> PortFuture<'_, Result<SessionDeletion>>;
    fn wait_until_recording_allowed(&self)
        -> PortFuture<'_, Result<()>>;
}
```

The port contains only core/std values and is object-safe. SQL candidate types, journal state, paths, and usage keys remain crate-private.

## Architectural choice

### Option A — independent retention service wrapping the indexed sink

Simple initially, but append and deletion would use separate locks and could race between physical append, SQL claim, pin selection, and removal. Rejected because correctness would depend on lock-order conventions across two public objects.

### Option B — retention inside `SegmentWriter`

Provides one filesystem worker but makes the frame-format adapter own SQLite, artifacts, session rows, and budget policy. Rejected as a Ports & Adapters violation and a recovery coupling.

### Option C — one store coordinator over existing writer/index (chosen)

Rename/evolve `IndexedRecordingSink` into `RecordingStore`, share one mutation gate, keep segment codec/writer and SQLite index cohesive, and add a narrow retention policy/removal module. This preserves existing authorities and gives the cross-resource protocol one owner.

## Trickiest unit

The deletion journal is the highest-risk unit. SQLite cannot atomically commit a file unlink, and the unsafe orders are symmetric: file-first can leave live indexes pointing at missing evidence; metadata-first can under-report leaked bytes. The staged-file protocol makes every phase idempotent, keeps bytes in usage until unlink, and resumes before serving the store. It must be proven with injected failures after prepare, after each rename, after metadata commit, and after unlink. Pinning/eviction policy is not allowed to consume this unit until those convergence tests pass.

## Implementation units

### Unit 1: Core retention contracts and budget-aware capture vocabulary

**Story:** `epic-durable-browser-memory-retention-core-contracts`

**Files:**
- `crates/krometrail-core/src/recording/retention.rs` (new)
- `crates/krometrail-core/src/recording/{mod.rs,session.rs}`
- `crates/krometrail-core/src/ports/{retention.rs,mod.rs}`
- `crates/krometrail-core/src/{lib.rs,error.rs}`
- `crates/krometrail-core/src/browser/control.rs`
- mechanical core/CDP browser-status and capture-state registry tests/fakes

**Acceptance criteria:**
- [ ] Default/global budget, usage, range, pin, retained-point, status, and deletion values validate constructors and Serde boundaries exactly as specified.
- [ ] `RetentionStore` is object-safe and domain-only; port source guards pass.
- [ ] `PausedBudget` is registry-backed and `BudgetExhausted` guidance is source-safe.
- [ ] `BrowserStatus` composes retention with existing capture cadence/statistics without duplicate counters.

### Unit 2: SQLite retention queries, sequence, usage, and deletion journal

**Story:** `epic-durable-browser-memory-retention-index-contracts`

**Files:**
- `crates/krometrail-store/src/index/schema_v2.rs` (new)
- `crates/krometrail-store/src/index/{migrations.rs,segments.rs,maintenance.rs,mod.rs}`
- `crates/krometrail-store/src/index/retention.rs` (new)
- `crates/krometrail-store/src/index/deletion.rs` (new)
- `crates/krometrail-store/tests/retention_index.rs` (new)

**Acceptance criteria:**
- [ ] Existing v1 databases migrate transactionally/idempotently to contiguous v2; future versions still refuse.
- [ ] Every segment receives one stable global retention sequence and unpinned candidates order exactly by sequence/id.
- [ ] Usage snapshots cover segment/index/browser-event/artifact classes with checked sums, distinct pinned bytes, retained endpoints, open overhead, and bounded SQLite slack.
- [ ] Pin overlap/idempotence and provenance candidate queries are transactional and never select pinned segments.
- [ ] Deletion batches/items survive reopen and expose enough typed data to replay every phase without parsing manifests or paths from errors.

### Unit 3: Crash-safe removal engine and retention policy

**Story:** `epic-durable-browser-memory-retention-removal-engine`

**Files:**
- `crates/krometrail-store/src/retention/{mod.rs,policy.rs,removal.rs,status.rs}` (new)
- `crates/krometrail-store/src/segments/writer.rs`
- `crates/krometrail-store/src/index/{deletion.rs,retention.rs,maintenance.rs}`
- `crates/krometrail-store/src/{recording.rs,lib.rs}`
- `crates/krometrail-store/tests/retention_removal.rs` (new)

**Acceptance criteria:**
- [ ] One `RecordingStore` owns append/index/removal mutation order; `IndexedRecordingSink` is removed rather than wrapped.
- [ ] Pin/unpin seals open target data and protects exactly intersecting segments; ordinary eviction never removes a protected segment.
- [ ] Oldest unpinned selection, artifact pruning/provenance invalidation, one-open-segment bound, pause, resume, and session deletion match the algorithms above.
- [ ] Removal filesystem work runs off the async executor with bounded handoff; every injected crash/failure phase converges on reopen without dangling metadata or unaccounted bytes.
- [ ] Session deletion removes all current session data and rejects later writes for that identity.

### Unit 4: CDP pause/resume and root composition

**Story:** `epic-durable-browser-memory-retention-capture-wiring`

**Files:**
- `crates/krometrail-cdp/src/capture/{mod.rs,pipeline.rs,tests.rs}`
- `crates/krometrail-cdp/src/session.rs`
- `src/app.rs`
- root/CDP tests

**Acceptance criteria:**
- [ ] `ProductionBrowserConnector::with_capture` receives the shared retention port; browser status includes live retention status.
- [ ] `BudgetExhausted` transitions to `PausedBudget`, keeps acknowledgement bounded, records explicit loss, waits without a lost wakeup, resumes after unpin/deletion, and stops promptly when cancelled.
- [ ] Other persistence errors remain terminal and no budget path deletes protected evidence.
- [ ] Root validates `KROMETRAIL_DISK_BUDGET_BYTES`, defaults to 10 GB, opens/resumes one `RecordingStore`, and shares it as recording+retention while retaining the index's focused query ports.

### Unit 5: Small-budget qualification

**Story:** `epic-durable-browser-memory-retention-qualification`

**Files:**
- `crates/krometrail-store/tests/retention_small_budget.rs` (new)
- `crates/krometrail-cdp/tests/retention_capture.rs` (new)
- `src/app.rs` tests

**Acceptance criteria:**
- [ ] Tiny deterministic budgets prove oldest-unpinned order across sessions, overlapping pin/unpin, all-pinned pause, automatic resume, and the one-open-segment overage bound.
- [ ] Mixed-source artifacts are removed before any source disappears; surviving artifacts retain all source rows.
- [ ] Failure injection/reopen proves prepare/stage/metadata/finalize retry and exact usage.
- [ ] Session deletion leaves no file, row, pin, event reference, artifact, or usage entry for the id.
- [ ] Unpolled operations do nothing; accepted mutations finish or remain journaled; paused shutdown is bounded.
- [ ] Locked workspace format/check/test/clippy gates pass without relying on unrelated working-tree changes.

## Implementation order

```text
core-contracts
    ↓
index-contracts
    ↓
removal-engine
    ↓
capture-wiring
    ↓
qualification
```

One feature owner should carry the five checkpoints. The linear chain protects contract/schema/durability order and is not a signal to dispatch five workers.

## Simplification and elimination

- Rename `IndexedRecordingSink` to the capability-complete `RecordingStore`; do not leave a forwarding compatibility wrapper.
- Extend the existing migration array, scanner, index, and maintenance modules; no second schema, recovery scanner, provenance parser, or temporal resolver.
- One shared mutation gate replaces informal lock ordering between recording and removal.
- One removal protocol serves budget eviction and explicit session deletion; only candidate selection differs.
- Keep live capture counters in `TargetCaptureStatus`; status composition references them instead of persisting/recounting a competing aggregate.
- Replace the current maintenance methods' per-call delete choreography with transaction helpers consumed by the journal engine; retain only recovery-specific primitives that the sibling feature needs.

## Testing

- **Core interface:** constructor/Serde invariants and port source guard protect the public contract.
- **Migration/order:** v1→v2 backfill plus interleaved sessions protect true deterministic global age rather than comparing session-relative clocks.
- **Small-budget integration:** real segment files and SQLite under byte-size budgets protect eviction, pins, status, and one-open overhead.
- **Artifact regression:** one artifact spanning two segments protects the “any missing source invalidates provenance” rule.
- **Crash matrix:** injected failure after each deletion phase plus reopen protects forward convergence and no usage under-reporting.
- **Capture regression:** scripted budget gate proves immediate acknowledgement, explicit drops, paused status, wake/resume, and shutdown cancellation.
- **Session deletion:** one session with frames/gaps/timeline/pins/artifact and another survivor protects complete scoped deletion.
- No test per SQL statement, getter, status field, or trivial error branch; stable seams and demonstrated risks carry the coverage.

## Risks

- **SQLite self-accounting has unavoidable bounded slack.** Updating its own usage row can grow WAL. Two-pass refresh plus explicit one-page/WAL-frame slack makes the bound observable; tests must not claim byte-perfect zero overhead.
- **Deletion journal is now startup-critical.** A malformed journal must fail store open rather than serve dangling metadata. Its schema and replay tests are the primary mitigation.
- **Global sequence migration relies on current row order for pre-v2 data.** That is the only persisted creation ordering available. New data has explicit sequence; old rows receive a deterministic best-effort order once and never reorder.
- **Paused capture can accumulate bounded loss.** CDP frames remain acknowledged and bounded queues can drop while storage is blocked; every known loss is an explicit gap. Stopping screencast instead would hide browser-side continuity and complicate restart semantics.
- **Session deletion of an active recording is destructive by definition.** The deleting tombstone and shared gate prevent resurrection, and the live stream fails explicitly. Callers should normally stop first, but correctness does not depend on them doing so.
- **SQLite-index feature is at review, not terminal done.** Its implemented contracts are present and dependency-ready. If review changes maintenance/schema assumptions, this design must be reconciled before Unit 2 rather than forked.

## Implementation roll-up

All five checkpoints are implemented and the qualification child is complete:

- `23c6ef3` — core retention contracts and budget-aware capture vocabulary.
- `a043d74` — SQLite sequence, usage, pin, provenance, and deletion-journal contracts.
- `a7e0eb7` — `RecordingStore`, bounded removal worker, eviction, pause/resume, and session deletion.
- `d1df912` — CDP pause/resume and root storage composition.
- `6944c63` — small-budget qualification, mixed-source artifact survival/invalidation, journal replay usage assertions, session-survivor deletion, unpolled behavior, source-safe budget errors, and the CDP test lint hardening needed for the locked gate.

Qualification evidence covers deterministic oldest-unpinned eviction across sessions, overlapping pins, all-pinned pause and unpin resume, one-open-segment bounded overhead, provenance invalidation, both prepared/metadata-removed journal replay phases with pending-byte accounting, complete scoped deletion with a surviving session, unpolled append semantics, and CDP acknowledgment/gap/state/shutdown behavior. No production retention defect was exposed by the final qualification; the only adjacent fix was scoping a CDP test mutex guard so Clippy's `await_holding_lock` gate remains green.

Focused gates passed:

- `cargo test -p krometrail-store --test retention_small_budget --locked -- --nocapture` — 7 passed.
- `cargo test -p krometrail-store --lib --locked -- --nocapture` — 21 passed.
- `cargo test -p krometrail-cdp --lib capture::tests --locked -- --nocapture` — 26 passed.

Isolated clean-worktree gates at `6944c63` passed:
`cargo fmt --all -- --check`, `cargo check --workspace --all-targets --locked`, `cargo test --workspace --all-targets --locked` (all suites green), and `cargo clippy --workspace --all-targets --locked -- -D warnings`.

The primary checkout's unrelated verified-interactions WIP and `.work/bin/work-view` remain untouched and un-staged. The qualification story advances directly to `done`; this feature is now at `review` for the required integrated review boundary.
