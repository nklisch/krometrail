---
id: refactor-split-cdp-session-supervisor-runtime
kind: feature
stage: review
tags: [refactor, browser]
parent: null
depends_on: []
release_binding: null
gate_origin: refactor-design
created: 2026-07-14
updated: 2026-07-14
---

# Split the CDP session supervisor runtime into focused modules

## Brief

`crates/krometrail-cdp/src/session.rs` is now 3,626 lines and mixes at least four distinct responsibilities in one file: connector/bootstrap at `session.rs:1-220`, steady-state supervisor execution at `session.rs:971-1170`, reconnect orchestration at `session.rs:2108-2346`, and shutdown/process/event-pump machinery at `session.rs:2397-2668`. The page-lifecycle, waits-and-batches, MCP cancellation, and durable-memory capture work all extended this file, so edits in one slice now force readers through unrelated runtime code and increase the chance that reconnect/shutdown/session-edge changes drift together.

Extract the production session implementation into focused private modules under `crates/krometrail-cdp/src/session/` while keeping `ProductionBrowserConnector` and the feature-gated `crate::session` public surface unchanged. Preserve reducer ownership, single-writer supervision, exact reconnect/shutdown semantics, and existing session-focused tests.

**Source lens**: code smell / missing abstraction / elimination-first god-module split

**Rationale**: reduces coordination cost in the highest-churn browser-control adapter without changing behavior, and makes future reconnect/shutdown/session fixes auditable in smaller modules.

**Black-box classification**: pure refactor. Connector/session exports, runtime behavior, stable errors, logging fields, cancellation semantics, reconnect behavior, capture shutdown ordering, and test outcomes remain unchanged.

## Acceptance criteria

- [x] `crates/krometrail-cdp/src/session.rs` becomes a focused module root (`session/mod.rs` or equivalent) that re-exports the same public entry points while moving reconnect, shutdown, and event-pump/process-watch helpers into private submodules.
- [x] Reducer/application behavior remains identical: single-writer supervision, request cancellation, reconnect attempt handling, shutdown deadlines, managed/attached ownership, and capture sequencing are unchanged.
- [x] Existing tests continue to exercise the same seams; no coverage is deleted, and session-focused tests move only as needed to match the new module layout.
- [x] `cargo fmt --all -- --check`, `cargo test --workspace --all-targets --locked`, and `cargo clippy --workspace --all-targets --locked -- -D warnings` pass.

## Scope notes

- Keep this refactor inside `crates/krometrail-cdp/src/session*` and directly adjacent tests/helpers only.
- Do not redesign reconnect policy, shutdown policy, process-death detection semantics, or target reduction inputs in the same change.
- Prefer a small number of cohesive submodules over another layer of adapter indirection.

## Refactor Overview

Black-box purity verified from the current item brief, `src/lib.rs`, `crates/krometrail-cdp/src/control/batch.rs`, `crates/krometrail-cdp/tests/page_lifecycle.rs`, `crates/krometrail-cdp/tests/page_observation.rs`, `crates/krometrail-cdp/tests/verified_interactions.rs`, and `crates/krometrail-cdp/tests/waits_and_batches.rs`: this split can stay behavior-preserving because every externally observable contract already hangs off `ProductionBrowserConnector`, `BrowserSessionPort`, reducer inputs/effects, and the existing tests. The refactor only moves private implementation across files.

Direct-read scan only; the target is one bounded module plus adjacent tests. The current file divides cleanly into five implementation slices:

- connector/session shell and shared state (`session.rs:1-672`)
- steady-state transport/effect runtime (`session.rs:673-970`, `2557-2763`)
- operation dispatch and page-result assembly (`session.rs:1171-1670`)
- reconnect transaction (`session.rs:1671-2336`)
- shutdown budget and terminal cleanup (`session.rs:2337-2556`)

### Target module boundaries

- `crates/krometrail-cdp/src/session/mod.rs`
  - keeps the public `ProductionBrowserConnector` surface, `ProductionSession`, `SessionShared`, `CaptureRuntime`, `SessionCaptureObserver`, shared error mapping, and crate-local re-exports used by sibling modules such as `control/batch.rs`
  - owns `mod operations; mod reconnect; mod runtime; mod shutdown;`
- `crates/krometrail-cdp/src/session/operations.rs`
  - owns request execution and page-result assembly only
- `crates/krometrail-cdp/src/session/reconnect.rs`
  - owns reconnect attempt control, bounded target restoration, and reconnect interruption handling only
- `crates/krometrail-cdp/src/session/runtime.rs`
  - owns connection bootstrap, reducer-effect application, the supervisor loop, transport event pumps, process watch, and target/session parser helpers only
- `crates/krometrail-cdp/src/session/shutdown.rs`
  - owns aggregate shutdown budgeting, capture flush ordering, detach/browser-close/process termination, and terminal state finalization only

The final shape deliberately avoids new traits or wrapper layers. Private functions move behind `pub(super)` boundaries where sibling modules need them; `crate::session` keeps re-exporting the same crate-local helpers that current siblings already import.

## Refactor Steps

### Step 1: Convert `session.rs` into a module root and extract operation dispatch
**Priority**: High
**Risk**: Medium
**Source Lens**: code smell / missing abstraction
**Files**: `crates/krometrail-cdp/src/session.rs` → `crates/krometrail-cdp/src/session/mod.rs`, `crates/krometrail-cdp/src/session/operations.rs`
**Story**: `refactor-split-cdp-session-supervisor-runtime-step-1-session-module-root-and-operations`

**Current State**:
```rust
// crates/krometrail-cdp/src/session.rs
pub(crate) struct SessionShared { /* shared reducer/session state */ }

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct OperationExecutionContext {
    pub(crate) deadline: Option<tokio::time::Instant>,
    pub(crate) parent_batch: Option<krometrail_core::InteractionId>,
}

pub(crate) async fn execute_operation(
    page_control: &mut PageControl,
    state: &mut SupervisorState,
    transport: Arc<dyn CdpTransport>,
    shared: &Arc<SessionShared>,
    request: BrowserOperationRequest,
    cancellation: &OperationCancellation,
    context: OperationExecutionContext,
) -> Result<BrowserOperationResult> { /* 360+ lines */ }
```

**Target State**:
```rust
// crates/krometrail-cdp/src/session/mod.rs
mod operations;
mod reconnect;
mod runtime;
mod shutdown;

pub(crate) use operations::{execute_operation, OperationExecutionContext};
pub struct ProductionBrowserConnector { /* unchanged public surface */ }
pub(crate) struct SessionShared { /* unchanged shared state */ }

// crates/krometrail-cdp/src/session/operations.rs
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct OperationExecutionContext {
    pub(crate) deadline: Option<tokio::time::Instant>,
    pub(crate) parent_batch: Option<krometrail_core::InteractionId>,
}

pub(crate) async fn execute_operation(
    page_control: &mut PageControl,
    state: &mut SupervisorState,
    transport: Arc<dyn CdpTransport>,
    shared: &Arc<SessionShared>,
    request: BrowserOperationRequest,
    cancellation: &OperationCancellation,
    context: OperationExecutionContext,
) -> Result<BrowserOperationResult> { /* moved unchanged in behavior */ }
```

**Implementation Notes**:
- Make the file-to-directory move atomic in this step: `session.rs` becomes `session/mod.rs` in the same commit that introduces `session/operations.rs`.
- Move `commit_supervisor_input`, `page_success_result`, `page_failure_result`, `build_page_result`, and `transport_page_error` with `execute_operation`; keep signatures unchanged except for `pub(crate)`/private visibility appropriate to the new file.
- Keep `control/batch.rs` importing `crate::session::{OperationExecutionContext, SessionShared, execute_operation}` by re-exporting those items from `session/mod.rs`; do not churn sibling imports yet.
- Leave stable error helpers in `mod.rs` because operations, reconnect, and shutdown all consume them.

**Acceptance Criteria**:
- [ ] `crate::session` remains the crate-local import surface for `control/batch.rs`; no caller behavior or signatures change.
- [ ] `BrowserOperationRequest::Batch`, state-changing page operations, and read-only operations still flow through the exact same dispatcher logic and cancellation checks.
- [ ] `cargo test --workspace --all-targets --locked` continues to cover page lifecycle, page observation, verified interaction, and waits/batches behavior unchanged.

**Rollback**: Collapse `session/mod.rs` and `session/operations.rs` back into one file and restore the original `session.rs` module path if the file move or private re-exports introduce compile churn.

---

### Step 2: Extract aggregate shutdown budgeting and terminal cleanup
**Priority**: High
**Risk**: Medium
**Source Lens**: code smell / pattern drift
**Files**: `crates/krometrail-cdp/src/session/mod.rs`, `crates/krometrail-cdp/src/session/shutdown.rs`
**Story**: `refactor-split-cdp-session-supervisor-runtime-step-2-shutdown-runtime`

**Current State**:
```rust
// crates/krometrail-cdp/src/session.rs
enum ShutdownPhase { /* Origin .. Complete */ }
struct ShutdownDeadline { /* absolute budget */ }
struct ShutdownPlan { /* cause, ownership, capture, deadline */ }

async fn perform_shutdown(
    connection: &mut Option<ConnectionResources>,
    process: &Option<Arc<Mutex<Option<ManagedChromeProcess>>>>,
    profile: &Option<Arc<Mutex<Option<ProfileLease>>>>,
    state: &SupervisorState,
    plan: ShutdownPlan,
) -> Result<()> { /* capture flush, detach, Browser.close, process terminate */ }

fn finish_state(shared: &Arc<SessionShared>, state: &mut SupervisorState) { /* terminal publish */ }
```

**Target State**:
```rust
// crates/krometrail-cdp/src/session/mod.rs
use shutdown::{finish_state, perform_shutdown, ShutdownDeadline, ShutdownPlan};

// crates/krometrail-cdp/src/session/shutdown.rs
pub(super) enum ShutdownPhase { /* unchanged */ }
pub(super) struct ShutdownDeadline { /* unchanged */ }
pub(super) struct ShutdownPlan { /* unchanged */ }

pub(super) async fn perform_shutdown(
    connection: &mut Option<ConnectionResources>,
    process: &Option<Arc<Mutex<Option<ManagedChromeProcess>>>>,
    profile: &Option<Arc<Mutex<Option<ProfileLease>>>>,
    state: &SupervisorState,
    plan: ShutdownPlan,
) -> Result<()> { /* moved unchanged in behavior */ }

pub(super) fn finish_state(shared: &Arc<SessionShared>, state: &mut SupervisorState) { /* moved */ }
```

**Implementation Notes**:
- Move `ShutdownBudgetSource`, `TokioShutdownBudgetSource`, `ShutdownDeadline`, `ShutdownPlan`, `perform_shutdown`, and `finish_state` together so the absolute-deadline semantics stay in one file.
- Preserve the current ordering exactly: optional capture shutdown before detach, detach before `Browser.close`, `Browser.close` before managed-process termination, then profile release and terminal state publication.
- Move the shutdown-budget unit tests (`shutdown_deadline_is_consumed_once_across_capture_and_browser_cleanup`, `shutdown_deadline_exhaustion_uses_process_force_cleanup`) next to `shutdown.rs` only if the new file would otherwise need test-only re-exports; keep external integration tests untouched.

**Acceptance Criteria**:
- [ ] Capture flush order, `flush_capture: false` reconnect-exhausted behavior, and shutdown incomplete error mapping remain identical.
- [ ] Managed versus attached ownership still decides whether `Browser.close` is attempted.
- [ ] Existing shutdown-focused tests continue to prove one absolute budget across capture/browser/process phases.

**Rollback**: Move the shutdown types and functions back into `session/mod.rs`; do not keep a half-extracted shutdown budget split.

---

### Step 3: Extract reconnect transaction control without changing policy
**Priority**: High
**Risk**: High
**Source Lens**: code smell / missing abstraction
**Files**: `crates/krometrail-cdp/src/session/mod.rs`, `crates/krometrail-cdp/src/session/reconnect.rs`
**Story**: `refactor-split-cdp-session-supervisor-runtime-step-3-reconnect-transaction`

**Current State**:
```rust
// crates/krometrail-cdp/src/session.rs
async fn finish_interrupted_reconnect(/* ... */) -> Result<()> { /* stop/cancel/process-death */ }
struct AttemptCancellation { /* cancellation flag */ }
struct AttemptControl { /* deadline + command race */ }
struct PartialSessionTracker { /* temporary attach cleanup */ }
struct PreparedReconnection { /* connection + state + staged effects */ }

async fn reconstruct_connection(
    runtime: &SupervisorRuntime,
    current_state: &SupervisorState,
    attempt: AttemptControl,
) -> std::result::Result<PreparedReconnection, AttemptFailure> { /* reconnect transaction */ }

async fn reconnect_loop_transactional(
    shared: &Arc<SessionShared>,
    state: &mut SupervisorState,
    connection: &mut Option<ConnectionResources>,
    runtime: &SupervisorRuntime,
    commands: &mut mpsc::Receiver<SupervisorCommand>,
) -> bool { /* backoff, interruption, commit */ }
```

**Target State**:
```rust
// crates/krometrail-cdp/src/session/mod.rs
use reconnect::reconnect_loop_transactional;

// crates/krometrail-cdp/src/session/reconnect.rs
pub(super) async fn reconnect_loop_transactional(
    shared: &Arc<SessionShared>,
    state: &mut SupervisorState,
    connection: &mut Option<ConnectionResources>,
    runtime: &SupervisorRuntime,
    commands: &mut mpsc::Receiver<SupervisorCommand>,
) -> bool { /* moved unchanged in behavior */ }
```

**Implementation Notes**:
- Move `finish_interrupted_reconnect`, `AttemptCancellation`, `AttemptControl`, `AttemptFailure`, `PartialSessionTracker`, `PreparedReconnection`, `ReconnectInterrupt`, `discard_partial_connection`, `recordable_reconnect_targets`, `restore_one_target`, `restore_targets`, `stage_reconnection_effects`, `reconstruct_connection`, and `reconnect_loop_transactional` as one cohesive block.
- Preserve every policy boundary: refreshed HTTP endpoint resolution per attempt, bounded target count/attach concurrency, no operation replay during reconnect, cancellation/process-death interruption, and reconnect-exhausted shutdown with `flush_capture: false`.
- Keep reconnect depending directly on reducer `SupervisorInput::Reconnected` and `SupervisorInput::ReconnectExhausted`; do not add a new abstraction layer between reconnect and the reducer.
- Keep helper visibility `pub(super)` or private; this is a file move, not a new internal API.

**Acceptance Criteria**:
- [ ] Reconnect backoff ordering, interruption handling, and staged-effect restrictions remain byte-for-byte behaviorally equivalent.
- [ ] Existing reconnect unit tests still cover target cap rejection, concurrency bound, deadline/cancellation cutoff, and process-death interruption before connection commit.
- [ ] No reconnect change alters logging fields, stable error codes, or capture flush policy.

**Rollback**: Re-inline the reconnect helpers into `session/mod.rs` as a single block; do not keep a partially duplicated reconnect path.

---

### Step 4: Extract steady-state connection/runtime plumbing and leave a thin module root
**Priority**: High
**Risk**: Medium
**Source Lens**: code smell / elimination-first god-module split
**Files**: `crates/krometrail-cdp/src/session/mod.rs`, `crates/krometrail-cdp/src/session/runtime.rs`
**Story**: `refactor-split-cdp-session-supervisor-runtime-step-4-runtime-connection-and-pumps`

**Current State**:
```rust
// crates/krometrail-cdp/src/session.rs
async fn restore_session_domains<F, Fut, E>(mut send: F) -> std::result::Result<(), E> { /* ... */ }
fn parse_visibility_result(value: &Value) -> std::result::Result<TargetVisibility, VisibilityProbeError> { /* ... */ }
struct ConnectionResources { /* transport, subscriptions, targets, compatibility, pumps */ }
async fn setup_connection(/* ... */) -> std::result::Result<ConnectionResources, CompatibilityProbeError> { /* ... */ }
async fn apply_effects(/* ... */) -> Result<()> { /* reducer effects */ }
struct SupervisorRuntime { /* endpoint/factory/process/profile/config */ }
async fn run_supervisor(/* ... */) { /* steady-state command loop */ }
async fn watch_process(/* ... */) { /* managed process death */ }
async fn pump_events(/* ... */) { /* target event stream */ }
```

**Target State**:
```rust
// crates/krometrail-cdp/src/session/mod.rs
use runtime::{apply_effects, run_supervisor, setup_connection, ConnectionResources, ProcessDeathSignal, SupervisorRuntime};
pub(crate) use runtime::{parse_visibility_result, VisibilityProbeError};

// crates/krometrail-cdp/src/session/runtime.rs
pub(super) async fn restore_session_domains<F, Fut, E>(mut send: F) -> std::result::Result<(), E>
where
    F: FnMut(&'static str) -> Fut,
    Fut: std::future::Future<Output = std::result::Result<(), E>>;

pub(super) struct ConnectionResources { /* unchanged fields */ }
pub(super) async fn setup_connection(
    transport: Arc<dyn CdpTransport>,
) -> std::result::Result<ConnectionResources, CompatibilityProbeError>;

pub(super) async fn apply_effects(
    state: &mut SupervisorState,
    effects: Vec<SupervisorEffect>,
    transport: Arc<dyn CdpTransport>,
    subscribers: Arc<SubscriberRegistry>,
    capture: Option<Arc<CaptureRuntime>>,
    shutdown_deadline: Option<ShutdownDeadline>,
) -> Result<()>;

pub(super) async fn run_supervisor(
    shared: Arc<SessionShared>,
    state: SupervisorState,
    connection: Option<ConnectionResources>,
    page_control: PageControl,
    runtime: SupervisorRuntime,
    commands: mpsc::Receiver<SupervisorCommand>,
);
```

**Implementation Notes**:
- Move `TARGET_EVENT_NAMES`, `SESSION_RESTORE_DOMAINS`, `TargetEventKind`, `VisibilityProbeError`, `parse_visibility_result`, `ConnectionResources`, `setup_connection`, `setup_connection_with_target_limit`, `apply_effects`, `SupervisorRuntime`, `ProcessDeathSignal`, `run_supervisor`, `watch_process`, `pump_events`, `parse_event`, `parse_target_list`, and `parse_target_info` together.
- Keep `run_supervisor` as the only steady-state owner of reducer commits, operation dispatch, reconnect handoff, and shutdown entry. The move must not split single-writer ownership across modules.
- Preserve process-watch cadence, generation-scoped event forwarding, and exact reducer/effect commit order.
- After this step, `session/mod.rs` should read as a composition root instead of a god file: connector construction, session shell, shared types, shared error mapping, and module wiring only.

**Acceptance Criteria**:
- [ ] Initial attach still restores `Page.enable`, `Runtime.enable`, and `Accessibility.enable` exactly once before the visibility probe.
- [ ] Event pumps still tag transport inputs with `ForConnectionGeneration`, and process death still reaches reconnect/shutdown through the same command channel.
- [ ] The module root is materially smaller while reducer ownership, logging, cancellation, and connection bootstrap behavior remain unchanged.

**Rollback**: Move the runtime helpers back into `session/mod.rs`; do not leave half the steady-state loop split across files.

## Implementation Order
1. `refactor-split-cdp-session-supervisor-runtime-step-1-session-module-root-and-operations`
2. `refactor-split-cdp-session-supervisor-runtime-step-2-shutdown-runtime`
3. `refactor-split-cdp-session-supervisor-runtime-step-3-reconnect-transaction`
4. `refactor-split-cdp-session-supervisor-runtime-step-4-runtime-connection-and-pumps`

## Risks and rollback notes

- The file-to-directory rename in Step 1 is inherently atomic; landing submodules without the module-root move would break Rust module resolution.
- Steps 2 through 4 are independently reversible because each extracts one cohesive responsibility and keeps the root-level call sites unchanged apart from module qualification.
- Highest-risk seam: reconnect/shutdown interaction. Keep reconnect-exhausted shutdown semantics and no-replay cancellation behavior under the existing tests before accepting later cleanup.

## Implementation notes

- Execution capability: highest; selected by the autopilot caller because private file moves cross single-writer supervision, reconnect, shutdown, capture, and cancellation.
- Review weight: standard (caller); implementation stops at feature review for independent adjudication by the autopilot owner.
- Files changed: `session.rs` became `session/mod.rs`; operation dispatch moved to `operations.rs`, shutdown policy to `shutdown.rs`, reconnect transaction control to `reconnect.rs`, and connection/effect/pump supervision to `runtime.rs`.
- Tests added/removed: none. Existing session, capture, lifecycle, observation, interaction, wait/batch, process, transport, and reducer tests remain unchanged.
- Simplification: replaced one 3,626-line production/test module with one composition root and four cohesive private modules; introduced no traits, wrappers, policy options, or public API changes.
- Discrepancies from design: session-local tests stayed in `session/mod.rs` to preserve shared fixtures; narrow `pub(super)` seams expose moved helpers only within the parent module. The crate-local `VisibilityProbeError` re-export has an explicit unused-import allowance so its existing path remains stable.
- Adjacent issues parked: none.
- Integrated verification: Rust 1.85.0 format check, locked all-target workspace check, locked all-target workspace tests (418 passed), and locked all-target workspace Clippy with `-D warnings` all passed. The real-browser opt-in was disabled, so environment-gated Chrome cases reported their normal skip-success behavior and no live Chrome run was claimed.
