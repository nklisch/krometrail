---
id: epic-agent-browser-operation-browser-page-lifecycle
kind: feature
stage: review
tags: [browser, agent-ux]
parent: epic-agent-browser-operation
depends_on: [epic-agent-browser-operation-page-observation]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Browser and Page Lifecycle

## Brief

Turn the existing supervised Chrome connection into the ordinary browser workspace an agent can operate. Expose start, explicit attach, stop/detach, and status together with page listing, creation, selection, closure, navigation, reload, and backward/forward history, returning post-operation live observations and interaction anchors for every state-changing standalone page operation.

Reuse the production connector's managed-profile defaults, exact target identities, local endpoint validation, capability-probed Electron renderer attachment, reconnect, and ownership-correct shutdown. This feature adds control services and CDP page operations; it does not rebuild process or target supervision, define rich element interactions, batch operations, persist interaction history, or register MCP tools.

## Epic context

- Parent epic: `epic-agent-browser-operation`
- Position in epic: consumer of `epic-agent-browser-operation-page-observation`; can progress independently of rich interaction after that shared boundary lands
- Inherited decisions: isolated reusable managed profiles remain the default; attach and temporary/named profile workflows are explicit; Electron Node main-process control remains excluded

## Simplification opportunity

- Extend the existing `BrowserConnector`, supervised session, exact-key target state, and generated browser-operation registry rather than creating a second lifecycle service, active-page target registry, profile manager, reconnect loop, or Electron-specific adapter.

## Foundation references

- `docs/VISION.md` — Core Experience and Local-First Operation
- `docs/SPEC.md` — Browser Lifecycle, Sessions and Targets, Current-State Observation, and Browser-Control Surface
- `docs/ARCHITECTURE.md` — Browser Connection, Target Lifecycle, Interaction Execution, and Failure Isolation
- `docs/EVALUATION.md` — Browser-Control Evaluation

## Design decisions

- **Dispatch:** Direct local probes only. The caller prohibited subagents, peer review, and pushes; the completed page-observation implementation, current connector/supervisor, launcher/profile guards, compatibility probe, and scripted/real tests resolve the design locally.
- **Lifecycle surface:** Keep `BrowserConnector::connect(BrowserConnectRequest::{Launch, Attach})` and `BrowserSessionPort::stop` as the start/attach/stop lifecycle. Add one coherent `BrowserSessionPort::status` snapshot instead of introducing a workspace singleton or second session manager. The later MCP adapter will own the returned session handle and translate lifecycle requests; this feature does not pre-register tools.
- **Default profile:** `LaunchBrowser::default()` selects a reusable managed profile named `default`. Another reusable name and a temporary profile remain explicit. Reusable profiles retain browser state after stop; temporary profile directories are deleted only after the owned browser is terminated; attached sessions always report an external profile and never acquire a lease.
- **Profile status:** Evolve `ProfileRef::Managed` to carry both identity and `Reusable | Temporary` persistence. A generated temporary identity is still opaque to callers, but status no longer misrepresents it as reusable. No path is exposed through core, status, logs, or errors.
- **Connection acceptance:** Launch and attach continue through the same local-endpoint resolution, `setup_connection`, compatibility registry, exact target reconciliation, and flat-session supervision. Electron support is accepted only when the observed endpoint is an Electron renderer and all required renderer capabilities pass; a Node inspector or branded-but-incapable endpoint remains rejected. There is no Electron branch in page control.
- **Selected page ownership:** Add `selected_target_key: Option<String>` to the existing single-writer `SupervisorState`. The exact browser target key—not URL/title and not a second map—is the reconnect-stable selection identity. Public status resolves it to `TargetId`. The reducer owns initial choice, explicit selection, close/failure fallback, and reconnect preservation.
- **Target addressing:** Add `PageSelection::{Selected, Target(TargetId)}` and migrate target-scoped observation requests to it now. Existing constructors taking a `TargetId` remain concise, but MCP and later interactions can use the selected page without inventing another resolution layer. Browser-scoped operations report `None` from the generated operation scope; page-scoped operations report their selection.
- **Operation source of truth:** Extend the existing macro-backed `BROWSER_OPERATION_REGISTRY` with page list/create/select/close/navigate/reload/back/forward. The declaration continues to generate kind, tagged request/result association, stable name, mutability, evidence policy, and browser/page scope metadata. Do not create a lifecycle-only enum or MCP schema mirror.
- **Create/select behavior:** Creating a page always uses `about:blank` when no URL is supplied and selects/activates the new page after the existing reducer has reconciled and attached its exact key. Explicit selection calls `Target.activateTarget` before committing selection. There is no `create-and-maybe-select` option; an agent can select another page afterward.
- **Close behavior:** Closing a page commits `TargetDestroyed` through the existing reducer after Chrome confirms `Target.closeTarget`. If the selected page closes, the reducer chooses the lexicographically first attached exact target key as a deterministic fallback. Closing the last page succeeds with no selection and an explicitly unavailable post-operation observation.
- **Navigation completion:** Navigate, reload, back, and forward do not imply network-idle waiting. They wait only for a bounded main-frame commit: returned loader identity when available, otherwise a changed main-frame loader, URL, or history index as appropriate. This uses bounded `Page.getFrameTree`/`Page.getNavigationHistory` reads rather than adding a permanent lifecycle subscription or the general wait subsystem.
- **Interaction results:** Every state-changing page request allocates an `InteractionId` before dispatch and returns ordered start/dispatch/completion/observation times, a success-or-failure outcome, and an honest `ObservationPart<LiveObservation>`. Dispatch failures remain returned interaction outcomes once an anchor exists; preflight failures before a target can be bound remain ordinary operation errors. Observation failure never converts a successful mutation into guessed failure or silently disappears.
- **Reference invalidation:** Any accepted navigation, reload, or history dispatch proactively invalidates the target's active snapshot generation before post-operation observation. The existing attachment/document checks remain the backstop across same-document races and reconnect.
- **Reconnect and cancellation:** Operations bind one exact target and attachment generation and are never replayed after reconnect. Stop signals an operation-cancellation token before entering the supervisor queue; transport-pump closure signals the current connection generation before queuing reconnect. In-flight completion polling exits with `cancelled` or `browser_disconnected`, returns its interaction outcome when anchored, and cannot delay bounded ownership-correct shutdown indefinitely.
- **Persistence boundary:** Interaction anchors and operation outcomes are returned values only. This feature neither appends interaction payloads to the timeline nor creates a private memory store. Durable interaction indexing belongs to durable browser memory; waits, batches, rich input, and MCP registration remain downstream.
- **UI surface:** The feature is a typed agent/browser-control API, not a human screen or journey. No UI mockups apply.

## Architectural choice

### Option A — extend the supervised session and generated operation executor (chosen)

Keep lifecycle ownership in `ProductionBrowserConnector`/`ProductionSession`, put selected-page state beside exact target state in the existing reducer, and extend the generated operation registry plus `PageControl`. Browser-scoped target mutations are synchronously reconciled through the same reducer/effect machinery before live observation. This preserves one process/profile/session/target owner and gives later interaction, batch, and MCP work one operation contract.

### Option B — add a workspace facade with its own active session and page map

A new `BrowserWorkspace` could own an optional session, current page, and convenience start/stop methods. It would make a future tool handler superficially simple, but it would duplicate lifecycle/selection state, race target events and reconnect, and require reconciliation with `ProductionSession`. Rejected because the later MCP adapter can hold the one returned session handle without becoming another domain owner.

### Option C — execute lifecycle and page commands directly from MCP

Thin handlers could call `Target.createTarget`, `Page.navigate`, and connector methods themselves. This avoids core types temporarily but duplicates request/result schemas, leaks CDP completion policy into the boundary adapter, bypasses interaction anchors, and makes batching reuse impossible. Rejected because MCP must remain a translation layer.

**Choice:** Option A. It adds only the missing domain values and operation variants while reusing the connector, reducer, supervisor actor, capability probe, profile guard, and live-observation path already proven in production tests.

## Trickiest unit: target mutation under one serialized owner

`Target.createTarget` and `Target.closeTarget` mutate the same target set that asynchronous CDP events feed into the reducer. The session actor must not wait for its own queued event while executing an operation. It instead performs the command, fetches the exact resulting target info when needed, feeds the corresponding existing reducer input synchronously, executes the reducer's existing attach/visibility/capture effects, and only then selects/observes the page. A later duplicate `targetCreated` or `targetDestroyed` event is idempotently ignored by the reducer.

```rust
// crates/krometrail-cdp/src/session.rs
async fn execute_operation(
    control: &mut PageControl,
    state: &mut SupervisorState,
    connection: &ConnectionResources,
    shared: &SessionShared,
    runtime: &SupervisorRuntime,
    request: BrowserOperationRequest,
) -> Result<BrowserOperationResult>;

async fn apply_reduction(
    state: &mut SupervisorState,
    input: SupervisorInput,
    connection: &ConnectionResources,
    shared: &SessionShared,
    runtime: &SupervisorRuntime,
) -> Result<()>;

// New reducer input; the reducer validates that the key is live and attached.
enum SupervisorInput {
    // existing variants...
    SelectTarget { target_key: String },
}
```

For create, `execute_operation` sends `Target.createTarget`, validates its non-empty `targetId`, reads `Target.getTargetInfo` for that exact key, applies `TargetCreated`, lets existing effects attach and probe visibility, sends `Target.activateTarget`, then applies `SelectTarget`. It never matches a URL/title or writes `targets_by_key` directly. For close, it sends `Target.closeTarget`, requires `success: true`, then applies `TargetDestroyed`; the later event is a no-op. Selection fallback is computed only by the reducer from live attached keys.

## Implementation units

### Unit 1: Core lifecycle, selection, interaction, and operation contracts

**Files:**

- `crates/krometrail-core/src/browser/control.rs` (new)
- `crates/krometrail-core/src/browser/operation.rs`
- `crates/krometrail-core/src/browser/observation.rs`
- `crates/krometrail-core/src/browser/session.rs`
- `crates/krometrail-core/src/browser/target.rs`
- `crates/krometrail-core/src/browser/mod.rs`
- `crates/krometrail-core/src/ports/browser.rs`
- `crates/krometrail-core/src/ports/mod.rs`
- `crates/krometrail-core/src/error.rs`
- `crates/krometrail-core/src/lib.rs`

**Story:** `epic-agent-browser-operation-browser-page-lifecycle-core-control-contracts`

```rust
pub const DEFAULT_MANAGED_PROFILE_NAME: &str = "default";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagedProfilePersistence { Reusable, Temporary }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ManagedProfileRef {
    pub identity: ProfileIdentity,
    pub persistence: ManagedProfilePersistence,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileRef { Managed(ManagedProfileRef), External }

impl Default for ManagedProfile {
    fn default() -> Self {
        Self::Reusable {
            name: ProfileIdentity::new(DEFAULT_MANAGED_PROFILE_NAME).expect("valid default"),
        }
    }
}
impl Default for LaunchBrowser {
    fn default() -> Self {
        Self { executable: None, profile: ManagedProfile::default(), initial_url: None }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "selection", content = "target_id", rename_all = "snake_case")]
pub enum PageSelection { Selected, Target(TargetId) }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PageStatus {
    pub target: SupervisedTarget,
    pub selected: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BrowserStatus {
    pub session_id: SessionId,
    pub state: BrowserSessionState,
    pub ownership: BrowserOwnership,
    pub profile: ProfileRef,
    pub compatibility: BrowserCompatibility,
    pub selected_target_id: Option<TargetId>,
    pub pages: Vec<PageStatus>,
    pub capture: Vec<TargetCaptureStatus>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ListPagesRequest;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CreatePageRequest { pub initial_url: Option<NonEmptyText> }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SelectPageRequest { pub target_id: TargetId }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ClosePageRequest { pub target: PageSelection }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NavigatePageRequest { pub target: PageSelection, pub url: NonEmptyText }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReloadPageRequest { pub target: PageSelection, pub bypass_cache: bool }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GoBackRequest { pub target: PageSelection }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GoForwardRequest { pub target: PageSelection }

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InteractionTiming {
    pub started_at: SessionTime,
    pub dispatched_at: SessionTime,
    pub completed_at: SessionTime,
    pub observed_at: Option<SessionTime>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InteractionAnchor {
    pub interaction_id: InteractionId,
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub operation: BrowserOperationKind,
    pub timing: InteractionTiming,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", content = "value", rename_all = "snake_case")]
pub enum PageOperationOutcome { Succeeded(PageChange), Failed(KrometrailError) }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "change", rename_all = "snake_case")]
pub enum PageChange {
    Created { target_id: TargetId },
    Selected { previous: Option<TargetId>, selected: TargetId },
    Closed { closed: TargetId, selected: Option<TargetId> },
    Navigated,
    Reloaded,
    WentBack,
    WentForward,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PageOperationResult {
    pub interaction: InteractionAnchor,
    pub outcome: PageOperationOutcome,
    pub observation: ObservationPart<LiveObservation>,
}
```

`InteractionTiming::new` enforces `started <= dispatched <= completed <= observed` when observation exists. `InteractionAnchor::new` requires a registry operation whose mutability is `StateChanging`. `PageOperationResult::new` requires `interaction.target_id` to match the mutated target; the live observation may target a new selection after close and is therefore not forced to match. Errors in failed outcomes add session, target, and interaction context without protocol details.

Extend the registry metadata with:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BrowserOperationScopeKind { Browser, Page }

pub enum BrowserOperationScope {
    Browser,
    Page(PageSelection),
}

impl BrowserOperationRequest {
    pub const fn scope(&self) -> BrowserOperationScope;
}
```

The one declaration adds `ListPages -> Vec<PageStatus>` as read-only/requested-only/browser-scoped and `CreatePage`, `SelectPage`, `ClosePage`, `NavigatePage`, `ReloadPage`, `GoBack`, and `GoForward -> PageOperationResult` as state-changing/live-observation with their correct browser or page scope. Existing inspection requests replace `target_id` with `target: PageSelection`; convenience constructors preserve direct-target call sites.

Add `ErrorCode::NavigationFailed`. Missing history entries remain `invalid_input`; unknown/closed pages remain `not_found`; target create/activate/close failures use `target_failed`; connection loss and stop retain `browser_disconnected`/`cancelled`.

Replace the component status getters on `BrowserSessionPort` with one coherent snapshot while retaining session origin, event subscription, operation execution, and stop:

```rust
pub trait BrowserSessionPort: Send + Sync {
    fn session_origin(&self) -> SessionOrigin;
    fn status(&self) -> PortFuture<'_, Result<BrowserStatus>>;
    fn subscribe(&self) -> PortFuture<'_, Result<Box<dyn BrowserSessionEvents>>>;
    fn execute(&self, request: BrowserOperationRequest)
        -> PortFuture<'_, Result<BrowserOperationResult>>;
    fn stop(&self) -> PortFuture<'_, Result<BrowserStopOutcome>>;
}
```

**Acceptance criteria:**

- [ ] Default, named reusable, temporary, and external profile semantics are distinguishable without exposing filesystem paths.
- [ ] One registry generates every observation and lifecycle operation association, stable name, mutability, evidence, and browser/page scope; tests fail if a new variant omits metadata.
- [ ] Selected/direct target addressing is shared by observation, navigation, and later interaction requests rather than resolved independently by MCP.
- [ ] Status is a coherent serializable snapshot; interaction and page-change values reject invalid timing, operation kinds, empty URLs, and mismatched context at constructors and Serde boundaries.
- [ ] Core remains free of Tokio, CDP, process, endpoint, and filesystem implementation types.

### Unit 2: Managed lifecycle, coherent status, and capability-probed attach

**Files:**

- `crates/krometrail-cdp/src/launcher/profile.rs`
- `crates/krometrail-cdp/src/launcher/startup.rs`
- `crates/krometrail-cdp/src/session.rs`
- `crates/krometrail-cdp/src/compatibility.rs`
- `src/app.rs`
- existing `BrowserSessionPort` fakes and connector tests

**Story:** `epic-agent-browser-operation-browser-page-lifecycle-lifecycle-profile-status`

`ProfileLease` constructs the richer managed profile reference from its existing lease kind. Reusable leases retain the directory and release only the lock; temporary leases retain process-ordered cleanup. Root composition passes `data_directory().join("browser-profiles")` to `LauncherConfig`, so the default reusable profile lives in Krometrail's durable local data directory rather than an incidental temporary/cache root. `KROMETRAIL_PROFILE_ROOT` remains an explicit override. No attach request enters profile acquisition.

`ProductionSession::status` takes one supervisor-state lock, derives selected/public pages from that revision, then combines immutable compatibility/ownership/profile and capture snapshots. Pages sort by exact browser key; ended sessions report no selected active page. The component getters are removed and internal tests/callers use status, preventing torn combinations such as `Ready` paired with a pre-reconnect target list.

The existing connection path remains authoritative:

```rust
BrowserConnector::connect(BrowserConnectRequest::Launch(LaunchBrowser::default()))
BrowserConnector::connect(BrowserConnectRequest::Launch(LaunchBrowser {
    profile: ManagedProfile::Reusable { name }, ..LaunchBrowser::default()
}))
BrowserConnector::connect(BrowserConnectRequest::Launch(LaunchBrowser {
    profile: ManagedProfile::Temporary, ..LaunchBrowser::default()
}))
BrowserConnector::connect(BrowserConnectRequest::Attach(AttachBrowser::new(endpoint)?))
```

Each successful `connect` returns only after compatibility, initial exact-target reconciliation, visibility probing, attachment, initial selection, and `Ready`. Attach still accepts only loopback HTTP/WebSocket endpoints through `LocalCdpEndpoint`; compatibility still classifies observed product identity and probes every required domain. The existing Electron opt-in test remains the acceptance path and gains status assertions; no Electron-specific launcher or renderer controller is added.

**Acceptance criteria:**

- [ ] A no-options launch uses reusable profile `default`; named reusable state survives stop/reopen; temporary data is removed only after managed process termination; attach creates no process/profile guard.
- [ ] Managed stop closes the browser and then releases the lease; attached stop detaches target sessions and leaves the external Chrome/Electron process alive; repeated stop returns the same outcome.
- [ ] Status atomically reports session state, ownership, profile kind/identity, compatibility, selected page, page lifecycle/visibility/generation, and capture status without paths, endpoints, or raw target/session keys.
- [ ] Node inspectors and endpoints missing required renderer capabilities fail before a session is returned; capable Electron renderers use the ordinary attached path and report `ElectronRenderer`.
- [ ] Root wiring shares the existing process clock/ID source/capture assembly and does not construct another connector or lifecycle service.

### Unit 3: Selected-page reducer and page list/create/select/close

**Files:**

- `crates/krometrail-cdp/src/targets/model.rs`
- `crates/krometrail-cdp/src/targets/reducer.rs`
- `crates/krometrail-cdp/src/targets/supervisor.rs`
- `crates/krometrail-cdp/src/control/mod.rs`
- `crates/krometrail-cdp/src/control/pages.rs` (new)
- `crates/krometrail-cdp/src/session.rs`

**Story:** `epic-agent-browser-operation-browser-page-lifecycle-selected-page-targets`

```rust
pub struct SupervisorState {
    // existing fields...
    pub selected_target_key: Option<String>,
}

impl SupervisorState {
    pub fn selected_target(&self) -> Option<&SupervisorTargetState>;
    pub fn resolve_selection(&self, selection: PageSelection)
        -> Result<&SupervisorTargetState>;
}
```

Selection invariants are reducer-owned:

1. `selected_target_key` is absent or names a non-terminal attached page in `targets_by_key`.
2. Initial reconciliation chooses the lexicographically first attached exact key.
3. Explicit `SelectTarget` accepts only an attached target.
4. Suspension retains the exact key so reconnect can restore the same logical `TargetId`.
5. Successful reconnect preserves the key when restored; otherwise it chooses the first attached key.
6. Closure/failure of the selected page chooses the first remaining attached key or `None`.
7. A selection change publishes `BrowserSessionEvent::SelectedTargetChanged { previous, selected }`; unchanged reconnect does not emit noise.

`PageControl::execute` resolves `PageSelection` from the supervisor snapshot immediately before dispatch. List pages uses the same status projection. Select sends browser-scoped `Target.activateTarget` first and commits reducer selection only after success. Create and close follow the tricky-unit synchronous reduction flow. All state changes create an interaction before command dispatch and call the existing `observe_live` implementation after the reducer has committed selection/target state. Close observes the fallback selected page; when none exists it returns `ObservationPart::Unavailable(not_found)` while preserving a successful close outcome.

The connector receives an `Arc<dyn IdSource>` independently of capture. `with_capture` continues to share its supplied source; default construction uses the existing private UUID-v4 adapter. No target ID is allocated by page control: only the target reducer creates `TargetId` values.

**Acceptance criteria:**

- [ ] Initial, explicit, fallback, and reconnect selection are deterministic and owned by the one reducer; URL/title never participate in identity or restoration.
- [ ] Create reconciles and attaches the exact returned key, selects it, and returns a live observation without waiting on an event queued behind the active actor command.
- [ ] Select activates before committing state; close confirms Chrome success before terminal reduction and returns the exact deterministic fallback.
- [ ] Duplicate asynchronous create/change/destroy events after synchronous reconciliation are idempotent and do not create a second `TargetId`, attachment, selection event, or capture stream.
- [ ] State-changing page outcomes retain interaction IDs/timing and honest live-observation degradation; list pages is screenshot-free.

### Unit 4: Navigation, history, observation, and cancellation

**Files:**

- `crates/krometrail-cdp/src/control/navigation.rs` (new)
- `crates/krometrail-cdp/src/control/mod.rs`
- `crates/krometrail-cdp/src/control/snapshot.rs`
- `crates/krometrail-cdp/src/session.rs`
- `crates/krometrail-cdp/src/transport/mod.rs`

**Story:** `epic-agent-browser-operation-browser-page-lifecycle-navigation-observations`

```rust
#[derive(Clone, Debug)]
pub(crate) struct NavigationConfig {
    pub(crate) commit_timeout: Duration, // default 5 seconds
    pub(crate) poll_interval: Duration,  // default 25 milliseconds
}

struct DocumentState {
    frame_id: String,
    loader_id: String,
    url: String,
    history_index: u32,
}

impl PageControl {
    async fn navigate(
        &mut self,
        transport: &dyn CdpTransport,
        state: &SupervisorState,
        request: NavigatePageRequest,
        cancel: &OperationCancellation,
    ) -> Result<BrowserOperationResult>;

    async fn reload(/* same binding/cancellation shape */) -> Result<BrowserOperationResult>;
    async fn go_back(/* same binding/cancellation shape */) -> Result<BrowserOperationResult>;
    async fn go_forward(/* same binding/cancellation shape */) -> Result<BrowserOperationResult>;
}
```

All four operations:

1. resolve and bind selected/direct target plus attachment generation;
2. allocate interaction identity and sample start;
3. inspect the current main-frame/history state needed for completion;
4. sample dispatch and send the exact session-scoped command;
5. on accepted dispatch, invalidate the target's active snapshot generation;
6. await only the bounded commit condition;
7. sample completion and invoke the shared live-observation path against the same target/attachment;
8. return success/failure plus observation and final timing.

Command mappings:

- navigate: `Page.navigate { url }`; reject non-empty `errorText`; use returned `loaderId` when present, otherwise accept URL/history change for same-document navigation;
- reload: `Page.reload { ignoreCache: bypass_cache }`; require changed loader or a bounded fresh document/readiness observation;
- back/forward: read `Page.getNavigationHistory`, fail `invalid_input` before dispatch when no adjacent entry exists, send `Page.navigateToHistoryEntry { entryId }`, then require the expected current index plus loader/URL evidence.

No operation waits for network idle, a complete document, or application-specific stability. The bounded commit helper is private navigation completion policy, not the general wait feature.

`OperationCancellation` is generation-aware. `ProductionSession::stop` marks stopping and signals it before queueing `Stop`; target-event pumps signal connection loss before enqueueing the reducer input. Every transport call and commit loop is wrapped in `tokio::select!`. A cancelled or disconnected anchored operation returns `PageOperationOutcome::Failed` with interaction context and an unavailable observation; it is never retried against a rebuilt session. Unanchored preflight errors continue through the outer `Result`.

**Acceptance criteria:**

- [ ] Navigate/reload/back/forward route only through the exact current flat session, never replay after reconnect, and proactively stale prior references once dispatch is accepted.
- [ ] Each successful operation returns the resulting live URL/history/snapshot/screenshot and an ordered interaction anchor; each post-anchor command/timeout/cancellation failure remains explicitly anchored.
- [ ] Back/forward at a history boundary fails before dispatch and does not fabricate an interaction or screenshot.
- [ ] Navigation completion is bounded, cancellation-aware, and does not imply network idle or add a permanent unbounded CDP subscription.
- [ ] Stop, connection loss, target closure, queue closure, malformed replies, command rejection, navigation `errorText`, commit timeout, and observation-part failures map to stable source-safe outcomes without hangs.

### Unit 5: Deterministic and real-browser lifecycle qualification

**Files:**

- `crates/krometrail-core/src/browser/control.rs` contract tests
- `crates/krometrail-cdp/src/control/tests.rs`
- `crates/krometrail-cdp/tests/page_lifecycle.rs` (new)
- `crates/krometrail-cdp/tests/support/scripted_cdp.rs`
- `crates/krometrail-cdp/tests/support/chrome.rs`
- `crates/krometrail-cdp/tests/support/mod.rs`
- `crates/krometrail-cdp/tests/chrome_session_real.rs`
- `crates/krometrail-cdp/tests/profile_ownership.rs`
- `tests/fixtures/browser/page-lifecycle/index.html` (new)
- `tests/fixtures/browser/page-lifecycle/second.html` (new)
- `tests/fixtures/browser/README.md`

**Story:** `epic-agent-browser-operation-browser-page-lifecycle-qualification`

Extend the shared `ScriptedCdp` rather than creating a lifecycle-only fake protocol stack. Deterministic tests assert exact browser/session command scope and JSON, status coherence, selected-key reducer transitions, create reconciliation before duplicate events, activate-before-select, close fallback, history bounds, navigation commit conditions, snapshot invalidation, partial observations, cancellation, and non-replay after reconnect without sleeps.

Add a dependency-free two-page fixture with stable titles/URLs, a deterministic `pushState` control, and a navigation marker. The opt-in production-connector Chrome test performs one cohesive workflow:

1. launch a temporary managed session and verify status/profile/initial selection;
2. create/select a second page and prove both exact target IDs remain distinct;
3. navigate to the second fixture, reload with measured live observation, push same-document history, then go back/forward;
4. create a snapshot before navigation and prove its reference is stale afterward;
5. close selected and unselected pages and prove deterministic fallback/no-selection behavior;
6. stop and prove managed process/profile cleanup.

Existing focused tests prove named reusable lease retention/reopen and attached-process survival. The existing opt-in Electron endpoint test proves the same status/page lifecycle surface when available; it remains environment-gated and does not become a default CI dependency. Linux/macOS real-Chrome qualification uses the same code; no high-DPI claim is added by this feature because screenshot scaling is already qualified by page observation.

**Acceptance criteria:**

- [ ] Default tests protect reducer/contract/command/cancellation behavior deterministically and do not depend on Chrome timing.
- [ ] Opt-in real Chrome proves the complete start/status/page/navigation/history/close/stop workflow through the production connector and existing supervisor.
- [ ] Named/temporary/external ownership, capable Electron attach, Node-inspector rejection, and attach-stop survival each retain focused coverage.
- [ ] Fixture files are standalone browser targets, documented, and do not add another Krometrail runtime.
- [ ] `cargo fmt --all -- --check`, workspace check/test/clippy with locked dependencies, and `cargo check -p krometrail-cdp --no-default-features --all-targets --locked` pass.

## Error and recovery semantics

| Condition | Stable code/outcome | Retry | Recovery |
| --- | --- | --- | --- |
| No browser installation / launch failure | `browser_not_found` / `browser_launch_failed` | after recovery | install/check browser and profile, then start again |
| Reusable profile already leased | `profile_in_use` | after recovery | stop the other session or choose another profile |
| Non-local/unavailable attach endpoint | `browser_launch_failed` | after recovery | enable a local renderer endpoint and retry |
| Node inspector or missing renderer capability | `browser_compatibility_failed` | after recovery | attach to a compatible Chrome/Electron renderer endpoint |
| Unknown/closed/direct page or no selected page | `not_found` | safe after refreshing | list pages and select a live attached target |
| Create/activate/close or attachment failure | `target_failed` | safe only when status still shows target | refresh status and retry or choose another page |
| Back/forward has no adjacent entry | `invalid_input` | never without state change | navigate first or choose the available direction |
| Navigation rejection, malformed reply, or commit timeout | anchored `navigation_failed` | safe only after observing status | inspect current live state before deciding whether to repeat |
| Reconnecting/connection lost during operation | anchored `browser_disconnected` | after recovery | wait for `Ready`, refresh status, and issue a new operation |
| Stop/cancellation during operation | anchored `cancelled` | never automatically | start the operation again only if still needed |
| Live page/snapshot/screenshot observation fails after successful mutation | successful change + unavailable observation part | per part | request a fresh live observation; never assume unseen state |
| Managed close/detach/flush exceeds deadline | `shutdown_incomplete` | after recovery | inspect process/session status before starting another session |

Raw endpoint strings, executable/profile paths, browser target keys, transport session IDs, loader/frame/history IDs, CDP replies, page content, and source errors remain outside stable messages and info logs.

## Implementation order

1. `epic-agent-browser-operation-browser-page-lifecycle-core-control-contracts`
2. `epic-agent-browser-operation-browser-page-lifecycle-lifecycle-profile-status`
3. `epic-agent-browser-operation-browser-page-lifecycle-selected-page-targets`
4. `epic-agent-browser-operation-browser-page-lifecycle-navigation-observations`
5. `epic-agent-browser-operation-browser-page-lifecycle-qualification`

One feature owner should carry the five checkpoints because core operation contracts, supervisor selection, and navigation observation share files and invariants. The stories preserve dependency and verification evidence; they are not parallel agent assignments.

## Simplification

- Replace split session component getters with one coherent `BrowserStatus` snapshot.
- Extend the existing operation registry, target reducer, supervisor command path, profile lease, and live observation; do not add a workspace singleton, target registry, lifecycle facade, profile manager, reconnect loop, Electron adapter, or interaction store.
- Use one `PageSelection` resolver for observation, lifecycle, navigation, and later interaction requests.
- Use one reducer-owned exact-key selection and one synchronous reducer/effect path for both commands and asynchronous target events.
- Reuse existing local endpoint validation, capability probe, `CdpTransport::send_raw`, snapshot registry, operation clock, process/profile guards, `ScriptedCdp`, Chrome support, and page-observation fixture patterns.
- Remove/update component-getter tests rather than duplicating them beside coherent status tests. Do not test trivial getters, every profile string, or every navigation poll iteration.
- No foundation document assertion changes: the current future-state lifecycle/control claims already match this design.

## Testing

- **Core interface tests:** Protect default/named/temporary/external profile wire semantics, coherent status validation, `PageSelection`, exhaustive registry generation, interaction timing/context, and operation outcome Serde. These are future MCP/public contract risks.
- **Reducer tests:** Protect exact-key initial/explicit/fallback/reconnect selection and duplicate target-event idempotence. These are the highest-value state-machine risks.
- **Scripted adapter tests:** Protect browser-vs-session command scope, mutation/reduction ordering, navigation commit rules, cancellation/non-replay, stale-reference invalidation, and honest partial observation. These are deterministic concurrency and protocol risks.
- **Focused ownership tests:** Retain profile lease and process-group tests; update them for richer status rather than re-testing the launcher implementation through every operation.
- **Opt-in real Chrome/Electron:** Protect assumptions fake JSON cannot establish: actual target creation/activation/closure, navigation/history behavior, process/profile ownership, and renderer capability attachment.
- **Test consolidation:** Reuse page-observation live-observation assertions and shared helpers. Do not duplicate screenshot geometry, AX decoding, image-header, or transport-envelope suites in lifecycle tests.

## Risks

- **Synchronous reconciliation versus queued events:** Chrome can emit target events before or after command responses. The reducer must make command-driven reconciliation and later duplicate events idempotent. Fallback: if an exact target cannot be fetched/reconciled after create, return an anchored target failure and let the queued event restore ordinary status; never write a second map.
- **Navigation completion ambiguity:** Same-document history changes may not replace a loader, while reload can expose transient old page state. The completion helper accepts only operation-specific loader/URL/history evidence and is bounded. It does not claim load/network stability.
- **Cancellation ordering:** Stop and transport loss can race a command response. One generation-aware cancellation signal plus one actor verdict must prevent replay and double completion. The interaction's completed outcome records whichever boundary wins; reducer/session state remains authoritative.
- **Selection across reconnect:** Exact browser keys normally survive reconnection, but a renderer may disappear and be replaced. Selection then falls back explicitly; URL/title similarity never restores identity.
- **Profile durability location:** Moving the default reusable profile under the Krometrail data directory changes where new default state is kept. There is no shipped profile contract or migration to preserve; an explicit `KROMETRAIL_PROFILE_ROOT` remains available. Implementation must not silently adopt the user's default Chrome profile.
- **Electron target churn:** Electron applications can create/destroy renderer targets differently from Chrome tabs. Capability probing and exact-key supervision remain the support boundary; native windows and Node main-process APIs are deliberately excluded.

## Pre-mortem

The riskiest assumption is that a browser-scoped create/close command can reconcile the exact target through the existing reducer before its asynchronous event reaches the actor, without duplicate attachments or target IDs. This fails if command handling waits on the actor's own queue or mutates the target map outside the reducer. The design avoids both: it fetches the exact returned key, applies the same reducer/effect path synchronously, and requires duplicate-event tests. If a renderer does not expose the target immediately, the operation returns an anchored failure while the normal event path remains authoritative; it never creates a speculative identity.

The second risk is returning an observation too early after navigation. A bounded main-frame commit is the minimum honest completion point and deliberately does not promise network idle. If a renderer cannot provide matching loader/history evidence, the operation reports `navigation_failed` with whatever current observation can be obtained rather than sleeping indefinitely or claiming success. The least certain area is same-document history behavior across Chrome/Electron versions, so scripted exact cases and opt-in renderer qualification are both required.

## Implementation notes

- Execution capability: highest (caller); this cohesive owner carried the five ordered checkpoints because contracts, reducer state, transport commands, navigation completion, and live observation share invariants and files.
- Review weight: standard (caller); the feature is intentionally left at `stage: review` with no self-approval.
- Commits: `67d09fb` core control contracts; `5adcc3b` lifecycle/profile/status and integrated adapter implementation; `c3cb5f9` selected-page/target qualification; `b21d372` navigation-observation qualification; `4ace299` complete qualification and formatting.
- Files changed: core lifecycle/status/selection/interaction contracts and registry; production launcher/session/reducer/page/navigation control; root profile assembly; shared scripted and Chrome support; lifecycle fixture and tests; affected status consumers.
- Verification: `cargo fmt --all -- --check`; locked workspace all-target check; 282 locked workspace tests across 27 suites; locked workspace all-target clippy with `-D warnings`; locked no-default CDP all-target check; 10 page-lifecycle tests under real Chrome in 6.76 seconds. All passed.
- Real-browser evidence: temporary managed launch and cleanup, coherent initial status/selection, exact distinct target creation, selected/direct page control, new- and same-document navigation, reload, back/forward, proactive stale-reference behavior, selected/unselected/last-page closure, ownership-correct stop, and named reusable profile persistence across reopen.
- Simplification: one connector, profile guard, compatibility path, reducer, exact-key target map, selected key, operation registry, snapshot registry, cancellation signal, and live-observation path serve the capability. No MCP tools, durable interaction store, generic waits/batches, or second manager were added.
- Design deviations: screenshot requests call selected/direct page scope `page` because `target` already names screenshot geometry; existing deterministic reducer allocation remains the sole `TargetId` authority while the independently injected `IdSource` supplies session/interaction IDs; malformed navigation baselines fail before interaction allocation; Electron end-to-end execution remains environment-gated while deterministic probes cover renderer classification and Node-inspector rejection.
- Adjacent issues parked: none.
