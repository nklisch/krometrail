---
id: epic-agent-browser-operation-waits-and-batches
kind: feature
stage: implementing
tags: [browser, agent-ux]
parent: epic-agent-browser-operation
depends_on: [epic-agent-browser-operation-browser-page-lifecycle, epic-agent-browser-operation-verified-interactions]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Explicit Waits and Ordered Batches

## Brief

Provide deliberate synchronization for elapsed time, text or element state, navigation, page conditions, and explicitly requested network quiet. Then compose lifecycle, navigation, and interaction operations into ordered per-target batches with per-step status, timing, and interaction anchors, stop-on-first-failure by default, opt-in continuation, optional per-step screenshots, and one final live observation.

Batches reuse the exact standalone operation registry, validation, execution, cancellation, timing, evidence, and interaction-anchor policies. This feature coordinates operations and reports partial outcomes; it does not introduce implicit global waits, cross-target ordering, durable storage, temporal queries, or MCP registration.

## Epic context and grounding

- Parent epic: `epic-agent-browser-operation`.
- This is the integration feature after page lifecycle and verified interactions. Both dependencies are implementation-complete at the design boundary; verified interactions is currently at review and its committed executor/registry is the input contract.
- Authoritative anchors: `docs/VISION.md`, `docs/SPEC.md`, `docs/ARCHITECTURE.md`, `docs/VISUAL-EVIDENCE.md`, and `docs/EVALUATION.md`.
- Existing seams: `BrowserOperationRequest`/`BrowserOperationResult` and `BROWSER_OPERATION_REGISTRY` in `krometrail-core`; `BrowserSessionPort::execute`; the single-writer supervisor in `krometrail-cdp/src/session.rs`; `PageControl`; `observe_live`; `OperationCancellation`; the snapshot resolver; lifecycle navigation completion; interaction action-family executors; `InteractionAnchor`, `InteractionTiming`, and `InteractionRecord`.
- The current `InteractionRecord.parent_batch: Option<InteractionId>` is the existing correlation seam for child interaction records. It is populated by execution context, not by changing public action requests.

## Design decisions

### One registry and one executor

Add `Wait` and `Batch` to the existing macro-backed operation declaration. Add registry metadata describing whether an operation is batchable; derive validation, operation kind, tagged request/result association, scope, display, and batch admission from that same declaration. There is no `WaitKind` registry, batch-only action enum, MCP schema mirror, or second CDP command router.

The batch adapter invokes the same standalone operation dispatcher for every child. It supplies an internal execution context containing the batch correlation id, one absolute batch deadline, and the shared cancellation token. Child requests are not rewritten and no action is replayed after reconnect. Existing action-specific validation, target resolution, completion policy, live evidence, and stale-reference behavior remain authoritative.

### Scope is one target

`BatchRequest` has one `PageSelection`. Every child must be page-scoped, must not be another batch, and must resolve to that same target. Browser-scoped operations (`list_pages`, `create_page`, `select_page`) and `close_page` are rejected because they change the target set or make the required final observation impossible. `observe_live` is reserved for batch finalization; explicit screenshot, inspect, snapshot, evaluation, navigation, interaction, and wait children may be admitted according to registry metadata. A `Selected` child is resolved by the same session selection at dispatch time; if it no longer names the batch target, the child fails explicitly rather than crossing targets.

There is no ordering guarantee between separate batches or standalone operations targeting different pages. The supervisor's existing serialized operation path remains the only ordering authority.

### Waits are explicit and never implicit

No navigation, click, fill, or other standalone operation gains a hidden network-idle or stabilization wait. A caller must submit `Wait` or include a wait step in a batch. Existing action completion remains `InputAcknowledged`, `Settled`, or bounded `NavigationAware` as declared by the action registry.

### Time is one monotonic deadline

Wait and batch deadlines are based on the session's monotonic clock/Tokio deadline, never wall-clock arithmetic. A wait has one absolute deadline created before its first probe. A batch has one absolute deadline created before its first child; a child receives the earlier of its own wait deadline and the batch deadline. Polling, event subscription, screenshotting, and final observation all consume that budget. Cancellation and browser disconnect win over timeout through the existing `OperationCancellation::race` path. No poll or child resets the parent deadline.

## Public core contracts

The contracts live in `krometrail-core::browser::wait` and `krometrail-core::browser::batch`, re-exported from `browser::mod` and `krometrail-core::lib`. Constructors enforce invariants; wire deserialization uses private wire structs plus `deserialize_validated`, matching existing observation and interaction contracts. Serde carries stable snake-case tags and bounded integer millisecond durations, not adapter-specific instants, CDP request ids, backend node ids, or transport session ids.

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaitTextMatch { Contains, Exact }

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaitPresence { Present, Absent }

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ElementState {
    Attached,
    Visible,
    Hidden,
    Enabled,
    Disabled,
    Editable,
    Checked,
    Unchecked,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UrlMatch { Exact, Prefix }

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "condition", content = "value", rename_all = "snake_case")]
pub enum WaitCondition {
    Elapsed { duration: Duration },
    Text {
        locator: Option<ElementLocator>, // None means document text
        text: NonEmptyText,
        match_mode: WaitTextMatch,
        presence: WaitPresence,
        case_sensitive: bool,
    },
    Element { locator: ElementLocator, state: ElementState },
    Navigation {
        readiness: DocumentReadiness,
        url: Option<(UrlMatch, NonEmptyText)>,
    },
    Page { expression: NonEmptyText }, // must evaluate to JSON boolean true
    NetworkQuiet { quiet_for: Duration },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct WaitRequest {
    pub target: PageSelection,
    pub condition: WaitCondition,
    pub timeout: Duration,
    pub poll_interval: Duration,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum WaitOutcome {
    Satisfied { matched_at: SessionTime },
    TimedOut { last_probe_at: SessionTime },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct WaitResult {
    pub context: ObservationContext,
    pub condition: WaitCondition,
    pub outcome: WaitOutcome,
    pub last_probe: Option<WaitProbe>,
}
```

`WaitProbe` is a bounded diagnostic projection, not raw page data: it records the last boolean match, observed URL/readiness for navigation, matched text length, element-state projection, or in-flight network count. It never includes page text, credentials, request headers, URLs beyond the requested predicate, or CDP ids. The result has no interaction anchor because a wait is read-only. Its `ObservationContext` supplies session, target, attachment generation, and monotonic start/completion timing.

`WaitRequest::new` rejects zero or overflowing durations, a timeout above the product's bounded operation ceiling (120 seconds), an elapsed duration greater than its timeout, a poll interval below 10 ms or above 5 seconds, empty expressions/text, invalid locators, and a network quiet period greater than the timeout. The wire form represents durations as non-negative integer milliseconds and rejects fractional, negative, zero, or overflow values. `Page` expressions are validated as non-empty side-effect-free evaluation requests and the adapter accepts only a boolean JSON result; `undefined`, objects, thrown exceptions, promises that exceed the operation budget, and non-boolean values are failures, not false matches. CSS selectors and URL predicates retain their existing validation and length bounds.

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchFailurePolicy { StopOnFailure, ContinueOnFailure }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BatchOptions {
    pub failure_policy: BatchFailurePolicy,
    pub include_step_screenshots: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BatchRequest {
    pub target: PageSelection,
    pub steps: Vec<BrowserOperationRequest>,
    pub timeout: Duration,
    pub options: BatchOptions,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchStepStatus { Succeeded, Failed, Skipped }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchSkipReason {
    PriorFailure,
    BatchCancelled,
    BatchTimedOut,
    TargetUnavailable,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BatchStepResult {
    pub index: u32,
    pub operation: BrowserOperationKind,
    pub target_id: TargetId,
    pub status: BatchStepStatus,
    pub started_at: SessionTime,
    pub completed_at: SessionTime,
    pub interaction: Option<InteractionAnchor>,
    pub result: Option<BrowserOperationResult>,
    pub error: Option<KrometrailError>,
    pub skip_reason: Option<BatchSkipReason>,
    pub screenshot: ObservationPart<EncodedScreenshot>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchOutcome {
    Completed,
    CompletedWithFailures,
    StoppedOnFailure,
    Cancelled,
    TimedOut,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BatchResult {
    pub batch_id: InteractionId,
    pub target_id: TargetId,
    pub started_at: SessionTime,
    pub completed_at: SessionTime,
    pub outcome: BatchOutcome,
    pub steps: Vec<BatchStepResult>,
    pub final_observation: ObservationPart<LiveObservation>,
}
```

The exact result enum is reused in `BatchStepResult`; the existing result enum is intentionally a domain value rather than a directly serialized wire enum, so the later MCP adapter emits its stable tagged response envelope from the same registry instead of duplicating operation execution or validation. The core enum's recursive batch result is boxed. `BatchRequest::new` requires 1–64 steps, a bounded nonzero batch timeout, and a target. It rejects nested batches, browser-scoped or non-batchable operations, `close_page`, and a child whose explicit page selection or reference target contradicts the batch target. It does not resolve a reference early: stale/reference actionability checks still happen immediately before that child executes.

`batch_id` is allocated from the existing `IdSource` as an `InteractionId` solely because `InteractionRecord.parent_batch` already owns that correlation type. It is not presented as a child interaction anchor and does not make `Batch` an input action. Each state-changing child returns its normal `InteractionAnchor`; each `InteractionRecord` receives `parent_batch: Some(batch_id)`. Read-only waits and inspections have no anchor and report `interaction: None`. This preserves the existing anchor invariant instead of inventing a second timeline identity.

The request types use private wire structs and custom `Serialize`/`Deserialize` where needed so durations are represented as integer milliseconds; the result types follow the existing domain-result convention and are translated at the MCP boundary.

The registry adds:

- `Wait(WaitRequest) => WaitResult`, `ReadOnly`, page-scoped, requested-only evidence, batchable.
- `Batch(BatchRequest) => BatchResult`, state-changing orchestration, page-scoped, live-observation evidence, not batchable.
- `batchable: bool` metadata to the existing `BrowserOperationDefinition`.

The existing action definitions, `CompletionKind`, `InteractionTiming`, `InteractionRecord`, `ObservationPart`, and `LiveObservation` remain the source of truth. Registry tests become exhaustive for the two new variants and their batchability/scope/mutability metadata.

## Wait execution semantics

The wait executor is an adapter module beside `control/interaction.rs` and `control/navigation.rs`; it is not a new session or automation engine. It binds one target and attachment generation, creates the absolute deadline, and dispatches one of the following probe strategies:

- **Elapsed:** no CDP polling; a cancellable sleep until `started + duration`, then `Satisfied`. The outer timeout still applies.
- **Text:** poll a bounded `Runtime.callFunctionOn`/evaluation projection. With no locator, inspect document text; with a CSS locator, re-query each time; with a snapshot `NodeReference`, use the existing resolver and return `StaleReference` on generation/document replacement rather than silently refreshing. `Present`/`Absent`, exact/contains, and case sensitivity are applied to the returned bounded text projection.
- **Element state:** use the existing snapshot/reference resolver for references and the existing selector path for CSS. Each probe returns only the requested state. `Attached`, visibility, enabled/disabled, editable, and checkedness are derived from one bounded DOM projection. A missing CSS element is a false result for positive states and a true result for `Hidden`/`Absent`-like negative states. A stale explicit reference is an explicit stale-reference failure.
- **Navigation:** subscribe to the existing named `Page.lifecycleEvent` stream before the first probe, use lifecycle events as an early wakeup, and poll `read_document` as the authority. `DocumentReadiness` maps to `loading`, `interactive`, and `complete`; an optional exact/prefix URL predicate is checked against the same document projection. Events are hints for promptness, not proof; event loss falls back to polling.
- **Page:** invoke the same side-effect-free, bounded evaluation path as `EvaluatePage`, with `await_promise: false`, and require a boolean `true`. Polling errors are returned immediately except a transient event/transport condition that the existing cancellation/reconnect policy classifies explicitly.
- **Network quiet:** only when the caller chooses `NetworkQuiet`, enable/reuse the target's Network domain and subscribe to `Network.requestWillBeSent`, `Network.loadingFinished`, and `Network.loadingFailed` for the wait lifetime. Track request ids from subscription onward. A quiet interval is satisfied only after the tracked set is empty continuously for `quiet_for`; a new request resets the interval. WebSocket and EventSource-style long-lived channels are excluded from the finite-request count and are disclosed in `WaitProbe`/limitations. Requests that began before subscription cannot be reconstructed by this operation and may be missed; the result states that quiet means “no tracked finite requests observed by this wait,” not “the page is globally idle.” If the transport cannot provide the named event stream or the Network domain cannot be enabled, the explicit request fails `Unsupported` rather than returning a false quiet result.

The polling timer is a wakeup, not a deadline. `tokio::select!` (cancellation first, then absolute deadline, then timer/event/probe) prevents cancellation from waiting for a long command. Each CDP probe is passed through the same bounded transport and operation cancellation path. Event subscriptions are operation-scoped and dropped on completion; they do not become implicit global network-idle policy or a second event recorder. The continuous browser-event recorder, when present, remains independent.

A condition that becomes true at the deadline is accepted only if its probe completed before the deadline. Otherwise the result is `TimedOut` with `WaitTimedOut`, the last bounded probe, and exact target/context. Timeout is a structured result in a batch step; standalone `Wait` returns its `WaitResult` without pretending that timeout is success. Browser disconnect, cancellation, stale references, target failure, evaluation failure, and unsupported event capability retain their existing stable error codes and retry advice.

## Batch execution semantics

The batch coordinator lives in the existing session operation path. It binds the target once for admission, allocates `batch_id`, validates the registry-derived child set, then executes children sequentially through the same standalone dispatcher. It never calls CDP action methods directly and never clones navigation, interaction, locator, screenshot, or error mapping logic.

For each child it:

1. records child start time and checks the shared cancellation/deadline;
2. verifies the child still resolves to the admitted target;
3. invokes the ordinary operation executor with `parent_batch: Some(batch_id)` and the remaining absolute deadline;
4. preserves the concrete standalone result, extracts its normal interaction anchor when present, and records child completion time;
5. obtains a per-step screenshot only when requested and the child result did not already provide one through its normal live observation;
6. applies `StopOnFailure` (default) or `ContinueOnFailure`.

A child `PageOperationResult` with `PageOperationOutcome::Failed` is failed; an `InteractionResult` that returns normally with a live observation is successful even if that observation has an unavailable component, because the mutation and evidence failure are already represented separately by `ObservationPart`; a top-level adapter error becomes the child error with target context. A `WaitResult::TimedOut` is failed. Invalid batch input is rejected before execution and has no partial result.

`ContinueOnFailure` continues after ordinary step failures, but never suppresses cancellation, browser disconnect, target closure/failure, global batch timeout, or an unrecoverable transport error. Steps not run receive `Skipped` with an explicit reason and no fabricated timing, anchor, or screenshot. A successful child after an earlier failure remains successful; the final `BatchOutcome` is `CompletedWithFailures`. Default policy returns `StoppedOnFailure`; it does not roll back already-applied browser state. There is no transactional or cross-target batch guarantee.

After the last executed/skipped step, the coordinator performs exactly one `observe_live` for the batch target, even when a prior step failed, unless the target is unavailable or the shared cancellation/deadline prevents it. That result is `final_observation`; it is an explicit `ObservationPart`, never an assumed success. Per-step screenshots are obtained through the existing screenshot path and are not stored or queried through temporal APIs. A final observation failure does not erase successful child results; it changes the batch outcome to a degraded terminal status in the result's structured observation/error projection and is visible to the caller.

## Adapter integration

- Extend `krometrail-core/src/browser/operation.rs` and module exports with the two variants and registry metadata.
- Add `wait.rs` and `batch.rs` under `krometrail-cdp/src/control/`, reusing `PageControl`, `bind_target`, `read_document`, snapshot resolution, `OperationCancellation`, `CdpTransport`, `TransportEvents`, screenshot helpers, and `observe_live`.
- Add an internal `OperationExecutionContext` (deadline, cancellation, optional parent batch) at the session/control boundary. Do not add parent/deadline fields to every public request. Existing standalone calls use the context with no parent and their current bounded policies.
- Route `Batch` from `execute_operation` into the coordinator; route `Wait` from `PageControl` through the same `BrowserSessionPort::execute` path. The supervisor remains the sole writer of target state and selection.
- Reuse the initial attachment/domain restoration path for `Page`, `Runtime`, `Accessibility`, and operation-scoped `Network` enablement. No second browser/session manager, event broker, storage writer, timeline writer, temporal query, or MCP registration is introduced.
- Keep sensitive text, expressions, selectors, URLs, network identifiers, and screenshots out of error strings and logs. Sanitized child interaction records continue to use the verified-interactions registry sanitizers.

## Implementation units and dependency graph

```text
core-contracts (no dependency)
        |
        v
wait-executor (core-contracts)
        |
        v
batch-coordinator (wait-executor)
        |
        v
qualification-and-wiring (batch-coordinator)
```

The four child stories are deliberately sequential checkpoints because all four extend the shared operation registry and session/control execution path. They are not separate implementation-agent ownership units. The feature owner should keep one coherent context across them.

### Child stories

- `epic-agent-browser-operation-waits-and-batches-core-contracts` — public wait/batch types, validated Serde, operation registry metadata, errors, and core unit tests. Depends on `[]`.
- `epic-agent-browser-operation-waits-and-batches-wait-executor` — explicit condition probing, absolute deadlines, cancellation, lifecycle/network events, and adapter tests. Depends on core contracts.
- `epic-agent-browser-operation-waits-and-batches-batch-coordinator` — target admission, sequential child dispatch, parent correlation, failure/continue policy, per-step screenshots, and final live observation. Depends on wait executor.
- `epic-agent-browser-operation-waits-and-batches-qualification-and-wiring` — composition wiring, full integration contracts, deterministic transport tests, real Chrome fixtures, and locked workspace gates. Depends on batch coordinator.

## Test and acceptance plan

### Core contract tests

- Exhaustive operation-registry count/order/kind/stable-name/scope/mutability/batchability assertions.
- Wait constructor and wire tests for every condition, bounded durations, elapsed/timeout relation, poll bounds, invalid selector/reference forms, expression/value rules, and network quiet limits.
- Element/text/navigation/page-condition serde round trips preserve tags and reject malformed or out-of-range input.
- Batch validation rejects empty/oversized/nested/browser-scoped/close/final-observation child operations and target mismatches while accepting selected and explicit same-target children.
- Batch results preserve child result kinds, optional anchors, skipped reasons, monotonic timing, bounded screenshot projection, and final observation parts.

### Deterministic adapter tests

Use the existing scripted transport/event fixtures and fake monotonic clocks. Cover immediate satisfaction, delayed satisfaction, elapsed sleep, repeated false probes, timeout at the absolute deadline, cancellation during sleep/probe/event wait, disconnect, stale reference, navigation event loss with polling fallback, non-boolean page conditions, and network request start/finish/failure/reset semantics. Assert no extra poll after deadline, no request-id or raw page data leaks, and no event subscription survives completion.

Batch tests cover ordered dispatch, same-target admission, selected-target stability, default stop, opt-in continuation, skipped-step reasons, terminal cancellation/disconnect behavior, child anchor and `parent_batch` propagation, child timing monotonicity, screenshot policy, exactly one final observation, final-observation degradation, no rollback, and no cross-target or nested execution.

### Real Chrome qualification

Add a local browser fixture with controls that become visible/enabled, delayed text, delayed navigation/readiness, a boolean page condition, finite XHR/fetch requests, and a long-lived connection. Run real Chrome tests for each wait kind, explicit network quiet limitations, stale reference after replacement/navigation, cancellation/deadline, and ordered batches combining navigation, interaction, waits, failure, continuation, per-step screenshots, and final observation. Verify browser state and returned contracts, not merely command completion. Run the locked workspace format/check/test/Clippy gates and the existing browser-control qualification suite. Linux is required; the existing macOS CI path runs the same deterministic/real-Chrome qualification where available and records unavailable platform evidence honestly.

## Acceptance boundary and non-goals

This feature is complete only when every child checkpoint is verified, the public operation registry and executor use one path for standalone and batch operations, every timeout/cancellation/degraded-evidence path is explicit, and real Chrome demonstrates all requested wait and batch modes.

It does not persist interaction records, add SQLite tables, expose temporal anchors or artifact queries, add browser-event MCP tools, add implicit network-idle behavior, support cross-target batches, guarantee rollback, replay operations after reconnect, or register MCP tools. The downstream MCP feature translates these contracts and derives its schemas from the registry.

## Design notes

- Dispatch capability: highest/raised capability selected by active autopilot because this is a cross-cutting public contract and CDP/session integration boundary; no subagents or questions were used.
- Review weight: standard from active autopilot/project defaults; this design is a substrate checkpoint, not an implementation or review pass.
- Simplification: one registry, one resolver, one standalone executor, one cancellation/deadline path, and one live-observation seam are extended rather than duplicated.
- Intentional deviation from a generic “batch all operations” design: browser-scoped mutations, close, nested batch, and child live observation are excluded so the one-target/final-observation invariant remains enforceable. Explicit screenshots remain available through the existing screenshot contract.
- Adjacent storage, temporal, MCP, and cross-target ordering concerns remain explicitly out of scope.
