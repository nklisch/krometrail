---
id: epic-durable-browser-memory-range-resolution
kind: feature
stage: done
tags: [storage, browser]
parent: epic-durable-browser-memory
depends_on: [epic-durable-browser-memory-sqlite-index]
release_binding: 1.0.0
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Natural-Anchor and Explicit Temporal Range Resolution

## Brief

Own the single temporal range resolver that every temporal request passes through. It accepts natural anchors — explicit session-relative time, explicit timestamps, interaction identifier, a window before and after an interaction, the most recent interaction, navigation or marker identifier, or a source-frame range — and resolves them to one explicit `ResolvedRange` against the SQLite index before any artifact generation or source-frame retrieval runs. Concentrating resolution here prevents each downstream consumer (the sibling `epic-temporal-debugging-workflow` and `epic-temporal-vision-toolkit` epics) from interpreting natural anchors differently.

The resolver reads from the SQLite index feature (frame ids, interaction ids, navigations, markers, gaps in a window) and produces a `ResolvedRange` carrying session, target, start/end session time, ordered frame ids, interaction ids, known gaps, and retention warnings. When an unspecified interaction range is requested, the resolver applies the bounded pre-action context through the interaction lifecycle and post-action observation plus bounded trailing context, and returns the exact resolved range with every response. Queries fail clearly when all requested evidence was evicted or never captured, when an anchor belongs to another session/target, or when a strict caller asks for uninterrupted evidence across known capture gaps.

This feature does not own artifact generation, the temporal visual crate, the debug bundle composition, or progressive source retrieval. It owns the resolver and the `ResolvedRange` core type that temporal-query consumers import.

## Epic context

- Parent epic: `epic-durable-browser-memory`
- Position in epic: consumer of the SQLite-index feature; produces the `ResolvedRange` contract that the sibling `epic-temporal-debugging-workflow` epic consumes for artifact generation and progressive source retrieval.
- Design decisions inherited: the resolver lives in this epic because it depends on the storage indexes the store owns; artifact generation is a separate concern owned by the temporal-vision epic; an unspecified interaction query resolves to bounded pre-action context through the interaction lifecycle and post-action observation, plus bounded trailing context; the resolved range is returned with every response.

## Dispatch and grounding

Direct-read design only. The autopilot caller requested highest capability, standard review, and no questions/subagents/peeragent/push. Local reads covered the feature and parent epic, `.agents/rules/agile-workflow.md`, `docs/agents.md`, all five foundation documents, the completed SQLite design/implementation notes, current `krometrail-core` contracts (`ids`, time, frame, gap, interaction anchor, timeline observation, ports), current `krometrail-store` SQLite schema/query code, and current browser page-lifecycle interaction-anchor code. No UI surface exists.

## Current-state constraints found in code

- `InteractionAnchor` and `InteractionTiming` exist in `crates/krometrail-core/src/browser/control.rs` and state-changing page operations return anchors, but no production code currently persists those anchors into SQLite.
- `NavigationId` and `MarkerId` exist in the core ID registry, and `ObservationKind::{Navigation, Marker}` exist in the generic timeline registry, but no structured navigation or marker record table exists. The generic timeline can index their IDs once an owning feature writes the observations.
- SQLite schema v1 already stores `frames` with `(session_id, target_id, session_time_be, capture_ordinal_be)`, generic `timeline_observations`, structured `capture_gaps`, `sessions`, `targets`, and segment/frame address metadata. It does not yet contain retention policy rows or structured interaction/navigation/marker rows.
- `FrameSource::frames_in_range` already orders retained frames by `capture_ordinal` after filtering by session time. Source-frame-id ranges need the same frame source to add an ordinal-range read rather than introducing a second frame query path.

## Design decisions

- **One resolver and one anchor registry:** `krometrail-core::timeline::range` owns `TemporalRangeAnchorKind`, `TemporalRangeAnchor`, `RangeResolutionOptions`, and `ResolvedRange`. MCP and artifact code import these types instead of re-declaring anchor variants.
- **One session and one target per resolution:** every successful `ResolvedRange` has exactly one `SessionId` and one `TargetId`. Explicit time/timestamp anchors require both. ID anchors may derive the target from storage, but any caller-supplied session/target must match or the resolver returns `InvalidInput`.
- **Frame order is capture-ordinal order:** all frame IDs in `ResolvedRange` are ordered by the per-target `CaptureOrdinal`, with `session_time` and `FrameId` only tie-breakers inside adapter queries. Timestamp ties never reorder evidence.
- **Wall-clock timestamps convert through the recorded session start:** explicit timestamps are `SystemTime` ranges scoped to a session and target. Conversion computes checked nanosecond offsets from `RecordingSession::started_at`; timestamps before the session start are outside captured evidence, and durations that cannot fit in `u64` nanoseconds are `InvalidTime`.
- **Default implicit interaction window:** an interaction without an explicit window resolves from `started_at - 150ms` (saturating at session zero) through `observed_at.unwrap_or(completed_at) + 250ms` (checked, never wrapping). These constants are part of the resolved policy returned with the range; callers can request a bounded explicit `InteractionWindow`.
- **Gaps are explicit, not hidden:** the resolver always reads `CaptureGapStore` for the resolved interval. With `CaptureGapPolicy::Include`, gaps appear in `ResolvedRange::gaps`. With `Reject`, the same interval fails clearly instead of pretending continuity.
- **Retention is explicit, not truncation:** all-evicted or never-captured ranges return `NotFound`. Partial overlap is allowed only under `RetentionPolicy::AllowPartial`; the resolved retained subrange and `RetentionWarning` entries disclose the requested bounds that were unavailable. `RequireComplete` turns the same condition into `NotFound`.
- **No fabricated structured rows:** this feature does not invent `InteractionRecord`, navigation payload, marker payload, browser-event rows, or artifact manifests. It defines the focused lookup surfaces the resolver needs. Browser-control sibling features own writing interaction anchors/navigation/marker timeline observations; richer structured records can be added later without changing `ResolvedRange`.

## Architectural choice

### Option A — core resolver with focused storage query ports (chosen)

Define the range request/response types and deterministic resolution policy in `krometrail-core`, then implement one `TemporalRangeResolver` over focused ports: `RecordingCatalog` reads, `FrameSource`, `CaptureGapStore`, `TimelineAnchorSource`, and `InteractionAnchorSource`. `SqliteIndex` implements the storage-facing ports using the existing schema and the same frame source helpers used by source-frame retrieval.

This keeps domain policy independent of SQLite while avoiding duplicate resolver logic in MCP, artifact generation, and source-frame tools.

### Option B — resolve anchors inside each temporal-query tool

Each tool would parse interactions, markers, source frames, gaps, and retention on its own. This is initially local but guarantees drift: storyboard, difference map, source-frame retrieval, and pinning could resolve “the last interaction” or a source-frame interval differently. Rejected because it contradicts the foundation “all temporal requests pass through one range resolver” contract.

### Option C — make SQLite return a ready-made `ResolvedRange`

The adapter could collapse SQL and policy into one implementation. It would be short, but it would put session-time arithmetic, interaction-window policy, and public error semantics inside infrastructure. Rejected because it violates Ports & Adapters and makes non-SQL tests harder than necessary.

**Choice:** Option A. Core owns policy and types; store owns query mechanics. The only storage-specific additions are focused lookup ports, not a god-port.

## Trickiest unit: supportable natural anchors without pretending missing records exist

The high-risk part is interaction/window/recent resolution. `InteractionAnchor` is the exact timing shape the resolver needs, but current production code returns it to callers and does not persist it. The design therefore separates the resolver contract from record production:

```rust
pub trait InteractionAnchorSource: Send + Sync {
    fn interaction_anchor(
        &self,
        interaction_id: InteractionId,
    ) -> PortFuture<'_, Result<Option<InteractionAnchor>>>;

    fn latest_interaction_anchor(
        &self,
        session_id: SessionId,
        target_id: TargetId,
    ) -> PortFuture<'_, Result<Option<InteractionAnchor>>>;
}
```

This port returns an existing core type; it does not create a new interaction record schema. Until the browser-operation sibling writes anchors durably, `SqliteIndex` returns `Ok(None)` and interaction anchors resolve as `NotFound` with a recovery message explaining that no durable interaction anchor exists for the session. Once the sibling persists anchors, the same resolver supports:

- `Interaction { interaction_id, window: None }` using the default 150ms/250ms policy;
- `Interaction { interaction_id, window: Some(...) }` using caller-supplied before/after bounds;
- `LatestInteraction { session_id, target_id, window }` using the latest persisted anchor for exactly that target.

Navigation and marker IDs are lighter: the resolver only needs their timeline point. They can be supported by generic `TimelineObservation` rows now, without a structured navigation/marker table. Browser siblings must write `ObservationKind::Navigation` and `ObservationKind::Marker` observations with typed payloads when those events exist; this feature only looks them up.

## Implementation units

### Unit 1: Core range contracts and registry

**Files:**

- `crates/krometrail-core/src/timeline/range.rs` (new)
- `crates/krometrail-core/src/timeline/mod.rs`
- `crates/krometrail-core/src/lib.rs`
- `crates/krometrail-core/src/error.rs` (tests only unless helper constructors are useful)

**Story:** `epic-durable-browser-memory-range-resolution-core-contracts`

Define the single public range contract:

```rust
use std::time::{Duration, SystemTime};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TemporalRangeAnchorKind {
    SessionTime,
    WallClock,
    Interaction,
    LatestInteraction,
    Navigation,
    Marker,
    SourceFrame,
}

impl TemporalRangeAnchorKind {
    pub const ALL: &'static [Self];
    pub const fn as_str(self) -> &'static str;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AnchorScope {
    pub session_id: Option<SessionId>,
    pub target_id: Option<TargetId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InteractionWindow {
    pub before: Duration,
    pub after: Duration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RetentionPolicy {
    RequireComplete,
    AllowPartial,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum CaptureGapPolicy {
    Include,
    Reject,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RangeResolutionOptions {
    pub retention: RetentionPolicy,
    pub capture_gaps: CaptureGapPolicy,
    pub implicit_interaction_window: InteractionWindow,
}

impl RangeResolutionOptions {
    pub const DEFAULT_PRE_INTERACTION_CONTEXT: Duration = Duration::from_millis(150);
    pub const DEFAULT_POST_INTERACTION_CONTEXT: Duration = Duration::from_millis(250);
    pub const DEFAULT: Self;
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "anchor", rename_all = "snake_case")]
pub enum TemporalRangeAnchor {
    SessionTime { scope: AnchorScope, range: SessionRange },
    WallClock { scope: AnchorScope, start: SystemTime, end: SystemTime },
    Interaction { scope: AnchorScope, interaction_id: InteractionId, window: Option<InteractionWindow> },
    LatestInteraction { session_id: SessionId, target_id: TargetId, window: Option<InteractionWindow> },
    Navigation { scope: AnchorScope, navigation_id: NavigationId, window: Option<InteractionWindow> },
    Marker { scope: AnchorScope, marker_id: MarkerId, window: Option<InteractionWindow> },
    SourceFrame { scope: AnchorScope, start_frame_id: FrameId, end_frame_id: FrameId },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RetentionWarning {
    RequestedStartBeforeOldestRetained { requested: SessionTime, oldest_retained: SessionTime },
    RequestedEndAfterNewestRetained { requested: SessionTime, newest_retained: SessionTime },
    PartiallyEvicted { requested: SessionRange, retained: SessionRange },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResolvedRange {
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub anchor_kind: TemporalRangeAnchorKind,
    pub requested_range: SessionRange,
    pub resolved_range: SessionRange,
    pub frame_ids: Vec<FrameId>,
    pub interaction_ids: Vec<InteractionId>,
    pub navigation_ids: Vec<NavigationId>,
    pub marker_ids: Vec<MarkerId>,
    pub gaps: Vec<CaptureGap>,
    pub retention_warnings: Vec<RetentionWarning>,
    pub options: RangeResolutionOptions,
}
```

Constructor validation enforces nonempty `frame_ids`, ordered `requested_range`/`resolved_range`, `resolved_range` contained in `requested_range` when retention is partial, no duplicate IDs in each ID vector, and nonempty warnings when policy allows partial retention and the ranges differ.

**Acceptance criteria:**

- [ ] Anchor kind stable names, `ALL`, Serde, and reverse lookup come from one declaration.
- [ ] `ResolvedRange::new` rejects empty frame lists, duplicate IDs, unordered ranges, partial retention without a warning, and resolved ranges outside the request.
- [ ] `InteractionWindow` construction rejects durations that cannot convert to `u64` nanoseconds at resolution time.
- [ ] Core exports the range types from `krometrail_core::timeline` and `krometrail_core` without importing store, CDP, MCP, or temporal-vision crates.

### Unit 2: Focused resolver query ports and SQLite support

**Files:**

- `crates/krometrail-core/src/ports/catalog.rs`
- `crates/krometrail-core/src/ports/frames.rs`
- `crates/krometrail-core/src/ports/range.rs` (new)
- `crates/krometrail-core/src/ports/mod.rs`
- `crates/krometrail-store/src/index/range.rs` (new)
- `crates/krometrail-store/src/index/frames.rs`
- `crates/krometrail-store/src/index/catalog.rs`
- `crates/krometrail-store/src/index/mod.rs`
- `crates/krometrail-store/tests/range_resolution.rs` (new)

**Story:** `epic-durable-browser-memory-range-resolution-store-queries`

Extend existing focused ports instead of adding a monolithic store trait:

```rust
pub trait RecordingCatalog: Send + Sync {
    fn put_session(&self, session: RecordingSession) -> PortFuture<'_, Result<()>>;
    fn put_target(&self, session_id: SessionId, target: PageTarget) -> PortFuture<'_, Result<()>>;
    fn session(&self, session_id: SessionId) -> PortFuture<'_, Result<Option<RecordingSession>>>;
    fn target(&self, session_id: SessionId, target_id: TargetId) -> PortFuture<'_, Result<Option<PageTarget>>>;
}

pub trait FrameSource: Send + Sync {
    fn frames_by_id(&self, frame_ids: Vec<FrameId>) -> PortFuture<'_, Result<Vec<EncodedFrame>>>;
    fn frame_metadata_by_id(&self, frame_ids: Vec<FrameId>) -> PortFuture<'_, Result<Vec<CapturedFrame>>>;
    fn frames_in_range(&self, session_id: SessionId, target_id: TargetId, range: SessionRange)
        -> PortFuture<'_, Result<Vec<EncodedFrame>>>;
    fn frames_in_ordinal_range(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        start: CaptureOrdinal,
        end: CaptureOrdinal,
    ) -> PortFuture<'_, Result<Vec<EncodedFrame>>>;
}

pub trait TimelineAnchorSource: Send + Sync {
    fn observation_for_payload(
        &self,
        scope: AnchorScope,
        kind: ObservationKind,
        payload: ObservationPayloadRef,
    ) -> PortFuture<'_, Result<Option<TimelineObservation>>>;

    fn latest_observation(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        kind: ObservationKind,
    ) -> PortFuture<'_, Result<Option<TimelineObservation>>>;
}

pub trait InteractionAnchorSource: Send + Sync {
    fn interaction_anchor(&self, interaction_id: InteractionId)
        -> PortFuture<'_, Result<Option<InteractionAnchor>>>;
    fn latest_interaction_anchor(&self, session_id: SessionId, target_id: TargetId)
        -> PortFuture<'_, Result<Option<InteractionAnchor>>>;
}
```

`SqliteIndex` implements the currently supportable pieces:

- catalog reads by decoding `record_json`; `NULL` placeholders return `Ok(None)` for the structured catalog value rather than pretending a complete session/target exists;
- `frame_metadata_by_id` reuses the same ordered frame-address lookup as `frames_by_id` but decodes metadata without returning payload bytes;
- `frames_in_ordinal_range` filters by `capture_ordinal_be BETWEEN ? AND ?` and orders by `capture_ordinal_be ASC, session_time_be ASC, frame_id ASC`;
- `TimelineAnchorSource` looks up typed marker/navigation observations from `timeline_observations` by payload JSON/sort key and optional scope;
- `InteractionAnchorSource` returns `Ok(None)` until the browser-operation sibling provides durable anchors. It does not create an `interactions` table in this feature.

**Acceptance criteria:**

- [ ] Source-frame id ranges use `FrameSource` and the same address/CRC/context checks as direct source-frame retrieval; no second frame read path appears.
- [ ] Frame range reads are deterministic by `CaptureOrdinal`, including tied session times and multiple targets with ordinal `1` on each target.
- [ ] Catalog placeholders are not treated as complete session/target records for wall-clock conversion.
- [ ] Marker/navigation lookup returns `InvalidInput` for payload kind mismatches and `Ok(None)` for absent anchors without leaking SQL or raw payload JSON.
- [ ] The core ports scanner still proves no `rusqlite`, `tokio`, CDP, MCP, or filesystem types leak into `krometrail-core` ports.

### Unit 3: `TemporalRangeResolver` policy and failure semantics

**Files:**

- `crates/krometrail-core/src/timeline/range.rs`
- `crates/krometrail-core/src/ports/range.rs`
- `crates/krometrail-core/src/error.rs` (only if helper constructors reduce duplication)
- `crates/krometrail-store/tests/range_resolution.rs`

**Story:** `epic-durable-browser-memory-range-resolution-resolver-semantics`

Implement the resolver as a core service over injected ports:

```rust
pub struct TemporalRangeResolver<C, F, G, T, I> {
    catalog: C,
    frames: F,
    gaps: G,
    timeline: T,
    interactions: I,
}

impl<C, F, G, T, I> TemporalRangeResolver<C, F, G, T, I>
where
    C: RecordingCatalog,
    F: FrameSource,
    G: CaptureGapStore,
    T: TimelineStore + TimelineAnchorSource,
    I: InteractionAnchorSource,
{
    pub fn resolve(
        &self,
        anchor: TemporalRangeAnchor,
        options: RangeResolutionOptions,
    ) -> PortFuture<'_, Result<ResolvedRange>>;
}
```

Resolution by anchor:

- `SessionTime`: require `session_id` and `target_id`, validate target exists when catalog is complete, and use the supplied `SessionRange`.
- `WallClock`: require `session_id` and `target_id`, read `RecordingSession`, convert `SystemTime` bounds to checked nanosecond offsets from `started_at`, reject reversed wall ranges, return `NotFound` for timestamps before session start or beyond known retained evidence unless partial retention is allowed.
- `SourceFrame`: read metadata for both frame IDs, require both endpoints to share one session and target and match any supplied scope, require `start.capture_ordinal <= end.capture_ordinal`, then read all frames in the inclusive ordinal range.
- `Marker` / `Navigation`: look up the typed timeline observation, validate scope, use `window.unwrap_or(Duration::ZERO before/after)` around the observation time, then frame/gap/timeline resolve.
- `Interaction`: look up `InteractionAnchor`, validate scope, apply caller window or the default policy around `started_at..observed_or_completed`.
- `LatestInteraction`: require explicit `session_id` and `target_id`, look up latest anchor for that exact target, then use interaction resolution.

Common finalize step:

1. Convert the anchor into a requested `SessionRange` without overflow.
2. Read retained frames through `FrameSource` only.
3. If no frames are retained, return `NotFound` distinguishing “anchor not found”, “no retained frames for target”, or “requested interval has no retained source frames”.
4. Compute `resolved_range` from the first and last retained frame session times.
5. If `resolved_range != requested_range`, either add `RetentionWarning`s under `AllowPartial` or fail with `NotFound` under `RequireComplete`.
6. Read `CaptureGapStore::gaps` for `resolved_range`; include or reject according to `CaptureGapPolicy`.
7. Read `TimelineStore::range` once for `resolved_range` and collect interaction/navigation/marker IDs from observations in deterministic timeline order. Frame IDs still come from `FrameSource` to keep one frame path.
8. Build `ResolvedRange` with exact requested/resolved bounds, ordered frame IDs, related IDs, full gap records, warnings, and the effective options.

Failure mapping:

| Condition | Error code | Notes |
| --- | --- | --- |
| malformed/reversed ranges, missing required scope, mismatched payload kind | `InvalidInput` | include stable recovery text when useful |
| wall-clock/session-time arithmetic overflow | `InvalidTime` | no wrapping or lossy conversion |
| anchor ID not found | `NotFound` | context carries supplied session/target when available |
| anchor belongs to another session or target | `InvalidInput` | “wrong target/session” is caller input, not missing data |
| no retained frames for the resolved request | `NotFound` | message distinguishes evicted vs never captured when retention metadata can prove it |
| partial retention with `RequireComplete` | `NotFound` | exact unavailable bound named in message and `ErrorContext::range` |
| gaps with `CaptureGapPolicy::Reject` | `NotFound` | gap IDs are not hidden in logs/tests; public message stays source-safe |
| store decode/CRC/SQL failure | `PersistenceFailed` | adapter-private details not exposed |

**Acceptance criteria:**

- [ ] Explicit session-time and wall-clock ranges resolve to the same frame IDs when they describe the same interval.
- [ ] Source-frame ranges include both endpoint frames and every retained frame whose capture ordinal lies between them, even when timestamps tie.
- [ ] Marker/navigation anchors use generic timeline observations without structured marker/navigation tables.
- [ ] Interaction/latest-interaction anchors return `NotFound` until durable anchors exist; the resolver does not fabricate an interaction table or infer timings from returned operation results in memory.
- [ ] Default interaction windows saturate before zero, checked-add after observed/completed time, and record the effective options in `ResolvedRange`.
- [ ] Gap include/reject and retention allow/strict policies are covered with focused tests.

### Unit 4: Qualification, sibling handoff, and simplification

**Files:**

- `crates/krometrail-core/src/timeline/range.rs`
- `crates/krometrail-core/src/ports/mod.rs`
- `crates/krometrail-store/tests/range_resolution.rs`
- affected sibling feature bodies only if implementation discovers a necessary handoff note

**Story:** `epic-durable-browser-memory-range-resolution-qualification-handoff`

Finish with integration tests and explicit handoffs rather than widening this feature beyond resolution.

Sibling-owned extension contracts:

- **Browser page lifecycle / verified interactions:** when a state-changing page operation returns `InteractionAnchor`, persist that same anchor through a durable interaction-anchor writer before or with the operation result. The writer must preserve `interaction_id`, `session_id`, `target_id`, `operation`, and all `InteractionTiming` fields. It may later wrap the anchor in a richer sanitized `InteractionRecord`, but the resolver consumes only the anchor projection through `InteractionAnchorSource`.
- **Navigation events:** navigation-producing browser features mint `NavigationId` and append one `TimelineObservation` with `ObservationKind::Navigation` at the accepted navigation commit point. A richer navigation record may store URL/loader/history evidence later; range resolution needs only the typed timeline point and target.
- **Markers:** caller/system marker features mint `MarkerId` and append one `ObservationKind::Marker` timeline observation at the declared session time. Marker labels belong to the marker-owning feature and temporal-debug bundle, not this resolver.
- **Retention:** retention owns authoritative eviction state and pin protection. This resolver consumes retained frame bounds/warnings but does not delete data, pin ranges, or compute budget candidates.

Simplification/cleanup during implementation:

- Keep `FrameSource` as the one frame-address/read surface; do not query frame rows through `TimelineStore` just to get frame IDs.
- Delete or update any test fake that assumes `FrameSource` has only the old two methods; do not add a second fake hierarchy.
- Keep error construction helpers small and source-safe; do not add a range-specific error enum outside `KrometrailError`.

**Acceptance criteria:**

- [ ] One end-to-end store test writes session/target catalog records, frames, a gap, marker/navigation timeline observations, and resolves explicit time, wall-clock, marker, navigation, source-frame, and currently-absent interaction anchors with expected outcomes.
- [ ] Wrong-target/wrong-session tests cover source-frame, marker/navigation, and interaction scopes.
- [ ] Boundary tests cover zero-length ranges, endpoint inclusivity, timestamp ties, `u64::MAX` overflow, wall times before session start, target with no frames, and gaps at the exact start/end.
- [ ] Documentation in this feature body remains honest about unimplemented durable interaction/navigation/marker writers; implementation notes do not claim persisted records that sibling features have not delivered.
- [ ] Workspace format, locked workspace check, locked workspace tests, and Clippy are green or any unrelated concurrent-owner failures are documented precisely in the implementation summary.

## Implementation order

1. `epic-durable-browser-memory-range-resolution-core-contracts`
2. `epic-durable-browser-memory-range-resolution-store-queries` (depends on core contracts)
3. `epic-durable-browser-memory-range-resolution-resolver-semantics` (depends on store queries)
4. `epic-durable-browser-memory-range-resolution-qualification-handoff` (depends on resolver semantics)

One feature owner should carry these checkpoints as a cohesive implementation bundle. The stories preserve ordering and evidence; they are not parallel worker assignments.

## Simplification

- One `TemporalRangeAnchor` registry replaces per-tool natural-anchor parsing.
- One `ResolvedRange` contract replaces duplicate storyboard/difference/source-frame range descriptions.
- Existing `FrameSource`, `CaptureGapStore`, `TimelineStore`, and `RecordingCatalog` grow focused read methods rather than introducing a single catch-all persistence trait.
- No interaction, navigation, marker, browser-event, pin, or artifact-manifest structured rows are invented here.

## Testing

- **Core contract tests:** registry stable names, Serde, constructor invariants, duplicate-ID rejection, effective option preservation.
- **Frame-order tests:** tied timestamps and same ordinal on different targets prove per-target `CaptureOrdinal` ordering.
- **Explicit anchor tests:** session-time and wall-clock ranges map to the same frames; source-frame ranges include endpoints.
- **Natural anchor tests:** marker/navigation generic timeline observations resolve with optional windows; interactions are `NotFound` until durable anchors exist.
- **Failure tests:** wrong target/session, missing scope, absent anchor, no retained frames, gap reject, partial retention strictness, overflow, and before-session wall timestamps.
- **Integration test:** `SqliteIndex` + segment writer + indexed recording + timeline/gap/catalog rows resolve without reading frames through a second SQL path.

Do not add a test per trivial accessor, SQL statement, or enum derive. Tests protect public range semantics, ordering, and failure modes.

## Risks

- **Interaction anchors are not yet persisted.** The resolver contract is ready, but interaction/window/recent anchors cannot succeed until browser-operation persistence writes anchors. Mitigation: return `NotFound` honestly and keep the sibling handoff explicit.
- **Partial retention semantics cross a future retention feature.** SQLite v1 has frame bounds but no full eviction-status authority. Mitigation: implement frame-bound warnings now and let the retention feature supply stronger evicted-vs-never metadata through the focused port without changing `ResolvedRange`.
- **Wall-clock conversion can be mistaken for source timestamp alignment.** The design uses `RecordingSession::started_at` to produce session offsets only. It does not compare Chrome `SourceTime` to wall time.
- **Gaps are policy-sensitive.** Visual artifacts may include gaps with warnings, while strict source consumers may reject them. Mitigation: `CaptureGapPolicy` is explicit and returned in the `ResolvedRange`.

## Handoff to dependent features

- `epic-temporal-debugging-workflow` consumes `ResolvedRange` unchanged for bundle generation, source references, pin controls, and progressive retrieval.
- `epic-temporal-vision-toolkit` receives ordered frame IDs and gap records; it does not resolve natural anchors.
- Browser-operation features own durable interaction-anchor, navigation, and marker writers. They should target the ports and timeline observations documented here rather than adding another range interpretation path.

## Implementation summary

- Execution capability: raised/high, implemented as one cohesive feature owner under autopilot. Review weight remains standard; this implementation pass stops at `stage: review` without self-review.
- Child checkpoints completed directly: core contracts, focused SQLite queries, resolver semantics, and qualification/handoff.
- Commits: `9650d3f` (core contracts), `c0f6fc4` (store queries), `3f6d519` (resolver semantics), and `dc7461c` (qualification and handoff).
- Delivered one registry-backed `TemporalRangeAnchor`/`ResolvedRange` contract, domain-only resolver ports, checked session/wall-clock arithmetic, inclusive capture-ordinal source-frame ranges, generic marker/navigation lookup, explicit retention/gap policies, and honest absence of durable interaction anchors.
- SQLite metadata and frame reads remain source-safe and reuse the established address/CRC path. Anchor lookup intentionally returns the payload record before resolver scope validation so wrong-session/target anchors produce `InvalidInput` rather than being misreported as missing data.
- Qualification evidence: `crates/krometrail-store/tests/range_resolution.rs` covers explicit, wall-clock, source-frame, marker, navigation, interaction absence, retention, gaps, boundaries, overflow, and scope failures.
- Verification: locked workspace format check, check, tests (379 passed across 37 suites), and Clippy with warnings denied all pass; focused range tests pass (8 tests).
- Honest deviation: durable interaction/latest-interaction resolution remains `NotFound` until browser-operation persistence implements `InteractionAnchorSource`; no interaction, navigation, marker, pin, or artifact tables were invented here.

## Review (2026-07-14)

**Verdict:** Approve with comments

**Blockers:** none
**Important:** none
**Nits:** One unused latest-observation port method and a one-use error-context helper are modest speculative abstractions; absent catalog rows and unscoped payload lookup would benefit from intent comments; zero-width default marker/navigation windows should be documented for callers.
**Rejected:** O(n²) bounded observation deduplication, absent speculative latest-anchor variants, and `NotFound` for pre-session wall time are not current defects.

Fresh-context standard review verified one registry/resolver/result authority, inclusive endpoint and capture-ordinal ordering, checked session/wall-clock conversion, exact scope errors, explicit gap/partial-retention policies, source-safe errors, reuse of the established frame-read authority, validated object-safe ports, and honest `NotFound` interaction anchors. Locked format/check/Clippy and 379 workspace tests passed, including eight focused integration tests. No material current-cycle risk remains.
