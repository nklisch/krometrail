---
id: epic-rust-cdp-capture-foundation-chrome-target-supervision
kind: feature
stage: review
tags: [browser]
parent: epic-rust-cdp-capture-foundation
depends_on: [epic-rust-cdp-capture-foundation-cdp-transport-gate]
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Chrome Session and Target Supervision

## Brief

Deliver the production browser-session capability around the transport selected by the compatibility gate. Krometrail can discover Chrome, launch an isolated reusable or temporary managed profile, attach to an explicit local endpoint, report browser and protocol compatibility before recording, and close a controlled browser or detach cleanly.

Supervise recordable page targets through flat CDP sessions so target creation, navigation, visibility, closure, and target-local failures remain isolated. Reconnection restores the browser connection and target attachments when safe, while unrecoverable loss ends the session through explicit cancellation and bounded cleanup. This feature owns lifecycle and target continuity, but not frame queueing, persistence, browser actions, or structured page snapshots.

## Epic context

- Parent epic: `epic-rust-cdp-capture-foundation`
- Position in epic: production browser adapter — consumes the qualified transport and supplies supervised target sessions to capture
- Design decisions inherited: exact `cdpkit` 0.4.0 follows the final5 real-browser decision; cdpkit remains replaceable; Krometrail owns reconnect, target restoration, cancellation, and cleanup

## Foundation references

- `docs/VISION.md` — Local-First Operation
- `docs/SPEC.md` — Browser Lifecycle, Sessions and Targets, and Errors and Degraded Operation
- `docs/ARCHITECTURE.md` — Browser Connection, Target Lifecycle, Failure Isolation, and Observability

## Scope

- Replace the provisional browser contracts with typed connection mode, profile/process ownership, compatibility, session, supervised-target, shutdown, and event contracts in `krometrail-core`.
- Add a production `krometrail-cdp::transport` boundary and an exact cdpkit 0.4.0 adapter. No cdpkit type crosses `transport/cdpkit.rs` or appears in core.
- Discover Chrome/Chromium on Linux and macOS; acquire a named reusable or temporary Krometrail-managed profile; launch on a loopback ephemeral debugging port; and own the resulting process tree.
- Attach only to an explicit loopback HTTP or WebSocket CDP endpoint. Attached processes and profiles remain externally owned.
- Probe browser identity and required renderer capabilities before returning a session. Electron is supported only as a renderer endpoint that proves the same capabilities; branding is advisory and Electron's Node main process remains out of scope.
- Enable target discovery and auto-attach with flat sessions, reconcile discovery races, isolate target failures, supervise disconnect/reconnect, and publish typed state changes.
- Root-wire the production connector and make `doctor` inspect browser availability without launching or attaching.
- Add deterministic transport/supervision tests and real-Chrome launch/attach tests against `tests/fixtures/browser/cdp-transport-gate`.

## Non-goals

- No `Page.startScreencast`, `Page.screencastFrame`, acknowledgement, frame decoding, bounded handoff, persistence, capture gaps, or capture statistics. Those belong to `epic-rust-cdp-capture-foundation-bounded-screencast-ingestion`.
- No navigation/input/snapshot/MCP browser-control surface, no Electron main-process inspector, and no remote/non-loopback endpoint.
- No automatic browser relaunch after process death, URL/title-based target identity guessing, profile migration, profile deletion API, or default-browser-profile modification.
- No fallback `chromey` or owned WebSocket implementation. A demonstrated cdpkit failure reopens the approved transport decision; it does not justify parallel production transports.
- Do not reuse or root-wire the disposable `spike` modules. Their evidence remains independently buildable behind feature flags.

## Design decisions

- **Dispatch:** direct-read only — the caller prohibited questions and subagents, and the parent, final5 decision, research, skill, current ports, workspace, and fixtures resolve the design surface.
- **Transport selection:** exact `cdpkit = 0.4.0` is enabled by `krometrail-cdp`'s default `cdpkit-transport` feature behind an owned object-safe transport trait. The existing spike features remain opt-in evidence paths and reuse the same workspace pin; no production behavior imports a `spike` module.
- **Target attachment:** install named target-event subscriptions before enabling `Target.setDiscoverTargets` and `Target.setAutoAttach(autoAttach=true, waitForDebuggerOnStart=false, flatten=true)`, then reconcile with `Target.getTargets`. Event and snapshot inputs are reduced through one idempotent state machine, preventing discovery/attach races from creating duplicate logical targets.
- **Target identity across reconnect:** preserve a Krometrail `TargetId` only when the same browser target key is rediscovered. Never match by URL or title. Missing old keys close their targets; newly observed keys receive new IDs.
- **Reconnect safety:** reconnect the CDP connection to the same endpoint while a managed child remains alive or an attached endpoint remains reachable. Do not relaunch Chrome or reopen URLs. Re-probe compatibility and rebuild discovery, flat attachments, and domain state on every successful connection.
- **Electron support:** classify endpoint kind from observed product/user-agent only for status; accept Chrome, Chromium, or Electron only when the runtime capability probe passes on a recordable page target. This supports Electron renderers without claiming control of its Node main process.
- **Profile ownership:** named and temporary profiles are always under Krometrail's configured profile root. A held lease prevents concurrent use of a named profile; temporary directories are deleted only by their owning guard. `ProfileRef::Managed` records the acquired Krometrail identity, while attach sessions report `ProfileRef::External` and never imply knowledge or ownership of an external profile. Attach mode never acquires, mutates, or deletes a profile.
- **Shutdown:** a managed session sends `Browser.close`, waits a bounded grace period, then terminates its owned process group if required. Attach mode cancels supervision and drops the transport without `Browser.close`. Drop guards remain a last-resort cancellation-safe cleanup path.
- **Visibility without capture:** probe initial `document.visibilityState` after attachment and expose `Unknown | Visible | Hidden`. The target state reducer accepts later visibility signals, but this feature does not start a screencast merely to obtain `Page.screencastVisibilityChanged`; the ingestion feature will feed that event into the same reducer.
- **Foundation updates:** code-first. Implementation must roll `docs/ARCHITECTURE.md`'s stale “historical decision not current” sentence to the final5 selection and document the landed boundary; `docs/SPEC.md` already describes the intended behavior.

## Architectural choice

### Considered approaches

1. **Expose cdpkit handles from the browser port.** This is smallest initially, but couples core and every later capture/control feature to one young dependency and makes the approved fallback expensive. Rejected.
2. **Build a generic owned WebSocket transport now.** This gives maximal envelope control, but duplicates a candidate that passed every unchanged gate and prematurely commits to the fallback's maintenance burden. Rejected.
3. **Use an owned narrow transport seam plus a Krometrail supervisor.** `transport` owns connection/request/event mechanics and cdpkit mapping; launcher, compatibility, and target modules own product policy. Core sees only typed browser-domain values. Chosen because it follows Ports & Adapters, preserves the approved replacement point, and keeps reconnect outside the library.

The trickiest unit is the target/reconnect reducer. It must merge initial snapshots, auto-attach events, target-info changes, detach events, connection loss, and cancellation without confusing a CDP session id or a reused URL with durable target identity. It is designed first as a deterministic state machine; asynchronous tasks only translate transport events into reducer inputs and execute emitted effects.

## Typed state, capabilities, and errors

`krometrail-core` remains infrastructure-free. Stable enums continue to use one declaration for variants, serialized names, and exhaustive tests.

```rust
pub enum BrowserConnectRequest {
    Launch(LaunchBrowser),
    Attach(AttachBrowser),
}

pub struct LaunchBrowser {
    pub executable: Option<std::path::PathBuf>,
    pub profile: ManagedProfile,
    pub initial_url: Option<String>,
}

pub enum ManagedProfile { Reusable { name: ProfileIdentity }, Temporary }
pub struct AttachBrowser { pub endpoint: String }
pub enum BrowserOwnership { Managed, Attached }
pub enum ProfileRef { Managed(ProfileIdentity), External }

pub enum BrowserInstallationSource { ExplicitRequest, EnvironmentOverride, PlatformDefault, PathLookup }
pub enum BrowserProduct { Chrome, Chromium, ElectronRenderer, OtherChromium }
pub struct BrowserProductVersion(NonEmptyText);
pub struct BrowserInstallation {
    pub executable: std::path::PathBuf,
    pub source: BrowserInstallationSource,
    pub product: BrowserProduct,
    pub version: BrowserProductVersion,
}
pub struct BrowserVersion {
    pub product: BrowserProduct,
    pub product_version: BrowserProductVersion,
    pub revision: NonEmptyText,
    pub protocol_version: NonEmptyText,
    pub user_agent: NonEmptyText,
    pub js_version: NonEmptyText,
}

pub enum BrowserSessionState { Connecting, Ready, Reconnecting, Stopping, Ended }
pub enum TargetVisibility { Unknown, Visible, Hidden }
pub enum BrowserStopOutcome { ManagedBrowserClosed, Detached }

pub enum RendererCapability {
    BrowserIdentity,
    TargetDiscovery,
    FlatTargetSessions,
    Page,
    Runtime,
    Accessibility,
    Input,
    Screencast,
}

pub struct CapabilitySupport {
    pub capability: RendererCapability,
    pub available: bool,
    pub required: bool,
    pub detail: Option<NonEmptyText>,
}

pub struct BrowserCompatibility {
    pub version: BrowserVersion, // product classification lives here as the SSOT
    pub capabilities: Vec<CapabilitySupport>,
}

pub struct SupervisedTarget {
    pub target: PageTarget,
    pub lifecycle: TargetLifecycle,
    pub visibility: TargetVisibility,
    pub attachment_generation: u64,
}

pub enum BrowserSessionEvent {
    SessionStateChanged { state: BrowserSessionState },
    SessionFailed { error: KrometrailError },
    TargetDiscovered { target: SupervisedTarget },
    TargetChanged { target: SupervisedTarget },
    TargetClosed { target_id: TargetId },
    TargetFailed { target_id: TargetId, error: KrometrailError },
}

pub trait BrowserSessionEvents: Send {
    fn next(&mut self) -> PortFuture<'_, Result<Option<BrowserSessionEvent>>>;
}

pub trait BrowserConnector: Send + Sync {
    fn installations(&self) -> PortFuture<'_, Result<Vec<BrowserInstallation>>>;
    fn connect(&self, request: BrowserConnectRequest)
        -> PortFuture<'_, Result<Arc<dyn BrowserSessionPort>>>;
}

pub trait BrowserSessionPort: Send + Sync {
    fn compatibility(&self) -> &BrowserCompatibility;
    fn ownership(&self) -> BrowserOwnership;
    fn profile(&self) -> &ProfileRef;
    fn state(&self) -> BrowserSessionState;
    fn targets(&self) -> PortFuture<'_, Result<Vec<SupervisedTarget>>>;
    fn subscribe(&self) -> PortFuture<'_, Result<Box<dyn BrowserSessionEvents>>>;
    fn stop(&self) -> PortFuture<'_, Result<BrowserStopOutcome>>;
}
```

Add stable core error codes `browser_not_found`, `browser_launch_failed`, `browser_process_terminated`, `browser_compatibility_failed`, `profile_in_use`, `target_failed`, `reconnect_exhausted`, `cancelled`, and `shutdown_incomplete`. `browser_process_terminated` is distinct from transport closure: a managed-child watcher emits a process-termination input with sanitized exit status, the session does not reconnect to or relaunch a dead owned child, and `SessionFailed` publishes that stable error before bounded cleanup. Adapter-private errors retain sources for structured logs and map once at the core boundary with safe messages, `RetryAdvice`, and concrete recovery. Unknown endpoint input fails before filesystem/process/network side effects; missing required capability fails before the session reaches `Ready`.

`BrowserSessionState` is the browser connector/supervisor connectivity state and does not replace `SessionLifecycle`, which remains the recording workflow state on `RecordingSession`. `RecordingSession.profile` and its constructor/accessor/wire form migrate in this story from `ProfileIdentity` to `ProfileRef`; managed recordings carry `ProfileRef::Managed`, while attached recordings carry `ProfileRef::External`. Its `BrowserVersion` use migrates to the complete runtime identity above.

`TargetLifecycle` gains `Suspended`, and the single lifecycle registry declares every legal edge: `Discovered -> Attached | Suspended | Closed | Failed`; `Attached -> Recording | Hidden | Suspended | Closed | Failed`; `Recording -> Hidden | Suspended | Closed | Failed`; `Hidden -> Recording | Suspended | Closed | Failed`; `Suspended -> Discovered | Attached | Recording | Hidden | Closed | Failed`; terminal `Closed` and `Failed` have no outgoing transitions. `SupervisorState` stores the pre-suspension lifecycle so exact-key restoration chooses the corresponding `Suspended` exit and increments attachment generation; absence after reconciliation closes the target. Target-local detach/probe failure transitions only that target to `Failed`. Exhaustive pair tests must reject every edge not listed.

## Replaceable cdpkit transport boundary

### Owned transport contract

**Files:** `crates/krometrail-cdp/src/transport/mod.rs`, `crates/krometrail-cdp/src/transport/cdpkit.rs`, `crates/krometrail-cdp/src/transport/error.rs`

```rust
pub enum CommandScope { Browser, Session(TransportSessionId) }
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct TransportSessionId(String);
pub struct NamedEvent { pub method: String, pub params: serde_json::Value }
pub struct TransportClose { pub reason: NonEmptyText }

pub trait TransportEvents: Send {
    fn next(&mut self) -> TransportFuture<'_, Result<Option<NamedEvent>, TransportError>>;
}

pub trait CdpTransport: Send + Sync {
    fn send_raw(&self, scope: &CommandScope, method: &str, params: serde_json::Value)
        -> TransportFuture<'_, Result<serde_json::Value, TransportError>>;
    fn subscribe_named(&self, scope: &CommandScope, method: &str)
        -> TransportFuture<'_, Result<Box<dyn TransportEvents>, TransportError>>;
    fn close_reason(&self) -> Option<TransportClose>;
    fn is_closed(&self) -> bool;
}

pub trait CdpTransportFactory: Send + Sync {
    fn connect(&self, browser_websocket_url: &str)
        -> TransportFuture<'_, Result<Arc<dyn CdpTransport>, TransportError>>;
}
```

The contract states the honest cdpkit limit: named subscriptions return event `params`, not wildcard/full envelopes. `CdpkitTransport` wraps browser/session senders, inserts session ids through `OwnedSession`, uses cdpkit's connection close state, and never reconnects. Command names and JSON decoding stay in compatibility/target adapters, where each response is fail-fast decoded into owned structs with `serde(deny_unknown_fields)` omitted intentionally for additive CDP compatibility. No spike type is imported.

The workspace adds `url = "2"`, used non-optionally by `krometrail-cdp` endpoint validation. The crate manifest declares `default = ["cdpkit-transport"]`; `cdpkit-transport` enables the optional exact workspace `cdpkit = 0.4.0` dependency plus only the Tokio sync/time, futures-util, and serde-json dependencies required by production. `cdp-spike` remains opt-in, and `cdp-spike-cdpkit = ["cdp-spike", "cdpkit-transport", "dep:libc"]` reuses the production workspace pin without making any spike module reachable from default production code.

## Implementation units and exact files

### Unit 1: Core browser supervision contracts

**Story:** `epic-rust-cdp-capture-foundation-chrome-target-supervision-contracts`

**Files:**
- `crates/krometrail-core/src/browser/mod.rs`
- `crates/krometrail-core/src/browser/target.rs`
- `crates/krometrail-core/src/browser/session.rs` (new)
- `crates/krometrail-core/src/ports/browser.rs`
- `crates/krometrail-core/src/recording/session.rs`
- `crates/krometrail-core/src/lifecycle.rs`
- `crates/krometrail-core/src/error.rs`
- `crates/krometrail-core/src/lib.rs`
- `crates/krometrail-core/src/ports/mod.rs`
- `src/app.rs`
- `tests/rust-runtime-smoke.rs`

Implementation notes:
- Preserve UUID-backed `TargetId`; CDP target/session keys remain validated opaque adapter-origin strings.
- Generate stable enum names and exhaustive round-trip coverage from existing registry macros.
- `BrowserSessionEvents` is runtime-neutral like existing ports; Tokio channels remain adapter-private.
- Migrate `RecordingSession`'s profile field/constructor/accessor/wire form from `ProfileIdentity` to `ProfileRef` and update its complete `BrowserVersion` fixtures; do not add capture behavior.
- This story owns the compile-real application transition required by the changed traits. Update `UnavailableBrowserConnector` to implement the complete new `BrowserConnector`: `installations()` returns an empty list, while `connect()` returns stable `browser_not_found` with browser-installation recovery. Change `doctor` to call `installations()` exactly once and report that same stable no-browser error when the list is empty. Update the runtime smoke from provisional `unsupported` text to `error[browser_not_found]` plus recovery. This adapter is deliberately transitional—not fake success—and keeps the workspace and tests green until Unit 4 replaces root composition.

Acceptance:
- Core has no cdpkit, CDP, WebSocket, Tokio, URL-parser, or filesystem adapter type.
- All state transitions, malformed serialized states, duplicate capabilities, and safe error mappings have exhaustive tests.
- Existing browser-port fakes are updated to prove managed/attach stop outcomes and event-stream closure.
- The stable error registry and exhaustive serde/display/mapping tests contain all nine designed codes: `browser_not_found`, `browser_launch_failed`, `browser_process_terminated`, `browser_compatibility_failed`, `profile_in_use`, `target_failed`, `reconnect_exhausted`, `cancelled`, and `shutdown_incomplete`.
- `src/app.rs` compiles against the changed traits with transitional `UnavailableBrowserConnector::installations() == []`; `doctor` never calls `connect()` and exits 1 with stable `error[browser_not_found]` plus recovery. `tests/rust-runtime-smoke.rs` rejects the old provisional `unsupported`/`browser transport is not available` behavior.

### Unit 2: Production transport and capability probe

**Story:** `epic-rust-cdp-capture-foundation-chrome-target-supervision-transport-adapter`

**Files:**
- `Cargo.toml`
- `crates/krometrail-cdp/Cargo.toml`
- `crates/krometrail-cdp/src/lib.rs`
- `crates/krometrail-cdp/src/transport/mod.rs` (new)
- `crates/krometrail-cdp/src/transport/cdpkit.rs` (new)
- `crates/krometrail-cdp/src/transport/error.rs` (new)
- `crates/krometrail-cdp/src/compatibility.rs` (new)
- `crates/krometrail-cdp/src/endpoint.rs` (new)
- `crates/krometrail-cdp/tests/support/scripted_cdp.rs` (new)
- `crates/krometrail-cdp/tests/production_transport.rs` (new)
- `crates/krometrail-cdp/tests/compatibility_probe.rs` (new)

```rust
pub struct LocalCdpEndpoint {
    // Constructible only by endpoint normalization/readiness code: loopback HTTP
    // origin plus the resolved browser WebSocket URL and a redacted display label.
    http_origin: url::Url,
    browser_websocket_url: url::Url,
    redacted_label: NonEmptyText,
}
```

Implementation notes:
- Normalize `http://127.0.0.1:<port>`, `http://[::1]:<port>`, `ws://127.0.0.1:<port>/...`, and localhost equivalents. Resolve HTTP endpoints through `/json/version`; reject credentials, fragments, non-loopback resolved addresses, TLS/public endpoints, and unsupported schemes before connect.
- `RENDERER_CAPABILITY_PROBES` is the single registry for capability id, requirement, scope, command, and decoder. Compatibility status derives from it rather than separate lists.
- Subscribe before discovery setup. Probe `Browser.getVersion`, target discovery/flat attachment, then required Page/Runtime/Accessibility/Input operations on one page. Verify screencast command/event availability without starting a screencast. Electron product detection never substitutes for this probe.
- A missing recordable page returns a bounded compatibility failure with a recovery action to open a renderer page; it does not attach to Electron's Node inspector.

Acceptance:
- Scripted tests prove browser/session scoping, named-event params, event-before-response ordering, close propagation, malformed response rejection, and no cross-session delivery.
- Chrome-, generic Chromium-, and Electron-labelled fixtures with equal capabilities are accepted; an Electron main-process-only/Node inspector fixture and every missing required capability are rejected with stable errors.
- `cargo check -p krometrail-cdp` compiles the production cdpkit adapter through the default feature; `cargo check -p krometrail-cdp --no-default-features` compiles the replaceable seam without cdpkit; spike modules remain absent unless their opt-in feature is supplied, and the spike regression remains green.
- `browser.compatibility.probed` tracing is emitted here with product, browser/protocol versions, endpoint kind, and registry-derived required-capability outcome. Endpoint credentials, full URLs, event params, and source/debug error strings are absent from info-level fields.

### Unit 3: Chrome discovery, profiles, endpoints, and process ownership

**Story:** `epic-rust-cdp-capture-foundation-chrome-target-supervision-managed-launch`

**Files:**
- `crates/krometrail-cdp/src/lib.rs`
- `crates/krometrail-cdp/src/launcher/mod.rs` (new)
- `crates/krometrail-cdp/src/launcher/discovery.rs` (new)
- `crates/krometrail-cdp/src/launcher/profile.rs` (new)
- `crates/krometrail-cdp/src/launcher/process.rs` (new)
- `crates/krometrail-cdp/src/launcher/startup.rs` (new)
- `crates/krometrail-cdp/tests/profile_ownership.rs` (new)
- `crates/krometrail-cdp/tests/process_ownership.rs` (new)

```rust
pub struct LauncherConfig {
    pub profile_root: PathBuf,
    pub startup_timeout: Duration,
    pub shutdown_timeout: Duration,
}

pub enum ProfileLeaseKind { Reusable, Temporary }
pub struct ProfileLease { /* private canonical path, lock, and cleanup guard */ }

pub enum SanitizedProcessExit { Code(i32), Signaled, Unknown }
pub struct ManagedChromeProcess { /* private child and process-group ownership */ }

pub struct LaunchedChrome {
    pub endpoint: LocalCdpEndpoint,
    pub profile: ProfileLease,
    pub process: ManagedChromeProcess,
}

pub trait ChromeLauncher: Send + Sync {
    fn installations(&self) -> LauncherFuture<'_, Result<Vec<BrowserInstallation>, LaunchError>>;
    fn launch(&self, request: &LaunchBrowser)
        -> LauncherFuture<'_, Result<LaunchedChrome, LaunchError>>;
}
```

Implementation notes:
- `LocalCdpEndpoint` is defined by Unit 2 in `endpoint.rs` and is an adapter-owned validated endpoint value, not a process owner; it exposes read-only WebSocket/redacted-label accessors, while endpoint resolution remains in that module. `ProfileLease` exclusively owns the profile lock and temporary cleanup guard and exposes read-only `profile_ref()`/`kind()` accessors. `ManagedChromeProcess` exclusively owns the child/process-group kill authority and exposes bounded `wait_for_termination()`/`terminate()` operations. `LaunchedChrome` transfers all three values into the session; drop order stops the child before releasing/deleting its profile. Attached sessions construct only a normalized `LocalCdpEndpoint` and never construct either ownership guard.
- The shared discovery helper accepts an optional launch-request executable and orders candidates: explicit request, environment override, platform stable-channel paths, then PATH names. `ChromeLauncher::installations()` calls it without a request (therefore environment/platform/PATH only); `launch()` supplies `LaunchBrowser.executable`. Canonicalize, require a regular executable file, invoke the bounded product-version probe, and deduplicate canonical paths. Each result populates the complete `BrowserInstallation { executable, source, product, version }`; Electron is not platform-discovered as a managed Chrome installation.
- Reusable profile names permit a conservative portable character set and map below `profile_root/profiles/<name>`. Hold an exclusive lock for the complete managed session. Temporary profiles live below `profile_root/tmp/` and their guard removes only its own directory.
- Launch with `--remote-debugging-address=127.0.0.1`, an OS-selected free port, `--user-data-dir`, no-first-run/default-browser prompts, and the optional initial URL. Headless/gpu/sandbox policy is not hard-coded product behavior; tests may add explicit test-only flags.
- Establish process/profile ownership synchronously before the first await. On Unix create an isolated process group. Startup cancellation, timeout, and drop terminate the owned tree and release/clean the correct profile. Never kill by executable name.

Acceptance:
- Fake filesystem/process tests prove discovery precedence, source/product/version classification, profile traversal rejection, named-profile exclusion, temporary cleanup, cancellation before endpoint readiness, graceful close, escalation, distinct managed-child termination notification, ownership transfer/drop order, and no attached-resource cleanup.
- Tests use injected process/endpoint probes rather than sleeps.
- `browser.discovery.completed`, `browser.launch.started|ready|failed`, and `browser.shutdown.completed|incomplete` tracing is emitted by this story with the parent design's sanitized fields. Full executable/profile paths, command-line secrets, and source/debug errors never appear at info level.

### Unit 4: Target reducer, session supervision, root wiring, and real browser tests

**Story:** `epic-rust-cdp-capture-foundation-chrome-target-supervision-session-supervisor`

**Files:**
- `crates/krometrail-cdp/src/targets/mod.rs` (new)
- `crates/krometrail-cdp/src/targets/model.rs` (new)
- `crates/krometrail-cdp/src/targets/reducer.rs` (new)
- `crates/krometrail-cdp/src/targets/supervisor.rs` (new)
- `crates/krometrail-cdp/src/session.rs` (new)
- `crates/krometrail-cdp/src/lib.rs`
- `src/app.rs` (sequential edit after Unit 1; replace the transitional connector with production composition)
- `src/cli.rs`
- `tests/rust-runtime-smoke.rs` (sequential edit after Unit 1; broaden the stable no-browser smoke to the environment-dependent production outcome)
- `crates/krometrail-cdp/tests/support/chrome.rs` (new)
- `crates/krometrail-cdp/tests/support/static_fixture.rs` (new)
- `crates/krometrail-cdp/tests/target_reducer.rs` (new)
- `crates/krometrail-cdp/tests/session_supervision.rs` (new)
- `crates/krometrail-cdp/tests/chrome_session_real.rs` (new)
- `docs/ARCHITECTURE.md`

```rust
pub struct ReconnectPolicy {
    pub delays: Box<[Duration]>,
    pub attempt_timeout: Duration,
}

pub struct TransportTargetInfo {
    pub target_key: String,
    pub target_type: String,
    pub url: String,
    pub title: String,
    pub attached: bool,
    pub browser_context_key: Option<String>,
}

pub struct ReconnectedTarget {
    pub info: TransportTargetInfo,
    pub session: Option<TransportSessionId>,
    pub visibility: TargetVisibility,
}
pub struct ReconnectedSnapshot {
    pub connection_generation: u64,
    pub compatibility: BrowserCompatibility,
    pub targets: Vec<ReconnectedTarget>,
}

pub struct SupervisorTargetState {
    pub target: SupervisedTarget,
    pub transport_session: Option<TransportSessionId>,
    pub prior_to_suspension: Option<TargetLifecycle>,
}
pub struct SupervisorState {
    pub session_state: BrowserSessionState,
    pub connection_generation: u64,
    pub revision: u64,
    pub compatibility: BrowserCompatibility,
    pub targets_by_key: HashMap<String, SupervisorTargetState>,
    pub target_key_by_session: HashMap<TransportSessionId, String>,
}

pub enum SupervisorInput {
    InitialTargets(Vec<TransportTargetInfo>),
    TargetCreated(TransportTargetInfo),
    TargetInfoChanged(TransportTargetInfo),
    Attached { target_key: String, session: TransportSessionId },
    Detached { session: TransportSessionId, reason: Option<String> },
    TargetDestroyed { target_key: String },
    VisibilityChanged { target_key: String, visibility: TargetVisibility },
    ConnectionLost(TransportClose),
    BrowserProcessTerminated { exit: SanitizedProcessExit },
    Reconnected(ReconnectedSnapshot),
    ReconnectExhausted,
    StopRequested,
    Cancelled,
}

pub enum ShutdownCause { StopRequested, Cancelled, BrowserProcessTerminated, ReconnectExhausted }
pub enum SupervisorEffect {
    Attach { target_key: String },
    Detach { session: TransportSessionId },
    ProbeInitialVisibility { target_key: String, session: TransportSessionId },
    Publish(BrowserSessionEvent),
    BeginReconnect,
    Shutdown { cause: ShutdownCause },
}
pub struct Reduction { pub state: SupervisorState, pub effects: Vec<SupervisorEffect> }
pub fn reduce(state: SupervisorState, input: SupervisorInput) -> Result<Reduction>;
```

Implementation notes:
- A target is recordable only when type is `page`, URL is nonempty, and it is not a devtools/internal target. Unsupported target types are ignored, not failed.
- One reducer serializes all target state. Event tasks carry a connection generation; stale-generation events are discarded after reconnect.
- Reconnect delays come from configuration and a `RetrySleeper` adapter so deterministic tests advance attempts without wall-clock sleeps. Each successful reconnect repeats compatibility, subscriptions, discovery, flat auto-attach, and reconciliation before `Ready`.
- Local target decode/domain failure emits `TargetFailed` and detaches that target only. Browser transport loss moves the session to `Reconnecting`. Exhaustion emits `reconnect_exhausted`, cancels target tasks, performs ownership-correct shutdown, and ends the session.
- Event fan-out is bounded. Each subscriber tracks the supervisor `revision`; overflow yields a typed lag/refresh-required error containing only the missed revision range, increments a measurable outbound-subscriber lag counter, and requires recovery through `targets()`. Supervisor state cannot be backpressured by observers. No acceptance or telemetry claim is made about cdpkit's private upstream queue depth because that depth is not observable through its API.
- `ProductionBrowserConnector::installations()` delegates directly to `ChromeLauncher::installations()`; discovery policy and precedence exist only in `launcher/discovery.rs`. Unit 1 already made `doctor` discovery-only and installed the compile-real unavailable transition. This story explicitly edits `src/app.rs` second to replace `UnavailableBrowserConnector` with the production connector—never retaining both or layering a stale compatibility path. `doctor` continues to call `installations()` exactly once and never calls `connect`: nonempty results print a stable availability summary and exit 0; empty results retain the exact stable `browser_not_found` error and recovery established by Unit 1. It does not launch, attach, allocate a port, acquire a profile, or mutate the filesystem.
- Edit the Unit 1 smoke sequentially rather than replacing an unrelated test: broaden its accepted outcomes to the environment-dependent production contract—success with `browser available:` or exit 1 with `error[browser_not_found]` plus recovery—while continuing to reject provisional `unsupported`/`browser transport is not available` text. An `src/app.rs` fake asserts one `installations()` call and panics if `connect()` is called.

Acceptance:
- Reducer table tests construct the defined `SupervisorState`, `SupervisorInput`, `SupervisorEffect`, `TransportTargetInfo`, and `ReconnectedSnapshot` values directly and cover initial snapshot/event races, duplicate attach, every legal and illegal `Suspended` transition, two flat sessions with no cross-delivery, navigation/title mutation, initial visibility, target-local detach during a pending operation, unrelated target survival, reconnect success, stale-generation rejection, changed target ids, retry exhaustion, explicit cancellation/stop, and slow-subscriber revision lag.
- Managed child exit is delivered as `BrowserProcessTerminated`, publishes `SessionFailed(browser_process_terminated)`, skips reconnect/relaunch, and performs bounded owned cleanup; a transport `ConnectionLost` while the managed child remains alive follows the finite reconnect path. These are separately asserted.
- Real-Chrome tests launch an isolated temporary profile, serve `tests/fixtures/browser/cdp-transport-gate`, create two page targets, verify flat isolated sessions and target events, disconnect/reconnect the transport, and verify bounded clean shutdown with no leaked process/profile. A second test attaches to that same loopback browser and proves stopping the attached session leaves the browser alive.
- An opt-in `KROMETRAIL_ELECTRON_ENDPOINT` real test runs the same capability probe against an explicitly debug-enabled Electron renderer when available; deterministic fixtures remain the required Electron contract in ordinary CI.
- No real or deterministic test starts a production screencast.

## Supervision lifecycle

1. Validate launch/attach input without side effects.
2. Acquire managed profile/process ownership or normalize the external loopback endpoint.
3. Resolve the browser WebSocket and connect one transport.
4. Install target subscriptions, enable flat auto-attach/discovery, fetch the target snapshot, and run the capability probe.
5. Return `Ready` only after reconciliation; publish supervised recordable targets.
6. On target events, reduce state and apply idempotent attach/detach/probe effects. Failures remain target-local.
7. On transport connection loss, mark the session `Reconnecting`, suspend nonterminal targets, reject new target operations, and retry using the finite injected policy only while the managed child is alive or the attached endpoint remains eligible.
8. On managed-process termination, publish `browser_process_terminated`, skip reconnect/relaunch, and begin bounded owned cleanup.
9. On reconnect, repeat steps 3–5. Preserve identity only for exact target keys; increment attachment generation and discard stale prior-generation events.
10. On explicit stop/cancellation or retry exhaustion, cancel event/retry tasks, close channels, detach or close according to ownership, wait for bounded cleanup, and return a typed outcome/error. No frame-store flush occurs in this feature.

## Profile and process ownership matrix

| Mode | Profile | Process | Stop behavior | Reconnect behavior |
|---|---|---|---|---|
| Managed reusable | Krometrail path + exclusive lease; retained | Krometrail process group | `Browser.close`, wait, kill owned tree if needed; retain profile | reconnect transport only while child lives |
| Managed temporary | Krometrail temporary guard; delete after process ends | Krometrail process group | same, then delete owned temp path | reconnect transport only while child lives |
| Attach Chrome/Chromium | reported as external; never mutated | external | cancel/drop transport only | reconnect same explicit endpoint |
| Attach Electron renderer | reported as external; never mutated | external Electron app | cancel/drop renderer transport only | reconnect same endpoint; never inspect/kill main process |

## Implementation summary

All four dependency-ordered child stories are `stage: done`. The implementation introduced infrastructure-free browser/session contracts, exact cdpkit 0.4.0 behind a replaceable production transport seam, strict local endpoints and capability-based renderer probing, managed Chrome discovery/profile/process ownership, and a deterministic single-writer target/session supervisor with finite reconnect and bounded event fan-out.

Production composition now uses `ProductionBrowserConnector`; `doctor` performs discovery only. Managed sessions own and clean process groups/profiles, attached sessions leave external resources alive, and descendant-reaping regressions plus repeated real Chrome runs leave zero process/profile/test-root leaks. The implementation deliberately contains no production screencast start, frame ingestion, persistence, actions, or snapshots. Workspace default/no-default tests, spike regression, real Chrome opt-in tests, formatting, and denied-warning clippy pass.

## Review findings and disposition (2026-07-13)

GLM completeness review found one receiver-confirmed material gap: the parent acceptance contract requires real Chrome disconnect/rebuild, while implementation proved reconnect only through the deterministic transport factory. A new child story, `...-real-reconnect`, owns a real-browser transport-sever/rebuild test without changing the production contract.

The receiver accepted four lower-risk robustness proposals—structured subscriber-lag recovery, graceful cancellation close, late-stop idempotency, and stale reusable-profile lease metadata—and parked them together as unbound backlog item `idea-harden-session-edge-semantics`. Dead no-op code is a nit; polling and approximate lag bookkeeping are proportionate to this local supervisor.

The real-reconnect follow-up now physically severs a real cdpkit connection through an owned loopback fault proxy while Chrome remains alive, verifies a new connection and exact target identity/generation restoration, exercises post-rebuild commands/events, and leaves zero resources. It also fixed a verified late-event reducer rejection that could otherwise discard committed target state. All five child stories are `stage: done`; the feature returns to `stage: review` for adversarial closure.

## Testing strategy

### Deterministic

- Core unit tests validate every enum/transition/error/serde invariant and object-safe fake port.
- Scripted CDP tests use an in-process fake WebSocket/controller with ordered envelopes; no machine port or sleeps. They assert subscription-before-enable order, flat session routing, malformed/additive fields, disconnect closure, and compatibility classification.
- Pure reducer table tests execute identical input sequences twice and compare complete state/effects. Property-style permutations cover snapshot/event duplicates and stale connection generations.
- Launcher tests inject filesystem, process spawn, endpoint readiness, and retry sleeper ports. Cancellation is triggered at each scripted await point.

### Real integration

- `chrome_session_real.rs` uses the existing dependency-free `cdp-transport-gate` fixture, a temporary profile, and discovered Chrome. Tests are marked with an explicit real-browser test configuration so CI jobs can require them; local default runs skip only when Chrome is genuinely unavailable and print the reason.
- Managed and attach paths are both exercised. Process/profile leak assertions run even after a forced transport disconnect.
- Electron uses an explicit environment endpoint because the project does not own or bundle Electron. The deterministic capability probe is mandatory; the real Electron test is opt-in evidence.

### Workspace verification

- `cargo fmt --all --check`
- `cargo check --workspace --all-targets`
- `cargo test --workspace --all-targets`
- production real-browser integration command documented by the story and run where Chrome is available
- `cargo clippy --workspace --all-targets -- -D warnings`
- spike regression: `cargo test -p krometrail-cdp --features cdp-spike-cdpkit --test cdpkit_transport_contract`
- dependency scan proving `krometrail-core` remains infrastructure-free and cdpkit appears only in the adapter/spike files

## Observability

Use `tracing` events with stable event names and fields:

- `browser.discovery.completed`: candidate count and selected installation kind, never arbitrary PATH contents.
- `browser.launch.started|ready|failed`: ownership mode, sanitized executable basename, profile kind, child id, elapsed time; never full profile path or command-line secrets.
- `browser.compatibility.probed`: product, browser/protocol version, endpoint kind, required capability result, Electron-renderer classification.
- `browser.session.state_changed`: managed/attached, prior/next state, connection generation, reconnect attempt.
- `browser.session.subscriber_lagged`: missed outbound revision range and current revision only; no inferred cdpkit queue depth.
- `browser.target.discovered|attached|changed|suspended|closed|failed`: Krometrail target id, hashed/opaque browser target key, target type, attachment generation; do not log title, URL query, page text, event params, or raw adapter errors.
- `browser.shutdown.completed|incomplete`: disposition, elapsed time, forced termination, unfinished task count.

Private source errors may be attached to local debug spans but never copied into `KrometrailError`, status objects, or info logs. Session status exposes compatibility and typed state, not credentials or full endpoint URLs.

## Implementation order

1. `epic-rust-cdp-capture-foundation-chrome-target-supervision-contracts` — land contracts atomically with the transitional root connector and stable doctor smoke so the whole workspace compiles and tests green.
2. `epic-rust-cdp-capture-foundation-chrome-target-supervision-transport-adapter`
3. `epic-rust-cdp-capture-foundation-chrome-target-supervision-managed-launch`
4. `epic-rust-cdp-capture-foundation-chrome-target-supervision-session-supervisor` — sequentially replace the Unit 1 transition in `src/app.rs` and broaden its smoke; do not preserve stale unavailable composition.

The chain is intentionally serialized. Unit 1 atomically owns the changed core traits plus their immediate root/test consumers; Unit 4 later owns an explicit second edit of those consumers to replace the transition. Managed launch consumes transport-owned `LocalCdpEndpoint`, and stories 2–4 each append exports to `crates/krometrail-cdp/src/lib.rs`; their `depends_on` edges make every shared-file edit compile-real rather than pretending the adapters can land concurrently. The split still preserves cohesive verification surfaces: compiling core contract/transition, transport/probe, managed resources, then integrated supervision/production composition.

## Risks and pre-mortem

- **Riskiest assumption — auto-attach ordering remains tractable through cdpkit's named subscriptions.** Mitigation: subscribe first, reconcile against `getTargets`, reduce idempotently, carry connection generations, and prove race permutations plus real Chrome. A routing/decoder/lifecycle patch or fork is forbidden; demonstrated failure reopens the approved fallback decision.
- **Upstream cdpkit subscriptions may queue beyond Krometrail's visibility.** The adapter drains them continuously, but cdpkit exposes no measurable queue depth, so this design makes no upstream-lag acceptance claim. Krometrail measures only its own bounded outbound subscriber channels via revision gaps; overflow returns refresh-required and increments the outbound lag counter. If real evidence later shows upstream growth, the transport decision is reopened rather than hidden behind invented telemetry.
- **Chrome process cleanup can leak descendants or delete the wrong profile under cancellation.** Ownership guards are established before awaits, process groups are killed only by held child identity, reusable profiles are never deleted, and cancellation-point tests verify every path.
- **Attach reconnect can bind to a replaced browser instance.** Exact target keys are the only continuity key. Changed keys become closed/new targets, compatibility is re-probed, and no URL/title matching fabricates continuity.
- **Electron branding is inconsistent.** Product strings are status hints only. Acceptance is capability-based against a page target; Node-only endpoints fail explicitly.
- **Visibility cannot be continuously observed before capture starts.** This feature reports a probed initial state and a reducer input for later events. It does not violate scope by starting a screencast; the next feature owns continuous visibility evidence.
- **Real-browser tests become silently optional.** The integration binary reports explicit skip reasons locally, while the implementation story must identify and run a Chrome-enabled command/CI lane before review. Deterministic tests do not replace that evidence.
- **Root wiring accidentally launches Chrome during `doctor`.** `doctor` calls discovery only. Launch and attach remain explicit application-service requests.

## Acceptance criteria

- [ ] Exact cdpkit 0.4.0 is production-enabled solely behind `krometrail-cdp::transport`; no cdpkit type or reconnect policy leaks into core, and no fallback implementation is pre-built.
- [ ] Linux/macOS discovery, named reusable profiles, temporary profiles, loopback attach, ownership-correct stop, and cancellation-safe process/profile cleanup satisfy deterministic and real-browser tests.
- [ ] Non-loopback/credential-bearing endpoints and invalid profile names fail before side effects; attach never closes a browser or mutates/deletes an external profile.
- [ ] Compatibility reports browser/protocol identity and one registry-derived result for every required renderer capability; capable Electron renderers pass and Node-main-process-only endpoints fail explicitly.
- [ ] Target supervision uses flat sessions, publishes creation/navigation/initial visibility/closure/failure changes, isolates target-local failures, and never confuses two sessions or duplicate discovery inputs.
- [ ] Reconnect is finite, observable, cancellation-aware, and reconstructs subscriptions/discovery/attachments/domain state. Exact target keys preserve identity; missing/changed keys close/create rather than URL-match.
- [ ] Managed browser death and reconnect exhaustion end the session with a stable structured error and bounded cleanup; explicit stop returns `ManagedBrowserClosed` or `Detached` correctly.
- [ ] Root composition uses the default-feature production connector; installation discovery delegates to `ChromeLauncher`, and the replaced doctor smoke proves the discovery-only success/no-browser contract without the provisional unsupported message.
- [ ] Real Chrome proves managed launch, attach-without-close, two target sessions, disconnect/rebuild, and leak-free shutdown against the current fixture. Electron has mandatory deterministic probe coverage and an opt-in real-endpoint test.
- [ ] Structured logs expose lifecycle, compatibility, target, reconnect, and cleanup measurements without page content, raw URLs, credentials, or serialized source errors.
- [ ] No production screencast ingestion, frame acknowledgement, bounded frame queue, persistence, or capture-gap behavior lands in this feature.
- [ ] Workspace format/check/test/clippy, production real-browser integration, spike regression, and dependency-boundary scans pass; `docs/ARCHITECTURE.md` is rolled forward to the landed final5 production boundary.

## Review repair ledger

- **B1 — profile semantics:** `ProfileRef` now distinguishes managed identity from externally owned/unknown attach profiles, `BrowserSessionPort` returns it, and Unit 1 explicitly migrates the downstream `RecordingSession` field, wire form, constructor, accessor, fixtures, and tests.
- **B2 — browser identity contracts:** `BrowserInstallation`, `BrowserInstallationSource`, `BrowserProduct`, `BrowserProductVersion`, and the full runtime `BrowserVersion` are defined with their fields and discovery/runtime roles.
- **B3 — compile-real ownership/dependencies:** stories are serialized contracts → transport → managed launch → supervisor. The contracts story atomically owns `src/app.rs` and `tests/rust-runtime-smoke.rs` with the changed traits, an empty-installations `UnavailableBrowserConnector`, and stable discovery-only `browser_not_found` doctor behavior. The supervisor story explicitly edits those files later to replace—not coexist with—the transition and broaden the smoke to production discovery outcomes. `LocalCdpEndpoint` is defined in Unit 2 and consumed in Unit 3; sequential `src/lib.rs` ownership is explicit in required files and dependency edges.
- **Error contract completeness:** Unit 1 acceptance now names all nine designed stable codes, preventing the earlier partial `browser_not_found`/`browser_process_terminated` allocation from silently omitting launch, compatibility, profile, target, reconnect, cancellation, or shutdown failures.
- **Supervisor contract gap:** `SupervisorState`, target state, input, effect, transport target info, reconnect snapshot, shutdown cause, and sanitized process exit are concrete. `browser_process_terminated` has a separate watcher input, event error, and no-reconnect path.
- **Resource ownership gap:** endpoint, profile lease, managed process, and `LaunchedChrome` transfer/drop responsibilities are explicit; attach constructs no ownership guards.
- **Acceptance allocation:** compatibility tracing is owned by story 2; discovery/launch/shutdown tracing by story 3; story 4 owns supervisor/target tracing and the exact doctor smoke replacement.
- **State/measurement gaps:** only observable outbound subscriber revision lag is measurable; `BrowserSessionState` and recording `SessionLifecycle` are distinct; connector discovery delegates to `ChromeLauncher`; every legal and illegal `Suspended` edge is specified.
- **Feature topology:** default builds enable production cdpkit through `cdpkit-transport`, no-default builds retain the seam, and spike features remain opt-in. No production/default path imports spike code.

## Advisory review

This repair resolves the recorded GLM findings without another advisory pass because the caller explicitly prohibited subagents. The feature remains `implementing` and must receive the normal independent feature review after implementation; this design does not claim that review has occurred.
