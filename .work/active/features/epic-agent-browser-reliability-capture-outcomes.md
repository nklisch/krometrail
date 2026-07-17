---
id: epic-agent-browser-reliability-capture-outcomes
kind: feature
stage: implementing
tags: [browser, storage, agent-ux]
parent: epic-agent-browser-reliability
depends_on: [durable-agent-diagnostics]
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Truthful capture and operation outcomes

## Brief

Correct GitHub issues #1, #2, and #9 by reporting browser mutation, live observation, and retained temporal capture as distinct facts. Preserve the concrete capture failure stage in durable diagnostics and surface unhealthy retained capture on subsequent operations without making current-state browser control depend on the recording pipeline.

Successful navigation or input must remain successful when only post-operation evidence degrades, with the evidence failure attached as a warning and correlated diagnostic. Automatically returned screenshots must be captured at a bounded compositor-ready boundary and remain distinguishable from retained screencast frames. A clean shutdown of an already-failed stream must not rewrite historical capture failure as cleanup failure; the managed-session feature owns the final lifecycle result.

## Epic context
- Parent epic: `epic-agent-browser-reliability`
- Position in epic: consumes durable diagnostic correlation and produces the outcome contract later documented by agent guidance.

## Simplification opportunity
- Consolidate page-operation and interaction evidence classification around one result projection instead of per-operation success rewriting.

## Foundation references
- `docs/SPEC.md` — browser-control and temporal-evidence contracts
- `docs/ARCHITECTURE.md` — capture pipeline, operation execution, and MCP projection
- `docs/VISUAL-EVIDENCE.md` — source-frame and live-screenshot distinction

## Design decisions

- **Capture failure recovery**: expose a precise terminal failure stage and warn on every later browser operation, but do not automatically restart a failed stream in this feature. Issue #1 was not reproducible in a clean store, and an automatic restart would hide repeated loss and create new capture epochs without proving the failed boundary safe.
- **Operation-versus-evidence boundary**: input dispatch or a committed page mutation is the success boundary. Completion probes, post-operation observation, interaction-evidence persistence, and retained capture are separately reportable evidence outcomes and never rewrite a proven mutation as failed.
- **Capture-health projection**: add one read-only `capture_statuses()` view to the unpublished browser-session port and let the MCP boundary turn failed statuses into warnings. This avoids an asynchronous retention-status read after every tool and keeps browser control independent of storage availability.
- **Failure vocabulary**: add the stable `capture_failed` error code and an additive `failure_stage` on capture status. The stage is a bounded safe enum, not an adapter error string; source chains remain in the durable diagnostic log supplied by `durable-agent-diagnostics`.
- **Compositor boundary**: post-operation observation waits for two renderer animation-frame callbacks under a 250 ms cap before taking its screenshot. A timeout or unsupported renderer is logged and observation proceeds, because visual readiness is a bounded quality improvement rather than a reason to erase the action result.
- **Upstream diagnostic contract**: this feature emits sanitized session/target/failure-stage events. MCP degradation uses the upstream `ResponseDiagnostics { correlation_id, log_path }` projection; no second domain diagnostic-reference type is introduced.
- **Dispatch rationale**: direct-read only. The feature spans three known seams, but their current types, call paths, and tests are explicit enough that exploratory fanout would duplicate local inspection.

## Architectural choice

Three approaches were considered:

1. **Session health plus result projection (chosen).** Keep `BrowserOperationResult` as the mutation/live-result model, expose current capture statuses through the session port, and let the MCP response mapper add `capture_failed` warnings. Correct post-dispatch classification in the CDP adapter and add a bounded compositor wait at the existing post-operation boundary. This changes few concepts, keeps capture independent of control, and gives every operation the same health behavior.
2. **A new execution envelope in core.** Return `{ result, evidence_warnings, capture_statuses }` from every browser-session execution. This could carry every evidence warning through one type, but would churn the port, batch recursion, fixtures, and callers for information already available at the session boundary.
3. **Fields on every operation result.** Add capture and evidence status to each page, interaction, wait, batch, and read-only result. This makes each value self-contained but duplicates policy across a registry that deliberately centralizes operation projection.

The chosen design uses existing separation already present in `PageOperationResult { outcome, observation }` and MCP `ToolResponse { status, warnings }`. It repairs the places that currently collapse those facts instead of replacing the result hierarchy.

The trickiest unit is the post-dispatch interaction boundary. Before dispatch, an error means the requested action did not run and the tool may fail normally. After CDP input dispatch succeeds, a completion or observation error cannot safely imply that replay is harmless. The adapter must therefore construct a dispatched interaction record and an unavailable live observation, allowing MCP to return `degraded` rather than `failed`.

## Implementation Units

### Unit 1: Stable capture failure classification

**Files**:
- `crates/krometrail-core/src/error.rs`
- `crates/krometrail-core/src/recording/session.rs`
- `crates/krometrail-core/src/recording/mod.rs`
- `crates/krometrail-core/src/lib.rs`

**Story**: `epic-agent-browser-reliability-capture-outcomes-capture-health`

```rust
define_stable_enum! {
    pub enum CaptureFailureStage {
        FrameEventStream => "frame_event_stream",
        VisibilityEventStream => "visibility_event_stream",
        FrameEnvelope => "frame_envelope",
        Acknowledgement => "acknowledgement",
        OrdinalAllocation => "ordinal_allocation",
        FrameDecode => "frame_decode",
        FramePersistence => "frame_persistence",
        GapPersistence => "gap_persistence",
    }
}

// Additive stable error-code arm.
ErrorCode::CaptureFailed => "capture_failed"

pub struct TargetCaptureStatus {
    // existing fields remain unchanged
    failure_stage: Option<CaptureFailureStage>,
}

impl TargetCaptureStatus {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        target_id: TargetId,
        attachment_generation: u64,
        state: CaptureStreamState,
        statistics: CaptureStatistics,
        queue_capacity: usize,
        queue_depth: usize,
        last_frame_session_time: Option<SessionTime>,
        ack_latency: CaptureTimingSummary,
        frame_cadence: CaptureTimingSummary,
        every_nth_frame: EveryNthFrame,
        failure_stage: Option<CaptureFailureStage>,
    ) -> Result<Self>;

    pub const fn failure_stage(&self) -> Option<CaptureFailureStage>;
}
```

**Implementation Notes**:
- `failure_stage` is `Some` exactly when `state == CaptureStreamState::Failed`; constructor and deserialize validation reject all mismatches.
- `CaptureFailureStage` names the failing boundary, not raw error text. `FrameEventStream` and `VisibilityEventStream` cover both closure and adapter read failure publicly; the durable log retains the sanitized causal class and source-chain detail.
- `CaptureFailed` uses `RetryAdvice::AfterRecovery` with recovery guidance to inspect browser status/diagnostics and start a new session before relying on temporal history again. It is distinct from `CaptureRejected`, which remains an input/admission failure.
- Adding the status field and enum arm is additive for the 1.x serialized contract; existing retained frame/gap formats do not change.

**Acceptance Criteria**:
- [ ] Every terminal runtime capture failure status identifies exactly one safe `failure_stage`.
- [ ] Non-failed capture statuses cannot carry a failure stage, including at the serde boundary.
- [ ] `capture_failed` serializes as a stable public error code with recovery that does not recommend replaying an already-dispatched action.
- [ ] Existing capture status fields and retained frame/gap formats remain readable and unchanged.

---

### Unit 2: Preserve the concrete pipeline failure stage

**Files**:
- `crates/krometrail-cdp/src/capture/pipeline.rs`
- `crates/krometrail-cdp/src/capture/tests.rs`

**Story**: `epic-agent-browser-reliability-capture-outcomes-capture-health`

```rust
struct RuntimeState {
    // existing state
    failure_stage: Option<CaptureFailureStage>,
}

impl StreamRuntime {
    fn fail(&self, stage: CaptureFailureStage) {
        // Store the first terminal stage, emit one sanitized diagnostic event,
        // close acceptance, and transition to Failed.
    }
}
```

**Implementation Notes**:
- Replace every unqualified `runtime.fail()` with the exact boundary stage. Keep the first failure stage if concurrent readers fail; later task shutdown must not overwrite the initiating cause.
- Classify malformed event metadata before queue handoff as `FrameEnvelope`; base64/header/domain construction failures as `FrameDecode`; sink frame and gap errors separately as `FramePersistence` and `GapPersistence`.
- Emit one `tracing::error!` event with `session_id`, `target_id`, `attachment_generation`, and `failure_stage`. Do not include frame bytes, event parameters, URL, title, selector, or adapter debug strings.
- Continue recording explicit `CaptureGap` evidence where the current contract requires it. Failure classification supplements rather than replaces bounded-loss accounting.
- Do not add automatic restart. The failed status remains visible until target/session teardown, and the lifecycle feature owns truthful stop classification.

**Acceptance Criteria**:
- [ ] Fault injection at event-stream, acknowledgement, frame decode, frame persistence, and gap persistence boundaries yields the expected first failure stage.
- [ ] Concurrent secondary failure cannot replace the initiating stage.
- [ ] The two-acknowledged/zero-persisted signature from issue #1 can be distinguished as event-stream, decode, frame-persistence, or gap-persistence failure instead of only `state=failed`.
- [ ] Capture failure does not change browser session readiness or block the control transport.

---

### Unit 3: Project failed capture health on every browser operation

**Files**:
- `crates/krometrail-core/src/ports/browser.rs`
- `crates/krometrail-cdp/src/session/mod.rs`
- `crates/krometrail-mcp/src/session.rs`
- `crates/krometrail-mcp/src/registry.rs`
- `crates/krometrail-mcp/src/response.rs`
- `crates/krometrail-mcp/src/server.rs`

**Story**: `epic-agent-browser-reliability-capture-outcomes-capture-health`

```rust
pub trait BrowserSessionPort: Send + Sync {
    // Existing methods remain.
    fn capture_statuses(&self) -> Vec<TargetCaptureStatus> {
        Vec::new()
    }
}

pub(crate) struct ExecutedBrowserOperation {
    pub result: BrowserOperationResult,
    pub capture_statuses: Vec<TargetCaptureStatus>,
}

impl BrowserSessionOwner {
    pub async fn execute(
        &self,
        request: BrowserOperationRequest,
        context: BrowserOperationContext,
    ) -> Result<ExecutedBrowserOperation>;
}

pub(crate) fn map_operation_result(
    tool: &str,
    result: BrowserOperationResult,
    capture_statuses: &[TargetCaptureStatus],
) -> Result<MappedResult, ResponseInvariantError>;
```

**Implementation Notes**:
- `ProductionSession::capture_statuses()` reads only the in-memory coordinator statuses; it does not call retention storage and cannot turn a successful control call into an I/O failure.
- `BrowserSessionOwner::execute` samples statuses immediately after execution and passes them to MCP mapping. Default empty status on other unpublished adapter implementations preserves test/fake simplicity.
- For each `Failed` status, append one `KrometrailError { code: CaptureFailed, context.session_id/target_id, ... }`. The safe message includes the stable failure-stage name and explicitly says current-state control may have succeeded while retained temporal frames are unavailable.
- Failed capture degrades an otherwise successful operation. It does not replace an existing operation failure, and duplicate failed statuses for old attachment generations are already removed by `CaptureCoordinator::statuses()`.
- Current-state `ImageRole::{RequestedScreenshot, LiveObservation, PostAction}` remains separate from temporal source-frame resources. A successful live screenshot never suppresses the capture warning.
- The diagnostics dependency adds `ResponseDiagnostics` whenever the resulting response is degraded/failed; this unit only supplies normal capture warnings and safe stage events.

**Acceptance Criteria**:
- [ ] Every operation-registry tool returns `degraded` plus a target-scoped `capture_failed` warning when any current target capture stream is failed.
- [ ] Successful current screenshots/images remain present and correctly role-labeled alongside the retained-capture warning.
- [ ] An already-failed operation keeps its original `error`; capture failure is an additional warning, not the primary action failure.
- [ ] Healthy, paused-budget, hidden, suspended, draining, and stopped capture states do not produce `capture_failed` warnings.

---

### Unit 4: Keep proven mutations successful when evidence degrades

**Files**:
- `crates/krometrail-cdp/src/control/interaction.rs`
- `crates/krometrail-cdp/src/control/pages.rs`
- `crates/krometrail-cdp/src/control/navigation.rs`
- `crates/krometrail-cdp/src/session/operations.rs`
- `crates/krometrail-cdp/tests/page_lifecycle.rs`
- `crates/krometrail-cdp/tests/waits_and_batches.rs`
- `crates/krometrail-mcp/src/response.rs`

**Story**: `epic-agent-browser-reliability-capture-outcomes-truthful-operation-evidence`

```rust
impl PageControl {
    fn unavailable_post_dispatch_observation(
        &self,
        bound: &BoundTarget,
        started_at: SessionTime,
        error: KrometrailError,
    ) -> Result<LiveObservation>;
}

// Page-operation success paths always retain their proven change:
PageOperationResult::new(
    interaction,
    PageOperationOutcome::Succeeded(change),
    observed.observation,
)
```

**Implementation Notes**:
- In `navigation_success`, `page_success_result`, and successful close-page handling, stop mapping `PostOperationObservation::interruption` to `PageOperationOutcome::Failed`. The observation already carries its own unavailable error and MCP already maps it to warnings/degradation.
- Keep navigation failure when dispatch or commit itself was rejected/unproven. `mutation_accepted` remains useful for collecting current evidence after an uncertain navigation but does not manufacture success.
- In `execute_interaction_request`, preserve the existing fail-before-dispatch behavior. After `dispatch_action` returns `Ok(())`, completion probe errors/timeouts/cancellation produce an `InteractionResult` whose record is `InteractionOutcome::Dispatched` and whose live observation has unavailable parts carrying a target/session/interaction-scoped `PageObservationFailed` error.
- Open-dialog handling remains its specialized blocked observation. Evidence failures must not suggest replay because the input may already have changed the page.
- Keep interaction-evidence persistence mandatory before dispatch when no sink exists. A post-dispatch sink failure remains the existing explicit `PersistenceFailed` uncertainty boundary and is not mislabeled as interaction failure.

**Acceptance Criteria**:
- [ ] A committed reload with interrupted or failed live observation maps to a succeeded page outcome and a degraded MCP response, never `navigation_failed`.
- [ ] A CDP input dispatch followed by completion-probe failure returns a dispatched interaction anchor and degraded observation instead of generic `interaction_failed`.
- [ ] A dispatch failure before input reaches CDP still returns the specific failed operation with no false dispatched record.
- [ ] MCP summaries and `is_error` agree: incomplete evidence after a proven action is degraded/non-error; an unproven action remains failed/error.

---

### Unit 5: Bounded compositor-ready post-action screenshots

**Files**:
- `crates/krometrail-cdp/src/control/pages.rs`
- `crates/krometrail-cdp/src/control/interaction.rs`
- `crates/krometrail-cdp/src/control/screenshot.rs`
- `crates/krometrail-cdp/tests/page_observation.rs`
- `crates/krometrail-cdp/tests/page_lifecycle.rs`
- `tests/fixtures/browser/page-observation/index.html`

**Story**: `epic-agent-browser-reliability-capture-outcomes-compositor-readiness`

```rust
const COMPOSITOR_READY_TIMEOUT: Duration = Duration::from_millis(250);

impl PageControl {
    async fn await_compositor_ready(
        &self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        cancel: &OperationCancellation,
        connection_generation: u64,
    ) -> CompositorReadiness;
}

enum CompositorReadiness {
    Ready,
    ProceededWithoutSignal,
}
```

**Implementation Notes**:
- Evaluate a bounded promise that resolves after two nested `requestAnimationFrame` callbacks. Use the existing operation cancellation fence and `Runtime.evaluate { awaitPromise: true, returnByValue: true, silent: true }`.
- Call this helper only for automatically returned post-action observations: once after interaction completion and once in `observe_after_operation` for page mutations. Standalone `observe_live` and explicit `take_screenshot` remain immediate caller-requested observations.
- Timeout, unsupported animation frames, hidden/background target, or renderer evaluation error returns `ProceededWithoutSignal`; emit a sanitized debug/warn diagnostic and continue to `Page.captureScreenshot`. The entire action remains bounded by the existing operation cancellation/deadline.
- Do not detect “dark” pixels or compose screenshots heuristically; legitimate applications can be dark, and `Page.captureScreenshot(fromSurface=true)` is already a complete current-state image rather than a screencast damage-frame API.
- Extend the page-observation fixture with an action that applies its final full-viewport style on animation frames, providing deterministic call-order qualification without embedding Krometrail logic in the fixture.

**Acceptance Criteria**:
- [ ] Automatic post-action capture issues the two-frame readiness probe before `Page.captureScreenshot`.
- [ ] Explicit screenshots and standalone live observations do not incur the readiness wait.
- [ ] A renderer that never produces the signal proceeds within 250 ms (subject to cancellation scheduling) and does not convert the action into failure.
- [ ] Opt-in real-Chrome qualification observes the frame-delayed fixture in its complete final viewport state after the action.

---

### Unit 6: Roll the public contract forward

**Files**:
- `docs/SPEC.md`
- `docs/ARCHITECTURE.md`
- `docs/EVALUATION.md`
- `docs/guide/troubleshooting.md`

**Stories**: all three checkpoints

**Implementation Notes**:
- State explicitly that action/mutation, current-state observation, and retained capture are independent outcomes; capture failure degrades every later operation without disabling control.
- Document `failure_stage` and `capture_failed` as safe public diagnostics while detailed source causes remain in the bounded local log.
- Replace the Promise/microtask implication in interaction execution with the bounded compositor-ready policy. Keep current screenshots distinct from authoritative retained source frames.
- Add the issue #1 counter signature and frame-delayed post-action fixture to evaluation/troubleshooting as diagnostic/qualification cases, not as historical release notes.
- Regenerate `docs/public/llms-full.txt` with `bun run docs:build`; never edit it directly.

**Acceptance Criteria**:
- [ ] Foundation docs describe the new intended contract without historical/migration prose.
- [ ] Troubleshooting directs operators from `capture_failed` and its correlation metadata to the targeted bounded log excerpt workflow defined by durable diagnostics.
- [ ] Generated documentation matches the source pages byte-for-byte after the documented build.

## Implementation Order

1. Add stable capture failure types and classify pipeline boundaries (`capture-health`).
2. Expose in-memory capture status through the session port and degrade every MCP operation when failed (`capture-health`).
3. Correct page and interaction post-dispatch outcome classification (`truthful-operation-evidence`).
4. Add the bounded two-frame compositor wait on top of the truthful degradation boundary (`compositor-readiness`).
5. Roll foundation, evaluation, troubleshooting, and generated docs forward across the completed behavior.
6. Run focused tests followed by the full workspace quality gate and real-Chrome qualification where the host provides Chrome.

## Child stories

- `epic-agent-browser-reliability-capture-outcomes-capture-health` — failure-stage preservation and every-operation health warnings — depends on: `[]`
- `epic-agent-browser-reliability-capture-outcomes-truthful-operation-evidence` — proven mutation/dispatch remains success when observation degrades — depends on: `[]`
- `epic-agent-browser-reliability-capture-outcomes-compositor-readiness` — bounded renderer frame boundary before automatic screenshots — depends on: `[epic-agent-browser-reliability-capture-outcomes-truthful-operation-evidence]`

The feature remains the normal implementation and review bundle; these stories are heterogeneous acceptance checkpoints, not separate worker assignments. `.work/bin/work-view` could not execute on this macOS host because the checked-in binary is Linux ELF, so cycle validation was performed manually: both root stories have no dependencies, and the compositor story depends only on its sibling with no reverse path.

## Simplification

- Reuse `PageOperationResult`'s existing outcome/observation split instead of introducing an execution envelope or fields on every operation variant.
- Centralize capture-health warning projection in `map_operation_result`; do not duplicate checks in 24 registry handlers.
- Replace unqualified capture failure calls with one stage-bearing path and one diagnostic emission.
- Retain `Page.captureScreenshot(fromSurface=true)` and remove no image validation; dark-pixel heuristics or screencast-frame composition would add false assumptions.
- Keep standalone observation immediate and make compositor waiting exclusive to automatic post-action evidence.

## Testing

- **Core invariant tests** in `recording/session.rs`: protect the additive status contract and serde rejection of failed/stage mismatches.
- **Capture boundary fault tests** in `capture/tests.rs`: protect root-cause preservation for event stream, acknowledgement, decode, frame persistence, and gap persistence without requiring the original machine-specific failure to reproduce.
- **MCP response tests** in `response.rs` and `server.rs`: protect every-operation degraded warnings, original failure precedence, diagnostic correlation integration, and live-versus-temporal image roles.
- **Interaction/navigation regression tests** in `page_lifecycle.rs` and `waits_and_batches.rs`: protect against replay-encouraging false failures after proven dispatch/commit.
- **Transport ordering tests** in `page_observation.rs`: protect the two-animation-frame probe ordering and its bounded fallback without coupling to pixels.
- **Opt-in real-browser qualification** against the frame-delayed fixture: protects the Chrome compositor seam implicated by issue #9. It supplements deterministic tests and may skip only through the repository's existing explicit Chrome opt-in mechanism.
- **Full gates**: `cargo fmt --all -- --check`, `cargo check --workspace --all-targets --locked`, `cargo test --workspace --all-targets --locked`, and `cargo clippy --workspace --all-targets --locked -- -D warnings`; then runtime `--version`, `--help`, and `doctor` smoke checks.
- No existing useful tests are removed. Update constructor fixtures mechanically for the additive `failure_stage` argument; do not duplicate those updates as separate behavior tests.

## Risks

- **Riskiest assumption**: two animation frames are a sufficient practical compositor boundary across Chrome versions. The fallback is to preserve the helper boundary and qualify a Page lifecycle/CDP signal later; the 250 ms cap prevents hidden targets from stalling control.
- **Machine-specific capture failure remains unreproduced**: stage-preserving fault tests and default durable logs make the next occurrence actionable, while every-operation warning prevents silent evidence loss. Automatic restart remains deliberately out of scope until a captured failure proves it safe.
- **Warning volume**: every operation will repeat a failed target warning. This is intentional prominence; warnings are bounded by one current generation per target and do not duplicate source chains.
- **Stable serialization growth**: `capture_failed` and `failure_stage` are additive, but generated MCP schemas and checked-in contract fixtures must be regenerated and reviewed for exact names.
- **Interaction uncertainty**: after a successful input dispatch, completion failure cannot prove application handling. Reporting `dispatched` rather than `succeeded` preserves that distinction and avoids unsafe automatic replay.
