---
id: epic-rust-cdp-capture-foundation-chrome-target-supervision
kind: feature
stage: implementing
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
- **Transport selection:** exact `cdpkit = 0.4.0` becomes a normal `krometrail-cdp` dependency behind an owned object-safe transport trait. This is the approved candidate and keeps replacement localized.
- **Target attachment:** install named target-event subscriptions before enabling `Target.setDiscoverTargets` and `Target.setAutoAttach(autoAttach=true, waitForDebuggerOnStart=false, flatten=true)`, then reconcile with `Target.getTargets`. Event and snapshot inputs are reduced through one idempotent state machine, preventing discovery/attach races from creating duplicate logical targets.
- **Target identity across reconnect:** preserve a Krometrail `TargetId` only when the same browser target key is rediscovered. Never match by URL or title. Missing old keys close their targets; newly observed keys receive new IDs.
- **Reconnect safety:** reconnect the CDP connection to the same endpoint while a managed child remains alive or an attached endpoint remains reachable. Do not relaunch Chrome or reopen URLs. Re-probe compatibility and rebuild discovery, flat attachments, and domain state on every successful connection.
- **Electron support:** classify endpoint kind from observed product/user-agent only for status; accept Chrome, Chromium, or Electron only when the runtime capability probe passes on a recordable page target. This supports Electron renderers without claiming control of its Node main process.
- **Profile ownership:** named and temporary profiles are always under Krometrail's configured profile root. A held lease prevents concurrent use of a named profile; temporary directories are deleted only by their owning guard. Attach mode never acquires, mutates, or deletes a profile.
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
pub enum BrowserProduct { Chrome, Chromium, ElectronRenderer, OtherChromium }
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
    pub version: BrowserVersion,
    pub product: BrowserProduct,
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
    fn profile(&self) -> &ProfileIdentity;
    fn state(&self) -> BrowserSessionState;
    fn targets(&self) -> PortFuture<'_, Result<Vec<SupervisedTarget>>>;
    fn subscribe(&self) -> PortFuture<'_, Result<Box<dyn BrowserSessionEvents>>>;
    fn stop(&self) -> PortFuture<'_, Result<BrowserStopOutcome>>;
}
```

Add stable core error codes `browser_launch_failed`, `browser_compatibility_failed`, `profile_in_use`, `target_failed`, `reconnect_exhausted`, `cancelled`, and `shutdown_incomplete`. Adapter-private errors retain sources for structured logs and map once at the core boundary with safe messages, `RetryAdvice`, and concrete recovery. Unknown endpoint input fails before filesystem/process/network side effects; missing required capability fails before the session reaches `Ready`.

`TargetLifecycle` gains `Suspended`. Connection loss transitions nonterminal targets to `Suspended`; restoration returns them to their prior attached/visible state with an incremented attachment generation, while absence after reconciliation closes them. Target-local detach/probe failure transitions only that target to `Failed`.

## Replaceable cdpkit transport boundary

### Owned transport contract

**Files:** `crates/krometrail-cdp/src/transport/mod.rs`, `crates/krometrail-cdp/src/transport/cdpkit.rs`, `crates/krometrail-cdp/src/transport/error.rs`

```rust
pub enum CommandScope { Browser, Session(TransportSessionId) }
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

The normal manifest pins cdpkit and enables only required Tokio/futures/serde-json features. `cdp-spike-cdpkit` reuses the same workspace pin but remains a separately gated evidence path.

## Implementation units and exact files

### Unit 1: Core browser supervision contracts

**Story:** `epic-rust-cdp-capture-foundation-chrome-target-supervision-contracts`

**Files:**
- `crates/krometrail-core/src/browser/mod.rs`
- `crates/krometrail-core/src/browser/target.rs`
- `crates/krometrail-core/src/browser/session.rs` (new)
- `crates/krometrail-core/src/ports/browser.rs`
- `crates/krometrail-core/src/lifecycle.rs`
- `crates/krometrail-core/src/error.rs`
- `crates/krometrail-core/src/lib.rs`
- `crates/krometrail-core/src/ports/mod.rs`

Implementation notes:
- Preserve UUID-backed `TargetId`; CDP target/session keys remain validated opaque adapter-origin strings.
- Generate stable enum names and exhaustive round-trip coverage from existing registry macros.
- `BrowserSessionEvents` is runtime-neutral like existing ports; Tokio channels remain adapter-private.
- Keep `RecordingSession` compatible with the richer profile/version values; do not add capture behavior.

Acceptance:
- Core has no cdpkit, CDP, WebSocket, Tokio, URL-parser, or filesystem adapter type.
- All state transitions, malformed serialized states, duplicate capabilities, and safe error mappings have exhaustive tests.
- Existing browser-port fakes are updated to prove managed/attach stop outcomes and event-stream closure.

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

Implementation notes:
- Normalize `http://127.0.0.1:<port>`, `http://[::1]:<port>`, `ws://127.0.0.1:<port>/...`, and localhost equivalents. Resolve HTTP endpoints through `/json/version`; reject credentials, fragments, non-loopback resolved addresses, TLS/public endpoints, and unsupported schemes before connect.
- `RENDERER_CAPABILITY_PROBES` is the single registry for capability id, requirement, scope, command, and decoder. Compatibility status derives from it rather than separate lists.
- Subscribe before discovery setup. Probe `Browser.getVersion`, target discovery/flat attachment, then required Page/Runtime/Accessibility/Input operations on one page. Verify screencast command/event availability without starting a screencast. Electron product detection never substitutes for this probe.
- A missing recordable page returns a bounded compatibility failure with a recovery action to open a renderer page; it does not attach to Electron's Node inspector.

Acceptance:
- Scripted tests prove browser/session scoping, named-event params, event-before-response ordering, close propagation, malformed response rejection, and no cross-session delivery.
- Chrome-, generic Chromium-, and Electron-labelled fixtures with equal capabilities are accepted; an Electron main-process-only/Node inspector fixture and every missing required capability are rejected with stable errors.
- Default builds compile production cdpkit while spike gates remain opt-in and unchanged.

### Unit 3: Chrome discovery, profiles, endpoints, and process ownership

**Story:** `epic-rust-cdp-capture-foundation-chrome-target-supervision-managed-launch`

**Files:**
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
- Deterministic discovery order: explicit executable, environment override, platform stable-channel paths, then PATH names. Canonicalize, require a regular executable file, and deduplicate canonical paths.
- Reusable profile names permit a conservative portable character set and map below `profile_root/profiles/<name>`. Hold an exclusive lock for the complete managed session. Temporary profiles live below `profile_root/tmp/` and their guard removes only its own directory.
- Launch with `--remote-debugging-address=127.0.0.1`, an OS-selected free port, `--user-data-dir`, no-first-run/default-browser prompts, and the optional initial URL. Headless/gpu/sandbox policy is not hard-coded product behavior; tests may add explicit test-only flags.
- Establish process/profile ownership synchronously before the first await. On Unix create an isolated process group. Startup cancellation, timeout, and drop terminate the owned tree and release/clean the correct profile. Never kill by executable name.

Acceptance:
- Fake filesystem/process tests prove discovery precedence, profile traversal rejection, named-profile exclusion, temporary cleanup, cancellation before endpoint readiness, graceful close, escalation, and no attached-resource cleanup.
- Tests use injected process/endpoint probes rather than sleeps.

### Unit 4: Target reducer, session supervision, root wiring, and real browser tests

**Story:** `epic-rust-cdp-capture-foundation-chrome-target-supervision-session-supervisor`

**Files:**
- `crates/krometrail-cdp/src/targets/mod.rs` (new)
- `crates/krometrail-cdp/src/targets/model.rs` (new)
- `crates/krometrail-cdp/src/targets/reducer.rs` (new)
- `crates/krometrail-cdp/src/targets/supervisor.rs` (new)
- `crates/krometrail-cdp/src/session.rs` (new)
- `crates/krometrail-cdp/src/lib.rs`
- `src/app.rs`
- `src/cli.rs`
- `tests/rust-runtime-smoke.rs`
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

pub enum SupervisorInput {
    InitialTargets(Vec<TransportTargetInfo>),
    TargetCreated(TransportTargetInfo),
    TargetInfoChanged(TransportTargetInfo),
    Attached { target_key: String, session: TransportSessionId },
    Detached { session: TransportSessionId, reason: Option<String> },
    TargetDestroyed { target_key: String },
    VisibilityChanged { target_key: String, visibility: TargetVisibility },
    ConnectionLost(TransportClose),
    Reconnected(ReconnectedSnapshot),
    Cancelled,
}

pub struct Reduction { pub state: SupervisorState, pub effects: Vec<SupervisorEffect> }
pub fn reduce(state: SupervisorState, input: SupervisorInput) -> Result<Reduction>;
```

Implementation notes:
- A target is recordable only when type is `page`, URL is nonempty, and it is not a devtools/internal target. Unsupported target types are ignored, not failed.
- One reducer serializes all target state. Event tasks carry a connection generation; stale-generation events are discarded after reconnect.
- Reconnect delays come from configuration and a `RetrySleeper` adapter so deterministic tests advance attempts without wall-clock sleeps. Each successful reconnect repeats compatibility, subscriptions, discovery, flat auto-attach, and reconciliation before `Ready`.
- Local target decode/domain failure emits `TargetFailed` and detaches that target only. Browser transport loss moves the session to `Reconnecting`. Exhaustion emits `reconnect_exhausted`, cancels target tasks, performs ownership-correct shutdown, and ends the session.
- Event fan-out is bounded. Slow subscribers receive an explicit lag/refresh-required error and recover through `targets()`; supervisor state cannot be backpressured by observers.
- `src/app.rs` constructs the production connector. `doctor` calls `installations()` and reports a stable no-browser error when empty; it does not launch, attach, or mutate profiles.

Acceptance:
- Deterministic tests cover initial snapshot/event races, duplicate attach, two flat sessions with no cross-delivery, navigation/title mutation, initial visibility, target-local detach during a pending operation, unrelated target survival, reconnect success, stale-generation rejection, changed target ids, retry exhaustion, cancellation at every await boundary, slow subscriber behavior, managed close, and attach detach.
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
7. On connection loss, mark the session `Reconnecting`, suspend nonterminal targets, reject new target operations, and retry using the finite injected policy.
8. On reconnect, repeat steps 3–5. Preserve identity only for exact target keys; increment attachment generation and discard stale prior-generation events.
9. On explicit stop/cancellation or retry exhaustion, cancel event/retry tasks, close channels, detach or close according to ownership, wait for bounded cleanup, and return a typed outcome/error. No frame-store flush occurs in this feature.

## Profile and process ownership matrix

| Mode | Profile | Process | Stop behavior | Reconnect behavior |
|---|---|---|---|---|
| Managed reusable | Krometrail path + exclusive lease; retained | Krometrail process group | `Browser.close`, wait, kill owned tree if needed; retain profile | reconnect transport only while child lives |
| Managed temporary | Krometrail temporary guard; delete after process ends | Krometrail process group | same, then delete owned temp path | reconnect transport only while child lives |
| Attach Chrome/Chromium | reported as external; never mutated | external | cancel/drop transport only | reconnect same explicit endpoint |
| Attach Electron renderer | reported as external; never mutated | external Electron app | cancel/drop renderer transport only | reconnect same endpoint; never inspect/kill main process |

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
- `browser.target.discovered|attached|changed|suspended|closed|failed`: Krometrail target id, hashed/opaque browser target key, target type, attachment generation; do not log title, URL query, page text, event params, or raw adapter errors.
- `browser.shutdown.completed|incomplete`: disposition, elapsed time, forced termination, unfinished task count.

Private source errors may be attached to local debug spans but never copied into `KrometrailError`, status objects, or info logs. Session status exposes compatibility and typed state, not credentials or full endpoint URLs.

## Implementation order

1. `epic-rust-cdp-capture-foundation-chrome-target-supervision-contracts`
2. After contracts, in parallel: `epic-rust-cdp-capture-foundation-chrome-target-supervision-transport-adapter` and `epic-rust-cdp-capture-foundation-chrome-target-supervision-managed-launch`
3. `epic-rust-cdp-capture-foundation-chrome-target-supervision-session-supervisor`

This is the minimal useful split: one shared contract foundation, two independently owned adapters, and one integration/reconciliation unit. Smaller stories would divide tightly coupled files without creating independent verification surfaces.

## Risks and pre-mortem

- **Riskiest assumption — auto-attach ordering remains tractable through cdpkit's named subscriptions.** Mitigation: subscribe first, reconcile against `getTargets`, reduce idempotently, carry connection generations, and prove race permutations plus real Chrome. A routing/decoder/lifecycle patch or fork is forbidden; demonstrated failure reopens the approved fallback decision.
- **Upstream cdpkit subscriptions are unbounded.** Target lifecycle volume is much lower than frames, but this remains real. Event readers drain continuously into the reducer's bounded command path; lag is measured and a sustained growth failure triggers transport reconsideration rather than an invisible queue claim.
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
- [ ] Root composition uses the production connector and `doctor` performs discovery only.
- [ ] Real Chrome proves managed launch, attach-without-close, two target sessions, disconnect/rebuild, and leak-free shutdown against the current fixture. Electron has mandatory deterministic probe coverage and an opt-in real-endpoint test.
- [ ] Structured logs expose lifecycle, compatibility, target, reconnect, and cleanup measurements without page content, raw URLs, credentials, or serialized source errors.
- [ ] No production screencast ingestion, frame acknowledgement, bounded frame queue, persistence, or capture-gap behavior lands in this feature.
- [ ] Workspace format/check/test/clippy, production real-browser integration, spike regression, and dependency-boundary scans pass; `docs/ARCHITECTURE.md` is rolled forward to the landed final5 production boundary.

## Advisory review

Design-time advisory review was skipped because the caller explicitly prohibited subagents. This is non-blocking under the principles policy. The feature is high-risk and must receive the normal independent feature review after implementation; this design does not claim that review has occurred.
