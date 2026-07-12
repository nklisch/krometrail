---
id: epic-rust-cdp-capture-foundation-rust-runtime-contracts
kind: feature
stage: review
tags: [browser, infra]
parent: epic-rust-cdp-capture-foundation
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Rust Capture Runtime Foundation

## Brief

Establish the Rust 2024 workspace, composition root, and the core browser-session and recording contracts that the CDP adapter and later storage, control, and temporal capabilities consume. The core owns domain identities, session timing, frame and capture-gap vocabulary, lifecycle states, errors, and infrastructure ports; infrastructure crates depend inward so transport findings can revise port details without reversing dependency direction.

Make Rust the only buildable Krometrail runtime in the same cutover. Remove the TypeScript/DAP implementation and align package entry points, development commands, CI, and current contributor documentation with the Rust workspace only after confirming the remote `v0.2.20` tag preserves the legacy implementation. This feature establishes contracts and runtime ownership, but does not select a production CDP transport, launch Chrome, or ingest real screencast frames.

## Epic context

- Parent epic: `epic-rust-cdp-capture-foundation`
- Position in epic: foundation feature — every other child consumes its workspace and core contracts
- Design decisions inherited: immediate single-runtime Rust cutover; the legacy implementation remains available at remote tag `v0.2.20`

## Foundation references

- `docs/VISION.md` — Local-First Operation and Product Boundaries
- `docs/SPEC.md` — Sessions and Targets, Continuous Visual Capture, and Errors and Degraded Operation
- `docs/ARCHITECTURE.md` — Rust Workspace, Domain Model, Time Model, Dependency Direction, and Technology Decisions

## Design decisions

- **Runtime cutover:** Land and verify the Rust workspace first, then delete the TypeScript/DAP product runtime in this feature. Do not retain a second executable, compatibility layer, npm product package, or TypeScript public API.
- **Legacy recovery gate:** Treat deletion as blocked until `git ls-remote --tags origin refs/tags/v0.2.20` equals `3fa4ffa16659648c6f4e229c2f7ae14d2fbc6558`; re-run the check immediately before deletion even though design-time verification passed.
- **Workspace topology:** Use one root Cargo package plus the five crates named by `docs/ARCHITECTURE.md`: `krometrail-core`, `krometrail-cdp`, `krometrail-store`, `krometrail-mcp`, and `temporal-vision`. The root package is both workspace owner and final `krometrail` binary.
- **Dependency ownership:** Put third-party versions and feature flags in root `[workspace.dependencies]`; member manifests use `{ workspace = true }`. Commit `Cargo.lock` because the workspace ships an application binary.
- **Core boundary:** `krometrail-core` owns opaque IDs, time, session/target/frame/gap/lifecycle/timeline/capability/error vocabulary, and infrastructure ports. It imports no infrastructure crate and no async runtime.
- **Async ports:** Use an object-safe boxed-`Future` alias from `std` rather than `async-trait` or Tokio types. This keeps the core runtime-agnostic and permits `Arc<dyn Port>` composition; the allocation cost is accepted at infrastructure boundaries rather than paid in domain algorithms.
- **External variability:** Clock and ID generation are injected ports. Source time remains distinct from observed monotonic time and normalized session time; no API compares unrelated clocks implicitly.
- **Contract maturity:** Stable domain invariants are fixed here, while browser-transport-facing request/response details are explicitly revisable by the next real-Chrome transport gate. This avoids freezing assumptions inferred from library documentation.
- **Capability source of truth:** Define capability identifiers, default availability, dependencies, and recording subsystems once in core. Tool membership and generated public schemas are added to that same registry when the MCP surface exists rather than duplicated early.
- **Distribution compatibility:** Rust replaces Bun for the product runtime, but GitHub release asset names remain exactly `krometrail-linux-x64`, `krometrail-linux-arm64`, `krometrail-darwin-x64`, `krometrail-darwin-arm64`, and `krometrail-windows-x64.exe`. Windows remains a best-effort artifact, not a supported environment. npm publication ends.
- **Bun after cutover:** Bun may remain only as development tooling for VitePress and preserved browser fixture applications. `package.json` becomes private and contains no `bin`, `main`, `types`, product runtime, or npm publication contract; Cargo package metadata is the version source of truth.
- **Legacy tests and fixtures:** Delete tests, benchmarks, fixtures, generated docs, and harnesses whose only contract is the DAP/TypeScript runtime. Preserve browser fixture applications that can exercise current browser-control or evaluation intent, clearly classifying them as test assets; migrate or prune them later from evidence rather than deleting potentially reusable browser behavior wholesale.
- **Legacy skills:** Delete the DAP, citty, React/Vue/Solid/Svelte runtime-observation, and TypeScript-specific project skills because they teach removed product surfaces. Do not claim replacement Rust/CDP skills before those commands exist; update the skill catalog to expose only truthful current guidance.
- **UI surface:** None. This is runtime and repository infrastructure only.
- **Dispatch:** Direct local reads only, as required by the caller. The feature is broad, but the parent epic, five foundation documents, current CI/release/install files, representative TypeScript contracts, and the supplied independent advisory provided sufficient evidence without additional exploration.

## Other agent review

- Invoked because: the immediate runtime replacement is a large, irreversible architectural cutover under autopilot.
- Reviewer (Phase 1 — advisory/completeness): GLM 5.2, supplied before this design pass.
  - Gaps and alternatives considered: root package/workspace shape; five crate skeletons; workspace dependency SSOT; complete core vocabulary; injected clock/ID sources; runtime-neutral async ports; transport-port revisability; Rust-before-teardown sequencing; remote tag verification; CI/release/install/version cutover; and whether all legacy tests, benchmarks, fixtures, and skills should be deleted.
- Phase 2 adversarial review: skipped because the caller supplied a completed Phase 1 advisory and explicitly prohibited spawning agents; concrete claims were verified locally instead.
- Accepted:
  - Root package plus five-crate workspace, centralized workspace dependencies, injected nondeterminism, runtime-neutral object-safe async ports, and structured errors.
  - Treat browser-facing port details as provisional until the real-browser gate while keeping dependency direction non-negotiable.
  - Make Rust green before teardown, reverify remote `v0.2.20`, and cut CI/release/install/version surfaces over without changing release asset names.
  - Seven stories with a safe critical path and parallel distribution/documentation finish.
- Rejected or narrowed:
  - Deleting all `tests/` and `benchmarks/` indiscriminately. DAP/runtime suites and benchmarks are dead after the locked cutover and should be removed, but browser fixture applications remain potentially useful for the browser-control and evaluation contracts in `docs/SPEC.md` and `docs/EVALUATION.md`; preserve only those assets, with explicit classification and no dependency on deleted product code.
  - Deleting every old skill without classification. Product- and TypeScript-specific skills are misleading and must go, but substrate rules remain, and replacement guidance should be added only when a real Rust/CDP command surface exists.

## Architectural choice

### Chosen: contract-first Cargo workspace, then atomic repository cutover

Create the full Cargo topology with compiling crate boundaries, implement stable core domain contracts and ports, wire the root binary, and prove the Rust workspace green before removing the former runtime. After removal, cut distribution and contributor surfaces over in parallel. This optimizes for dependency-direction correctness and recoverability: every deletion is protected by both a remote immutable tag and a working replacement build.

### Alternative: keep TypeScript and Rust buildable in parallel

A dual-runtime migration would reduce short-term disruption and permit behavior-by-behavior comparison, but it contradicts the locked single-runtime decision, creates two sources of truth for commands and release packaging, and invites compatibility shims that the foundation explicitly does not want.

### Alternative: one Rust crate until behavior stabilizes

A monolith would minimize initial manifests and cross-crate types, but it would erase the browser/domain/storage/MCP/visual-analysis boundaries already fixed in `docs/ARCHITECTURE.md`. Splitting it later would be more expensive precisely when real capture code and persistence make dependency reversal harder to unwind.

### Alternative: freeze complete transport and storage APIs now

Comprehensive ports could make later stories appear more concrete, but they would encode guesses before `cdpkit` is tested against real Chrome and before segment/SQLite work begins. The chosen design fixes domain invariants and narrow capability-shaped ports while explicitly allowing the transport gate to revise browser request/response details.

## Tricky unit first: stable domain time and loss semantics with provisional infrastructure

The highest-risk design problem is distinguishing what must be stable now from what real CDP evidence may invalidate. IDs, three-clock separation, monotonic ordering, explicit gaps, lifecycle transitions, and structured failures are product invariants and belong in core now. Exact CDP command envelopes, sequence interpretation, reconnect mechanics, and screencast acknowledgement APIs are adapter findings and must not leak into core. Ports therefore exchange domain-owned requests and observations, return runtime-neutral futures, and are labeled provisional where the next gate has authority to revise them.

## Implementation units

### Unit 1: Root Cargo package and five crate skeletons

**Story:** `epic-rust-cdp-capture-foundation-rust-runtime-contracts-workspace-skeleton`

**Files:**
- `Cargo.toml`
- `Cargo.lock`
- `rust-toolchain.toml`
- `src/main.rs`
- `crates/krometrail-core/Cargo.toml`
- `crates/krometrail-core/src/lib.rs`
- `crates/krometrail-cdp/Cargo.toml`
- `crates/krometrail-cdp/src/lib.rs`
- `crates/krometrail-store/Cargo.toml`
- `crates/krometrail-store/src/lib.rs`
- `crates/krometrail-mcp/Cargo.toml`
- `crates/krometrail-mcp/src/lib.rs`
- `crates/temporal-vision/Cargo.toml`
- `crates/temporal-vision/src/lib.rs`

```toml
[package]
name = "krometrail"
version = "0.2.20"
edition = "2024"
rust-version = "1.85"

[workspace]
resolver = "2"
members = [
  "crates/krometrail-core",
  "crates/krometrail-cdp",
  "crates/krometrail-store",
  "crates/krometrail-mcp",
  "crates/temporal-vision",
]

[workspace.package]
edition = "2024"
rust-version = "1.85"
license = "MIT"
repository = "https://github.com/nklisch/krometrail"

[workspace.dependencies]
# Exact compatible ranges/features are declared once here; Cargo.lock pins releases.
serde = { version = "1", features = ["derive"] }
thiserror = "2"
uuid = { version = "1", features = ["serde"] }
clap = { version = "4", features = ["derive"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread", "signal"] }
tracing = "0.1"
```

**Implementation notes:**
- Every member inherits edition, minimum Rust, license, repository, and dependency declarations from the root.
- `krometrail-core` must not depend on Tokio, any sibling Krometrail crate, CDP, SQLite, MCP, or image libraries.
- `temporal-vision` must not depend on any Krometrail crate. Skeleton it now, but do not invent its later visual API in this feature.
- The root binary may initially expose only Cargo-generated compile behavior; Unit 4 owns executable semantics. Do not temporarily route to Bun.

**Acceptance criteria:**
- [ ] `cargo metadata --no-deps` reports the root package and exactly five workspace member crates.
- [ ] `cargo check --workspace --all-targets` succeeds on the skeleton before any legacy deletion.
- [ ] All member manifests use workspace-owned dependency declarations and `Cargo.lock` is tracked.
- [ ] A dependency-graph check proves `krometrail-core` has no infrastructure dependency and `temporal-vision` has no Krometrail dependency.

### Unit 2: Core identity, time, session, frame, gap, lifecycle, timeline, and capability vocabulary

**Story:** `epic-rust-cdp-capture-foundation-rust-runtime-contracts-core-domain`

**Files:**
- `crates/krometrail-core/src/ids.rs`
- `crates/krometrail-core/src/time.rs`
- `crates/krometrail-core/src/browser/mod.rs`
- `crates/krometrail-core/src/browser/target.rs`
- `crates/krometrail-core/src/recording/mod.rs`
- `crates/krometrail-core/src/recording/session.rs`
- `crates/krometrail-core/src/recording/frame.rs`
- `crates/krometrail-core/src/recording/gap.rs`
- `crates/krometrail-core/src/lifecycle.rs`
- `crates/krometrail-core/src/timeline/mod.rs`
- `crates/krometrail-core/src/timeline/observation.rs`
- `crates/krometrail-core/src/capabilities/mod.rs`
- `crates/krometrail-core/src/lib.rs`

```rust
// ids.rs
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct IdValue(uuid::Uuid);

impl IdValue {
    pub const fn from_uuid(value: uuid::Uuid) -> Self;
    pub const fn as_uuid(&self) -> &uuid::Uuid;
}

macro_rules! typed_ids { /* defines the opaque UUID-backed public newtypes below */ }
typed_ids!(
    SessionId, TargetId, FrameId, InteractionId, MarkerId, SegmentId,
    ArtifactId, GapId, NavigationId
);

// time.rs — nanoseconds are integer domain values; no floating-point ordering.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct ObservedTime(u64);
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct SessionTime(u64);
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct SourceTime(i128);

impl ObservedTime {
    pub const fn from_nanos(value: u64) -> Self;
    pub const fn as_nanos(self) -> u64;
}
impl SessionTime {
    pub const ZERO: Self;
    pub const fn from_nanos(value: u64) -> Self;
    pub const fn as_nanos(self) -> u64;
}
impl SourceTime {
    pub const fn from_nanos(value: i128) -> Self;
    pub const fn as_nanos(self) -> i128;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionOrigin { observed: ObservedTime }
impl SessionOrigin {
    pub const fn new(observed: ObservedTime) -> Self;
    pub fn normalize(self, observed: ObservedTime) -> Result<SessionTime>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SessionRange { pub start: SessionTime, pub end: SessionTime }
impl SessionRange {
    pub fn new(start: SessionTime, end: SessionTime) -> Result<Self>;
    pub const fn contains(self, value: SessionTime) -> bool;
}

// recording/session.rs
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct DiskBudgetBytes(std::num::NonZeroU64);
impl DiskBudgetBytes { pub fn new(value: u64) -> Result<Self>; pub const fn get(self) -> u64; }

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CaptureStatistics {
    pub received_frames: u64,
    pub accepted_frames: u64,
    pub dropped_frames: u64,
    pub persisted_frames: u64,
    pub gap_count: u64,
}
impl CaptureStatistics { pub fn validate(self) -> Result<Self>; }

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RecordingSession {
    pub id: SessionId,
    pub origin: ObservedTime,
    pub started_at: std::time::SystemTime,
    pub ended_at: Option<std::time::SystemTime>,
    pub browser: BrowserVersion,
    pub profile: ProfileIdentity,
    pub lifecycle: SessionLifecycle,
    pub disk_budget: DiskBudgetBytes,
    pub capabilities: Vec<CapabilityId>,
    pub statistics: CaptureStatistics,
}
impl RecordingSession {
    pub fn new(
        id: SessionId,
        origin: ObservedTime,
        started_at: std::time::SystemTime,
        browser: BrowserVersion,
        profile: ProfileIdentity,
        disk_budget: DiskBudgetBytes,
        capabilities: Vec<CapabilityId>,
    ) -> Result<Self>;
    pub fn transition(&mut self, next: SessionLifecycle, ended_at: Option<std::time::SystemTime>) -> Result<()>;
}

// browser/target.rs
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BrowserVersion { pub product: String, pub revision: String, pub protocol: String }
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct ProfileIdentity(String);
impl ProfileIdentity { pub fn new(value: impl Into<String>) -> Result<Self>; pub fn as_str(&self) -> &str; }
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PageTarget { pub id: TargetId, pub browser_target_key: String, pub url: String, pub title: String }
impl PageTarget {
    pub fn new(id: TargetId, browser_target_key: impl Into<String>, url: impl Into<String>, title: impl Into<String>) -> Result<Self>;
}

// recording/frame.rs
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageFormat { Jpeg, Png }
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PixelDimensions { width: std::num::NonZeroU32, height: std::num::NonZeroU32 }
impl PixelDimensions {
    pub fn new(width: u32, height: u32) -> Result<Self>;
    pub const fn width(self) -> u32;
    pub const fn height(self) -> u32;
}
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DeviceScaleFactor(f64);
impl DeviceScaleFactor { pub fn new(value: f64) -> Result<Self>; pub const fn get(self) -> f64; }

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureWarning {
    MissingSourceTime,
    SourceTimestampRounded,
    SourceSequenceDiscontinuity,
    ViewportMetadataIncomplete,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CapturedFrame {
    pub id: FrameId,
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub source_sequence: u64,
    pub source_time: Option<SourceTime>,
    pub observed_time: ObservedTime,
    pub session_time: SessionTime,
    pub format: ImageFormat,
    pub image: PixelDimensions,
    pub viewport: PixelDimensions,
    pub device_scale_factor: DeviceScaleFactor,
    pub warnings: Vec<CaptureWarning>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct EncodedFrame { pub metadata: CapturedFrame, pub bytes: std::sync::Arc<[u8]> }
impl EncodedFrame { pub fn new(metadata: CapturedFrame, bytes: impl Into<std::sync::Arc<[u8]>>) -> Result<Self>; }

// recording/gap.rs
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureGapReason {
    IngestionQueueSaturated,
    PersistenceRejected,
    SourceSequenceDiscontinuity,
    TargetHidden,
    ScreencastPaused,
    BrowserDisconnected,
    CaptureStopped,
}
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CaptureGap {
    pub id: GapId,
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub range: SessionRange,
    pub reason: CaptureGapReason,
    pub estimated_missing_frames: Option<std::num::NonZeroU64>,
    pub detail: Option<String>,
}
impl CaptureGap {
    pub fn new(
        id: GapId,
        session_id: SessionId,
        target_id: TargetId,
        range: SessionRange,
        reason: CaptureGapReason,
        estimated_missing_frames: Option<std::num::NonZeroU64>,
        detail: Option<String>,
    ) -> Result<Self>;
}

// lifecycle.rs
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionLifecycle { Starting, Recording, Reconnecting, Stopping, Ended }
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetLifecycle { Discovered, Attached, Recording, Hidden, Closed, Failed }
impl SessionLifecycle { pub fn transition(self, next: Self) -> Result<Self>; }
impl TargetLifecycle { pub fn transition(self, next: Self) -> Result<Self>; }

// timeline/observation.rs
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationKind {
    Frame, InteractionBoundary, Navigation, TargetLifecycle, VisibilityChange,
    CaptureGap, ConsoleMessage, JavascriptException, NetworkLifecycle, Marker,
}
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum ObservationPayloadRef {
    Frame(FrameId), Interaction(InteractionId), Navigation(NavigationId),
    Gap(GapId), Marker(MarkerId), External(String),
}
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TimelineObservation {
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub session_time: SessionTime,
    pub source_time: Option<SourceTime>,
    pub observed_time: ObservedTime,
    pub kind: ObservationKind,
    pub payload: ObservationPayloadRef,
}
impl TimelineObservation {
    pub fn new(
        session_id: SessionId,
        target_id: TargetId,
        session_time: SessionTime,
        source_time: Option<SourceTime>,
        observed_time: ObservedTime,
        kind: ObservationKind,
        payload: ObservationPayloadRef,
    ) -> Result<Self>;
}

// capabilities/mod.rs — one registry, no duplicated capability lists.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CapabilityId { Control, TemporalVision, BrowserEvents, PageState, FrameworkState }
impl CapabilityId { pub const ALL: [Self; 5]; }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapabilityDefault { Enabled, Disabled, Unavailable }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecordingSubsystem { VisualCapture, BrowserEvents, PageState, FrameworkState }
pub struct CapabilityDefinition {
    pub id: CapabilityId,
    pub default: CapabilityDefault,
    pub dependencies: &'static [CapabilityId],
    pub recording_subsystems: &'static [RecordingSubsystem],
}
pub static CAPABILITY_REGISTRY: &[CapabilityDefinition];
pub fn capability(id: CapabilityId) -> &'static CapabilityDefinition;
pub fn validate_capability_selection(enabled: &[CapabilityId]) -> Result<()>;
```

**Implementation notes:**
- Typed ID inner values stay private; all formatting/parsing/serde behavior is implemented once by the macro and tested for every registered type.
- `SourceTime` is retained evidence only. It deliberately has no ordering or subtraction API against `ObservedTime`/`SessionTime`.
- `SessionOrigin::normalize` rejects observed times preceding the origin; `SessionRange`, disk budget, capture-statistics consistency, dimensions, scale factor, frame bytes, profile/target keys, and payload-kind compatibility fail fast. A session can set `ended_at` only while transitioning to `Ended`, and an ended session requires it.
- Exact interpretation of Chrome's floating timestamp and sequence field is deferred to the transport gate; the adapter must perform checked conversion to these domain values and attach a `CaptureWarning` when evidence was missing or rounded.
- Lifecycle transition tables are exhaustive and centralized; invalid transitions return `ErrorCode::InvalidLifecycleTransition`.
- `CapabilityId::ALL` and `CAPABILITY_REGISTRY` must agree in one module-level test. Page/framework state remain `Unavailable`.

**Acceptance criteria:**
- [ ] All IDs are non-interchangeable at compile time and round-trip through display, parse, and serde.
- [ ] Tests prove monotonic normalization, underflow rejection, valid/invalid ranges, non-zero budgets, internally consistent statistics, finite positive scale factors, non-empty frame payloads, and invalid lifecycle rejection.
- [ ] Session contracts retain start/end, browser/profile, budget, enabled capabilities, lifecycle, and capture statistics; frame metadata retains source, observed, and normalized times independently.
- [ ] Every known gap is explicit; no API models missing capture as an ordinary frame interval.
- [ ] Timeline constructors reject mismatched observation kinds/payload references.
- [ ] Capability defaults match `docs/SPEC.md` and dependency validation rejects unavailable or missing prerequisites.

### Unit 3: Structured errors and runtime-independent ports

**Story:** `epic-rust-cdp-capture-foundation-rust-runtime-contracts-core-ports`

**Files:**
- `crates/krometrail-core/src/error.rs`
- `crates/krometrail-core/src/ports/mod.rs`
- `crates/krometrail-core/src/ports/clock.rs`
- `crates/krometrail-core/src/ports/ids.rs`
- `crates/krometrail-core/src/ports/browser.rs`
- `crates/krometrail-core/src/ports/recording.rs`
- `crates/krometrail-core/src/ports/timeline.rs`
- `crates/krometrail-core/src/lib.rs`

```rust
// error.rs
pub type Result<T, E = KrometrailError> = std::result::Result<T, E>;

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidInput,
    InvalidLifecycleTransition,
    InvalidTime,
    NotFound,
    Unsupported,
    BrowserDisconnected,
    CaptureRejected,
    PersistenceFailed,
    BudgetExhausted,
    Internal,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetryAdvice { Never, Safe, AfterRecovery }
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ErrorContext {
    pub session_id: Option<SessionId>,
    pub target_id: Option<TargetId>,
    pub interaction_id: Option<InteractionId>,
    pub range: Option<SessionRange>,
}
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct NonEmptyText(String);
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[error("text must not be empty or whitespace-only")]
pub struct EmptyTextError;
impl NonEmptyText {
    pub fn new(value: impl Into<String>) -> std::result::Result<Self, EmptyTextError>;
    pub fn as_str(&self) -> &str;
}
impl std::fmt::Display for NonEmptyText {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result;
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error, serde::Serialize, serde::Deserialize)]
#[error("{code:?}: {message}")]
pub struct KrometrailError {
    pub code: ErrorCode,
    pub message: NonEmptyText,
    pub context: ErrorContext,
    pub retry: RetryAdvice,
    pub recovery: Option<NonEmptyText>,
}
impl KrometrailError {
    pub fn new(code: ErrorCode, message: NonEmptyText) -> Self;
    pub fn with_context(self, context: ErrorContext) -> Self;
    pub fn with_retry(self, retry: RetryAdvice) -> Self;
    pub fn with_recovery(self, recovery: NonEmptyText) -> Self;
}

// ports/mod.rs
pub type PortFuture<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

// ports/clock.rs and ids.rs
pub trait MonotonicClock: Send + Sync { fn now(&self) -> ObservedTime; }
pub trait WallClock: Send + Sync { fn now(&self) -> std::time::SystemTime; }
pub trait IdSource: Send + Sync { fn next(&self) -> IdValue; }

// ports/browser.rs — provisional until the real-browser transport gate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BrowserConnectRequest { Launch(LaunchBrowser), Attach(AttachBrowser) }
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaunchBrowser { pub profile: ProfileIdentity, pub initial_url: Option<String> }
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttachBrowser { pub endpoint: String }
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserCompatibility { pub version: BrowserVersion, pub required_domains: Vec<DomainSupport> }
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DomainSupport { pub domain: String, pub available: bool, pub detail: Option<String> }

pub trait BrowserConnector: Send + Sync {
    fn connect(&self, request: BrowserConnectRequest) -> PortFuture<'_, Result<std::sync::Arc<dyn BrowserSessionPort>>>;
}
pub trait BrowserSessionPort: Send + Sync {
    fn compatibility(&self) -> &BrowserCompatibility;
    fn page_targets(&self) -> PortFuture<'_, Result<Vec<PageTarget>>>;
    fn close(&self) -> PortFuture<'_, Result<()>>;
}

// ports/recording.rs
pub trait RecordingSink: Send + Sync {
    fn append_frame(&self, frame: EncodedFrame) -> PortFuture<'_, Result<()>>;
    fn append_gap(&self, gap: CaptureGap) -> PortFuture<'_, Result<()>>;
    fn flush(&self, session_id: SessionId) -> PortFuture<'_, Result<()>>;
}

// ports/timeline.rs
pub trait TimelineStore: Send + Sync {
    fn append(&self, observation: TimelineObservation) -> PortFuture<'_, Result<()>>;
    fn range(&self, session_id: SessionId, target_id: TargetId, range: SessionRange)
        -> PortFuture<'_, Result<Vec<TimelineObservation>>>;
}
```

**Implementation notes:**
- Core errors are source-safe, cloneable boundary values. Adapters retain underlying error chains in local logs and map them once into `KrometrailError`; serialized errors never expose credentials or arbitrary debug output.
- Non-empty message and recovery text are validated. Stable error codes, context, retry guidance, and concrete recovery satisfy the public degraded-operation contract without coupling core to MCP.
- `PortFuture` is the only asynchronous primitive in core. Port traits contain no Tokio channel, task, cancellation token, WebSocket, SQL, filesystem, or CDP type.
- Browser port shapes are intentionally the narrow minimum needed to establish ownership. The transport-gate feature may revise these signatures based on real Chrome, but it may not move the trait into `krometrail-cdp` or introduce an outward core dependency.
- Recording and timeline are separate ports so append-only payload persistence and searchable observation indexing can fail and recover independently in later storage design.

**Acceptance criteria:**
- [ ] Compile-time fake adapters implement every port without Tokio or infrastructure imports.
- [ ] Port tests execute returned futures with a test-only executor and cover success, structured failure, flush, and range behavior.
- [ ] Structured errors round-trip through serde with stable snake-case codes and retain all provided domain context.
- [ ] Empty messages/recovery actions and unsafe internal details are rejected or excluded.
- [ ] A source scan/metadata assertion proves core public APIs contain no infrastructure-specific types.

### Unit 4: Rust composition root and executable contract

**Story:** `epic-rust-cdp-capture-foundation-rust-runtime-contracts-composition-root`

**Files:**
- `src/main.rs`
- `src/app.rs`
- `src/cli.rs`
- `tests/rust-runtime-smoke.rs`

```rust
// src/cli.rs
#[derive(Debug, clap::Parser)]
#[command(name = "krometrail", version, about)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}
#[derive(Debug, clap::Subcommand)]
pub(crate) enum Command { Doctor }

// src/app.rs
pub(crate) struct RuntimeDependencies {
    pub clock: std::sync::Arc<dyn MonotonicClock>,
    pub wall_clock: std::sync::Arc<dyn WallClock>,
    pub ids: std::sync::Arc<dyn IdSource>,
    pub browser: std::sync::Arc<dyn BrowserConnector>,
    pub recording: std::sync::Arc<dyn RecordingSink>,
    pub timeline: std::sync::Arc<dyn TimelineStore>,
}
pub(crate) struct Runtime { dependencies: RuntimeDependencies }
impl Runtime {
    pub(crate) fn new(dependencies: RuntimeDependencies) -> Self;
    pub(crate) async fn run(self, command: Command) -> Result<()>;
}

// src/main.rs
fn main() -> std::process::ExitCode;
```

**Implementation notes:**
- The root package is the only module allowed to import and assemble infrastructure crates. Infrastructure crates may expose explicit placeholder constructors, but no fake adapter may be selected for a normal product operation.
- Until the transport and MCP features land, `--version` and `--help` succeed and `doctor` reports that browser transport is not yet available with a non-zero structured failure. The binary must never silently launch the legacy runtime or claim capture works.
- Tokio is owned by the binary/infrastructure layer, not core. `main` creates the runtime once and maps `KrometrailError` to a concise stderr message and stable non-zero exit.
- The dependency struct makes all nondeterminism and persistence explicit. Later features replace placeholders at this one composition point.

**Acceptance criteria:**
- [ ] `cargo run -- --version` prints the Cargo package version and exits zero.
- [ ] `cargo run -- --help` is truthful and mentions no DAP/TypeScript commands.
- [ ] An unavailable operation fails loudly rather than using fake success or legacy code.
- [ ] Root-only imports establish the intended dependency direction; core remains infrastructure-free.
- [ ] `cargo fmt --all --check`, `cargo check --workspace --all-targets`, `cargo test --workspace --all-targets`, and `cargo clippy --workspace --all-targets -- -D warnings` all pass before teardown begins.

### Unit 5: Verified legacy runtime and dead-contract removal

**Story:** `epic-rust-cdp-capture-foundation-rust-runtime-contracts-legacy-runtime-removal`

**Files/directories:**
- remove `src/**/*.ts` after the Rust pre-teardown gate
- remove TypeScript product tests under `tests/unit/`, `tests/integration/`, and `tests/e2e/`
- remove DAP-only `tests/fixtures/{bun,cpp,csharp,go,kotlin,launch-json,node,python,ruby,swift}/`, `tests/harness/`, and TypeScript helpers
- remove the old debugger `tests/agent-harness/` and `benchmarks/`
- preserve and classify reusable `tests/fixtures/browser/` applications; remove only fixtures proven to encode no current browser-control or temporal-evaluation behavior
- remove obsolete runtime config (`tsconfig.json`, `vitest.config.ts`, `biome.json`, `Dockerfile.test`) when no preserved dev asset consumes it

```text
Required destructive gate:
remote refs/tags/v0.2.20 == 3fa4ffa16659648c6f4e229c2f7ae14d2fbc6558
AND local Rust fmt/check/test/clippy gate == green
```

**Implementation notes:**
- Run the remote check immediately before the first deletion and record the command and SHA in this story's implementation notes.
- Use `git ls-files` to classify every removed test/fixture path. No generic `rm -rf tests benchmarks` is acceptable.
- Browser fixture applications are not a second runtime: they are controlled target pages. Keep only their own minimal package metadata/server assets needed to exercise dynamic DOM, framework, canvas, navigation, forms, and visual behavior from the current spec/evaluation.
- Do not copy old DAP types, errors, tests, generated docs, or adapters into Rust as compatibility baggage. Git tag `v0.2.20` is the historical source.
- Product package/CI/docs cleanup is finalized by Units 6 and 7; this unit removes executable code and dead tests first.

**Acceptance criteria:**
- [ ] Remote tag verification matches the locked SHA and is recorded before deletion.
- [ ] No TypeScript file remains in a product runtime or library path.
- [ ] No DAP adapter, debugger command, old daemon, framework-observation runtime, old product test, or DAP benchmark remains buildable.
- [ ] Preserved browser fixtures are enumerated with a current foundation use; all other legacy fixtures are removed.
- [ ] Rust fmt/check/test/clippy remain green after deletion.
- [ ] Repository search finds no package entry point or command that can execute the old runtime.

### Unit 6: CI, release, installer, version, and development-tooling cutover

**Story:** `epic-rust-cdp-capture-foundation-rust-runtime-contracts-distribution-cutover`

**Files:**
- `.github/workflows/release.yml`
- `.github/workflows/deploy-pages.yml`
- `.github/workflows/ci.yml`
- `scripts/install.sh`
- `scripts/dev-install.sh`
- `scripts/bump-version.ts` (retained as Bun development tooling, reading/writing root Cargo version)
- `package.json`
- `bun.lock` if docs/fixture tooling still requires Bun

```text
Cargo.toml [package].version
  ├─ clap --version at compile time
  ├─ release tag v<version>
  └─ scripts/bump-version.ts update target

GitHub assets (unchanged names):
  krometrail-linux-x64
  krometrail-linux-arm64
  krometrail-darwin-x64
  krometrail-darwin-arm64
  krometrail-windows-x64.exe
  checksums.txt
```

**Implementation notes:**
- Add ordinary CI for `cargo fmt --check`, `cargo check`, `cargo test`, and clippy with warnings denied. Cache Cargo artifacts without caching produced release binaries.
- Release jobs build Rust target triples on appropriate runners or a verified cross tool, rename outputs to the existing public asset names, attest each binary, generate checksums, and create the GitHub release. Do not publish npm.
- Keep `scripts/install.sh` download URLs and installed executable name stable. Update platform comments/support claims, but retain all existing asset lookup names so existing install links do not break.
- `scripts/dev-install.sh` builds with Cargo and copies `target/release/krometrail`.
- `package.json` is private and contains only VitePress/browser-fixture development tasks still used by the repository. It has no version mirror, `bin`, `main`, `types`, product dependencies, product test scripts, build scripts, or npm publish surface.
- The bump script parses and updates exactly root `[package].version`, validates semver, verifies the working tree and tests per the release convention, then commits/tags/pushes. It must fail if it finds zero or multiple root version assignments.
- Pages may continue using Bun/VitePress temporarily, but its workflow must run independently from product compilation and must not generate legacy product API docs.

**Acceptance criteria:**
- [ ] Pull-request CI exercises the complete Rust quality gate.
- [ ] A workflow/config test verifies every existing binary asset name and installer mapping.
- [ ] `scripts/install.sh` remains POSIX shell and checksum verification still covers the selected artifact.
- [ ] Cargo is the sole product version source; npm publication and TypeScript product entry fields are absent.
- [ ] The release workflow contains no Bun build or npm publish step and produces all five existing binary names.
- [ ] Docs/fixture-only Bun tooling cannot execute or import product runtime code.

### Unit 7: Contributor docs and skill/catalog alignment

**Story:** `epic-rust-cdp-capture-foundation-rust-runtime-contracts-docs-skills-alignment`

**Files/directories:**
- `README.md`
- `AGENTS.md`
- `CLAUDE.md`
- `docs/agents.md`
- remove `docs/legacy/` and old generated product docs after tag verification
- `.agents/skills/`
- `.claude/skills/`
- `plugin/skills/`
- `plugin/settings.json`
- `tap.json`

**Implementation notes:**
- Roll current docs forward: Rust commands, workspace map, supported environment, current limited executable state, release process, and the five authoritative foundation documents. Do not retain migration-history prose; Git and `v0.2.20` carry it.
- Preserve the agile-workflow substrate section/rules and any tool-neutral repository guidance. Remove instructions for citty, DAP adapters, Bun product runtime, React/Vue/Solid/Svelte observation, generated Zod contracts, and npm publication.
- Remove published Krometrail debug/chrome/MCP skills that describe commands unavailable in the new runtime. An empty or reduced catalog is more truthful than a compatibility fiction; replacement skills are authored with the capabilities that implement those commands.
- Delete `docs/legacy/` and old generated runtime docs rather than keeping stale current-tree assertions. Foundation docs already define intended Rust behavior; change them only if implementation reveals a genuine contradiction.
- Check all inbound links before deleting docs or skills and rewrite current navigation in the same change.

**Acceptance criteria:**
- [ ] A new contributor can build, test, lint, run, and release the Rust workspace using only current docs.
- [ ] Repository search outside the remote tag finds no claim that Krometrail's product runtime is Bun/TypeScript, supports DAP commands, or publishes npm.
- [ ] No installed/published skill advertises an unavailable command or old framework-state implementation.
- [ ] The five foundation documents remain authoritative and internally linked; no stale legacy/generated doc is presented as current.
- [ ] Agile-workflow rules and substrate navigation remain intact.

## Implementation order

1. `...-workspace-skeleton` — create the Rust package/workspace and five compiling crates.
2. `...-core-domain` — establish stable domain invariants on that workspace.
3. `...-core-ports` — add structured failures and runtime-neutral infrastructure seams over the domain.
4. `...-composition-root` — assemble the explicit Rust runtime boundary and pass the full Rust quality gate.
5. `...-legacy-runtime-removal` — reverify remote `v0.2.20`, classify retained browser fixtures, and remove the old runtime/dead contracts.
6. In parallel after removal:
   - `...-distribution-cutover` — replace CI/release/install/version/package surfaces.
   - `...-docs-skills-alignment` — roll contributor docs and truthful skill catalogs forward.
7. Run the complete Rust quality gate, release-workflow static checks, installer tests, link checks, and repository stale-reference scan across the integrated result.

The chain is deliberately narrow through teardown because each step is a safety gate. Only distribution and prose/catalog alignment have independent write ownership and may run in parallel.

## Testing

### Core unit tests

Use colocated `#[cfg(test)]` modules for private invariants and public integration tests under `crates/krometrail-core/tests/` for consumer behavior. Table-drive every typed ID, lifecycle transition, capability definition, gap reason, and error code from its authoritative registry/enum. Cover malformed/empty data, overflow/underflow, non-finite scale, zero dimensions, empty payloads, source/observed/session clock separation, payload-kind mismatches, missing capability dependencies, serde round trips, and display/parse stability.

### Port contract tests

Implement deterministic in-memory clock, ID, browser, recording, and timeline adapters in `krometrail-core` test support only. Exercise object-safe `Arc<dyn Port>` calls and poll `PortFuture` with a test executor. Contract tests prove success and structured failure without Tokio in core. Reuse the same adapter contract suites from real infrastructure crates as they land.

### Workspace architecture tests

Use `cargo metadata --no-deps --format-version 1` in a repository test/script to assert member count, dependency direction, workspace dependency inheritance, root binary naming, and absence of infrastructure dependencies from `krometrail-core`/`temporal-vision`. Run rustfmt, check, tests, and clippy across all targets.

### Cutover tests

Before deletion, capture green Rust gate output and the exact remote-tag SHA in the removal story. After deletion, repeat the gate and scan tracked files for TypeScript product entry points, DAP/runtime symbols, npm publication, and stale command claims. Maintain an explicit allowlist for preserved browser test assets so `.ts`/`.js` fixture files cannot be confused with a second product runtime.

### Distribution tests

Statically validate the release matrix-to-asset mapping, installer URL selection, checksum lookup, executable extension, package privacy/no-entry-points, and Cargo version ownership. Run shell syntax checks on installers. Exercise `krometrail --version`, `--help`, and explicit unavailable-operation failure from the Rust binary.

### Integration boundary

No real Chrome, CDP command, frame ingestion, SQLite, MCP, or visual algorithm integration belongs in this feature. The next transport-gate feature supplies the first real-browser contract test and may revise only the provisional browser-facing port details based on measured evidence.

## Safe cutover checklist

1. Verify the remote tag by exact ref and SHA; do not rely only on a local tag.
2. Land workspace/domain/ports/composition and pass the complete Rust quality gate.
3. Record retained browser fixture rationale path-by-path.
4. Remove legacy product/runtime tests and code without copying compatibility types into Rust.
5. Cut package, CI, release, install, version, docs, and skills to one truthful Rust runtime.
6. Repeat all gates and stale-reference scans after integration.
7. Preserve public binary asset names and installer behavior; rollback is `git revert`, while historical source recovery is remote `v0.2.20`.

## Risks

- **Riskiest assumption:** The first browser port shape may still encode an incorrect connection/session split. Mitigation: keep it narrow, label it provisional, and give the real-Chrome transport gate explicit revision authority while preserving inward dependency direction.
- **Timestamp conversion:** CDP may provide fractional or differently based source timestamps. Mitigation: core preserves checked integer source evidence but defines no implicit cross-clock arithmetic; the gate owns conversion evidence.
- **Premature contract breadth:** Capability, error, and observation variants can grow. Mitigation: one registry/enum per growing set and exhaustive tests prevent duplicated lists; only foundation-required variants land now.
- **Destructive cutover:** Removing the old runtime can strand behavior or release plumbing. Mitigation: immutable remote tag verification, Rust-before-teardown gates, path classification, unchanged asset names, and an integrated post-removal gate.
- **Fixture ambiguity:** Preserving all browser fixtures could leave stale framework-product claims; deleting all could discard useful browser-control/evaluation targets. Mitigation: preserve only target applications with a documented current use, never their old recorder/harness code.
- **Cross-platform release drift:** Rust target builds may require runner/tool changes, especially arm64. Mitigation: treat each existing asset as a tested matrix row and fail release if any expected artifact or attestation is absent.
- **Least certain area:** Exact release cross-compilation action/tool choice is implementation-time and version-sensitive. The distribution story must choose a maintained mechanism from current CI evidence without changing the locked output contract.

## Implementation summary

All seven child stories reached `stage: done`:

- `epic-rust-cdp-capture-foundation-rust-runtime-contracts-workspace-skeleton`
- `epic-rust-cdp-capture-foundation-rust-runtime-contracts-core-domain`
- `epic-rust-cdp-capture-foundation-rust-runtime-contracts-core-ports`
- `epic-rust-cdp-capture-foundation-rust-runtime-contracts-composition-root`
- `epic-rust-cdp-capture-foundation-rust-runtime-contracts-legacy-runtime-removal`
- `epic-rust-cdp-capture-foundation-rust-runtime-contracts-distribution-cutover`
- `epic-rust-cdp-capture-foundation-rust-runtime-contracts-docs-skills-alignment`

The implementation followed the designed dependency chain and then completed distribution and documentation alignment as separate serialized deliveries to avoid shared-index conflicts. The remote `v0.2.20` tag was verified before classified legacy deletion. The integrated Rust formatting, check, 29-test workspace suite, clippy, distribution contract suite, installer shell checks, and documentation build/link checks passed. Cargo is now the sole product runtime and version source; Bun remains docs/browser-fixture development tooling only.

## Other agent review

- Invoked because: completed feature review requires fresh, multi-model deep evaluation under autopilot.
- Scope: two classes in fixed order; each reviewer performed a three-round internal convergence pass.
- Reviewer (Phase 1 — completeness): GLM 5.2 xhigh.
  - Verified every original story criterion, the full Rust/distribution gates, cutover recovery, dependency direction, and foundation alignment.
  - Converged with no blocker or important findings; identified stale deleted-harness ignore rules and unlocked bump-script gates as nits.
- Reviewer (Phase 2 — adversarial): GPT-5.6 Sol high.
  - Confirmed the Phase 1 nits, then found that public fields and derived deserialization bypass validated core constructors.
  - Found stale developer-install reuse, missing release-tag/version validation, unlocked retained Bun docs tooling, untested MSRV, and incomplete exhaustive enum/transition coverage.
- Accepted:
  - Seal and validate all invariant-bearing construction and Serde boundaries.
  - Complete exhaustive lifecycle/gap/error contract coverage from authoritative variant sets.
  - Harden developer install, release tag/version matching, Bun lock reproducibility, MSRV CI, locked version-bump gates, and stale ignore rules.
- Rejected:
  - None of the surviving above-nit findings; each was verified against current code or an isolated reproduction.

## Review findings

**Verdict**: Request changes

- Blocker: invalid core aggregate state can bypass constructors through public fields and derived deserialization.
  - Follow-up: `epic-rust-cdp-capture-foundation-rust-runtime-contracts-core-invariant-boundaries`
- Important: exhaustive lifecycle, gap-reason, and error-code coverage promised by the design is incomplete.
  - Follow-up: `epic-rust-cdp-capture-foundation-rust-runtime-contracts-exhaustive-contract-coverage`
- Important: distribution/toolchain paths permit stale installs, tag/version mismatch, unlocked docs dependencies, unproved MSRV compatibility, and unlocked release-helper gates.
  - Follow-up: `epic-rust-cdp-capture-foundation-rust-runtime-contracts-distribution-integrity`

The feature returns to `stage: implementing` until these review-created stories are verified.

## Review remediation summary

All three review-created stories reached `stage: done`. Core aggregates now enforce invariants through private fields, validated APIs, and validated Serde while preserving valid wire shapes. Lifecycle transitions, gap reasons, and error codes have exhaustive single-sourced coverage. Distribution now rebuilds developer installs, validates release tags against Cargo, locks Bun documentation dependencies, tests Rust 1.85, performs narrow lock refreshes before locked release gates, and removes stale harness ignores.

The post-remediation gate passes with 38 total workspace tests (35 core plus 3 executable smoke tests), locked clippy, distribution failure-path contracts, and documentation dependency/build checks. The feature is ready for a fresh two-model implementation review.

## Second other-agent review

- Reviewer (Phase 1 — completeness): GLM 5.2 xhigh, three-round convergence.
  - Verified all eight prior findings resolved and the original ten-story scope complete; no new findings.
- Reviewer (Phase 2 — adversarial): GPT-5.6 Sol high, three-round convergence.
  - Confirmed invariant and exhaustive-test remediation, then found ambiguous release ref provenance, stale identifier documentation, restart-repeating process IDs, unsafe distribution-test output mutation, non-single-sourced typed-ID coverage, and an unbounded glibc compatibility promise.
- Accepted follow-ups:
  - `epic-rust-cdp-capture-foundation-rust-runtime-contracts-release-provenance`
  - `epic-rust-cdp-capture-foundation-rust-runtime-contracts-identifier-integrity`
  - `epic-rust-cdp-capture-foundation-rust-runtime-contracts-linux-compatibility`

## Stuck at review

This feature has now completed two implementing → review → unresolved-review cycles. The second review's blockers and important findings are captured in the three follow-up stories above. Per the autopilot review circuit breaker, the feature remains at `stage: review` and is escalated rather than being automatically bounced or approved again. Autopilot may implement and fast-review the concrete child stories, but final feature approval requires explicit operator resolution of this escalation.

## Escalated remediation status

All three second-review follow-ups reached `stage: done`. Release artifacts now bind to one exact tag SHA, distribution fixtures are hermetic, lockfile checks preserve package multiplicity, runtime identifiers use UUID v4 with single-sourced typed-ID coverage, architecture identifiers are aligned, and Linux assets use pinned musl cross-builds with architecture-matched pre-upload smoke gates. The integrated Rust and distribution checks pass with 40 tests. These fixes clear the recorded technical findings, but the circuit-breaker escalation intentionally remains unresolved until the operator authorizes another feature-level review or disposition.
