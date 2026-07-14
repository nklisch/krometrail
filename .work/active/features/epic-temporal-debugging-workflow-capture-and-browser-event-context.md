---
id: epic-temporal-debugging-workflow-capture-and-browser-event-context
kind: feature
stage: done
tags: [browser, storage, agent-ux]
parent: epic-temporal-debugging-workflow
depends_on: [epic-temporal-debugging-workflow-resolved-temporal-queries]
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Capture Quality and Browser Event Context

## Brief

Deliver the lightweight recorded browser context needed to interpret a resolved visual interval. Record and retain sanitized console messages, uncaught exceptions, request/response lifecycle metadata, failed requests, navigation/lifecycle changes, target visibility, and dialogs through the existing browser-events capability and generic timeline authority, with structured payload storage only where an owned redacted contract earns it.

For one `ResolvedRange`, provide deterministic capture-quality and event context: frame availability and cadence evidence, declared gaps, capture warnings, retention truncation, relevant interaction/navigation markers, and queryable browser events. Preserve enough timing to let the debug-bundle composer select errors, failures, navigation, and events nearest major visual-change moments while keeping verbose event sets available for focused drill-down.

This feature does not persist sensitive headers, cookies, authentication values, request or response bodies by default. It does not add page-state or framework-state evidence, infer that an event caused a visual change, diagnose defects, or define the final bundle presentation.

## Epic context

- Parent epic: `epic-temporal-debugging-workflow`
- Position in epic: correlated-context producer — runs alongside artifact generation after resolved temporal queries are available

## Simplification opportunity

- Extend the existing capability registry, generic timeline index, and global usage ledger instead of creating a second event timeline or one table/tool per CDP event. Keep one sanitized browser-event contract and derive capture quality from authoritative frame, gap, retention, and capture-status evidence rather than duplicating counters in bundle code.

## Foundation references

- `docs/SPEC.md` — Browser Events, Action Timeline, Temporal Queries, and Local Data and Telemetry
- `docs/ARCHITECTURE.md` — Domain Model, Capability Registry, Recording Store, and Observability
- `docs/VISUAL-EVIDENCE.md` — Capture Gaps, Markers, and Temporal Debug Bundle

## Grounding and dispatch

- **Driver:** active autopilot `--all`; all decisions below were resolved from current contracts and completed implementation without questions.
- **Dispatch:** direct-read only. The caller prohibited nested agents and peer review. Grounding covered project rules/conventions; all five foundation documents; the parent and sibling features; artifact design commit `78d76d5`; the implemented resolved-query service and schema v3; the current capability, timeline, gap, frame, capture-status, and range contracts; CDP transport/session/reconnect/capture/wait/dialog/log-safety code and tests; store migrations, timeline, usage, retention, recovery, deletion, and root composition.
- **Current seams:** browser-events already exists as a default-enabled capability but has no production subsystem; `TimelineObservation` is the generic ordering/index authority; cdpkit exposes named params-only event streams backed by upstream unbounded channels; capture and target supervision already carry attachment/connection generations; network-quiet currently creates private subscriptions and calls `Network.enable` itself; `RecordingStore` owns the mutation/deletion/budget gate; `ResolvedRange` already contains exact frame IDs, gaps, and retention warnings.
- **UI:** this is a local domain/store/adapter capability with no human screen or journey. Mockups are intentionally skipped.
- **Review weight:** standard at implementation review. Design-time advisory review was skipped by explicit caller boundary.

## Design decisions

### Vocabulary, identity, and timing

- **One semantic registry:** Add one macro-backed `BROWSER_EVENT_REGISTRY` in core. It generates `BrowserEventKind`, stable names, class, payload compatibility, default compact priority, and registry-wide tests. CDP owns only a source-method table that references these core kinds; it cannot define a second semantic event enum.
- **Kinds:** The registry contains `console_message`, `javascript_exception`, `network_request_started`, `network_response_received`, `network_request_finished`, `network_request_failed`, `navigation`, `page_lifecycle`, `target_lifecycle`, `target_visibility`, `dialog_opened`, `dialog_closed`, `capture_status_changed`, `collection_state_changed`, and `collection_gap`. Capture/collection entries are operational context in the same generic timeline, not a second status timeline.
- **Identity:** Every retained event has a UUID-backed `BrowserEventId`, exact `SessionId`/`TargetId`, non-zero per-target `BrowserEventOrdinal`, and non-zero attachment generation. Every logical network request has a UUID-backed `NetworkRequestId`; raw CDP request IDs are generation-local in-memory map keys and are never persisted.
- **Ordering:** A short, non-awaiting per-target dispatcher critical section allocates ordinals and performs `try_send`, so queue order and ordinal order agree. Ordinals continue across attachment generations and fence stale readers exactly like capture ordinals. Store and query order is `(session_time, event_ordinal, event_id)` ascending. Content/time deduplication is forbidden because equal legitimate messages are evidence; only exact event-ID replay is idempotent.
- **Clocks:** The event pump samples daemon `ObservedTime` immediately after a named event is returned, normalizes it through the session origin, and clamps only against the preceding accepted/dropped event for that target so session time never decreases. Native timestamps remain optional `BrowserSourceTimestamp { clock, time, rounded }`, where clocks distinguish CDP monotonic seconds from Runtime/Log epoch milliseconds. Source time is exposed as evidence and never used to order, join, or compare unrelated clocks.
- **Timeline:** Add typed `ObservationPayloadRef::BrowserEvent(BrowserEventId)` and `ObservationKind::BrowserEvent`. Remove the unused external console/exception/network observation variants during v5 migration rather than maintaining duplicate semantic sets. Explicit operation `NavigationId` observations remain distinct anchors; nearby browser navigation events do not mint or claim those IDs.

### Sanitized payload and privacy contract

- **Allowlist construction:** A normalizer constructs a core `BrowserEventPayload` only from named allowlisted fields. The payload type has no fields for raw CDP JSON, transport session IDs, browser target keys, CDP request/frame/loader IDs, headers, cookies, authentication values, post data, request/response bodies, script source, DOM values, or file chooser paths. Extra-info/body/data events are not subscribed and body-fetch commands are never sent.
- **Console:** Retain source (`runtime` or `log`), normalized level, console method/type, at most 16 primitive argument type tags, one redacted UTF-8 preview of at most 2,048 bytes, and at most 16 sanitized stack frames. Remote objects are not serialized; object previews, properties, and by-value objects are absent.
- **Exceptions:** Retain a redacted exception type/name (64 bytes), redacted text (2,048 bytes), and at most 16 sanitized stack frames. Source snippets, exception object previews, script IDs, execution-context IDs, and raw descriptions are absent.
- **Network:** Retain `NetworkRequestId`, an allowlisted/common HTTP method or hash-only `other`, allowlisted resource type, sanitized URL, bounded initiator kind plus at most eight sanitized stack frames, response status `0..=999`, cache/service-worker booleans, and classified failure (`cancelled`, `blocked`, `dns`, `connection`, `timeout`, `other`). Method/URL/type are optional on out-of-order orphan response/failure events. `loadingFinished` records lifecycle completion only, not body bytes.
- **Navigation/lifecycle:** Retain main/child-frame scope, transition kind, sanitized URL when supplied, and an allowlisted lifecycle name. Raw frame/loader IDs are used only in the generation-local main-frame tracker and never persisted.
- **Targets/dialogs:** Retain target lifecycle/visibility enum values and attachment generation. Dialog records retain dialog type, whether message/default prompt/user input existed, and accepted/dismissed state; dialog message, default prompt, and prompt text are never retained or hashed.
- **Capture status:** Retain the existing validated `TargetCaptureStatus` snapshot on state transitions. This remains available when CDP browser-events is disabled because capture quality belongs to temporal recording, not the event-inspection presentation capability.
- **URLs:** `SanitizedUrl` retains an allowlisted scheme, normalized origin for HTTP(S)/WS(S), optional non-default port, path segment count, lowercase allowlisted extension, SHA-256 of the path only, and booleans indicating removed credentials/query/fragment. It never retains username, password, query, fragment, raw path, basename, or local directory. `file:` retains only scheme, path hash/count, and extension; `data:`/`blob:` retain only a scheme classification and redaction flag. Query/fragment/credentials are removed before hashing.
- **Text/stack redaction:** A single `EventRedactor` strips URL credentials/query/fragments, absolute POSIX/Windows/file paths, bearer/basic values, cookie-like values, and values following case-insensitive `password|passwd|token|secret|authorization|api[_-]?key|session` assignments. It truncates on UTF-8 boundaries and reports only `truncated` and redaction count. Stack function names are run through the same redactor at 128 bytes; stack URLs use `SanitizedUrl`.
- **Hard bounds:** One normalized payload is at most 8 KiB serialized JSON. A target may retain at most 4,096 live raw-request correlations. Over-limit stacks/arguments/text are deterministically truncated; malformed or oversized inputs become collection-gap counts, never raw fallback records.
- **Logs/errors:** Event values, raw params, URLs, text, stack frames, local paths, and serialized payload JSON never enter tracing or stable errors. Logs contain Krometrail session/target IDs, attachment generation, stable source/kind, queue depth/counts, and error code only. Adapter source errors remain source-safe debug classifications without `serde_json::Value` or CDP error text.

### Capability, probe, and disable semantics

- **No parallel capability:** `CapabilityId::BrowserEvents` remains the product capability and stays default-enabled in `CAPABILITY_REGISTRY`. Its `RecordingSubsystem::BrowserEvents` now root-wires the collector; no `console`, `network`, or `events-v2` capability is added.
- **Transport support:** Extend the existing renderer compatibility registry with optional `Log` and `Network` support plus `Page.setLifecycleEventsEnabled`; existing Page/Runtime support remains required for browser control. The disposable probe sends enable commands but creates no event subscriptions. Missing optional support does not reject the browser session; it produces an explicit unavailable source/class in browser-event status.
- **Configured disable:** `BrowserEventConfig::disabled()` installs no semantic CDP subscriptions and persists no semantic CDP events. It still permits capture-status context and an explicit collection-state sample. Later MCP registration reads the same capability selection. Disabled browser-events does not disable control, visual capture, or explicit network-quiet waits.
- **Network wait exception:** When browser-events is disabled, the session domain authority may enable the minimum Network stream on demand for a wait. Those events feed the wait fan-out but are not persisted. This is operational use by `control`, not an implicit re-enable of `browser-events`.
- **Degradation:** Status is `disabled | starting | operational | degraded | suspended | stopped | failed`, with unavailable source classes and aggregate dropped/persisted counts. Subscription/normalization/queue/persistence failures create coalesced `collection_gap` evidence when persistence is available; a currently unpersistable gap remains visible in live status and makes bounded shutdown incomplete rather than being reported as complete evidence.

### One domain authority and nonblocking routing

- **Session ownership:** Introduce one `SessionDomainAuthority` per supervised browser session and one `TargetEventRuntime` per exact `(TargetId, attachment_generation, TransportSessionId)`. Only this authority subscribes to Runtime/Log/Network/Page semantic events and enables those domains. Capture exclusively retains `Page.screencastFrame` and `Page.screencastVisibilityChanged` because its immediate-ack path is special.
- **Subscribe before enable:** For a newly attached target, install and start all configured named-event drains before enabling their domains. Then restore in exact order: `Page.enable`; `Page.setLifecycleEventsEnabled(enabled=true)` when lifecycle collection is configured; `Runtime.enable`; `Log.enable` when supported/configured; `Network.enable` when configured or already demanded by a wait; `Accessibility.enable`. Mandatory Page/Runtime/Accessibility failure still fails target attachment. Optional event-source failure degrades only browser-events.
- **No disable races:** A domain is monotonic within one attachment generation: `not_installed -> installed -> enabled | failed`. Consumers obtain fan-out subscriptions; they never send enable/disable commands. Once Network is enabled for events or a wait it remains enabled until detach. This removes duplicate `Network.enable`, subscriber theft, and refcount-disable races.
- **Network fan-out:** `Network.requestWillBeSent`, `loadingFinished`, and `loadingFailed` feed one bounded broadcast of typed `NetworkActivity`. The persistence normalizer and every network-quiet wait receive independent subscriptions. A wait subscribes before asking the authority to ensure Network, tracks only from that point, excludes WebSocket/EventSource, and fails explicitly if its receiver lags; it never claims quiet after lost activity.
- **Semantic sources:** Subscribe exactly once per generation to `Runtime.consoleAPICalled`, `Runtime.exceptionThrown`, `Log.entryAdded`, `Network.requestWillBeSent`, `Network.responseReceived`, `Network.loadingFinished`, `Network.loadingFailed`, `Page.frameNavigated`, `Page.navigatedWithinDocument`, `Page.lifecycleEvent`, `Page.javascriptDialogOpening`, and `Page.javascriptDialogClosed`. Redirect responses in `requestWillBeSent.redirectResponse` produce a response event before the next request-start event under the same `NetworkRequestId`.
- **Target/capture sources:** Supervisor lifecycle effects and capture visibility/status callbacks submit already-sanitized typed candidates through nonblocking ingress. They do not await SQLite and do not route through the supervisor command queue.
- **Bounded handoff:** Defaults are 32 active targets, 256 queued normalized events per target, 16 MiB process-wide pending payloads, 128 rows/256 KiB per store batch, 64 coalesced gap-ledger entries, 1,024 network fan-out entries, and 4,096 live request correlations per target. A pump normalizes only bounded allowlisted fields, then performs `try_send`; it never awaits persistence or another consumer.
- **Drop aggregation:** Every observed candidate receives an ordinal even if rejected. Consecutive drops with the same target/generation/reason/class coalesce to one gap carrying first/last session time and ordinal plus exact saturating count. Reasons are `invalid_payload`, `payload_limit`, `queue_saturated`, `fanout_lag`, `persistence_rejected`, `subscription_closed`, `source_unavailable`, and `reconnect_boundary`. Gap-ledger pressure conservatively merges time/ordinal ranges and reports that merge.
- **Persistence failure:** The writer drains its bounded queue regardless of store health. A failed batch becomes a pending persistence gap; later batches probe with bounded backoff and flush the gap before new events. It never blocks target supervision, frame acknowledgement/handoff, or browser operations. Event append holds the recording mutation gate only for a bounded SQLite transaction and performs no filesystem work.
- **Generation lifecycle:** Detach/reconnect closes acceptance and aborts exact-generation event drains, emits a reconnect-boundary gap/status, and fences late callbacks. Reconnect installs fresh streams before domains and preserves Krometrail target/event/request identity rules without persisting old transport session IDs. Target close and session shutdown drain/flush event batches under the existing aggregate shutdown deadline before transport detach.

### Durable schema v5, usage, retention, and recovery

- **Migration ownership:** Artifact commit `78d76d5` exclusively assigns schema v4 to `epic-temporal-debugging-workflow-artifact-generation-and-cache-artifact-schema-and-publication`. This feature exclusively owns the next contiguous migration, `schema_v5.rs`, and edits the migration registry only after artifact v4. If committed HEAD advances beyond v4 before implementation, use the next contiguous version and preserve this dependency; never edit or recreate v4.
- **One payload table:** A `browser_events` table stores the structured sanitized payload and projected kind/class/severity/priority/times needed for deterministic filtering. Its typed `BrowserEventId` is referenced atomically by one generic timeline row. There are no per-kind tables, raw event table, event sidecar timeline, body table, header table, or session-ID map table.
- **Schema shape:** v5 adds strict `browser_events` and `browser_event_unavailable_ranges` tables, range/filter/retention indexes, and typed timeline payload support. Event rows include UUID identities, non-zero ordinal/generation blobs, point/affected ranges, optional source clock/time, observed/session time, registry kind/class, severity rank, compact priority, bounded payload JSON byte count, accounted bytes, and shared global retention sequence. Unique constraints cover event ID and `(session,target,event_ordinal)`.
- **Atomic append:** `BrowserEventSink::append_event_batch` accepts at most 128 ordered events and 256 KiB. One immediate transaction validates exact replay/conflict, allocates shared retention sequence values, inserts each event, its typed timeline row, and its `usage(class='browser_event')` entry. Byte-equivalent replay is a no-op; ID/ordinal reuse with different data is `PersistenceFailed`.
- **Deterministic reads:** Decode revalidates registry kind/payload/class/severity/priority and all projected times/scope. Store ordering never depends on SQLite rowid. Corrupt JSON, unknown names, impossible times, and projection disagreement return source-safe `PersistenceFailed` without values or SQL.
- **Global accounting:** Browser-event accounted bytes are exact serialized payload/projection bytes plus a fixed documented row allowance. `refresh_index_usage` moves to SQLite live-page accounting (`page_count - freelist_count` after WAL checkpoint), classifies browser-event bytes as a subset by subtracting them from index bytes, and reports reusable freelist pages as accounting slack. Thus events are not double-counted and metadata deletion lowers effective managed usage even before file compaction.
- **Independent eviction:** Derived artifacts remain first eviction candidates. After that, retention compares the oldest unpinned sealed segment with the oldest browser event through the shared `retention_sequence`; it removes the globally older candidate. Event removal is metadata-only and batched to at most 256 rows/1 MiB. Browser events are contextual and regenerability is not claimed, but they are not source frames: range pins continue protecting source segments, not browser events. Event context can therefore be evicted while pinned visual evidence remains, and queries report that explicitly.
- **Unavailable ranges:** Event eviction records per-target contiguous ordinal/time tombstones in `browser_event_unavailable_ranges` with reason `retention_evicted`. Collection overflow remains a `collection_gap` event. Recovery-discarded corruption uses reason `corrupt_discarded`. Queries distinguish no matching events from known unavailable evidence and never infer loss from ordinal arithmetic.
- **Budget pressure:** Event append may evict bounded oldest event rows in its short transaction. It never performs segment/artifact file deletion. If that cannot make room because older protected/file-backed evidence dominates the budget, it returns `BudgetExhausted`; the CDP writer aggregates a persistence gap while visual capture and supervision continue under their existing policies.
- **Session deletion:** Existing session deletion removes event rows, typed timeline refs, unavailable-range tombstones, request/event usage, and status samples in the same metadata phase, and prevents late generation publication through the existing deleted-session fence. Another session is unaffected.
- **Recovery:** Startup runs v4 artifact recovery first, then browser-event reconciliation before exposing the store. Valid payload rows missing timeline/usage are repaired. Malformed payload/projection rows with valid scope/time are removed and replaced by a corruption-unavailable tombstone; an orphan timeline ref is removed and recorded unavailable. Unrecoverable scope/time corruption fails startup source-safely. Recovery is chunk-bounded and idempotent; SQLite transactions remain the byte durability authority.

### Capture-quality and event-query contract

- **One resolved input:** Add one `TemporalContextQuery` application port. `TemporalContextRequest` owns exactly one existing `ResolvedRange`; it accepts only an optional subrange clip, event filter/selection, and at most 16 bounded focus times supplied by a later visual-change composer. It has no natural anchors and cannot resolve again.
- **Clipping:** The effective event range is the intersection of the optional clip and `ResolvedRange.resolved_range`; a disjoint clip is invalid. Focus times must lie in that effective range. Browser events are joined only by exact session ID, target ID, and normalized session time. Wall time and native CDP source clocks never participate.
- **Capture availability:** Load metadata in exact `ResolvedRange.frame_ids` order and prove identity/session/target/order/time agreement. Return requested and retained bounds, exact frame count, first/last frame identity/time/ordinal, complete `ResolvedRange.gaps` and retention warnings, and table-driven frame-warning summaries.
- **Cadence:** For at least two frames, derive exact adjacent session-time deltas and return interval count, minimum, median (nearest-rank 50), p95 (nearest-rank 95), and maximum nanoseconds. p95 is retained because sparse stalls are operationally important; p99 and FPS are omitted because short intervals make them noisy. Computation is deterministic `O(n log n)` over at most 20,000 frame IDs and stores no duplicate counters.
- **Gap summary:** Clip and union overlapping declared gap ranges before summing known duration, sum known missing-frame estimates with saturation, and separately report whether any gap lacks an estimate. Point gaps remain zero-duration explicit loss. No gaps are inferred from frame times or ordinals.
- **Capture status/generation:** Read the latest retained `capture_status_changed` sample at or before range start plus retained transitions through range end, capped at 128. Return start/end status and ordered generation/state transitions. Missing/evicted status is an explicit warning. Visual epoch count is deliberately deferred to the artifact feature's geometry/scale epoch authority rather than reimplemented here.
- **Filters:** `BrowserEventFilter` is a unique registry-derived class set plus minimum severity (`debug|info|warning|error`). Operational collection gaps and event-retention warnings are returned regardless of semantic class filter because they qualify evidence completeness.
- **Compact mode:** Default limit is 24 (maximum 64). Candidate reads are bounded: at most `4 * limit` SQL-priority candidates plus two predecessors/two successors per focus time. Rank by compact priority (exceptions/console errors/request failures; HTTP 5xx/4xx; navigation/dialog; other), then minimum focus distance, then `(session_time,event_ordinal,event_id)`. Deduplicate by event ID and finally present selected events chronologically with a `CompactSelectionReason`. Proximity is correlation distance only.
- **Verbose mode:** Uses the same `TemporalContextQuery` port with chronological selection, default page size 100 and maximum 1,000. `BrowserEventCursor` carries exact session/target/time/ordinal/event ID; scope or filter/range mismatch is invalid. The next page uses a strict tuple comparison, so ties neither repeat nor disappear.
- **Truncation:** Results report matched count, selected/returned count, next cursor for chronological pages, and explicit `EventQueryWarning::Truncated`. Collection-gap/unavailable-range warnings have their own bounded count/truncation warning and can never be silently filtered away.
- **Errors:** Invalid limits/clip/focus/cursor return `InvalidInput`; missing frame IDs or concurrently deleted sessions return `NotFound`; row/projection corruption returns `PersistenceFailed`; resource bounds return `ResourceLimitExceeded`. Errors include only Krometrail scope IDs/ranges and stable recovery guidance.

## Architectural choice

### Option A — persist raw CDP params and redact when queried

This preserves maximum future flexibility but makes privacy dependent on every reader, permanently couples durable data to protocol/library drift, stores fields the product explicitly excludes, and turns raw JSON size into a recording risk. Rejected.

### Option B — one collector/table per CDP domain

Separate console, exception, network, page, target, and dialog pipelines look locally simple. They duplicate enable/restore state, compete with network waits, create cross-table ordering and retention joins, and repeat redaction/error handling. Rejected.

### Option C — one sanitized core vocabulary, one session domain authority, one typed payload table, and one context service (chosen)

Core owns the semantic registry, privacy-safe types, store/query ports, and deterministic selection/quality policy. CDP owns source extraction, bounded pumps, domain lifecycle, and fan-out. `RecordingStore` atomically adds payload plus generic timeline identity and remains the one usage/retention/deletion authority. The context service consumes an already `ResolvedRange`. This has the fewest durable concepts while making sensitive fields absent before persistence.

A separate event-segment file format was considered for cheap physical truncation. Lightweight bounded rows fit the existing SQLite authority, and live-page/freelist accounting plus batched metadata eviction solves budget reuse without introducing another file/recovery protocol. If measured event volume makes SQLite harmful, `BrowserEventSink`/`BrowserEventSource` preserve a later adapter change without altering core payload/query contracts.

## Trickiest unit first: domain ownership without stealing or starving streams

The riskiest boundary is cdpkit's named-event API: every subscription has an upstream unbounded channel, while current network waits create their own subscriptions and enable the domain. The design therefore makes subscription count and drain ownership explicit before adding persistence:

```rust
// crates/krometrail-cdp/src/events/domain.rs
pub(crate) struct SessionDomainAuthority {
    config: BrowserEventConfig,
    targets: HashMap<(TargetId, u64), TargetEventRuntime>,
    ingress: Arc<EventIngress>,
}

impl SessionDomainAuthority {
    pub(crate) async fn restore_target(
        &mut self,
        binding: EventTargetBinding,
        transport: Arc<dyn CdpTransport>,
        support: &BrowserCompatibility,
    ) -> Result<EventRestoreOutcome>;

    pub(crate) async fn network_activity(
        &mut self,
        binding: &BoundTarget,
        transport: &dyn CdpTransport,
    ) -> Result<tokio::sync::broadcast::Receiver<NetworkActivity>>;

    pub(crate) fn suspend_target(&mut self, target_id: TargetId, generation: u64);
    pub(crate) async fn stop_target(
        &mut self,
        target_id: TargetId,
        generation: u64,
        deadline: tokio::time::Instant,
    ) -> EventStopOutcome;
}
```

`restore_target` installs every configured named stream exactly once, starts drains, and only then sends ordered enable commands. `network_activity` subscribes the wait before serialized on-demand enable and returns the existing fan-out; it never subscribes to transport directly. Event drains normalize bounded fields and call `try_send`; the store writer is a separate task. A closed source degrades that source and, only when the transport is actually closed, reuses generation-aware disconnect cancellation. No semantic event path enters the supervisor command channel or capture frame/ack task.

This unit is implemented and qualified against fakes before schema work. It can proceed while artifact v4 is pending; only migration/store integration waits for the cross-feature checkpoint.

## Implementation units

### Unit 1: core browser-event registry, privacy values, and ports

**Story:** `epic-temporal-debugging-workflow-capture-and-browser-event-context-browser-event-contracts-and-privacy`

**Files:**

- `crates/krometrail-core/src/browser/{events.rs,privacy.rs}` (new)
- `crates/krometrail-core/src/browser/mod.rs`
- `crates/krometrail-core/src/ids.rs`
- `crates/krometrail-core/src/timeline/{observation.rs,mod.rs}`
- `crates/krometrail-core/src/ports/{browser_events.rs,mod.rs}` (new)
- `crates/krometrail-core/src/capabilities/mod.rs`
- `crates/krometrail-core/src/lib.rs`

Representative boundary:

```rust
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum BrowserEventSeverity { Debug, Info, Warning, Error }

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum BrowserEventClass {
    Console, Exception, Network, Navigation, Lifecycle, Target, Dialog, Capture, Operational,
}

pub struct BrowserEventDefinition {
    pub kind: BrowserEventKind,
    pub stable_name: &'static str,
    pub class: BrowserEventClass,
    pub default_compact_priority: u8,
}

pub static BROWSER_EVENT_REGISTRY: &[BrowserEventDefinition];

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct BrowserEventOrdinal(NonZeroU64);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BrowserSourceTimestamp {
    pub clock: BrowserSourceClock,
    pub time: SourceTime,
    pub rounded: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RedactedText {
    pub text: String,          // <= 2,048 UTF-8 bytes
    pub truncated: bool,
    pub redaction_count: u16,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SanitizedUrl {
    pub scheme: SanitizedUrlScheme,
    pub origin: Option<String>,
    pub non_default_port: Option<u16>,
    pub path_sha256: Option<[u8; 32]>,
    pub path_segment_count: u16,
    pub extension: Option<String>,
    pub had_credentials: bool,
    pub had_query: bool,
    pub had_fragment: bool,
    pub fully_redacted: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum BrowserEventPayload {
    Console(ConsoleEvent),
    Exception(ExceptionEvent),
    NetworkRequestStarted(NetworkRequestStarted),
    NetworkResponseReceived(NetworkResponseReceived),
    NetworkRequestFinished(NetworkRequestFinished),
    NetworkRequestFailed(NetworkRequestFailed),
    Navigation(NavigationEvent),
    PageLifecycle(PageLifecycleEvent),
    TargetLifecycle(TargetLifecycleEvent),
    TargetVisibility(TargetVisibilityEvent),
    DialogOpened(DialogOpenedEvent),
    DialogClosed(DialogClosedEvent),
    CaptureStatus(TargetCaptureStatus),
    CollectionState(BrowserEventCollectionState),
    CollectionGap(BrowserEventCollectionGap),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BrowserEvent {
    pub id: BrowserEventId,
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub attachment_generation: u64,
    pub ordinal: BrowserEventOrdinal,
    pub session_time: SessionTime,
    pub source_time: Option<BrowserSourceTimestamp>,
    pub observed_time: ObservedTime,
    pub severity: BrowserEventSeverity,
    pub payload: BrowserEventPayload,
}

impl BrowserEvent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(/* exact fields above */) -> Result<Self>;
    pub fn kind(&self) -> BrowserEventKind;
    pub fn class(&self) -> BrowserEventClass;
    pub fn affected_range(&self) -> SessionRange;
    pub fn compact_priority(&self) -> u8;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserEventBatch {
    pub session_id: SessionId,
    pub events: Vec<BrowserEvent>,
}

pub trait BrowserEventSink: Send + Sync {
    fn append_event_batch(
        &self,
        batch: BrowserEventBatch,
    ) -> PortFuture<'_, Result<()>>;
}
```

`BrowserEvent::new` validates non-nil IDs, generation/ordinal, session time not after observed time, registry/payload match, source-clock rules, severity, URL/text/stack limits, and the 8 KiB serialized ceiling. `BrowserEventBatch::new` requires one session, strict per-target ordinal order, at most 128 rows, and at most 256 KiB.

**Acceptance criteria:**

- [ ] Adding a semantic kind requires one registry row and automatically covers stable names, class, payload match, and compact default priority.
- [ ] IDs/ordinal/event/payload/privacy values round-trip through validated Serde and reject unknown fields, malformed times, duplicate IDs, and over-limit values.
- [ ] The public payload type cannot represent headers, cookies, auth, query/fragment values, bodies, raw params/session IDs, local paths, or dialog/fill/upload values.
- [ ] Table-driven privacy tests cover credentials/query/fragment URLs, file/Windows/POSIX paths, console/exception secrets, stack URLs/functions, dialog prompts, and network method/name boundaries.
- [ ] Core and ports use only core/std types; no CDP, SQLite, URL-parser, or tracing type crosses inward.

### Unit 2: session domain authority, redaction, routing, waits, and drop aggregation

**Story:** `epic-temporal-debugging-workflow-capture-and-browser-event-context-session-domain-authority-and-routing`

**Files:**

- `crates/krometrail-cdp/src/events/{mod.rs,domain.rs,normalize.rs,privacy.rs,network.rs,pipeline.rs,status.rs}` (new)
- `crates/krometrail-cdp/src/{compatibility.rs,lib.rs}`
- `crates/krometrail-cdp/src/session/{mod.rs,runtime.rs,reconnect.rs,shutdown.rs}`
- `crates/krometrail-cdp/src/control/wait.rs`
- `crates/krometrail-cdp/src/capture/{mod.rs,pipeline.rs}`
- `crates/krometrail-cdp/tests/{browser_events.rs,waits_and_batches.rs,session_supervision.rs}`
- `crates/krometrail-cdp/tests/support/scripted_cdp.rs`

```rust
#[derive(Clone, Debug)]
pub struct BrowserEventConfig {
    pub enabled: bool,
    pub per_target_queue_capacity: NonZeroUsize, // default 256, hard max 1,024
    pub global_pending_bytes: NonZeroUsize,      // default 16 MiB, hard max 64 MiB
    pub store_batch_rows: NonZeroUsize,          // default 128, hard max 128
    pub store_batch_bytes: NonZeroUsize,         // default 256 KiB, hard max 256 KiB
    pub network_fanout_capacity: NonZeroUsize,   // default 1,024
    pub request_map_capacity: NonZeroUsize,      // default 4,096
    pub gap_ledger_capacity: NonZeroUsize,       // default 64
}

impl Default for BrowserEventConfig;
impl BrowserEventConfig { pub fn disabled() -> Self; pub fn validate(&self) -> Result<()>; }

pub fn with_browser_events(
    self,
    clock: Arc<dyn MonotonicClock>,
    ids: Arc<dyn IdSource>,
    sink: Arc<dyn BrowserEventSink>,
    config: BrowserEventConfig,
) -> Self;
```

The CDP source registry maps named methods to bounded normalizer functions returning zero, one, or two core candidates. Privacy conversion occurs before queue handoff. `Page.screencastVisibilityChanged` remains capture-owned and forwards a typed visibility candidate through the observer. `execute_wait` receives the session domain authority and replaces private Network subscriptions/enable with `network_activity`.

**Acceptance criteria:**

- [ ] Two targets route same-named events only to their Krometrail target/generation; reconnect fences late old-generation events while ordinals continue.
- [ ] Restore subscribes before the exact enable order, uses one subscription per method/generation, and optional Log/Network failures degrade events without failing control/capture.
- [ ] Explicit network-quiet waits and default event recording share one Network enable and independent fan-out; neither steals events, and lag fails rather than claiming quiet.
- [ ] Queue/global-byte/request-map saturation stays bounded, coalesces exact drop ranges/counts, and never awaits the store from event, capture, supervisor, or operation paths.
- [ ] Redirect, out-of-order response/failure, source timestamp clock/unit, dialog lifecycle, visibility, and target lifecycle normalize without raw IDs or sensitive fields.
- [ ] Controlled barriers prove a stalled/failing event sink does not delay frame ack/handoff, target reconnect, a browser operation, or another target's event drain.
- [ ] Table-driven source coverage checks every CDP source registry row; tests are not duplicated one per semantic variant where registry coverage suffices.

### Unit 3: schema v5, event store, usage, retention, deletion, and recovery

**Story:** `epic-temporal-debugging-workflow-capture-and-browser-event-context-schema-v5-retention-and-recovery`

**Cross-feature dependency:** `epic-temporal-debugging-workflow-artifact-generation-and-cache-artifact-schema-and-publication`

**Files:**

- `crates/krometrail-store/src/index/schema_v5.rs` (new; exclusive v5 ownership)
- `crates/krometrail-store/src/index/{migrations.rs,browser_events.rs,timeline.rs,retention.rs,maintenance.rs,deletion.rs,mod.rs}`
- `crates/krometrail-store/src/{recording.rs,recovery.rs,lib.rs}`
- `crates/krometrail-store/tests/{browser_events.rs,browser_event_recovery.rs,retention_small_budget.rs,sqlite_schema.rs}`

Schema checkpoint (names remain v5 even though artifact v4 may rebuild artifact tables):

```sql
CREATE TABLE browser_events (
    event_id BLOB PRIMARY KEY CHECK(length(event_id)=16),
    session_id BLOB NOT NULL CHECK(length(session_id)=16),
    target_id BLOB NOT NULL CHECK(length(target_id)=16),
    event_ordinal_be BLOB NOT NULL CHECK(length(event_ordinal_be)=8),
    attachment_generation_be BLOB NOT NULL CHECK(length(attachment_generation_be)=8),
    session_time_be BLOB NOT NULL CHECK(length(session_time_be)=8),
    affected_start_time_be BLOB NOT NULL CHECK(length(affected_start_time_be)=8),
    affected_end_time_be BLOB NOT NULL CHECK(length(affected_end_time_be)=8),
    source_clock TEXT NULL,
    source_time_be BLOB NULL CHECK(source_time_be IS NULL OR length(source_time_be)=16),
    source_rounded INTEGER NOT NULL CHECK(source_rounded IN (0,1)),
    observed_time_be BLOB NOT NULL CHECK(length(observed_time_be)=8),
    kind TEXT NOT NULL,
    class TEXT NOT NULL,
    severity_rank INTEGER NOT NULL CHECK(severity_rank BETWEEN 0 AND 3),
    compact_priority INTEGER NOT NULL CHECK(compact_priority BETWEEN 0 AND 255),
    payload_json TEXT NOT NULL
        CHECK(length(CAST(payload_json AS BLOB)) BETWEEN 2 AND 8192),
    accounted_bytes_be BLOB NOT NULL CHECK(length(accounted_bytes_be)=8),
    retention_sequence INTEGER NOT NULL UNIQUE CHECK(retention_sequence>0),
    UNIQUE(session_id,target_id,event_ordinal_be),
    CHECK(affected_start_time_be<=affected_end_time_be),
    CHECK((source_clock IS NULL)=(source_time_be IS NULL)),
    FOREIGN KEY(session_id,target_id) REFERENCES targets(session_id,target_id) ON DELETE CASCADE
) STRICT;

CREATE TABLE browser_event_unavailable_ranges (
    unavailable_id INTEGER PRIMARY KEY,
    session_id BLOB NOT NULL CHECK(length(session_id)=16),
    target_id BLOB NOT NULL CHECK(length(target_id)=16),
    start_time_be BLOB NOT NULL CHECK(length(start_time_be)=8),
    end_time_be BLOB NOT NULL CHECK(length(end_time_be)=8),
    first_ordinal_be BLOB NULL CHECK(first_ordinal_be IS NULL OR length(first_ordinal_be)=8),
    last_ordinal_be BLOB NULL CHECK(last_ordinal_be IS NULL OR length(last_ordinal_be)=8),
    event_count_be BLOB NOT NULL CHECK(length(event_count_be)=8),
    reason TEXT NOT NULL CHECK(reason IN ('retention_evicted','corrupt_discarded')),
    CHECK(start_time_be<=end_time_be),
    CHECK((first_ordinal_be IS NULL)=(last_ordinal_be IS NULL)),
    FOREIGN KEY(session_id,target_id) REFERENCES targets(session_id,target_id) ON DELETE CASCADE
) STRICT;

CREATE INDEX browser_event_range_idx ON browser_events(
    session_id,target_id,session_time_be,event_ordinal_be,event_id
);
CREATE INDEX browser_event_filter_idx ON browser_events(
    session_id,target_id,class,severity_rank,session_time_be,event_ordinal_be,event_id
);
CREATE INDEX browser_event_priority_idx ON browser_events(
    session_id,target_id,compact_priority,session_time_be,event_ordinal_be,event_id
);
CREATE INDEX browser_event_retention_idx ON browser_events(retention_sequence,event_id);
CREATE INDEX browser_event_unavailable_idx ON browser_event_unavailable_ranges(
    session_id,target_id,start_time_be,end_time_be,unavailable_id
);
```

The migration deletes only legacy timeline rows with the never-production external console/exception/network kinds before removing those core variants. It leaves explicit navigation/marker/target rows and all source/artifact evidence untouched.

Store port reads are semantic and bounded:

```rust
pub trait BrowserEventSource: Send + Sync {
    fn count_events(&self, selector: BrowserEventSelector) -> PortFuture<'_, Result<u64>>;
    fn chronological_events(
        &self,
        selector: BrowserEventSelector,
        cursor: Option<BrowserEventCursor>,
        limit: EventPageLimit,
    ) -> PortFuture<'_, Result<Vec<BrowserEvent>>>;
    fn priority_candidates(
        &self,
        selector: BrowserEventSelector,
        limit: EventCandidateLimit,
    ) -> PortFuture<'_, Result<Vec<BrowserEvent>>>;
    fn nearest_candidates(
        &self,
        selector: BrowserEventSelector,
        focus_times: Vec<SessionTime>,
        each_side: u8,
    ) -> PortFuture<'_, Result<Vec<BrowserEvent>>>;
    fn unavailable_ranges(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        range: SessionRange,
        limit: u16,
    ) -> PortFuture<'_, Result<Vec<BrowserEventUnavailableRange>>>;
    fn capture_status_samples(
        &self,
        session_id: SessionId,
        target_id: TargetId,
        range: SessionRange,
        limit: u16,
    ) -> PortFuture<'_, Result<CaptureStatusSamples>>;
}
```

**Acceptance criteria:**

- [ ] Fresh and artifact-v4 databases migrate transactionally to contiguous v5; future versions refuse and a failed v5 rolls back to v4.
- [ ] Batch append atomically creates exact payload, generic timeline reference, shared retention sequence, and usage; replay is idempotent and conflicting ID/ordinal reuse fails.
- [ ] Range/filter/priority/nearest/cursor queries follow documented ties regardless of insertion plan and validate every row against the core registry.
- [ ] Event batches/rows/payloads/queries enforce exact limits; corrupt/unknown rows and errors are source-safe and never expose payload/URL/text/SQL/path values.
- [ ] Global budget compares event and segment retention sequence, evicts events independently, records/coalesces unavailable tombstones, and does not double-count SQLite event bytes.
- [ ] Pins protect source segments but not events; event eviction leaves frames readable and query warnings honest.
- [ ] Session deletion/reopen recovery removes or repairs event/timeline/usage/tombstone state, another session survives, and a second recovery pass is a no-op.

### Unit 4: deterministic capture-quality and event-context service

**Story:** `epic-temporal-debugging-workflow-capture-and-browser-event-context-range-context-query-service`

**Files:**

- `crates/krometrail-core/src/timeline/context.rs` (new)
- `crates/krometrail-core/src/timeline/mod.rs`
- `crates/krometrail-core/src/ports/browser_events.rs`
- `crates/krometrail-core/src/{error.rs,lib.rs}`
- `crates/krometrail-store/src/recording.rs`
- `crates/krometrail-store/tests/range_context.rs` (new)

```rust
pub const MAX_CAPTURE_QUALITY_FRAMES: usize = 20_000;
pub const MAX_FOCUS_TIMES: usize = 16;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BrowserEventFilter {
    pub classes: Vec<BrowserEventClass>, // unique, registry order canonicalized
    pub minimum_severity: BrowserEventSeverity,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum BrowserEventSelection {
    Compact { limit: EventCompactLimit },       // default 24, max 64
    Chronological {
        limit: EventPageLimit,                  // default 100, max 1,000
        cursor: Option<BrowserEventCursor>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TemporalContextRequest {
    pub range: ResolvedRange,
    pub clip: Option<SessionRange>,
    pub filter: BrowserEventFilter,
    pub selection: BrowserEventSelection,
    pub focus_times: Vec<SessionTime>,
}

impl TemporalContextRequest {
    pub fn compact(range: ResolvedRange, focus_times: Vec<SessionTime>) -> Result<Self>;
    pub fn new(/* exact fields above */) -> Result<Self>;
    pub fn effective_range(&self) -> Result<SessionRange>;
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FramePoint {
    pub frame_id: FrameId,
    pub capture_ordinal: CaptureOrdinal,
    pub session_time: SessionTime,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CadenceSummary {
    pub interval_count: u64,
    pub min_nanos: u64,
    pub median_nanos: u64,
    pub p95_nanos: u64,
    pub max_nanos: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CaptureQuality {
    pub requested_range: SessionRange,
    pub retained_range: SessionRange,
    pub frame_count: u64,
    pub first_frame: FramePoint,
    pub last_frame: FramePoint,
    pub cadence: Option<CadenceSummary>,
    pub frame_warnings: Vec<CaptureWarningSummary>,
    pub gaps: Vec<CaptureGap>,
    pub gap_summary: CaptureGapSummary,
    pub retention_warnings: Vec<RetentionWarning>,
    pub capture_status: CaptureStatusEvidence,
    pub warnings: Vec<CaptureQualityWarning>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BrowserEventContext {
    pub effective_range: SessionRange,
    pub matched_count: u64,
    pub events: Vec<SelectedBrowserEvent>,
    pub next_cursor: Option<BrowserEventCursor>,
    pub collection_gaps: Vec<BrowserEventCollectionGap>,
    pub unavailable_ranges: Vec<BrowserEventUnavailableRange>,
    pub warnings: Vec<EventQueryWarning>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TemporalContext {
    pub range: ResolvedRange,
    pub capture_quality: CaptureQuality,
    pub browser_events: BrowserEventContext,
}

pub trait TemporalContextQuery: Send + Sync {
    fn context(
        &self,
        request: TemporalContextRequest,
    ) -> PortFuture<'_, Result<TemporalContext>>;
}
```

`RecordingStore` implements `TemporalContextQuery` by holding its mutation gate for metadata/event reads, then invoking `TemporalContextService<FrameSource, BrowserEventSource>`. It never reads encoded frame bytes. Candidate selection remains pure core policy; the SQLite adapter only performs bounded semantic reads.

**Acceptance criteria:**

- [ ] Exact frame identity/order validation, 0/1/many frame cadence, tied times, nearest-rank quantiles, warning aggregation, overlapping/point gaps, and missing estimates are deterministic.
- [ ] Requested/retained bounds and existing retention warnings are copied, not reinterpreted; visual epoch counting is absent and documented as artifact-owned.
- [ ] Capture status at/before start, transitions, generations, missing/evicted status, and transition cap are explicit.
- [ ] Compact priority/focus distance/ties/dedup/final chronological order and truncation are independent of SQL insertion and concurrency.
- [ ] Verbose cursor pages cover equal-time events without repeats/omissions; filters, clipping, focus bounds, cursor scope, and all limits validate before reads.
- [ ] Collection drops and event-retention/corruption warnings remain visible under every class/severity filter; no result claims causality.

### Unit 5: root composition and integrated qualification

**Story:** `epic-temporal-debugging-workflow-capture-and-browser-event-context-composition-and-qualification`

**Files:**

- `src/app.rs`
- `crates/krometrail-cdp/src/session/{mod.rs,runtime.rs,shutdown.rs}`
- `crates/krometrail-cdp/tests/browser_events.rs`
- `crates/krometrail-store/tests/{browser_events.rs,range_context.rs,retention_small_budget.rs}`
- existing root composition tests in `src/app.rs`

Root keeps one concrete `Arc<RecordingStore>` and wires it as `BrowserEventSink` and `TemporalContextQuery`. It constructs `BrowserEventConfig::default()` from the existing default capability selection, passes the same clock/IDs/session origin used by capture, and retains `Arc<dyn TemporalContextQuery>` in `RuntimeDependencies` for the later bundle/MCP features. MCP routes/resources do not change.

Qualification uses one scripted two-target/two-generation CDP fixture and one real v5 store fixture with tied frame/event times, warnings, target visibility, a dialog, redirects, failures, tiny budgets, event eviction, a capture gap, and persisted capture-status transitions.

**Acceptance criteria:**

- [ ] Root default has operational browser-events and one shared store/timeline/usage authority; explicit disabled config makes no semantic subscriptions and leaves control/capture/network-wait behavior intact.
- [ ] Scripted target/generation/reconnect tests prove exact routing, domain restore, old-generation fencing, gap/status persistence, and source-safe logs.
- [ ] A redaction corpus inspects serialized rows and stable errors for fill text, dialog/default-prompt text, upload directories/files, URL credentials/query/fragments, console/exception secrets, stack/local paths, network names, headers/cookies/auth, and body-shaped sentinels.
- [ ] Network wait coexistence sees the same starts/completions while event persistence is saturated; lag returns explicit uncertainty.
- [ ] v4→v5, usage, independent event eviction, pins, session deletion, crash/recovery, corrupt row/timeline/usage, and second-pass idempotence pass.
- [ ] Context tests cover ordering/focus selection/truncation/cursor, capture-quality edge cases, retention/drop warnings, and unavailable status.
- [ ] Controlled barriers prove event flood/store stall cannot starve supervisor input, frame acknowledgement/ingestion, operations, or another target. CI asserts ordering/bounds, not host timing.
- [ ] Locked Rust 1.85 format/check/test/Clippy gates pass. Real-Chrome coverage may be ignored/manual and cannot be claimed unless enabled.

## Implementation order

1. `epic-temporal-debugging-workflow-capture-and-browser-event-context-browser-event-contracts-and-privacy`
2. Two semantically parallel checkpoints become available after core contracts:
   - `epic-temporal-debugging-workflow-capture-and-browser-event-context-session-domain-authority-and-routing` — depends only on core contracts and can use a fake sink while artifact schema work proceeds.
   - `epic-temporal-debugging-workflow-capture-and-browser-event-context-schema-v5-retention-and-recovery` — depends on core contracts **and** `epic-temporal-debugging-workflow-artifact-generation-and-cache-artifact-schema-and-publication`, so migration registry edits occur only after artifact v4.
3. `epic-temporal-debugging-workflow-capture-and-browser-event-context-range-context-query-service` — depends on schema v5/store reads.
4. `epic-temporal-debugging-workflow-capture-and-browser-event-context-composition-and-qualification` — depends on domain routing and context query service.

These are checkpoints for one future feature owner, not five implementation agents. The graph preserves real migration write ownership without inventing a semantic dependency between CDP/core event work and artifact generation.

## Simplification and elimination

- Keep `CapabilityId::BrowserEvents`; add no parallel capability, event tool, server, or event timeline.
- Replace unused external console/exception/network timeline placeholders with one typed browser-event reference and one core registry.
- Replace network wait's private subscriptions and repeated `Network.enable` with one session domain authority and bounded fan-out.
- Reuse `SessionOrigin`, `TimelineObservation`, `RecordingStore`'s mutation/deletion gate, shared retention sequence, usage ledger, `FrameSource`, `ResolvedRange`, `CaptureGap`, `RetentionWarning`, and `TargetCaptureStatus`.
- Derive range cadence/warnings from frame metadata; do not persist range counters or duplicate live capture histograms.
- Defer visual epochs to artifact generation, event presentation to the bundle/MCP features, and request/response bodies indefinitely by default.
- Do not add one table/test/type per CDP event when registry/table-driven payload and tests protect the same contract.

## Testing strategy

- **Registry/privacy unit:** table-driven semantic registry, Serde, redaction corpus, URL/path policy, stack/text/argument limits, and payload-shape negative assertions protect the highest-risk boundary.
- **CDP interface:** scripted same-name multi-target/generation routing, ordered domain restore, reconnect fencing, drop aggregation, and network-wait fan-out protect cdpkit's unbounded named-stream limitations.
- **Store interface:** artifact-v4→event-v5 migration, atomic payload/timeline/usage replay, deterministic indexes, independent eviction/tombstones, deletion, corruption, and recovery protect durable evidence.
- **Complex pure policy:** cadence quantiles/gap union and compact focus selection/cursor ties receive focused examples because incorrect ordering or confidence would mislead later bundles.
- **Concurrency regression:** barriers, bounded channels, and paused Tokio time prove no event sink/flood can starve capture/supervision/operations without flaky stopwatch thresholds.
- **Root seam:** one end-to-end fake-CDP → sanitized store → same-range context query protects capability/default wiring.
- Do not test trivial getters, every SQL statement, each event variant independently, MCP routes not owned here, or raw CDP values by snapshot.

## Integrated implementation evidence

All five dependency-ordered checkpoints are complete:

1. Core browser-event contracts and privacy — `ea82451`.
2. Session-domain authority and generation-fenced routing — `64e7f48`.
3. Transactional schema v5, retention, deletion, and recovery — `f5e3056`.
4. Deterministic resolved-range context query service — `1507e8b`.
5. Root composition and integrated qualification — `8f866a2`.

The integrated seam now has one default capability selection, one process clock and ID source shared by capture/events/control, and one concrete `RecordingStore` behind recording, retention, timeline, browser-event sink/source, artifact, temporal-query, and temporal-context ports. The connector installs browser events from the registry default; explicit capability omission selects disabled semantic collection without removing control/capture composition, and MCP consumes the same selection without gaining any new route or resource.

The final scripted qualification drives two targets through two transport generations and reconnect into the real schema-v5 store, verifies target isolation, old-generation fencing, ordinal continuity, and private-field exclusion, then reads the exact retained browser events through a `TemporalContextRequest` over the same session/target/range. Existing focused suites jointly cover network fan-out/lag, bounded queue and sink stalls, frame acknowledgement ordering, supervisor/operation independence, aggregate shutdown, migration rollback/future refusal, usage, global event/segment retention, pins, session deletion, crash recovery/idempotence/corruption, deterministic query ordering/focus/cursors/truncation, and capture-quality warnings.

Final locked Rust 1.85 evidence:

- `cargo fmt --all -- --check` — passed.
- `cargo check --workspace --all-targets --locked` — passed.
- `cargo test --workspace --all-targets --locked` — passed.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` — passed.

The implementation made no MCP route/resource, bundle, artifact/temporal-vision, foundation-document, or live-Chrome claim. Opt-in browser tests retained their existing no-browser skip behavior. The feature is ready for integrated review; this transition does not perform or complete that review.

## Risks and rollback

- **cdpkit upstream queues are still unbounded.** Krometrail drains one stream per method continuously, does bounded synchronous extraction, and never leaves duplicate/unconsumed streams in a generation. If sustained qualification still shows upstream growth, disable the noisy event source/browser-events explicitly and apply the transport reference's owned-adapter fallback rules; never pretend the downstream queue bounded cdpkit.
- **Text redaction cannot prove arbitrary application strings are secret-free.** Structural exclusions make headers/bodies/query/dialog/upload values absent; conservative token/path redaction and limits reduce free-text risk. If corpus or live evidence finds leakage, rollback is to retain only level/type/text hash for console/exception until the redactor is corrected. Schema/query contracts allow absent text.
- **Event SQLite writes could contend with frames.** Batches are short, bounded, and filesystem-free; CDP never awaits them. If measurements show frame latency, lower event batch/queue limits or disable semantic collection while preserving capture/status/query warnings. Do not split usage/deletion authority speculatively.
- **Network fan-out lag can invalidate quiet waits.** The wait fails explicitly and can be retried; it never resets to a false quiet state. Raising the bounded fan-out is a measured configuration change, not a correctness workaround.
- **Native source clocks can be misread.** Clock kind travels with every source timestamp, while all correlation uses session time. Unknown or malformed timestamps become `None`/gap evidence, not guessed conversion.
- **Independent event eviction can leave pinned frames without context.** That is intentional under one global budget and is surfaced through unavailable-range warnings. If product evidence later requires event pinning, extend the existing retention port in a separate feature rather than silently changing pin guarantees here.
- **Migration collision:** schema v4 belongs exclusively to artifact publication. The v5 child is blocked on that exact story. If HEAD has advanced, rebase to the next contiguous version and update tests/registry once; never edit artifact items or merge duplicate version modules.

## Pre-mortem

The most damaging failure is a collector that appears bounded while cdpkit's hidden upstream subscriptions grow because Krometrail added duplicate or intermittently undrained streams. The design attacks that before persistence: one authority, one subscription per method/generation, subscribe-before-enable, continuously draining pumps, no per-wait subscriptions, and a sustained no-starvation/memory qualification. The fallback is explicit source/capability disablement or the documented owned transport boundary, not weakening bounds.

The next failure is a useful-looking event row that leaks a token or local path. Raw payload persistence is structurally impossible, and URL/dialog/network fields are allowlisted before handoff; free text remains the least certain area. The redaction corpus and serialized-row negative sentinels protect it, while hash/type-only console/exception fallback can reduce fidelity without changing identity/query/schema.

Finally, an event flood could consume the global budget and evict visual evidence unexpectedly. Shared retention sequence ordering, event-first metadata batches only when genuinely older, source-segment pins, classified live-page accounting, and explicit event tombstones keep the trade visible. Artifact outputs remain derived and first-evictable; capture frames remain the authoritative center.

## Review (2026-07-14)

**Verdict**: Approve after fix

**Blockers**: none

**Important finding fixed**:
- A transient event-store failure permanently left collection status at `Failed` after the writer durably persisted its gap and resumed. Focused story `bug-recover-browser-event-collection-status` reproduced the stale status, restores only a current `Failed` state to `Operational` or source-aware `Degraded` after successful writes, passed the full Rust 1.85 gate, and was reviewed/archived in `9ff5d28`, `a579d1d`, and `71fae25`.

**Nits adjudicated**:
- Effective-clip gap semantics and exact collection/unavailable truncation counts are parked together as `idea-temporal-context-clip-and-truncation-exactness` before MCP exposure.
- Rare Windows drive-relative and single-rooted-backslash path forms are parked as `idea-redact-windows-drive-relative-paths` for defense-in-depth corpus coverage.
- Reusing the bounded network capacity for low-volume page signals is harmless; no extra configuration is warranted without measured pressure.

**Evidence**: Independent cross-model standard review verified all eleven material lenses: registry authority, structural privacy, domain subscription/order/generation ownership, shared network waits, bounded nonblocking ingestion, shutdown/disable semantics, transactional v5 persistence, global accounting/retention, recovery, deterministic temporal context, and root integration. Focused Rust 1.85 checks and 369 feature-crate tests passed; the accepted status fix then passed current locked Rust 1.85 format, full workspace tests, and Clippy with warnings denied. Standard weight requires no re-review.
