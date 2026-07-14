---
id: epic-temporal-debugging-workflow-resolved-temporal-queries
kind: feature
stage: review
tags: [storage, browser, agent-ux]
parent: epic-temporal-debugging-workflow
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Resolved Temporal Queries

## Brief

Deliver the application-facing temporal query boundary that turns explicit ranges, interaction-relative windows, recent interactions, markers, navigations, and source-frame anchors into one exact retained interval. Every request resolves once through the existing `TemporalRangeResolver` and returns the existing `ResolvedRange`, including requested and retained bounds, ordered source-frame identities, related timeline identities, declared gaps, and retention warnings.

Make interaction-relative querying operational rather than nominal by durably projecting the existing browser-operation interaction anchors and required navigation or marker timeline points into the store surfaces already consumed by range resolution. The browser executor remains the authority for action timing and identity; this feature persists and reads those same contracts rather than inferring timings from MCP responses or inventing a second interaction model.

This feature owns query validation, anchor resolution, and durable anchor availability. It does not decode frames, generate artifacts, correlate browser events, expose MCP routes, replay actions, or compare sessions.

## Epic context

- Parent epic: `epic-temporal-debugging-workflow`
- Position in epic: foundation capability — artifact generation and context evidence consume its once-resolved ranges

## Simplification opportunity

- Replace the current deliberate `InteractionAnchorSource` absence with one durable projection of the existing interaction contract, and keep all anchor forms behind the existing resolver. Do not add per-tool range parsing, a second `ResolvedRange`, or an in-memory production anchor cache.

## Foundation references

- `docs/SPEC.md` — Action Timeline, Temporal Ranges, and Errors and Degraded Operation
- `docs/ARCHITECTURE.md` — Interaction Execution, Temporal Range Resolution, and Recording Store
- `docs/VISUAL-EVIDENCE.md` — Progressive Detail and Markers

## Grounding and dispatch

- **Driver:** active autopilot `--all`; ambiguities were resolved from repository evidence without questions.
- **Dispatch:** direct-read only. The caller prohibited nested agents and peer review. Grounding covered the parent epic, the archived durable-memory range-resolution and retention designs, the archived browser-operation design/review, all five foundation documents, project rules/conventions, the CDP transport reference, and representative core/store/CDP/MCP/root source and tests.
- **Current seams:** `TemporalRangeResolver`, `TemporalRangeAnchor`, `RangeResolutionOptions`, and `ResolvedRange` already exist in core; `SqliteIndex` deliberately returns no interaction anchors; browser actions return `InteractionRecord`/`InteractionAnchor`; generic marker/navigation observations already have typed payloads; `RecordingStore` already owns the cross-file/SQLite mutation gate.
- **UI:** no human screen or journey exists. Mockups are intentionally skipped.
- **Review weight:** standard at implementation review. Design-time advisory review was skipped by explicit caller boundary.

## Design decisions

- **Application request:** Add one `TemporalQueryRequest` that directly embeds the existing `TemporalRangeAnchor` plus retention and capture-gap policy. It is the only application-facing range request; MCP and artifact features will consume it instead of parsing ranges again.
- **Implicit interaction policy:** Omitted interaction/latest-interaction windows remain exactly 150 ms before `started_at` and 250 ms after `observed_at.unwrap_or(completed_at)`. The request cannot replace this default. Explicit natural-anchor windows use whole milliseconds and are limited to 120 seconds on each side.
- **Default policy:** `RequireComplete` retention and `Include` capture gaps. Partial edge eviction requires explicit `AllowPartial`; known gaps remain visible by default and can be rejected explicitly.
- **Interaction authority:** Persist the existing `InteractionAnchor` and, where browser control already produces it, the exact existing `InteractionRecord`. Page lifecycle/navigation operations have an anchor but no `InteractionRecord`; the database row therefore has an optional record projection rather than fabricating one.
- **Response fence:** Production state-changing operations require an injected inward-facing `InteractionEvidenceSink`. The CDP executor commits the returned anchor/record before publishing a standalone result and before a batch advances past that step. MCP never persists interactions.
- **Persistence failure:** Browser effects and SQLite cannot be atomic. If the effect occurred but the evidence transaction fails, return `PersistenceFailed` with the interaction context, `RetryAdvice::Never`, and recovery instructing the caller to inspect current page/recording state rather than repeat the action blindly. There is no degraded success whose anchor is unqueryable.
- **Transaction owner:** `RecordingStore` implements the sink under its existing mutation gate. One SQLite immediate transaction writes the interaction projection, deduplicated interaction-boundary observations, and an optional successful explicit-navigation observation.
- **Navigation and markers:** Successful explicit navigate/reload/back/forward operations mint an existing `NavigationId` and project one generic `ObservationKind::Navigation` point at completion. Marker producers continue to append typed `ObservationKind::Marker` points through the generic timeline port. No navigation/marker payload table or browser-event correlation is added.
- **Latest tie order:** Latest interaction is ordered by effective observation (`observed_at` or `completed_at`), then completion, dispatch, start, and finally interaction UUID bytes, all descending, within one exact session and target.
- **Retention truth:** Segment eviction preserves compact coalesced per-target eviction ranges while deleting frame payload/index rows. Interaction/navigation/marker anchors remain queryable until session deletion, so the resolver can distinguish “anchor exists but evidence was evicted” from “no frame was ever captured in that interval.”
- **Continuity:** `AllowPartial` may trim only an evicted prefix or suffix. An evicted hole between returned frames is rejected because one `ResolvedRange` cannot honestly represent disjoint retained intervals. Capture gaps remain separate declared evidence and are never inferred from ordinals or eviction.
- **Metadata-only resolution:** The resolver reads frame metadata/identities, not encoded frame payload bytes. This removes unnecessary segment I/O and keeps frame decoding/artifact work outside this feature.
- **Query consistency:** `RecordingStore` holds its mutation gate for one application query while the core service performs metadata reads. Eviction, session deletion, frame append, marker append, and interaction projection cannot interleave and make the returned frame identities stale before resolution completes.
- **Sanitization:** The store serializes only browser-produced `InteractionRecord::sanitized_parameters`; it never reconstructs parameters from requests or MCP responses, never logs record JSON, and stores no URLs/labels for navigation/marker points. Fill text, prompt text, and directory paths retain the existing browser-control redaction guarantees.

## Architectural choice

### Option A — persist in MCP after response projection

A tool handler could inspect `BrowserOperationResult` and append an interaction row. It is locally convenient, but batches, non-MCP callers, and future application services could bypass persistence; a successful response could escape first; and protocol code would own transaction ordering. Rejected.

### Option B — let CDP write SQLite directly

The browser executor could receive `SqliteIndex` and issue inserts. This would preserve timing proximity but leak infrastructure into the browser adapter, bypass `RecordingStore` deletion/retention serialization, and make tests depend on SQLite. Rejected.

### Option C — one core sink, one store transaction, and one core query service (chosen)

Core defines `InteractionEvidenceSink`, `InteractionRecordSource`, `TemporalQuery`, and `TemporalQueryRequest`. The production CDP executor sends existing anchor/record values through the sink before returning; `RecordingStore` owns the durable transaction and query mutation gate; `SqliteIndex` implements focused reads; `TemporalQueryService` delegates every request to the existing `TemporalRangeResolver`. Root wiring joins them. This is the smallest design that keeps browser timing authoritative, persistence durable, MCP thin, and range policy singular.

An asynchronous outbox/eventual projection was also rejected: it would improve action latency but directly violate the requirement that a successful operation already have a queryable anchor.

## Trickiest unit: browser side effect to durable query fence

The hard boundary is not SQL decoding; it is ordering a non-transactional browser effect with transactional metadata for standalone and batch operations. The implementation must centralize the fence in the shared CDP `execute_operation` path, not duplicate it in action handlers or MCP:

```rust
// crates/krometrail-core/src/ports/range.rs
pub trait InteractionEvidenceSink: Send + Sync {
    fn append_operation_evidence(
        &self,
        anchor: InteractionAnchor,
        record: Option<InteractionRecord>,
        persisted_at: ObservedTime,
        navigation_id: Option<NavigationId>,
    ) -> PortFuture<'_, Result<()>>;
}

pub trait InteractionRecordSource: Send + Sync {
    fn interaction_record(
        &self,
        interaction_id: InteractionId,
    ) -> PortFuture<'_, Result<Option<InteractionRecord>>>;
}
```

Ordering is exact:

1. CDP allocates the existing interaction identity and performs the browser operation.
2. Browser control finalizes its existing `InteractionAnchor` and optional `InteractionRecord` from authoritative session timing.
3. For a successful explicit navigation only, CDP allocates a `NavigationId`; it does not infer browser-event causality.
4. `InteractionEvidenceSink::append_operation_evidence` commits the interaction row plus timeline points under the `RecordingStore` mutation gate.
5. Only after commit does standalone execution return or a batch step become successful and allow the next step.
6. If step 4 fails, the current step is a persistence failure; default batch policy stops later steps. No automatic browser retry occurs.

The sink deduplicates equal timing points and writes one `InteractionBoundary` observation for every distinct value among start, dispatch, completion, and optional observation. A navigation observation uses completion time. All observations carry the same persisted-at daemon `ObservedTime`; source time remains absent.

`ProductionBrowserConnector` without a sink rejects state-changing work before dispatch. Read-only browser operations remain available. Production root always supplies the shared `RecordingStore`; tests supply a deliberate recording fake rather than a production no-op or in-memory cache.

## Implementation units

### Unit 1: validated query and availability contracts

**Story:** `epic-temporal-debugging-workflow-resolved-temporal-queries-core-query-contracts`

**Files:**

- `crates/krometrail-core/src/timeline/query.rs` (new)
- `crates/krometrail-core/src/timeline/{range.rs,mod.rs}`
- `crates/krometrail-core/src/ports/{range.rs,frames.rs,mod.rs}`
- `crates/krometrail-core/src/lib.rs`

Define the application boundary and focused availability/evidence ports:

```rust
pub const MAX_NATURAL_ANCHOR_WINDOW: Duration = Duration::from_secs(120);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct InteractionWindow {
    before: Duration,
    after: Duration,
}

impl InteractionWindow {
    pub fn new(before: Duration, after: Duration) -> Result<Self>;
    pub const fn before(self) -> Duration;
    pub const fn after(self) -> Duration;
}
// Serde wire: { "before_ms": <u64>, "after_ms": <u64> }.
// Deserialize rejects fractional/non-integer values, unknown fields, and either side >120s.

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TemporalQueryRequest {
    pub anchor: TemporalRangeAnchor,
    pub retention: RetentionPolicy,
    pub capture_gaps: CaptureGapPolicy,
}

impl TemporalQueryRequest {
    pub fn new(
        anchor: TemporalRangeAnchor,
        retention: RetentionPolicy,
        capture_gaps: CaptureGapPolicy,
    ) -> Result<Self>;

    pub fn strict(anchor: TemporalRangeAnchor) -> Result<Self>;
    pub fn options(&self) -> RangeResolutionOptions;
}

pub trait TemporalQuery: Send + Sync {
    fn resolve_range(
        &self,
        request: TemporalQueryRequest,
    ) -> PortFuture<'_, Result<ResolvedRange>>;
}

pub struct TemporalQueryService<C, F, G, T, I> {
    resolver: TemporalRangeResolver<C, F, G, T, I>,
}

impl<C, F, G, T, I> TemporalQueryService<C, F, G, T, I> {
    pub const fn new(resolver: TemporalRangeResolver<C, F, G, T, I>) -> Self;
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FrameAvailability {
    pub retained_bounds: Option<SessionRange>,
    pub evicted_ranges: Vec<SessionRange>,
}

impl FrameAvailability {
    pub fn new(
        retained_bounds: Option<SessionRange>,
        evicted_ranges: Vec<SessionRange>,
    ) -> Result<Self>;
}
```

`TemporalQueryRequest::new` validates the anchor through the same constructors the resolver consumes: explicit session/wall-clock ranges require session+target scope; interaction/navigation/marker optional windows are bounded; latest interaction always carries exact session+target; source-frame endpoints are non-nil typed IDs. It does not query storage and does not duplicate anchor resolution.

Extend `FrameSource` with metadata-only methods and availability:

```rust
fn frame_metadata_in_range(
    &self,
    session_id: SessionId,
    target_id: TargetId,
    range: SessionRange,
) -> PortFuture<'_, Result<Vec<CapturedFrame>>>;

fn frame_metadata_in_ordinal_range(
    &self,
    session_id: SessionId,
    target_id: TargetId,
    start: CaptureOrdinal,
    end: CaptureOrdinal,
) -> PortFuture<'_, Result<Vec<CapturedFrame>>>;

fn frame_availability(
    &self,
    session_id: SessionId,
    target_id: TargetId,
) -> PortFuture<'_, Result<FrameAvailability>>;
```

Refine `TemporalRangeResolver` without changing its public result type:

- use metadata-only reads for all anchor forms;
- treat a request inside retained bounds with no intersecting eviction tombstone as complete even when no frame lands exactly on each requested endpoint;
- classify empty evidence as evicted when a tombstone intersects, otherwise never captured;
- allow explicit partial retention only for a contiguous edge-trimmed interval;
- reject internal eviction holes;
- add `RetentionWarning::EvictedRanges { ranges: Vec<SessionRange> }` while retaining exact requested/resolved bounds;
- continue deterministic frame ordering by capture ordinal and related identity ordering by generic timeline order.

**Acceptance criteria:**

- [ ] The request and every anchor variant round-trip through validated Serde; millisecond windows enforce 0..=120s per side and omitted interaction windows produce exactly 150ms/250ms options.
- [ ] Core request/service/availability/evidence ports expose only core/std types and remain object-safe where declared as ports.
- [ ] The resolver no longer reads encoded payloads to produce frame identities.
- [ ] Complete, edge-evicted, internally-evicted, never-captured, wrong-scope, and gapped contracts are deterministic and source-safe.

### Unit 2: SQLite interaction projection and eviction memory

**Story:** `epic-temporal-debugging-workflow-resolved-temporal-queries-durable-anchor-index`

**Files:**

- `crates/krometrail-store/src/index/schema_v3.rs` (new)
- `crates/krometrail-store/src/index/{migrations.rs,interactions.rs,range.rs,frames.rs,timeline.rs,deletion.rs,mod.rs}`
- `crates/krometrail-store/src/recording.rs`
- `crates/krometrail-store/tests/temporal_query_index.rs` (new)

Schema v3 is additive and transactional:

```sql
CREATE TABLE interactions (
    interaction_id BLOB PRIMARY KEY CHECK(length(interaction_id) = 16),
    session_id BLOB NOT NULL CHECK(length(session_id) = 16),
    target_id BLOB NOT NULL CHECK(length(target_id) = 16),
    operation TEXT NOT NULL,
    started_time_be BLOB NOT NULL CHECK(length(started_time_be) = 8),
    dispatched_time_be BLOB NOT NULL CHECK(length(dispatched_time_be) = 8),
    completed_time_be BLOB NOT NULL CHECK(length(completed_time_be) = 8),
    observed_time_be BLOB NULL CHECK(observed_time_be IS NULL OR length(observed_time_be) = 8),
    record_json TEXT NULL,
    FOREIGN KEY(session_id, target_id) REFERENCES targets(session_id, target_id) ON DELETE CASCADE
) STRICT;

CREATE INDEX interaction_latest_idx ON interactions(
    session_id, target_id, observed_time_be, completed_time_be,
    dispatched_time_be, started_time_be, interaction_id
);

CREATE TABLE evicted_frame_ranges (
    eviction_id INTEGER PRIMARY KEY,
    session_id BLOB NOT NULL CHECK(length(session_id) = 16),
    target_id BLOB NOT NULL CHECK(length(target_id) = 16),
    start_time_be BLOB NOT NULL CHECK(length(start_time_be) = 8),
    end_time_be BLOB NOT NULL CHECK(length(end_time_be) = 8),
    CHECK(start_time_be <= end_time_be),
    UNIQUE(session_id, target_id, start_time_be, end_time_be),
    FOREIGN KEY(session_id, target_id) REFERENCES targets(session_id, target_id) ON DELETE CASCADE
) STRICT;

CREATE INDEX evicted_frame_range_idx ON evicted_frame_ranges(
    session_id, target_id, start_time_be, end_time_be, eviction_id
);

CREATE UNIQUE INDEX navigation_anchor_id_idx
ON timeline_observations(kind, payload_sort_key) WHERE kind='navigation';
CREATE UNIQUE INDEX marker_anchor_id_idx
ON timeline_observations(kind, payload_sort_key) WHERE kind='marker';
CREATE UNIQUE INDEX interaction_boundary_point_idx
ON timeline_observations(kind, payload_sort_key, session_time_be)
WHERE kind='interaction_boundary';
```

`SqliteIndex` implements `InteractionAnchorSource` from structured columns, `InteractionRecordSource` from validated `record_json`, and metadata availability queries. The record is optional because page lifecycle operations currently return only an anchor. When present, decode must prove `record.anchor()? == stored_anchor`; mismatches are `PersistenceFailed`.

`append_operation_evidence_tx` validates and commits one exact projection. Repeating the same interaction ID with byte-equivalent domain values is idempotent; conflicting reuse is `PersistenceFailed`. Latest ordering uses:

```sql
ORDER BY COALESCE(observed_time_be, completed_time_be) DESC,
         completed_time_be DESC,
         dispatched_time_be DESC,
         started_time_be DESC,
         interaction_id DESC
LIMIT 1
```

Before an eviction transaction removes a segment's frame rows, it records the segment's minimum/maximum frame time as an evicted range and coalesces overlapping ranges for that session/target. Explicit session deletion does not retain tombstones; cascading deletion removes interactions and eviction ranges together with targets/session. Compact anchors and tombstones remain through ordinary segment eviction so queries can explain lost evidence.

`RecordingStore` implements `InteractionEvidenceSink`; it also becomes the production `TimelineStore` writer so marker/navigation writes use the same mutation gate and cannot race session deletion. Timeline reads delegate to `SqliteIndex`.

**Acceptance criteria:**

- [ ] Fresh and v2 databases migrate to v3 in one transaction; future versions still refuse.
- [ ] Existing `InteractionAnchor` values and optional exact `InteractionRecord` values round-trip without a copied persistence model.
- [ ] Equal-time latest interactions choose the documented UUID tie-break; exact-id lookup is independent of insertion order.
- [ ] Interaction boundary rows and optional navigation row commit atomically and idempotently with the interaction projection.
- [ ] Eviction ranges survive segment removal, coalesce deterministically, distinguish evicted from never-captured intervals, and disappear on session deletion.
- [ ] SQL/Serde/record failures map to source-safe `PersistenceFailed`; record JSON and sanitized values never enter logs or stable errors.

### Unit 3: CDP persistence fence and navigation points

**Story:** `epic-temporal-debugging-workflow-resolved-temporal-queries-operation-persistence-ordering`

**Files:**

- `crates/krometrail-cdp/src/session/evidence.rs` (new)
- `crates/krometrail-cdp/src/session/{mod.rs,operations.rs}`
- `crates/krometrail-cdp/src/control/batch.rs`
- `crates/krometrail-cdp/tests/temporal_evidence.rs` (new)

Add an optional sink to `ProductionBrowserConnector`, configured separately from frame capture:

```rust
pub fn with_interaction_evidence(
    mut self,
    sink: Arc<dyn InteractionEvidenceSink>,
) -> Self;
```

The central non-batch return path calls one helper:

```rust
async fn persist_result_evidence(
    result: &BrowserOperationResult,
    sink: &dyn InteractionEvidenceSink,
    clock: &dyn MonotonicClock,
    ids: &dyn IdSource,
) -> Result<()>;
```

The helper handles only state-changing results:

- page operation: clone its existing `InteractionAnchor`, no record; allocate a navigation ID only for `Succeeded(Navigated | Reloaded | WentBack | WentForward)`;
- action operation: derive the anchor with `InteractionResult::anchor()` and clone the exact record;
- read-only/wait/screenshot/evaluation: no write;
- batch: never re-project the outer result, because every child recursively traverses the same non-batch path and is committed before its `DispatchOutcome::Completed`.

A missing sink rejects a state-changing request before CDP dispatch. A sink failure after dispatch is remapped to a persistence error carrying session/target/interaction, retry `Never`, and recovery “inspect the current page and recording status before deciding whether to repeat the action.” It is not rewritten as an interaction failure and is never automatically retried.

**Acceptance criteria:**

- [ ] Standalone state-changing execution does not return before its sink future commits; read-only execution does not call the sink.
- [ ] Batch step N is persisted before step N+1 dispatches; a persistence failure is a failed step and obeys existing stop/continue policy without duplicating prior evidence.
- [ ] All page/action operation result variants project exactly once; batch IDs remain parent references in existing records and do not become a second interaction row/model.
- [ ] Only successful explicit navigation operations mint navigation IDs; click-driven/browser-event correlation remains absent.
- [ ] Missing/failing sinks produce explicit no-auto-retry failures, while deterministic test sinks prove no browser operation result outruns persistence.

### Unit 4: application service and composition

**Story:** `epic-temporal-debugging-workflow-resolved-temporal-queries-query-service-composition`

**Files:**

- `crates/krometrail-store/src/recording.rs`
- `crates/krometrail-store/src/lib.rs`
- `src/app.rs`
- existing root composition tests

`RecordingStore` implements `TemporalQuery` by acquiring its mutation gate and constructing the core service over its one `SqliteIndex`:

```rust
impl TemporalQuery for RecordingStore {
    fn resolve_range(
        &self,
        request: TemporalQueryRequest,
    ) -> PortFuture<'_, Result<ResolvedRange>>;
}
```

Inside the guarded future, it calls:

```rust
TemporalQueryService::new(TemporalRangeResolver::new(
    Arc::clone(&self.index), // RecordingCatalog
    Arc::clone(&self.index), // FrameSource metadata
    Arc::clone(&self.index), // CaptureGapStore
    Arc::clone(&self.index), // TimelineStore + TimelineAnchorSource
    Arc::clone(&self.index), // InteractionAnchorSource
))
.resolve_range(request)
.await
```

Root keeps the concrete shared `Arc<RecordingStore>` long enough to wire it as recording, retention, timeline writer, interaction-evidence sink, and temporal-query service. It wires `SqliteIndex` only as focused read adapters. `RuntimeDependencies` gains `temporal_queries: Arc<dyn TemporalQuery>` for later temporal consumers, but `build_service` and MCP routing do not change in this feature.

**Acceptance criteria:**

- [ ] Production browser, recording, timeline writes, and temporal query all share one store/index authority; no no-op or memory anchor adapter is root-wired.
- [ ] A query holds the mutation gate through resolution, so frame eviction/session deletion cannot invalidate its returned identities mid-resolution.
- [ ] Startup migration failure prevents browser dispatch and query service construction.
- [ ] MCP still receives only its current browser-control dependencies and contains no SQL, sink call, temporal route, or resource implementation.

### Unit 5: qualification across operation, store, and resolver

**Story:** `epic-temporal-debugging-workflow-resolved-temporal-queries-qualification`

**Files:**

- `crates/krometrail-core/src/timeline/{query.rs,range.rs}` tests
- `crates/krometrail-store/tests/temporal_queries.rs` (new)
- `crates/krometrail-cdp/tests/temporal_evidence.rs`
- `src/app.rs` tests

Build one focused store fixture with a real v3 SQLite database/segment writer, two targets, tied frame times, interactions, one explicit navigation, one marker, a known capture gap, and forced segment eviction. Add a scripted browser integration whose sink is the same `RecordingStore`; execute an operation, then immediately query its returned interaction ID through `TemporalQuery`.

**Acceptance criteria:**

- [ ] Constructor/Serde tests cover `TemporalQueryRequest`, all seven existing anchor forms (session time, wall clock, source frame, interaction, latest interaction, navigation, marker), policies, unknown fields, malformed scope/ranges, and bounded whole-millisecond windows.
- [ ] Migration/round-trip tests cover anchor-only page operations, action records including parent batch and sanitized parameters, navigation/marker timeline points, and source-safe corrupted-row rejection.
- [ ] Ordering tests cover equal interaction times/latest UUID tie-break, equal timeline times, and source-frame capture-ordinal order.
- [ ] Implicit interaction queries prove exact 150ms pre-start through observed/completed plus 250ms trailing context and return those effective options.
- [ ] Retention tests distinguish fully evicted, edge-partially-evicted, internal eviction hole, never-captured interval, and session deletion; only explicit `AllowPartial` accepts a contiguous evicted edge.
- [ ] Wrong-session/target tests cover interaction, source-frame, navigation, and marker anchors; gap include/reject tests preserve full declared gap records.
- [ ] Operation-to-query integration proves a successful standalone action and each successful batch step are immediately queryable; a delayed sink blocks publication and a failing sink prevents a success response/next default-policy step.
- [ ] Tests inspect persisted fill/dialog/upload records and prove fill text, prompt text, and directory components are absent while permitted metadata remains.
- [ ] Locked format, workspace check/test, and Clippy gates pass. No test is added for trivial wrappers, getters, each SQL statement, or MCP behavior outside this feature.

## Implementation order

1. `epic-temporal-debugging-workflow-resolved-temporal-queries-core-query-contracts`
2. `epic-temporal-debugging-workflow-resolved-temporal-queries-durable-anchor-index` — depends on core contracts
3. `epic-temporal-debugging-workflow-resolved-temporal-queries-operation-persistence-ordering` — depends on the durable sink/index
4. `epic-temporal-debugging-workflow-resolved-temporal-queries-query-service-composition` — depends on store and CDP ordering
5. `epic-temporal-debugging-workflow-resolved-temporal-queries-qualification` — depends on the composed path

One feature owner should carry these checkpoints as one cohesive implementation/review bundle. They expose durability order and acceptance evidence, not five worker assignments.

## Simplification and elimination

- Keep `TemporalRangeAnchor`, `TemporalRangeResolver`, and `ResolvedRange`; add no range parser, `ResolvedRange` copy, timeline authority, or production memory cache.
- Replace the deliberate empty `InteractionAnchorSource` adapter with real structured reads.
- Stop loading encoded frame bytes merely to return frame IDs; metadata-only resolver reads remove segment I/O from the query boundary.
- Use one `interactions` projection with optional exact record JSON; do not create separate action/page/batch interaction models or tables.
- Use generic timeline rows for interaction boundaries, navigations, and markers; do not create navigation/marker payload tables in this feature.
- Reuse `RecordingStore`'s mutation gate for interaction writes, marker writes, retention, deletion, and coherent queries instead of adding another lock/coordinator.
- Keep MCP unchanged. The final MCP feature imports the application query port rather than inheriting hidden persistence work.

## Testing strategy

- **Core boundary:** all anchor/request Serde and meaningful validation protect the future generated API contract.
- **Complex policy unit:** availability classification, contiguous partial retention, internal holes, implicit windows, and gap policies protect the resolver's hardest semantics.
- **Store interface:** v2→v3 migration, exact interaction round-trip, latest ordering, timeline atomicity, and eviction tombstones protect durable queryability.
- **Ordering regression:** delayed/failing sinks and multi-step batches protect the response fence around irreversible browser effects.
- **End-to-end seam:** operation result → same-store interaction query protects the product workflow this feature exists to enable.
- **Redaction regression:** query raw persisted JSON only in tests to prove existing sanitization survives storage; do not snapshot entire records or schemas.
- Retire/update the old range test asserting interaction anchors are always absent. Do not add wrapper delegation tests or assertions that mirror implementation-only SQL text.

## Risks and rollback

- **Browser effect cannot roll back with SQLite.** A process crash can occur after the browser effect but before commit. The response fence prevents a false published success, and persistence failure is non-retryable by default. Rollback/fallback is to reject state-changing operations when no sink is healthy while keeping read-only inspection available; never silently restore the old no-persistence path.
- **Eviction memory can grow.** Coalescing bounds growth to retained discontinuities rather than frames; session deletion removes it. If evidence shows unacceptable index growth, replace rows with a proven interval compaction strategy without changing `FrameAvailability` or query semantics.
- **Mutation-gated queries can add write latency.** Metadata-only resolution keeps the critical section bounded and avoids segment reads. If measured contention is material, move to a store-owned read snapshot/lease behind the same `TemporalQuery` port; do not weaken the coherent-return guarantee speculatively.
- **Record JSON depends on browser sanitization.** The sink accepts only browser-produced records and validates anchor identity; focused persisted-secret tests guard the trusted seam. If a new action adds sensitive fields, its sanitizer must change before that operation can pass qualification.
- **Navigation points are intentionally narrow.** They represent successful explicit navigation controls, not nearby lifecycle/browser events or causal attribution. Later browser-event correlation can add evidence without replacing IDs/timing written here.
- **Partial retention is one interval.** Internal eviction holes cannot fit `ResolvedRange`; rejecting them is safer than flattening disjoint evidence. A future multi-range contract would be a separate external-contract decision, not an implicit extension here.

## Pre-mortem

The most likely production failure is an apparently successful click/navigation whose browser effect is visible but whose interaction anchor is absent because persistence was delayed, bypassed in a batch, or raced session deletion. That would make “inspect the last interaction” fail exactly when needed. The design attacks this at the shared execution and store-mutation boundaries: every non-batch result crosses one sink fence, batches recurse through it per step, and writes/query/deletion share one gate. The least certain area is the unavoidable crash window between browser effect and commit; no local architecture can make Chrome and SQLite one transaction. The fallback is therefore explicit uncertainty and no automatic retry, not a false atomicity claim.

## Implementation notes

- Execution capability: highest, explicitly selected by the autopilot caller for the cross-cutting temporal semantics, migration/retention, CDP ordering, and irreversible-effect failure boundary. One feature owner carried all five ordered checkpoints.
- Review weight: standard, from the caller. Implementation stops at `stage: review`; no independent feature review was run by this owner.
- Checkpoints and commits: core query contracts `27475fa`; durable anchor index `a8e3edd`; operation persistence ordering `6198b71`; query service composition `ef9bf21`; qualification `76f63e6`.
- Files changed: core query/range contracts and ports; SQLite v3 interaction/eviction schema, reads, writes, deletion, and store authority; CDP session evidence fence and deterministic tests; root composition; focused core/store/CDP qualification tests. MCP and foundation documents are unchanged.
- Tests added/updated: validated seven-anchor request wire; complete/partial/internal-hole/never-captured/gap policy; v2→v3 and corruption/replay/latest ordering; real removal tombstone coalescing; missing/delayed/failing sink and two-step batch ordering; same-store operation-to-query standalone/batch seam; persisted fill/dialog/upload redaction; session deletion. The obsolete “interaction anchors are always absent” assertion was replaced with durable resolution coverage.
- Exact semantics delivered: one validated `TemporalQueryRequest` delegates to the existing resolver/result; natural windows are whole milliseconds and bounded to 120 seconds; implicit interaction context remains exactly 150 ms before start through observed-or-completed plus 250 ms; complete retention/include-gaps is default; only contiguous tombstoned edges permit explicit partial results; metadata-only frame selection preserves capture-ordinal order; typed timeline ordering stays deterministic.
- Persistence semantics delivered: one transactional v3 projection stores the exact existing anchor and optional exact sanitized record, atomically writes distinct interaction boundaries and optional explicit-navigation points, validates idempotent replay/conflicts, orders latest ties by UUID bytes, coalesces eviction intervals before segment metadata removal, and cascades compact evidence on session deletion.
- Browser/root semantics delivered: state-changing work without a sink is rejected before dispatch; every non-batch page/action result crosses the sink before publication; batch children cross the same fence and the outer batch is not projected; post-effect commit failure is `PersistenceFailed`, retry `Never`, with inspect-before-repeat recovery; only successful explicit navigate/reload/back/forward mint navigation IDs; root uses one concrete `RecordingStore` for recording, retention, timeline writes, interaction evidence, and coherent temporal queries while MCP still receives only browser control.
- Simplification: reused the existing interaction/range/timeline models and registry; added no parser, result copy, action copy, navigation/marker table, production memory cache, or MCP persistence. `InteractionRecord::anchor()` and registry-derived operation decoding remove persistence copies.
- Design deviations/rejections: v3 transaction deduplicates historical exact marker/navigation/boundary rows before unique indexes so valid v2 stores migrate; qualification directly establishes one internal-hole post-eviction fixture while the real removal worker separately proves tombstone creation/coalescing; a scheduler-racy process-table test gained a bounded wait. No external behavior decision or foundation conflict was found.
- Adjacent issues parked: none.
- Integrated verification: under Rust 1.85.0, format check, locked workspace all-target check, locked workspace all-target tests, and locked workspace all-target Clippy `-D warnings` all pass. No live-Chrome execution was enabled or claimed.
