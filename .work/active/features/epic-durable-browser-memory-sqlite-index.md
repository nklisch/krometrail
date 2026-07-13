---
id: epic-durable-browser-memory-sqlite-index
kind: feature
stage: implementing
tags: [storage, browser]
parent: epic-durable-browser-memory
depends_on: [epic-durable-browser-memory-segment-format]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# SQLite Metadata Index and Timeline Indexing

## Brief

Own the searchable metadata layer of the recording store: a versioned SQLite schema running in write-ahead logging mode that makes recorded data queryable across active and stopped sessions. The index is the single metadata authority — sessions, targets, frame addresses, segment registrations, capture gaps, interactions, markers, browser events, pins, artifact manifests, and usage accounting — and it implements the existing `TimelineStore` port plus the structured-persistence surface for records core already defines.

Timeline observations are persisted generically: one index over every `ObservationKind` (frame, interaction boundary, navigation, target lifecycle, visibility change, capture gap, console message, JavaScript exception, network lifecycle, marker) rather than ten parallel tables. Structured per-kind tables are added only for records `krometrail-core` already defines today (`CaptureGap`), with explicit extension-point ports for richer structured records (`InteractionRecord`, browser-event payloads) that arrive when sibling epics define those types.

This feature does not own the segment byte format, budget accounting, eviction, recovery reconciliation, or range resolution. It is the metadata authority that retention, recovery, and range-resolution read from and mutate.

## Epic context

- Parent epic: `epic-durable-browser-memory`
- Position in epic: foundational metadata feature — depends on the segment-format feature for the `(segment_id, byte_offset)` frame-address contract; consumed by retention, recovery, and range-resolution.
- Design decisions inherited: timeline observations are indexed generically by kind; structured tables exist only for core-defined records; the index is the single searchable metadata surface; ports are extended in focused slices (a frame-source read port is added here, alongside the existing `TimelineStore` write/range port).

## Simplification opportunity

- Persist all `ObservationKind` values through one generic timeline index keyed by `(session, target, session_time, kind, payload_ref)` rather than maintaining one table per observation kind. The discriminator and payload-ref columns are enough for range queries; structured detail tables layer on top only where core defines a structured record.
- Drive table membership for observation kinds from the existing `ObservationKind` registry in `krometrail-core` so adding a kind does not require hand-editing the schema in two places.
- Replace the in-memory `FakeTimeline` test double's assumed surface with the real adapter wired through the composition root.

## Foundation references

- `docs/VISION.md` — Local-First Operation
- `docs/SPEC.md` — Sessions and Targets, Action Timeline, Browser Events, Disk Budget and Retention, Temporal Ranges
- `docs/ARCHITECTURE.md` — Recording Store, Domain Model, Crash Recovery, Failure Isolation
- `docs/EVALUATION.md` — Storage and Retention Evaluation

## Scope and honest non-goals

**In scope:**

- A versioned SQLite schema with migrations, running in WAL mode, covering sessions, targets, frame indexes, segment registrations, capture gaps, pins, artifact manifests, usage accounting, and one generic timeline observation index.
- The `TimelineStore` adapter backed by SQLite.
- Focused core ports for current domain records: recording-session/target catalog persistence, capture-gap persistence/range reads, and encoded-frame reads by ids or session range.
- An indexed recording facade that orders a complete segment append before the atomic SQLite segment/frame/timeline transaction and replaces the segment writer's temporary unsupported gap path.
- Primitive store-local mutations consumed by retention and recovery.
- A schema extension seam for future interaction, browser-event, and artifact types without defining those sibling-owned Rust contracts now.

**Non-goals:**

- Segment encoding and rotation policy.
- Budget policy, pin/unpin behavior, eviction selection, session deletion orchestration, or status calculation.
- Open-segment recovery and reconciliation policy.
- Natural-anchor range resolution.
- Defining interaction, browser-event, or artifact-manifest Rust types before their owning features land.

## Execution policy and grounding

- **Driver:** active autopilot `--all`; no questions, subagents, peeragent, or push.
- **Effective worker capability:** highest/raised because this feature fixes durable schema and migration rules, cross-file durability ordering, generic query ordering, redaction boundaries, and the ports consumed by three dependent storage features.
- **Effective review weight:** `standard` (autopilot/project default). Design-time advisory review is intentionally skipped because the caller prohibited subdelegation; feature-level implementation review remains required.
- **Dispatch rationale:** direct-read only. Grounding covered the parent and all sibling feature bodies, the completed segment-format implementation and tests, core recording/time/id/timeline/error/port contracts, the CDP gap and frame persistence path, root composition, workspace manifests, all five foundation docs, `AGENTS.md`, `.agents/rules/agile-workflow.md`, and the principles skill.
- **UI surface:** none. This is a local persistence adapter and core port boundary.
- **Rolling Foundation:** additive. The design concretizes the standing SQLite/WAL/index claims and contradicts no current or intended foundation assertion.

## Design decisions

### 1. Use exact `rusqlite` 0.33.0 with bundled SQLite and no default features

Current registry verification on 2026-07-13 found `rusqlite` 0.40.1 as latest (released 2026-06-06), but its published MSRV policy is “latest stable Rust at release,” which is newer than Krometrail's Rust 1.85 contract. `rusqlite` 0.33.0 was released 2025-01-19, when stable Rust was 1.84, and its synchronous APIs needed here were compiled in a Rust-1.85-constrained probe: `Connection::open_with_flags`, `busy_timeout`, `pragma_update`, `pragma_query_value`, `transaction_with_behavior(TransactionBehavior::Immediate)`, strict tables, and transaction-local `user_version` updates.

```toml
# Cargo.toml
rusqlite = { version = "=0.33.0", default-features = false, features = ["bundled"] }

# crates/krometrail-store/Cargo.toml
rusqlite.workspace = true
serde_json.workspace = true
```

Bundling gives Linux and macOS one tested SQLite feature/version surface and avoids a system-library prerequisite. Disabling defaults omits statement-cache/hashlink machinery this adapter does not need. SQL remains private to `krometrail-store::index`; neither core nor callers see `rusqlite` values.

### 2. One synchronous connection, one mutex, bounded transactions

`SqliteIndex` owns `Mutex<rusqlite::Connection>`. Rusqlite is synchronous and matches the store's existing blocking segment boundary; introducing an async pool would add runtime coupling and would not make SQLite a multi-writer database. The single connection serializes writes and gives deterministic insertion tie-breaks. WAL prepares the file format for later read connections without requiring a pool now.

Every port method returns an `async` block that performs work when first polled. Once a poll enters SQLite, the bounded transaction completes or rolls back; dropping the future cannot interrupt between related statements. No transaction contains `.await`. A five-second busy timeout bounds external lock contention. Mutex poison, busy timeout, migration failure, decode failure, and SQL failure all map to source-safe `ErrorCode::PersistenceFailed`; SQL text, file paths, raw values, and driver errors remain out of the public error and may appear only in local debug logs.

Lock order is always indexed-recording mutation gate → segment writer → SQLite connection. Frame reads release the SQLite mutex before opening a segment file. This prevents lock inversion and keeps slow file reads from blocking metadata queries.

### 3. Observation stable names come from the core declaration

The macro-backed observation declaration grows stable names in the same source entry that already generates enum/payload compatibility:

```rust
// crates/krometrail-core/src/timeline/observation.rs

define_observation_contract! {
    Frame => ("frame", typed(Frame, FrameId)),
    InteractionBoundary => ("interaction_boundary", typed(Interaction, InteractionId)),
    Navigation => ("navigation", typed(Navigation, NavigationId)),
    TargetLifecycle => ("target_lifecycle", external),
    VisibilityChange => ("visibility_change", external),
    CaptureGap => ("capture_gap", typed(Gap, GapId)),
    ConsoleMessage => ("console_message", external),
    JavascriptException => ("javascript_exception", external),
    NetworkLifecycle => ("network_lifecycle", external),
    Marker => ("marker", typed(Marker, MarkerId)),
}

impl ObservationKind {
    pub const ALL: &'static [Self] = /* generated */;
    pub const fn as_str(self) -> &'static str /* generated */;
    pub fn from_stable_name(value: &str) -> Option<Self> /* generated */;
}
```

Serde rename attributes, `ALL`, SQL encoding, SQL decoding, and compatibility tests derive from that declaration. The migration does not contain a hand-maintained observation-kind `CHECK (...)` list; the adapter validates every decoded name through `from_stable_name`, so adding a kind changes one registry and requires no schema migration.

### 4. Capture gaps gain declaration time and persist atomically with their timeline row

`CaptureGap` currently cannot become a lossless `TimelineObservation` because it has a range but no `ObservedTime`. Add `observed_time` to the existing type rather than fabricating it in the store:

```rust
// crates/krometrail-core/src/recording/gap.rs
pub fn new(
    id: GapId,
    session_id: SessionId,
    target_id: TargetId,
    range: SessionRange,
    observed_time: ObservedTime,
    reason: CaptureGapReason,
    estimated_missing_frames: Option<NonZeroU64>,
    detail: Option<String>,
) -> Result<Self>;

pub const fn observed_time(&self) -> ObservedTime;
```

Construction rejects a range whose end exceeds declaration time. CDP `declare_gap`/`declare_gap_range` sample the existing monotonic clock when the gap is created; coalescing retains the maximum declaration time. `SqliteIndex::append_gap` inserts the structured row and a generic `capture_gap` observation at `range.start()` in one immediate transaction. Range gap reads use interval overlap (`gap_start <= query_end AND gap_end >= query_start`), not only the generic observation point.

### 5. Add three focused core port slices; do not add speculative sibling types

```rust
// crates/krometrail-core/src/ports/catalog.rs
pub trait RecordingCatalog: Send + Sync {
    fn put_session(&self, session: RecordingSession) -> PortFuture<'_, Result<()>>;
    fn put_target(
        &self,
        session_id: SessionId,
        target: PageTarget,
    ) -> PortFuture<'_, Result<()>>;
}

// crates/krometrail-core/src/ports/gaps.rs
pub trait CaptureGapStore: Send + Sync {
    fn append_gap(&self, gap: CaptureGap) -> PortFuture<'_, Result<()>>;
    fn gaps(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        range: SessionRange,
    ) -> PortFuture<'_, Result<Vec<CaptureGap>>>;
}

// crates/krometrail-core/src/ports/frames.rs
pub trait FrameSource: Send + Sync {
    /// Returns exactly one frame per requested id, preserving request order.
    /// Any missing id fails the whole request with NotFound.
    fn frames_by_id(
        &self,
        frame_ids: Vec<FrameId>,
    ) -> PortFuture<'_, Result<Vec<EncodedFrame>>>;

    /// Returns retained frames in capture-ordinal order for one target.
    fn frames_in_range(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        range: SessionRange,
    ) -> PortFuture<'_, Result<Vec<EncodedFrame>>>;
}
```

`TimelineStore::range` is documented as inclusive and deterministically ordered. These traits use domain values only and retain the existing object-safe `PortFuture` pattern.

No `InteractionRecord`, browser-event payload, pin, or artifact-manifest Rust type is introduced here. Generic observations can index their existing typed id or opaque external payload reference now. Structured persistence is added later as a focused port in the same migration array when the owning core type exists. This is the extension seam; an empty generic byte-payload god-port would weaken redaction and generated-contract guarantees.

### 6. Schema v1 uses lossless fixed-width boundary encodings

UUID-backed ids are 16-byte BLOBs. Unsigned `u64` values (session/observed times, ordinals, byte offsets, sizes) are fixed-width big-endian 8-byte BLOBs, preserving the complete core domain and sorting lexicographically in unsigned numeric order. `SourceTime(i128)` is a 16-byte two's-complement BLOB (round-trip only; it is not an ordering authority). Private `index::codec` helpers perform checked conversion. SQLite signed integers are used only for bounded counts/dimensions and internal row ids.

Schema version 1 is one forward-only migration:

```sql
CREATE TABLE sessions (
    session_id       BLOB PRIMARY KEY CHECK(length(session_id) = 16),
    record_json      TEXT NULL
) STRICT;

CREATE TABLE targets (
    session_id       BLOB NOT NULL CHECK(length(session_id) = 16),
    target_id        BLOB NOT NULL CHECK(length(target_id) = 16),
    record_json      TEXT NULL,
    PRIMARY KEY(session_id, target_id),
    FOREIGN KEY(session_id) REFERENCES sessions(session_id) ON DELETE CASCADE
) STRICT;

CREATE TABLE segments (
    segment_id       BLOB PRIMARY KEY CHECK(length(segment_id) = 16),
    session_id       BLOB NOT NULL CHECK(length(session_id) = 16),
    target_id        BLOB NOT NULL CHECK(length(target_id) = 16),
    state            TEXT NOT NULL CHECK(state IN ('open', 'sealed')),
    relative_path    TEXT NOT NULL UNIQUE,
    start_time_be    BLOB NOT NULL CHECK(length(start_time_be) = 8),
    end_time_be      BLOB NULL CHECK(end_time_be IS NULL OR length(end_time_be) = 8),
    file_bytes_be    BLOB NOT NULL CHECK(length(file_bytes_be) = 8),
    payload_bytes_be BLOB NOT NULL CHECK(length(payload_bytes_be) = 8),
    record_count_be  BLOB NOT NULL CHECK(length(record_count_be) = 8),
    FOREIGN KEY(session_id, target_id) REFERENCES targets(session_id, target_id)
) STRICT;

CREATE TABLE frames (
    frame_id         BLOB PRIMARY KEY CHECK(length(frame_id) = 16),
    session_id       BLOB NOT NULL CHECK(length(session_id) = 16),
    target_id        BLOB NOT NULL CHECK(length(target_id) = 16),
    segment_id       BLOB NOT NULL CHECK(length(segment_id) = 16),
    byte_offset_be   BLOB NOT NULL CHECK(length(byte_offset_be) = 8),
    session_time_be  BLOB NOT NULL CHECK(length(session_time_be) = 8),
    source_time_be   BLOB NULL CHECK(source_time_be IS NULL OR length(source_time_be) = 16),
    observed_time_be BLOB NOT NULL CHECK(length(observed_time_be) = 8),
    capture_ordinal_be BLOB NOT NULL CHECK(length(capture_ordinal_be) = 8),
    format           TEXT NOT NULL CHECK(format IN ('jpeg', 'png')),
    image_width      INTEGER NOT NULL CHECK(image_width > 0),
    image_height     INTEGER NOT NULL CHECK(image_height > 0),
    viewport_width   INTEGER NOT NULL CHECK(viewport_width > 0),
    viewport_height  INTEGER NOT NULL CHECK(viewport_height > 0),
    device_scale     REAL NOT NULL CHECK(device_scale > 0.0),
    warnings_json    TEXT NOT NULL,
    UNIQUE(segment_id, byte_offset_be),
    UNIQUE(session_id, target_id, capture_ordinal_be),
    FOREIGN KEY(session_id, target_id) REFERENCES targets(session_id, target_id),
    FOREIGN KEY(segment_id) REFERENCES segments(segment_id)
) STRICT;

CREATE TABLE timeline_observations (
    observation_id   INTEGER PRIMARY KEY,
    session_id       BLOB NOT NULL CHECK(length(session_id) = 16),
    target_id        BLOB NOT NULL CHECK(length(target_id) = 16),
    session_time_be  BLOB NOT NULL CHECK(length(session_time_be) = 8),
    source_time_be   BLOB NULL CHECK(source_time_be IS NULL OR length(source_time_be) = 16),
    observed_time_be BLOB NOT NULL CHECK(length(observed_time_be) = 8),
    capture_ordinal_be BLOB NULL CHECK(capture_ordinal_be IS NULL OR length(capture_ordinal_be) = 8),
    kind             TEXT NOT NULL,
    payload_json     TEXT NOT NULL,
    payload_sort_key BLOB NOT NULL,
    FOREIGN KEY(session_id, target_id) REFERENCES targets(session_id, target_id)
) STRICT;

CREATE TABLE capture_gaps (
    gap_id           BLOB PRIMARY KEY CHECK(length(gap_id) = 16),
    session_id       BLOB NOT NULL CHECK(length(session_id) = 16),
    target_id        BLOB NOT NULL CHECK(length(target_id) = 16),
    start_time_be    BLOB NOT NULL CHECK(length(start_time_be) = 8),
    end_time_be      BLOB NOT NULL CHECK(length(end_time_be) = 8),
    observed_time_be BLOB NOT NULL CHECK(length(observed_time_be) = 8),
    reason           TEXT NOT NULL,
    estimated_missing_be BLOB NULL CHECK(estimated_missing_be IS NULL OR length(estimated_missing_be) = 8),
    detail           TEXT NULL,
    FOREIGN KEY(session_id, target_id) REFERENCES targets(session_id, target_id)
) STRICT;

CREATE TABLE pins (
    pin_id            INTEGER PRIMARY KEY,
    session_id        BLOB NOT NULL CHECK(length(session_id) = 16),
    target_id         BLOB NOT NULL CHECK(length(target_id) = 16),
    start_time_be     BLOB NOT NULL CHECK(length(start_time_be) = 8),
    end_time_be       BLOB NOT NULL CHECK(length(end_time_be) = 8),
    UNIQUE(session_id, target_id, start_time_be, end_time_be),
    FOREIGN KEY(session_id, target_id) REFERENCES targets(session_id, target_id)
) STRICT;

CREATE TABLE pin_segments (
    pin_id            INTEGER NOT NULL,
    segment_id        BLOB NOT NULL CHECK(length(segment_id) = 16),
    PRIMARY KEY(pin_id, segment_id),
    FOREIGN KEY(pin_id) REFERENCES pins(pin_id) ON DELETE CASCADE,
    FOREIGN KEY(segment_id) REFERENCES segments(segment_id) ON DELETE CASCADE
) STRICT;

CREATE TABLE artifacts (
    artifact_id       BLOB PRIMARY KEY CHECK(length(artifact_id) = 16),
    session_id        BLOB NOT NULL CHECK(length(session_id) = 16),
    target_id         BLOB NOT NULL CHECK(length(target_id) = 16),
    kind              TEXT NOT NULL,
    start_time_be     BLOB NOT NULL CHECK(length(start_time_be) = 8),
    end_time_be       BLOB NOT NULL CHECK(length(end_time_be) = 8),
    manifest_json     TEXT NOT NULL,
    relative_path     TEXT NOT NULL UNIQUE,
    byte_len_be       BLOB NOT NULL CHECK(length(byte_len_be) = 8),
    FOREIGN KEY(session_id, target_id) REFERENCES targets(session_id, target_id)
) STRICT;

CREATE TABLE artifact_frames (
    artifact_id       BLOB NOT NULL CHECK(length(artifact_id) = 16),
    source_position   INTEGER NOT NULL CHECK(source_position >= 0),
    frame_id          BLOB NOT NULL CHECK(length(frame_id) = 16),
    PRIMARY KEY(artifact_id, source_position),
    UNIQUE(artifact_id, frame_id),
    FOREIGN KEY(artifact_id) REFERENCES artifacts(artifact_id) ON DELETE CASCADE,
    FOREIGN KEY(frame_id) REFERENCES frames(frame_id)
) STRICT;

CREATE TABLE usage (
    class             TEXT NOT NULL CHECK(class IN ('segment', 'index', 'browser_event', 'artifact')),
    object_key        BLOB NOT NULL,
    session_id        BLOB NULL CHECK(session_id IS NULL OR length(session_id) = 16),
    byte_len_be       BLOB NOT NULL CHECK(length(byte_len_be) = 8),
    PRIMARY KEY(class, object_key)
) STRICT;

CREATE INDEX frame_range_idx
    ON frames(session_id, target_id, session_time_be, capture_ordinal_be);
CREATE INDEX timeline_range_idx
    ON timeline_observations(session_id, target_id, session_time_be,
                             capture_ordinal_be, observed_time_be, kind, payload_sort_key);
CREATE INDEX gap_range_idx
    ON capture_gaps(session_id, target_id, start_time_be, end_time_be);
CREATE INDEX segment_retention_idx
    ON segments(state, start_time_be, segment_id);
CREATE INDEX artifact_range_idx
    ON artifacts(session_id, target_id, start_time_be, end_time_be);
```

Session/target identity placeholders (`record_json IS NULL`) are inserted transactionally when the first observation arrives, because the current capture pipeline can persist before a future lifecycle service supplies full `RecordingSession`/`PageTarget` records. `RecordingCatalog::put_*` fills the generated core JSON contract later with an upsert; a placeholder never claims unavailable metadata. This preserves foreign keys without inventing partial domain values.

The artifact/pin tables are schema extension points only. No public writer exists until the owning feature supplies its domain type and validation. Generic interaction and browser-event observations need no structured table today.

### 7. Forward-only migration and startup contract

```rust
// crates/krometrail-store/src/index/migrations.rs
pub(crate) struct Migration {
    pub version: u32,
    pub sql: &'static str,
}

pub(crate) const LATEST_SCHEMA_VERSION: u32 = 1;
pub(crate) const MIGRATIONS: &[Migration] = &[Migration { version: 1, sql: V1_SQL }];

pub(crate) fn migrate(connection: &mut rusqlite::Connection) -> Result<()>;
```

`SqliteIndex::open` creates the parent directory, opens with `READ_WRITE | CREATE | NO_MUTEX`, applies `busy_timeout(5s)`, enables foreign keys, requests `journal_mode=WAL` and verifies the returned value, sets `synchronous=FULL`, then migrates. Version `0` applies v1 in `TransactionBehavior::Exclusive`, updates `PRAGMA user_version=1` inside the transaction, and commits. Reopen at v1 is a no-op. A version greater than `LATEST_SCHEMA_VERSION`, a missing migration step, or any failed statement prevents startup; the transaction leaves the prior version intact. There are no ad-hoc `ALTER TABLE` calls outside the ordered migration array.

### 8. Generic timeline ordering is explicit and deterministic

`TimelineStore::append` accepts every validated observation except `Frame` and `CaptureGap`; those two must use the indexed-frame and structured-gap paths so metadata cannot become detached from its authoritative record. It inserts only metadata and payload reference JSON — never a browser-event body.

Inclusive range SQL orders by:

```sql
ORDER BY
    session_time_be ASC,
    CASE WHEN capture_ordinal_be IS NULL THEN 1 ELSE 0 END ASC,
    capture_ordinal_be ASC,
    observed_time_be ASC,
    kind ASC,
    payload_sort_key ASC,
    observation_id ASC
```

For equal session times, frame observations use the target's authoritative capture ordinal. Other ties are resolved by observed time, registry-derived stable kind name, typed-id/opaque-ref sort key, then the connection's insertion id. The ordering is deterministic but does not claim causality between unrelated observation kinds at the same clock reading. `frames_in_range` independently orders by `capture_ordinal_be`, then session time and frame id.

### 9. Indexed recording is an ordered facade, not a distributed transaction fiction

The raw `SegmentWriter` stops implementing `RecordingSink`; it becomes the frame-payload primitive. It reports indexable segment state:

```rust
// crates/krometrail-store/src/segments/writer.rs
#[derive(Clone, Debug)]
pub(crate) struct SegmentRegistration {
    pub segment_id: SegmentId,
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub state: SegmentState,
    pub relative_path: PathBuf,
    pub start_time: SessionTime,
    pub end_time: Option<SessionTime>,
    pub file_bytes: u64,
    pub payload_bytes: u64,
    pub record_count: u64,
}

pub(crate) struct FrameWriteCommit {
    pub address: FrameAddress,
    pub active_segment: SegmentRegistration,
    pub sealed_segment: Option<SegmentRegistration>,
}

impl SegmentWriter {
    pub(crate) fn append_indexable(&self, frame: &EncodedFrame) -> Result<FrameWriteCommit>;
    pub(crate) fn flush_indexable(
        &self,
        session_id: SessionId,
    ) -> Result<Vec<SegmentRegistration>>;
}
```

```rust
// crates/krometrail-store/src/recording.rs
pub struct IndexedRecordingSink {
    mutations: Mutex<()>,
    segments: Arc<SegmentWriter>,
    index: Arc<SqliteIndex>,
}

impl IndexedRecordingSink {
    pub fn new(segments: Arc<SegmentWriter>, index: Arc<SqliteIndex>) -> Self;
}

impl RecordingSink for IndexedRecordingSink {
    fn append_frame(&self, frame: EncodedFrame) -> PortFuture<'_, Result<FrameAddress>>;
    fn append_gap(&self, gap: CaptureGap) -> PortFuture<'_, Result<()>>;
    fn flush(&self, session_id: SessionId) -> PortFuture<'_, Result<()>>;
}
```

Frame append sequence under the mutation gate:

1. `SegmentWriter::append_indexable(&frame)` writes and flushes the complete CRC-guarded record before returning `FrameAddress`, plus any rotation registration.
2. One immediate SQLite transaction upserts identity placeholders and segment registrations, inserts the frame row, and inserts its `ObservationKind::Frame` row with capture ordinal.
3. Commit, then return the address.

There is no cross-filesystem distributed transaction. The safe failure asymmetry is intentional: a segment append followed by index failure leaves an unclaimed complete record for recovery; the index can never claim a frame before the segment writer has returned a complete record. Rotation/session flush calls `sync_data` and seals before the corresponding segment-state update commits. A later recovery story owns power-loss reconciliation; this feature owns the ordering test. The global mutation gate preserves append→index commit order even when target workers race; SQLite is one writer anyway.

`append_gap` performs only the structured-gap/timeline transaction and therefore replaces the temporary `Unsupported` route without touching segment bytes. `flush` seals all session segments first, then updates every registration in one transaction. If either half fails, `PersistenceFailed` propagates and the caller does not report a successful flush.

### 10. Frame reads use addresses and bounded seek reads

The current `read_frame_at(&[u8], FrameAddress)` requires loading a whole segment. Add a bounded reader primitive and keep the slice helper as a cursor wrapper:

```rust
// crates/krometrail-store/src/segments/scanner.rs
pub fn read_frame_from<R: std::io::Read + std::io::Seek>(
    reader: &mut R,
    address: FrameAddress,
) -> Result<EncodedFrame>;

pub fn read_frame_at(bytes: &[u8], address: FrameAddress) -> Result<EncodedFrame> {
    read_frame_from(&mut std::io::Cursor::new(bytes), address)
}
```

`SqliteIndex` first resolves ids/ranges to `(FrameId, FrameAddress)` rows and releases the connection. For each address it tries the registered relative path, tolerates the `.open`→`.kts` rename race by retrying the sealed path once, seeks to the header and record prefix, reads only the declared record, verifies CRC through the shared codec, and checks the decoded frame id/session/target against the index row. Missing files/records, context mismatches, or corruption are `PersistenceFailed`, not `NotFound`; an absent requested frame id is `NotFound`.

### 11. Redaction is a write-boundary rule, not query-time filtering

Schema v1 has no raw browser-event payload table and no API accepting request/response bodies or headers. `timeline_observations.payload_json` stores only `ObservationPayloadRef` — a typed id or caller-owned opaque payload reference — never event content. Thus generic console/network observations can be indexed without persisting sensitive values.

When the sibling browser-event feature defines its normalized payload type, it must add a migration and a focused port that accepts only its sanitized type; cookies, authentication values, sensitive headers, and request/response bodies are removed before that port is called. This feature deliberately does not add `append_bytes`, generic JSON payload storage, or query-time redaction. Similarly, interaction and artifact structured writers wait for their generated core contracts. This omission is the strongest enforceable redaction boundary available before those types exist and avoids inventing them here.

Target/session catalog JSON remains local recording metadata. Adapter errors and logs never include catalog JSON, URLs, titles, gap detail, paths, or SQL parameters.

### 12. Primitive maintenance surface stays store-local

Retention and recovery live in `krometrail-store`, so SQL mechanics do not need core ports:

```rust
// crates/krometrail-store/src/index/maintenance.rs
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum UsageClass { Segment, Index, BrowserEvent, Artifact }

pub(crate) struct UsageEntry {
    pub class: UsageClass,
    pub object_key: Box<[u8]>,
    pub session_id: Option<SessionId>,
    pub byte_len: u64,
}

impl SqliteIndex {
    pub(crate) fn remove_frame_rows(
        &self,
        segment_id: SegmentId,
        from_offset: Option<ByteOffset>,
    ) -> Result<Vec<FrameId>>;
    pub(crate) fn remove_segment(&self, segment_id: SegmentId) -> Result<()>;
    pub(crate) fn remove_artifact(&self, artifact_id: ArtifactId) -> Result<()>;
    pub(crate) fn update_usage(&self, entry: UsageEntry) -> Result<()>;
    pub(crate) fn remove_usage(
        &self,
        class: UsageClass,
        object_key: &[u8],
    ) -> Result<()>;
}
```

`remove_frame_rows` deletes matching frame timeline rows and frame rows in one transaction and returns ids for recovery/artifact provenance handling. `remove_segment` refuses while indexed frames still reference the segment, forcing callers to compose deletion explicitly. `update_usage` is an upsert; checked Rust summation over fixed-width values avoids SQLite signed overflow. Selection policy, physical-file deletion, artifact provenance policy, pin semantics, and session deletion orchestration remain in retention/recovery.

## Architectural choice

### Option A — `rusqlite` adapter plus indexed-recording facade (chosen)

Keep core ports domain-only, put SQL/migrations/codecs in `krometrail-store::index`, and compose the existing segment writer with that index in one `RecordingSink`. This provides explicit append ordering, one metadata authority, and a narrow synchronous boundary with one mature dependency.

### Option B — put SQLite directly inside `SegmentWriter`

This can make append sequencing look simpler but conflates frame bytes, searchable metadata, gap records, queries, and migrations in one adapter. Recovery and retention would need to depend on a writer object even when no capture is active. Rejected because the architecture deliberately separates segment payloads from index metadata.

### Option C — use an async ORM or connection pool

An ORM would require mapping hand-written domain copies and hides the durability-critical SQL transactions; a pool adds runtime/concurrency machinery around a single SQLite writer. Both enlarge the dependency and generated-contract surface without improving this feature's current throughput or correctness. Rejected in favor of direct, versioned SQL isolated in the store.

**Choice:** Option A. It is the smallest reversible architecture that preserves Ports & Adapters, explicit durability ordering, forward migrations, and future read concurrency.

## Trickiest unit

The indexed frame append is the highest-risk unit. Segment bytes and SQLite cannot share one transaction, so correctness depends on an asymmetric protocol rather than an impossible atomic commit: complete record first, one SQL transaction second, orphan payload on failure, never a dangling index claim. The implementation must also carry rotation/seal state into the same SQL transaction and serialize racing appends so a later ordinal is not temporarily committed ahead of an earlier one. The `IndexedRecordingSink` story implements this only after the schema and generic adapter are proven, and its fault-injection tests force the failure window.

## Implementation units

### Unit 1: Core metadata ports, observation names, and lossless gaps

**Story:** `epic-durable-browser-memory-sqlite-index-core-contracts`

**Files:**
- `crates/krometrail-core/src/timeline/observation.rs`
- `crates/krometrail-core/src/recording/gap.rs`
- `crates/krometrail-core/src/ports/{catalog,frames,gaps}.rs` (new)
- `crates/krometrail-core/src/ports/{mod,timeline}.rs`
- `crates/krometrail-core/src/{lib,recording/mod}.rs`
- mechanical `CaptureGap::new` callers in `krometrail-core`, `krometrail-cdp`, and `krometrail-store` tests

**Acceptance criteria:**
- [ ] One macro declaration generates every observation stable name, `ALL`, Serde names, and reverse lookup.
- [ ] `CaptureGap` retains declaration time losslessly and rejects impossible range/declaration ordering; CDP samples the existing clock and coalesces with the maximum observed time.
- [ ] `RecordingCatalog`, `CaptureGapStore`, and `FrameSource` are object-safe domain-only ports with the exact signatures above; timeline ordering is documented.
- [ ] Core port source guards still exclude runtime, transport, filesystem, and database types.

### Unit 2: Versioned schema, codecs, and startup

**Story:** `epic-durable-browser-memory-sqlite-index-schema-migrations`

**Files:**
- workspace/store `Cargo.toml` and `Cargo.lock`
- `crates/krometrail-store/src/index/{mod,codec,migrations,schema_v1}.rs` (new)
- `crates/krometrail-store/src/lib.rs`
- `crates/krometrail-store/tests/sqlite_schema.rs` (new)

**Acceptance criteria:**
- [ ] Exact `rusqlite` 0.33.0 bundled/no-default dependency is locked and no database dependency enters core.
- [ ] A file-backed open verifies WAL, foreign keys, FULL synchronous mode, busy timeout, and schema v1.
- [ ] Fresh migration, idempotent reopen, rollback on a forced migration failure, and future-version refusal are deterministic.
- [ ] Every id/u64/i128 codec round-trips boundary values and unsigned BLOB ordering matches Rust ordering.
- [ ] Schema has all v1 tables/indexes above and no browser-event raw-payload column.

### Unit 3: Timeline, catalog, and structured gap adapter

**Story:** `epic-durable-browser-memory-sqlite-index-timeline-catalog`

**Files:**
- `crates/krometrail-store/src/index/{catalog,gaps,timeline}.rs` (new)
- `crates/krometrail-store/src/index/mod.rs`
- `crates/krometrail-store/tests/sqlite_timeline.rs` (new)
- core fake-port tests updated to the focused port surface

**Acceptance criteria:**
- [ ] Session/target placeholders preserve foreign keys and later generated core JSON upserts round-trip without fabricating missing metadata.
- [ ] Every `ObservationKind` round-trips through its registry-derived name; frame/capture-gap writes through generic `append` are rejected.
- [ ] Inclusive range results follow the exact deterministic order, including tied frame times ordered by capture ordinal.
- [ ] A capture gap and its timeline row commit or roll back together; overlap queries preserve all gap fields and deterministic `(start,end,id)` order.
- [ ] SQL failures expose only `PersistenceFailed` and never include values, SQL, paths, or driver text.

### Unit 4: Indexed frame writes and address-backed reads

**Story:** `epic-durable-browser-memory-sqlite-index-indexed-recording`

**Files:**
- `crates/krometrail-store/src/recording.rs` (new)
- `crates/krometrail-store/src/segments/{writer,scanner,mod}.rs`
- `crates/krometrail-store/src/index/{frames,segments}.rs` (new)
- `crates/krometrail-store/src/{index/mod,lib}.rs`
- `crates/krometrail-store/tests/indexed_recording.rs` (new)
- existing segment-writer tests updated to the payload primitive

**Acceptance criteria:**
- [ ] Production `RecordingSink` is the indexed facade; raw segment writing has no public unsupported gap implementation.
- [ ] A complete record precedes one atomic segment/frame/timeline transaction; rotation registration and frame row agree.
- [ ] Forced SQL failure after append leaves a readable orphan record and no frame/timeline claim; no failure path can create a dangling claim.
- [ ] Gap append writes no segment bytes and is queryable immediately with full metadata.
- [ ] Reads by ids preserve input order; reads by range use capture ordinal; both seek only the addressed record from open or sealed files and verify identity/context/CRC.
- [ ] Racing target appends and flushes converge deterministically without lock inversion or partially committed metadata.

### Unit 5: Maintenance primitives, composition, and qualification

**Story:** `epic-durable-browser-memory-sqlite-index-maintenance-qualification`

**Files:**
- `crates/krometrail-store/src/index/maintenance.rs` (new)
- `crates/krometrail-store/src/index/mod.rs`
- `src/app.rs`
- root/core/store tests and `crates/krometrail-store/tests/sqlite_maintenance.rs` (new)

**Acceptance criteria:**
- [ ] Frame-row removal, empty-segment removal, artifact-row removal, and usage upsert/removal are immediate, transactional, idempotent where stated, and preserve foreign-key integrity.
- [ ] Root opens `<data_dir>/index.sqlite3` before capture, injects one shared index into timeline/catalog/gap/frame ports, and injects `IndexedRecordingSink` into CDP; `UnavailableTimelineStore` is deleted.
- [ ] Migration/open failure prevents runtime construction; flush failure cannot be reported as success.
- [ ] Two handles contend only up to the configured busy timeout; cancelled-before-poll operations do nothing and once-polled transactions complete or roll back.
- [ ] Deterministic file-backed tests cover reopen persistence, WAL operation, equal-time ordering, open→sealed reads, corruption/missing files, and source-safe failures.
- [ ] Locked workspace format/check/test/clippy gates pass.

## Implementation order

```text
core-contracts
    ↓
schema-migrations
    ↓
timeline-catalog
    ↓
indexed-recording
    ↓
maintenance-qualification
```

One feature owner should carry all checkpoints. The chain is sequencing and durable acceptance evidence, not five worker assignments.

## Simplification and elimination

- Remove `UnavailableTimelineStore` from root composition.
- Remove `RecordingSink` from the payload-only `SegmentWriter`; one production facade owns frame plus metadata persistence, so the temporary unsupported gap path disappears instead of being wrapped forever.
- Keep one observation registry and one generic timeline table; no per-kind SQL tables except the current structured `CaptureGap` record.
- Keep SQL codecs and maintenance inside `krometrail-store`; no SQL-shaped core DTOs or god storage port.
- Keep one connection and no pool/ORM/cache feature until measured concurrency requires it.
- Do not add raw browser-event payload storage before a sanitized domain contract exists.

## Testing

- **Schema/migration interface:** temp-file migration/reopen/future-version/forced-failure tests protect startup and forward-only compatibility.
- **Boundary codecs:** UUID and full-range numeric ordering/round-trip tests protect silent truncation, especially values above `i64::MAX`.
- **Timeline contract:** registry exhaustiveness and tied-time fixtures protect stable kind encoding and deterministic generic ordering.
- **Gap regression:** a real hidden/saturation-style gap through `IndexedRecordingSink` protects the removal of the temporary `Unsupported` path and lossless interval metadata.
- **Durability fault test:** append a valid segment record, force the frame-index transaction to abort, and assert readable orphan bytes with no frame/timeline rows. This protects the only safe cross-resource failure direction.
- **Frame-source interface:** id-order and capture-range reads across open/sealed segments protect address resolution without loading whole segments.
- **Maintenance interface:** partial-tail frame-row removal and segment refusal-with-live-frames protect recovery/retention composition.
- **Concurrency/cancellation:** bounded lock contention, race ordering, and transaction rollback tests protect shutdown and multi-target behavior.
- No test is added per SQL statement, getter, or trivial wrapper; SQL shape is tested through stable adapter behavior plus one schema inventory assertion.

## Risks

- **Cross-resource atomicity is ordered, not absolute.** SQLite and segment files cannot commit atomically. The design guarantees orphan payloads rather than false index claims; recovery must reconcile them. The feature must not describe this as a distributed transaction.
- **Per-frame power-loss durability remains tiered.** `SegmentWriter` flushes complete records before index claim and syncs at seal/flush, matching the completed segment feature. A sudden power loss can still remove an open tail after a WAL commit; recovery's checksum scan removes any resulting claim. Changing to per-frame `sync_data` requires measured performance evidence and is not hidden here.
- **One synchronous connection can delay the async capture worker.** Transactions are small and bounded, matching the current blocking writer. If measurement shows contention, a later adapter-internal read connection can be added without changing ports or schema.
- **Identity placeholders may outlive failed lifecycle registration.** They explicitly carry `record_json = NULL`, so they do not fabricate metadata. Recovery/retention can identify them; catalog upsert completes them when lifecycle wiring exists.
- **Bundled dependency is intentionally older than latest.** Exact 0.33.0 is selected to honor Rust 1.85. Upgrading requires an MSRV probe and normal dependency review, not an unconstrained semver range.
- **Future structured payload migrations must preserve redaction.** No raw payload path exists now. The owning sibling feature must supply a sanitized type and migration; a generic bytes escape hatch remains forbidden.

## Handoff to dependent features

- **Retention:** use `segments`, `pins`, `artifacts`, and `usage` plus the maintenance primitives; own candidate policy, physical deletion ordering, pin behavior, artifact provenance decisions, session deletion, and status.
- **Recovery:** use segment registrations, `remove_frame_rows`, `remove_segment`, frame upsert internals, and usage updates; own open-file scan/truncate/seal, orphan insertion/removal policy, idempotence, and crash simulation.
- **Range resolution:** consume `TimelineStore`, `CaptureGapStore`, and `FrameSource`; do not introduce a second frame query or ordering path.
- **Browser operation/events:** index interaction/browser observations generically now; add structured storage only after their core record types and sanitizers exist.
- **Temporal debugging:** add the artifact-manifest writer against the existing `artifacts`/`artifact_frames` schema after the manifest type lands; do not redeclare it here.
