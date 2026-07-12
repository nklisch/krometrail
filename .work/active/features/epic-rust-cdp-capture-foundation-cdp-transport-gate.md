---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate
kind: feature
stage: implementing
tags: [browser, infra]
parent: epic-rust-cdp-capture-foundation
depends_on: [epic-rust-cdp-capture-foundation-rust-runtime-contracts]
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# CDP Transport Compatibility Gate

## Brief

Prove the selected Rust CDP path against real Chrome before production lifecycle and capture code commits to it. A deliberately disposable `cdpkit` spike exercises every required protocol domain, browser-level commands, flat target sessions, typed operations, raw command and event escape hatches, and sustained `Page.screencastFrame` acknowledgement while recording browser and protocol versions as evidence.

Turn the spike results into an explicit transport decision: adopt `cdpkit` only when all required gates pass; otherwise use the evidence to choose between `chromey` and a minimal owned transport. Spike-only scaffolding must not become the production capture pipeline. This feature qualifies and selects the adapter mechanism; it does not own Chrome profiles, reconnect supervision, or bounded production ingestion.

## Epic context

- Parent epic: `epic-rust-cdp-capture-foundation`
- Position in epic: transport gate — depends on the Rust contracts and blocks production browser integration
- Design decisions inherited: evidence-gated `cdpkit` adoption with an explicit `chromey` or owned-transport fallback

## Foundation references

- `docs/SPEC.md` — Supported Environment, Sessions and Targets, Continuous Visual Capture, and Errors and Degraded Operation
- `docs/ARCHITECTURE.md` — Browser Connection, Frame Ingestion, and Technology Decisions
- `docs/EVALUATION.md` — Capture-Fidelity Evaluation and Timing Integrity

## Research grounding

- Versioned evidence: `docs/research/rust-cdp-transport-2026-07.md`
- Execution reference: `.agents/skills/rust-cdp-transport/SKILL.md`
- Dispatch rationale: direct-read only because the autopilot caller prohibited subagents and questions; crates.io metadata, pinned published source, CI/tests, current GitHub issues, and the official CDP source supplied the required evidence.
- Decision posture: final v2 Linux and macOS reports select exact `cdpkit` 0.4.0 from gate revision `07b0990c0d9e4fea9057fcab5c35e56691ff69eb`, unchanged configuration/fixture/source attestation, and bound candidate-contract evidence. `chromey` 2.52.0 and the owned raw-envelope transport remain documented fallbacks only after a demonstrated cdpkit failure; retained v1/prior-v2 evidence is historical.

## Design decisions

- **Spike location:** Keep all disposable qualification code in the existing `krometrail-cdp` crate behind non-default `cdp-spike` and candidate-specific feature flags. A sixth crate would make disposal clearer but would expand the workspace for code that must never become a product boundary.
- **Production boundary:** Leave `crates/krometrail-cdp/src/lib.rs` as the truthful empty production adapter boundary outside spike feature gates. Do not add `unimplemented!`, a fake success adapter, root composition wiring, production lifecycle/capture code, or a core-port revision without evidence that the current seam is unworkable.
- **Candidate decision:** The generated v2 decision selects exact `cdpkit` 0.4.0. Both platform reports pass the strict platform-faithful contract with canonical RSS fields, observed lifecycle/wire-trace results, and identical immutable gate provenance. Its adapter remains replaceable; `chromey` and owned-transport work stay late-bound.
- **Shared qualification contract:** One spike-only `SpikeTransport` trait and one `run_transport_scenarios` suite drive the in-memory fake and candidate adapters. This makes the fake a proof of the harness and prevents candidate-specific gate drift.
- **Raw compatibility claim:** Represent cdpkit's escape hatch as named raw event parameters, not a wildcard/full-envelope stream. The limitation is explicit in types, evidence, and selection. If a real foundation requirement needs authoritative full envelopes, cdpkit fails rather than the requirement being weakened.
- **Evidence authority:** Rust v2 evidence types and `TransportGateId::ALL` are the source of truth; checked-in JSON Schema and `decision.json` are generated under `docs/evidence/cdp-transport/v2/`. The retained v1 Linux/macOS reports and decision are historical only. Raw logs and profiles stay under ignored `target/` paths.
- **Quantified sustained gate:** Require both 60 seconds and 1,000 frames, prompt typed acknowledgement before a deliberately saturated capacity-1 handoff, explicit drops, bounded ack-latency and RSS-trend proxies, and honest disclosure that cdpkit subscriber queue depth cannot be inspected.
- **Cross-platform decision:** Linux supplies implementation and first qualification evidence; unchanged decisive gates run on macOS before the feature can complete. A macOS failure blocks selection rather than creating a platform exception.
- **UI surface:** None; this is disposable protocol qualification and evidence.
- **Dispatch:** Direct-read only because the autopilot caller prohibited questions and subagents. The parent, foundation docs, implemented crate/core boundary, versioned research, and supplied independent advisory bounded the design sufficiently.

## Other agent review

- Invoked because: transport selection is a risky architectural commitment and the caller supplied a GLM 5.2 design advisory.
- Reviewer: GLM 5.2, advisory/completeness pass supplied before this design.
- Accepted:
  - Keep the spike in `krometrail-cdp` behind non-default feature flags; do not add a sixth crate.
  - Use one spike-only transport contract/scenario suite for deterministic fake and candidate adapters.
  - Pin exact cdpkit 0.4.0; produce versioned, machine-readable, reproducible evidence.
  - Cover fake routing, protocol drift, ordering, and disconnect without sleeps; cover real Chrome typed/raw/flat-session and sustained screencast behavior.
  - Acknowledge promptly before deliberate bounded-handoff saturation; measure ack latency and memory trend without inventing queue-depth visibility.
  - Require Linux implementation evidence and decisive macOS evidence before done.
  - Create chromey/owned fallback work only after demonstrated cdpkit failure; do not revise core ports or implement production capture unless evidence forces it.
  - Roll the final decision into evidence, research, skill, feature, epic, and the architecture's current technology assertion.
- Rejected or narrowed:
  - Rejected any implication that cdpkit offers wildcard raw envelopes. Its API exposes parameters for a named event, so the design names `NamedEventParams` and records the missing envelope/wildcard capability as a limitation or failure where required.
  - Rejected queue-depth assertions for cdpkit's unbounded subscriber. The gate uses process RSS trend and ack/handoff counters as explicit proxies and states what they cannot prove.
  - Narrowed “fallback” to a late-bound substrate transition: implementation creates a candidate story only after evidence identifies which fallback is justified. Pre-creating both would commit work that may never be needed.
- Phase 2 adversarial review: skipped because the caller explicitly prohibited subagents; the supplied advisory was checked against source, research, and foundation contracts locally. Design-time advisory remains non-blocking; final autopilot completion still requires its configured fresh-context review path.

## Architectural choice

### Chosen: feature-gated qualification laboratory inside `krometrail-cdp`

The existing adapter crate owns a non-default spike module containing a candidate-neutral contract, deterministic fake peers, a real-Chrome runner, and generated evidence contracts. Exact cdpkit 0.4.0 is the first adapter. The same scenarios run against fake and candidate implementations, then Linux and macOS reports feed deterministic selection. This optimizes for a narrow replaceable production seam, reproducibility, and truthful failure evidence while preventing the spike from becoming the production pipeline.

### Alternative: a sixth `krometrail-cdp-spike` crate

A separate crate would make dependency and deletion boundaries obvious, but it would add permanent workspace topology for disposable qualification code and duplicate access patterns around `krometrail-cdp`. Feature gates supply the same compile-time isolation with less repository surface.

### Alternative: candidate-specific integration tests without a transport contract

Direct cdpkit tests would be shorter initially, but any fallback would receive a different harness and incomparable evidence. It also risks encoding cdpkit's named-params limitation as the product contract. A shared spike trait keeps scenarios and measurements stable while candidate adapters expose their actual capabilities.

### Alternative: implement an owned raw-envelope transport immediately

An owned transport gives the strongest protocol-drift boundary, but it assumes the maintenance cost before evidence shows it is necessary. Late-binding preserves that option without outrunning the gate.

## Tricky unit first: honest protocol evolution and backpressure evidence

The hardest unit is not sending a CDP command; it is proving that a young client does not hide protocol evolution or backlog while appearing healthy. The design therefore distinguishes named event parameters from full envelopes in the type system, runs deterministic unknown-event/additive-field/unknown-enum scripts, records acknowledgement before every bounded handoff attempt, and measures only observable ack latency, drops, frame continuity, and process RSS. It never equates a bounded downstream queue with cdpkit's unbounded subscriber or claims unavailable queue depth.

## Implementation units

### Unit 1: Spike feature boundary and exact dependency pin

**Story:** `epic-rust-cdp-capture-foundation-cdp-transport-gate-spike-contract-harness`

**Files:**
- `Cargo.toml`
- `crates/krometrail-cdp/Cargo.toml`
- `crates/krometrail-cdp/src/lib.rs`
- `Cargo.lock` (first changed when the cdpkit feature is selected)

```toml
# root Cargo.toml
[workspace.dependencies]
cdpkit = "=0.4.0"
futures-util = "0.3"
schemars = "1"
tokio-tungstenite = "=0.30.0"

# crates/krometrail-cdp/Cargo.toml
[features]
default = []
cdp-spike = ["dep:futures-util", "dep:schemars", "dep:serde_json", "dep:tokio-tungstenite"]
cdp-spike-cdpkit = ["cdp-spike", "dep:cdpkit"]

[dependencies]
cdpkit = { workspace = true, optional = true }
futures-util = { workspace = true, optional = true }
schemars = { workspace = true, optional = true }
serde_json = { workspace = true, optional = true }
tokio-tungstenite = { workspace = true, optional = true }
```

```rust
// crates/krometrail-cdp/src/lib.rs
#[cfg(feature = "cdp-spike")]
#[doc(hidden)]
pub mod spike;
```

**Implementation notes:**
- Candidate dependencies are unavailable under default features; default product composition remains unchanged.
- The exact dependency is declared once at workspace root and Cargo.lock records the crates.io checksum. Evidence copies version/checksum values, not a machine cache path.
- Spike code may use Tokio because it is infrastructure qualification; no spike type enters `krometrail-core`.

**Acceptance criteria:**
- [ ] Default workspace metadata does not select cdpkit and the root binary cannot invoke the spike.
- [ ] `cdp-spike-cdpkit` resolves exactly cdpkit 0.4.0 from crates.io.
- [ ] No sixth workspace member or production adapter stub is added.

### Unit 2: Single spike transport, deterministic fake, and scenario suite

**Story:** `epic-rust-cdp-capture-foundation-cdp-transport-gate-spike-contract-harness`

**Files:**
- `crates/krometrail-cdp/src/spike/mod.rs`
- `crates/krometrail-cdp/src/spike/contract.rs`
- `crates/krometrail-cdp/src/spike/fake.rs`
- `crates/krometrail-cdp/src/spike/scripted_peer.rs`
- `crates/krometrail-cdp/src/spike/scenarios.rs`
- `crates/krometrail-cdp/tests/transport_contract.rs`
- `crates/krometrail-cdp/tests/fixtures/protocol/{unknown-event,additive-field,unknown-enum}.json`

```rust
pub type SpikeFuture<'a, T> =
    std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;
pub type EventStream = std::pin::Pin<
    Box<dyn futures_util::Stream<Item = Result<NamedEventParams, SpikeError>> + Send>,
>;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum TransportScope {
    Browser,
    Session { session_id: String },
}

#[derive(Clone, Debug, PartialEq)]
pub struct NamedEventParams {
    pub method: String,
    pub scope: TransportScope,
    pub params: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScreencastFrame {
    pub scope: TransportScope,
    pub sequence: i64,
    pub data: String,
    pub metadata: serde_json::Value,
}

pub trait SpikeTransportFactory: Send + Sync {
    fn candidate(&self) -> CandidateIdentity;
    fn connect<'a>(&'a self, browser_ws_url: &'a str)
        -> SpikeFuture<'a, Result<Box<dyn SpikeTransport>, SpikeError>>;
}

pub trait SpikeTransport: Send + Sync {
    fn send_raw<'a>(&'a self, scope: &'a TransportScope, method: &'a str, params: serde_json::Value)
        -> SpikeFuture<'a, Result<serde_json::Value, SpikeError>>;
    fn subscribe_named<'a>(&'a self, scope: &'a TransportScope, method: &'a str)
        -> SpikeFuture<'a, Result<EventStream, SpikeError>>;
    fn run_typed_probe<'a>(&'a self, session: &'a TransportScope)
        -> SpikeFuture<'a, Result<TypedProbeEvidence, SpikeError>>;
    fn attach_flat_page<'a>(&'a self, target_id: &'a str)
        -> SpikeFuture<'a, Result<TransportScope, SpikeError>>;
    fn start_screencast<'a>(&'a self, session: &'a TransportScope)
        -> SpikeFuture<'a, Result<(), SpikeError>>;
    fn next_screencast_frame<'a>(&'a self, session: &'a TransportScope)
        -> SpikeFuture<'a, Result<ScreencastFrame, SpikeError>>;
    fn ack_screencast<'a>(&'a self, session: &'a TransportScope, sequence: i64)
        -> SpikeFuture<'a, Result<(), SpikeError>>;
    fn close_reason(&self) -> Option<DisconnectEvidence>;
}

pub async fn run_transport_scenarios(
    factory: &dyn SpikeTransportFactory,
    peer: &mut ScriptedCdpPeer,
) -> ScenarioEvidence;
```

**Implementation notes:**
- `FakeTransport` and every candidate adapter implement this one trait. `ScriptedCdpPeer` provides a deterministic WebSocket endpoint for candidate wire tests; its script advances through explicit barriers/oneshots rather than sleeps.
- The suite drives two sessions through 100 uniquely tagged command results and 100 same-named events per session, with zero cross-delivery. It then covers event-before-response, detach with one pending call, named unknown event, additive field, unknown enum, forced disconnect, closed subscription, a fresh connection, and explicit two-session rebuild.
- A timeout may guard against deadlock and produce `SpikeErrorCode::Deadline`; it cannot be used to sequence the script.

**Acceptance criteria:**
- [ ] Fake and candidate paths share the exact scenario function and gate registry.
- [ ] Routing/drift/disconnect scenarios contain no `sleep` and pass repeatedly without timing sensitivity.
- [ ] The raw event contract explicitly lacks a full-envelope/wildcard claim.

### Unit 3: Structured error and machine-readable evidence contracts

**Story:** `epic-rust-cdp-capture-foundation-cdp-transport-gate-spike-contract-harness`

**Files:**
- `crates/krometrail-cdp/src/spike/error.rs`
- `crates/krometrail-cdp/src/spike/evidence.rs`
- `docs/evidence/cdp-transport/v2/schema.json`

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SpikeErrorCode {
    Connect, Command, Protocol, Routing, SubscriptionClosed,
    Disconnected, Deadline, Invariant, Io, Evidence,
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error, serde::Serialize, schemars::JsonSchema)]
#[error("{code:?}: {message}")]
pub struct SpikeError {
    pub code: SpikeErrorCode,
    pub message: String,
    pub gate: Option<TransportGateId>,
    pub retryable: bool,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum TransportGateId {
    DeterministicRouting,
    TypedDomains,
    FlatSessionIsolation,
    RawBrowserCommand,
    RawSessionCommand,
    NamedRawEventParams,
    ProtocolDriftSurvival,
    SustainedScreencast,
    PromptAcknowledgement,
    BoundedHandoffSaturation,
    BoundedMemoryProxy,
    DisconnectCleanup,
    ExplicitReconnectRebuild,
}
impl TransportGateId { pub const ALL: [Self; 13]; }

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GateStatus { Pass, Fail, Blocked, NotRun }

#[derive(Clone, Debug, serde::Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GateResult {
    pub id: TransportGateId,
    pub status: GateStatus,
    pub summary: String,
    pub measurements: std::collections::BTreeMap<String, f64>,
    pub failure: Option<SpikeError>,
}

#[derive(Clone, Debug, serde::Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransportEvidenceV1 {
    pub schema_version: u16,
    pub candidate: CandidateIdentity,
    pub source: SourceIdentity,
    pub environment: SanitizedEnvironment,
    pub browser: BrowserEvidence,
    pub fixture: FixtureEvidence,
    pub configuration: GateConfiguration,
    pub gates: Vec<GateResult>,
    pub limitations: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TransportDecision { AdoptCdpkit, AdoptChromey, OwnTransport }

#[derive(Clone, Debug, serde::Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransportDecisionV1 {
    pub schema_version: u16,
    pub decision: TransportDecision,
    pub candidate: CandidateIdentity,
    pub evidence: Vec<EvidenceDigest>,
    pub gates: Vec<GateResult>,
    pub limitations: Vec<String>,
    pub rejected_alternatives: Vec<String>,
    pub rationale: String,
}

pub fn validate_evidence(value: &TransportEvidenceV1) -> Result<(), SpikeError>;
pub fn sanitize_evidence(value: TransportEvidenceV1) -> Result<TransportEvidenceV1, SpikeError>;
pub fn decide(reports: &[TransportEvidenceV1]) -> Result<TransportDecisionV1, SpikeError>;
pub fn write_json_schema(path: &std::path::Path) -> Result<(), SpikeError>;
```

**Implementation notes:**
- Evidence records git revision, candidate crate version/checksum, protocol source revision or explicit `unavailable`, Rust version, OS/arch, Chrome product/revision/protocol, fixture SHA-256, gate configuration/results, and limitations.
- Never serialize absolute Chrome/profile/workspace paths, WebSocket URLs, ports, hostnames, usernames, environment variables, command lines, page data, or raw error debug chains. Raw runner output stays under `target/cdp-transport-gate/`.
- Validation derives completeness from `TransportGateId::ALL`, rejects duplicate/missing gates and non-finite measurements, and prevents `Pass` without gate-specific required values.

**Acceptance criteria:**
- [ ] Rust types generate the checked-in JSON Schema; no hand-copied schema registry exists.
- [ ] Strict round-trip and malformed/redaction tests pass.
- [ ] Reports are reproducible from committed fixture/code/config and contain no machine-specific secrets or paths.

### Unit 4: Exact cdpkit adapter and real-Chrome Linux harness

**Story:** `epic-rust-cdp-capture-foundation-cdp-transport-gate-cdpkit-linux-qualification`

**Files:**
- `crates/krometrail-cdp/src/spike/cdpkit_adapter.rs`
- `crates/krometrail-cdp/src/spike/chrome_harness.rs`
- `crates/krometrail-cdp/src/spike/fixture_server.rs`
- `crates/krometrail-cdp/src/bin/cdp-transport-gate.rs`
- `crates/krometrail-cdp/tests/cdpkit_transport_contract.rs`
- `tests/fixtures/browser/cdp-transport-gate/index.html`
- `tests/fixtures/browser/cdp-transport-gate/animation.js`
- `docs/evidence/cdp-transport/v2/cdpkit-linux.json`

```rust
pub struct CdpkitTransportFactory;
pub struct CdpkitTransport { /* private cdpkit CDP/session ownership */ }
impl SpikeTransportFactory for CdpkitTransportFactory { /* exact 0.4.0 */ }
impl SpikeTransport for CdpkitTransport { /* no fork or patch */ }

#[derive(Debug, clap::Parser)]
pub struct GateCli {
    #[arg(long)] pub chrome_binary: std::path::PathBuf,
    #[arg(long, value_enum)] pub platform: EvidencePlatform,
    #[arg(long)] pub output: std::path::PathBuf,
    #[arg(long, default_value_t = 60)] pub minimum_seconds: u64,
    #[arg(long, default_value_t = 1000)] pub minimum_frames: u64,
    #[arg(long, default_value_t = 120)] pub hard_stop_seconds: u64,
}

pub async fn run_real_chrome_gate(
    factory: &dyn SpikeTransportFactory,
    configuration: GateConfiguration,
    chrome_binary: &std::path::Path,
) -> Result<TransportEvidenceV1, SpikeError>;
```

**Implementation notes:**
- The runner owns only a disposable temp profile/process and loopback fixture server. It parses Chrome's browser endpoint, performs the gate, kills/reaps Chrome, removes the profile, and never enters root composition or production lifecycle modules.
- Typed gate: browser `Browser.getVersion`; page `Page.enable`, `Runtime.evaluate`, `Accessibility.enable` plus `getFullAXTree`, harmless `Input.dispatchMouseEvent`; Target discovery/auto-attach with `flatten=true` and two pages.
- Raw gate: browser- and session-scoped method strings and a named `Value` event subscription established before trigger. Unknown named event/additive field/unknown enum must not close connection or the named raw path. Evidence states there is no wildcard envelope.
- Session gate routes 100 command tokens and 100 `Runtime.consoleAPICalled` tokens for each page with zero cross-session delivery; covers event-before-response and detach-during-command.
- Fixture continuously mutates a canvas and visible counter using `requestAnimationFrame`, with unique per-page tokens and deterministic controls for event/drift cases.

**Acceptance criteria:**
- [ ] Shared fake scenarios pass through exact cdpkit 0.4.0 unchanged.
- [ ] Linux real-Chrome evidence covers every gate and is schema-valid/sanitized.
- [ ] Any fork, routing/decoder/lifecycle patch, lost required raw evidence, or hidden reconnect is a candidate failure.

### Unit 5: Sustained acknowledgement, saturation, and bounded-memory proxy

**Story:** `epic-rust-cdp-capture-foundation-cdp-transport-gate-cdpkit-linux-qualification`

**Files:** same runner, adapter, fixture, and Linux evidence files as Unit 4.

```rust
#[derive(Clone, Debug, serde::Serialize, schemars::JsonSchema)]
pub struct ScreencastMeasurements {
    pub capture_elapsed_seconds: f64,
    pub frames_received: u64,
    pub frames_acknowledged: u64,
    pub handoff_accepted: u64,
    pub handoff_dropped: u64,
    pub handoff_elapsed_seconds: f64,
    pub saturation_attempts: u64,
    pub ack_latency_ms_p50: f64,
    pub ack_latency_ms_p95: f64,
    pub ack_latency_ms_p99: f64,
    pub ack_latency_ms_max: f64,
    pub rss_samples: u64,
    pub rss_peak_bytes: u64,
    pub rss_first_window_median_bytes: u64,
    pub rss_last_window_median_bytes: u64,
    pub rss_theil_sen_bytes_per_minute: f64,
    pub upstream_queue_depth_available: bool,
}

pub async fn run_screencast_gate(
    transport: &dyn SpikeTransport,
    session: &TransportScope,
    config: &ScreencastGateConfiguration,
    rss: &dyn RssSampler,
) -> Result<ScreencastMeasurements, SpikeError>;
```

**Implementation notes:**
- Run until both the 60-second and 1,000-frame minima are met; the configured 120-second global hard stop is authoritative when slow capture has not met both minima. Each frame receive and acknowledgement remains independently phase-bounded.
- For every returned event, start the receive-to-ack-completion timer after `next_screencast_frame` returns, await typed `ScreencastFrameAck` before `try_send` to a bounded capacity-1 handoff, and hold the consumer saturated for at least 10 seconds and 100 attempts; require at least one drop and continued frame/ack progress.
- Ack proxy passes at p99 ≤ 250 ms and max ≤ 1,000 ms. RSS proxy samples once/second, excludes 10-second warmup, compares first/last 20-second medians (growth ≤ 32 MiB), and computes Theil-Sen slope (≤ 8 MiB/minute).
- `upstream_queue_depth_available` must be false for cdpkit. The report may conclude “bounded in this measured run” only from RSS/counter evidence; it cannot claim the library queue is structurally bounded.
- Forced disconnect resolves active subscriptions/pending calls within 1 second; fresh connection and two-session rebuild complete within 5 seconds with no transparent reconnect.

**Acceptance criteria:**
- [ ] `frames_acknowledged == frames_received`, ack occurs before every handoff attempt, and saturation produces explicit drops without stopping CDP.
- [ ] All thresholds, raw measurements, proxy limitations, browser/platform/config metadata, and failures are committed.
- [ ] No disk/image analysis or production bounded-ingestion implementation enters the event loop.

### Unit 6: Decisive macOS reproduction

**Story:** `epic-rust-cdp-capture-foundation-cdp-transport-gate-macos-decisive-evidence`

**Files:**
- `docs/evidence/cdp-transport/v2/cdpkit-macos.json`
- `.github/workflows/cdp-transport-gate.yml` only if a manual rerun workflow is required

**Implementation notes:**
- Run the unchanged candidate adapter, fixture digest, scenario registry, schema, and thresholds on stable Chrome/macOS.
- Evidence includes OS/arch but excludes host identity, serials, paths, endpoints, and environment values.
- If Linux cdpkit fails, create exactly one fallback story based on evidence, cycle-check it, and add it as a dependency here. Do not create both alternatives. The fallback must pass Linux before macOS runs.

**Acceptance criteria:**
- [ ] Every decisive gate has measured macOS evidence; no macOS-only waiver or implementation exists.
- [ ] The committed report reproduces from the documented command and validates against the same schema.
- [ ] A failure blocks decision rollup.

### Unit 7: Deterministic selection and rolling evidence

**Story:** `epic-rust-cdp-capture-foundation-cdp-transport-gate-transport-decision-rollup`

**Files:**
- `docs/evidence/cdp-transport/v2/decision.json`
- `docs/evidence/cdp-transport/v2/README.md`
- `docs/research/rust-cdp-transport-2026-07.md`
- `.agents/skills/rust-cdp-transport/SKILL.md`
- `.work/active/features/epic-rust-cdp-capture-foundation-cdp-transport-gate.md`
- `.work/active/epics/epic-rust-cdp-capture-foundation.md`
- `docs/ARCHITECTURE.md`

**Implementation notes:**
- `decide` validates both platform reports, SHA-256 digests, candidate/version/config consistency, complete gate registry, and redaction before producing `decision.json`.
- Adopt cdpkit only if every required fake/Linux/macOS gate passes unchanged. A fork or required patch is failure. Test chromey only for a cdpkit failure its handler plausibly addresses; choose owned transport when evolution is lost before raw preservation, ack/backpressure is obscured, sessions misroute, or a fork is required.
- Update current assertions in place: research and skill name the selected mechanism/version and limitations; feature and epic record actual evidence/conditional story; architecture names the adapter while preserving replaceability and Krometrail-owned reconnect/capture policy.
- Keep the spike available only via non-default features for reproduction. Production implementation belongs to following features.

**Acceptance criteria:**
- [ ] Evidence, decision, research, skill, feature, epic, and architecture agree exactly.
- [ ] No gate is waived and no named-params limitation is promoted to wildcard-envelope support.
- [ ] Core ports remain unchanged unless a report-cited incompatibility proves revision unavoidable; no production capture code lands.

## Decision rollup (schema v2)

- Selection: exact `cdpkit` 0.4.0, Cargo.lock checksum `c3fdb566d913b31e0014391a94c0db4ed871dbb76577dd1b2f2c5f6df158bfaa`.
- Immutable provenance: gate/source revision `07b0990c0d9e4fea9057fcab5c35e56691ff69eb`, source-attestation digest `sha256:b4147b12577e980123bfb711d314dd17f22b0639303956e97441af74a8b297b0`, configuration digest `sha256:06388b5f8ad042093d22408dedb8d02d5a04a9e59d485158edc533334bab956e`, and browser fixture digest `sha256sum-of-ordered-fixture-files:9b42ae730d12a95772a946bf55e4838a5443b6cb4c536424570219041b6e2a68:84ba666539a996012a781637c1a894d8c7a4789cfca84661bd7cf8b79efa2e13`.
- Accepted current evidence: v2 Linux `a7195eda1667e613b1b3f857fd56cc60153500544493a86afac8448706d20270`; v2 macOS `46901e41bb2a4bb674d76d9dce41fc4200032280cd9720daaaad965ee89d257b` from hosted run `29207244853`.
- Decision output: `docs/evidence/cdp-transport/v2/decision.json`, generated solely by `decide_from_files`; decision SHA-256 `91f9032315dd3501068e1dd692b12fbda7ce0d7a57c9b5a49444db73c2a5c015`.
- The generated decision preserves all 13 platform-labelled gates and the identical candidate-contract fixture digest `sha256:6dc599e64e0245b5f29eae0644dddb3a5e7222a234b7e2602a6a8577a25e677e`, trace digest `sha256:6c6be028c511d4d8c28cbecec368a7d4f09e0d87612741d02ac19a8663964d54`, 942 observations, and three drift fixtures for both platforms.
- Exact post-receive ack p99/max are Linux `0.3979589999999999/2.785427 ms` and macOS `1.062666/7.058083 ms`; receive → ack completion → bounded handoff and measured capture/handoff elapsed fields remain the contract.
- Limitations remain explicit: named event params only, unbounded subscriber with no queue-depth introspection, and RSS/ack-latency proxies. No chromey or owned-transport failure was evidenced; production adapter, reconnect policy, and bounded capture/backpressure remain downstream work.

## Conditional fallback protocol

No fallback story exists at design time. If cdpkit fails:

1. Finish and commit schema-valid cdpkit failure evidence; never convert the failing gate to `NotRun` or lower its threshold.
2. Apply the research selection rule to choose one next experiment:
   - `chromey` only for demonstrated lifecycle, target-ordering, or sustained-capture failure that its mature handler could plausibly address;
   - owned Tokio/tokio-tungstenite raw-envelope transport when protocol evolution is lost before the raw boundary, ack/backpressure is obscured, routing fails, or a fork is needed.
3. Create one implementing child story under this feature with the same `SpikeTransport`, scenarios, evidence schema, Linux thresholds, and exact candidate pin/generation provenance.
4. Run `.work/bin/work-view --blocking <new-story-id>` before adding it to `macos-decisive-evidence.depends_on`.
5. Do not implement that fallback in core or production modules. Its purpose remains qualification and evidence.

## Implementation order

1. `...-spike-contract-harness` — feature gates, exact contract/error/evidence types, generated schema, fake transport, scripted peer, and shared deterministic scenarios.
2. `...-cdpkit-linux-qualification` — exact cdpkit adapter, shared fake suite, real-Chrome fixture/runner, Linux typed/raw/routing/screencast/disconnect evidence.
3. Conditional only after a demonstrated failure: create and run one evidence-justified fallback story, then add it to the macOS story dependency after cycle checking.
4. `...-macos-decisive-evidence` — unchanged decisive qualification on stable Chrome/macOS.
5. `...-transport-decision-rollup` — validate/hash evidence, select without waivers, and roll the result through evidence, research, skill, feature, epic, and architecture.

The chain is intentionally sequential: harness integrity precedes candidate evidence; Linux determines whether fallback work exists; macOS validates the qualifying mechanism; only complete cross-platform evidence permits selection.

## Testing and fixtures

### Deterministic contract tests

`crates/krometrail-cdp/tests/transport_contract.rs` runs the shared scenario suite against `FakeTransport`. `crates/krometrail-cdp/tests/cdpkit_transport_contract.rs` runs it against exact cdpkit through `ScriptedCdpPeer`. Fixtures inject an unknown method, additive field, and unknown enum value. Barriers and oneshots establish ordering; source/tests contain no sleeps. Repeated execution must preserve the same scenario results.

### Real browser fixture

`tests/fixtures/browser/cdp-transport-gate/` is dependency-free static HTML/JS served by the spike's loopback server. It provides continuous canvas mutation, visible sequence counters, unique page tokens, console events, and deterministic trigger functions. Its SHA-256 enters every report. It contains no application credentials, external requests, random timing, or machine path.

### Real Chrome gate

Commands from a clean checkout:

```bash
export GATE_SHA=07b0990c0d9e4fea9057fcab5c35e56691ff69eb
cargo test --locked -p krometrail-cdp --features cdp-spike --test transport_contract
cargo test --locked -p krometrail-cdp --features cdp-spike-cdpkit --test cdpkit_transport_contract
cargo run --locked -p krometrail-cdp --features cdp-spike-cdpkit --bin cdp-transport-gate -- gate \
  --chrome-binary "$CHROME_BIN" --expected-git-revision "$GATE_SHA" \
  --minimum-seconds 60 --minimum-frames 1000 --saturation-seconds 10 \
  --saturation-attempts 100 --hard-stop-seconds 120 \
  --output target/cdp-transport-gate/cdpkit-linux.raw.json
cargo run --locked -p krometrail-cdp --features cdp-spike-cdpkit --bin cdp-transport-gate -- validate-and-normalize \
  --input target/cdp-transport-gate/cdpkit-linux.raw.json \
  --output target/cdp-transport-gate/cdpkit-linux.sanitized.json
cargo run --locked -p krometrail-cdp --features cdp-spike-cdpkit --bin cdp-transport-gate -- validate-decisive \
  --input target/cdp-transport-gate/cdpkit-linux.sanitized.json --platform linux \
  --expected-git-revision "$GATE_SHA"
```

The hosted macOS workflow runs the same gate from exact `workflow_dispatch` ref+SHA inputs and names its artifact with the resolved SHA. The runner also exposes equivalent `schema`, `validate-and-normalize`, and `decide` subcommands; normalization strips machine-local data before a file can enter `docs/evidence/`.

### Quality gates

```bash
cargo fmt --all --check
cargo check --workspace --all-targets --locked
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo check -p krometrail-cdp --all-targets --features cdp-spike-cdpkit --locked
cargo test -p krometrail-cdp --all-targets --features cdp-spike-cdpkit --locked
cargo clippy -p krometrail-cdp --all-targets --features cdp-spike-cdpkit --locked -- -D warnings
```

Default and spike-feature gates both pass. The default dependency graph proves spike code is not selected by the product.

## Risks

- **Riskiest assumption:** cdpkit's unbounded per-subscription channel may accumulate while downstream appears bounded. Mitigation: ack before handoff, deliberate sustained saturation, frame/ack/drop counters, RSS trend thresholds, and explicit denial of queue-depth visibility. Failure selects a fallback; it is not papered over.
- **Protocol-drift blind spot:** named `Value` subscriptions preserve params but cannot observe every envelope. Mitigation: type and evidence name the limitation, fixtures prove only the supported path, and selection fails if the foundation needs authoritative wildcard envelopes.
- **False determinism:** a fake can pass because sleeps happen to align. Mitigation: no sleeps; explicit script barriers/oneshots; event-before-response and detach ordering are controlled by the peer.
- **Cross-platform variability:** memory and ack latency vary by host. Mitigation: thresholds measure bounded trend and prompt response rather than absolute RSS, while reports identify OS/arch/browser/config and keep raw measurements.
- **Evidence leakage:** Chrome paths/endpoints/profile directories can reveal local identity. Mitigation: typed sanitized environment, strict normalizer/redaction tests, raw outputs ignored under `target/`, and committed reports contain revisions/digests rather than paths.
- **Spike leakage into production:** qualification helpers could become tempting lifecycle code. Mitigation: non-default `spike` module, no root wiring, no core types, explicit disposable naming, and final selection story forbids production capture implementation.
- **Fallback uncertainty:** cdpkit may fail in a way chromey can or cannot address. Mitigation: late-bind exactly one follow-up from demonstrated evidence; do not pre-create or parallelize speculative adapters.
- **Least certain area:** whether 1,000 Chrome screencast frames arrive inside 60 seconds on all supported machines. The gate requires both conditions and permits up to 120 seconds; failing to reach 1,000 is decisive evidence rather than a threshold change.

## Implementation summary

The original child stories reached their pre-remediation milestones, but the feature review returned the gate for remediation. The final v2 Linux and macOS reports now pass the unchanged contract from exact revision `07b0990c0d9e4fea9057fcab5c35e56691ff69eb` with identical configuration/fixture/source-attestation provenance, and `docs/evidence/cdp-transport/v2/decision.json` selects exact cdpkit 0.4.0.

The generated v1 and prior-v2 reports/decision remain historical evidence only. The current v2 decision preserves platform-labelled gate/candidate results, canonical RSS fields, observed lifecycle measurements, and identical immutable provenance. Spike features remain non-default; no production adapter, root wiring, capture pipeline, or core-port revision landed.

## Feature review (2026-07-12)

**Verdict:** Needs fixes; returned to `stage: implementing`.

GLM completeness review reproduced all committed reports and decisions and identified an unenforced reconnect deadline plus narrative drift. GPT-5.6 Sol adversarial review confirmed those findings and found selection-critical evidence-integrity defects: the scripted candidate lifecycle path used a disconnected expected-message deque; several real-Chrome gates recorded static rather than observed values; disconnect and global hard-stop claims were not fully enforced; Linux provenance had been edited after capture; Linux and macOS used materially different evidence implementations; and the decision exposed Linux-only gate measurements.

No threshold or requirement was waived. The v1 reports and decision remain historical implementation outputs. Nine evidence-integrity and portability follow-ups were ultimately required after the first review, including exact endpoint binding and runtime nondeterminism discovered during fresh qualification. All 14 child stories are now `stage: done`.

The final v2 reports were emitted from exact revision `07b0990c0d9e4fea9057fcab5c35e56691ff69eb` on Linux and hosted macOS run `29207244853`; both pass all 13 unchanged gates with identical candidate trace results and current observed measurements. The generated platform-faithful decision selects exact cdpkit 0.4.0. The manual workflow remains exact-ref+SHA only with resolved-SHA artifacts and no cdp-transport push trigger.

A fresh GLM review approved the first remediation, but the second Sol adversarial review reproduced new blockers: protocol-drift checks did not assert committed fixture params; local Linux provenance did not attest a clean source tree; architecture diagrams contradicted acknowledgement order; recursive redaction/status consistency was bypassable; candidate trace equality/results were incomplete; capture deadline/cancellation behavior was unsafe; and latency included frame wait while claiming receive-to-ack timing.

Six additional stories repaired those defects, recaptured both platforms from clean exact revision `07b0990c0d9e4fea9057fcab5c35e56691ff69eb`, preserved prior v2 evidence under `historical/`, and regenerated decision digest `91f9032315dd3501068e1dd692b12fbda7ce0d7a57c9b5a49444db73c2a5c015`.

The next fresh GLM pass found only a corrected Chrome-version typo, but final Sol review reproduced four remaining blockers: descendant Chrome process trees were not group-owned; compile-time worktree paths leaked through shared Cargo cache and a test false-skipped; candidate trace summaries were not reconstructable from committed trace material; and Rust decisive validation did not pin the canonical hard stop. It also reproduced encoded hostname/email/IPv6 redaction bypasses. Four new stories repair and recapture this evidence. The feature returns to `stage: implementing`; no prior report or threshold is waived.
