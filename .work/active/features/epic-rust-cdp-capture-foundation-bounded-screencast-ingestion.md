---
id: epic-rust-cdp-capture-foundation-bounded-screencast-ingestion
kind: feature
stage: drafting
tags: [browser]
parent: epic-rust-cdp-capture-foundation
depends_on: [epic-rust-cdp-capture-foundation-chrome-target-supervision]
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-13
---

# Bounded Screencast Ingestion

## Implementation discovery (2026-07-13)

The opt-in production test invalidated a load-bearing design premise and bounced this feature to drafting rather than weakening evidence:

- Chrome 149 does not expose a per-frame increasing number in `Page.screencastFrame.params.sessionId`; it exposes the acknowledgement token used by `Page.screencastFrameAck`, observed as constant `1` across a live stream. This is corroborated by the already-committed canonical final5 Linux trace, where all 101 sampled real screencast events have `sessionId: 1`, and by its macOS counterpart. The design review's B1 rejection incorrectly dismissed canonical real-browser evidence as scripted.
- Therefore the current `source_sequence`, `SourceSequenceDiscontinuity` gap/warning, and strict-increase acceptance claims fabricate continuity semantics Chrome does not supply. Redesign must preserve the opaque acknowledgement token only long enough to acknowledge, introduce an honestly named Krometrail-owned ordering value if needed, and rely on explicit known loss/lifecycle gaps rather than inferred browser sequence gaps.
- Initial capture also failed live because `ProbeInitialVisibility` accepts only one raw-result shape while reconnect already supports both cdpkit shapes. The initial probe must establish observed Visible/Hidden state before Ready reconciliation without waiting for a later visibility event.
- `KROMETRAIL_REAL_CHROME_TESTS=1 cargo test -p krometrail-cdp --test capture_real --locked -- --nocapture` reproduced four bounded liveness failures (zero captures), so the real-fidelity story remains unfinished.

Revise this feature's design, parent/child acceptance criteria, core metadata vocabulary, capture engine behavior/tests, rust-CDP reference skill, and real-Chrome evidence together. Preserve acknowledgement-before-handoff, boundedness, three clocks, explicit known gaps, and all already-approved lifecycle/shutdown behavior.

## Brief

Turn each supervised page target into a production live visual stream without allowing storage or later image work to stall CDP. Acknowledge each received screencast frame to completion before decoding or attempting bounded ingestion, preserve sequence and viewport metadata, and expose per-target statistics for received, acknowledged, accepted, and dropped frames.

Normalize every observation onto a monotonic session clock while preserving Chrome source time and daemon observed time as distinct evidence. Saturation, sequence loss, target visibility pauses, and other capture interruptions produce explicit, differently classified gaps rather than implied continuity. Cancellation stops acceptance, drains or reports accepted work under a bounded flush policy, and leaves downstream persistence behind a port; this feature does not implement durable segments, retention, or temporal artifacts.

## Epic context

- Parent epic: `epic-rust-cdp-capture-foundation`
- Position in epic: core capture capability — consumes supervised flat target sessions and supplies the validated live frame stream to later storage work
- Design decisions inherited: the production path uses exact `cdpkit` 0.4.0 selected by final5; spike code remains separate; Krometrail owns acknowledgement ordering, bounded handoff, gaps, reconnect reconstruction, and cancellation

## Foundation references

- `docs/SPEC.md` — Sessions and Targets, Continuous Visual Capture, and Errors and Degraded Operation
- `docs/ARCHITECTURE.md` — Time Model, Frame Ingestion, Capture Tasks, Failure Isolation, and Observability
- `docs/VISUAL-EVIDENCE.md` — Source Frames and Capture Gaps
- `docs/EVALUATION.md` — Capture-Fidelity Evaluation and Timing Integrity

## Scope

- Allocate one recording identity per production browser session and sample its fixed monotonic `SessionOrigin` before any capture subscription, `Page.startScreencast`, or first frame; expose both through the browser-session port.
- Start one screencast only for an exact supervised target attachment after the browser session is `Ready`, the target is `Attached`, its flat transport session is known, and initial visibility has been observed.
- Subscribe to `Page.screencastFrame` and `Page.screencastVisibilityChanged` before `Page.startScreencast`; request every frame (`everyNthFrame: 1`).
- On each frame, timestamp receipt, extract only `params.sessionId`—the integer frame number defined by the official `Page.screencastFrame`/`Page.screencastFrameAck` contract—await successful acknowledgement, and only then parse metadata or attempt bounded handoff.
- Keep one bounded queue, one bounded drop ledger, one worker, frame-number state, fixed-size timing histograms, and one status snapshot per target attachment. Enforce an aggregate configured memory ceiling across active streams. A slow or failed downstream target never stalls another target or the CDP event reader.
- Preserve globally unique `FrameId`, `SessionId`, `TargetId`, Chrome screencast sequence, optional Chrome source timestamp, daemon observed time, normalized session time, format, encoded-image dimensions, viewport dimensions, device scale, and capture warnings.
- Emit explicit `CaptureGap` values for queue saturation, malformed/oversized frame rejection, source-sequence discontinuity, hidden-target intervals, browser disconnection, downstream rejection, and bounded shutdown abandonment.
- Stop acceptance before cancellation, best-effort `Page.stopScreencast` only on a live matching generation, drain accepted work, flush once, detach targets, close a managed browser, and terminate its process under one aggregate shutdown deadline whose remaining budget is threaded through every step.
- Expose per-target generation, state, queue depth/capacity, receipt/ack/accept/drop/persist/gap counters, last-frame time, and bounded acknowledgement-latency/frame-cadence p50/p95/p99/max summaries through `BrowserSessionPort::capture_statuses()` and typed session events.
- Add deterministic fake-transport coverage and opt-in real-Chrome fidelity coverage against the retained browser fixture.

## Non-goals

- No SQLite, segment files, retention, disk-budget behavior, durable-store implementation, payload offsets, or crash recovery.
- No image transcoding, pixel decode, visual measurement, artifact generation, frame selection, temporal queries, or `temporal-vision` dependency. Reading JPEG/PNG headers after bounded handoff is metadata extraction, not visual analysis.
- No browser-control actions, current-state screenshots, accessibility snapshots, MCP tools, or public command/configuration surface.
- No chromey or owned-WebSocket fallback, no cdpkit fork, and no reuse of spike runtime types.
- No claim that a quiet visible page is a capture gap. Only an observed sequence break, explicit visibility signal, bounded drop/rejection, lifecycle interruption, or shutdown abandonment declares missing evidence.
- No persistence-success guarantee while the root still injects the explicit unavailable recording adapter. The capture-to-`RecordingSink` seam is production code; the durable implementation belongs to the storage epic.

## Execution policy and grounding

- **Driver:** active autopilot `--all`; no questions were asked.
- **Effective worker:** `highest`.
- **Effective review weight:** `standard`. Design-time advisory review was not dispatched because the caller explicitly prohibited subdelegation; this is non-blocking at design time. Final feature/epic review remains required by autopilot policy.
- **Dispatch rationale:** direct-read only after the local scope probe. The feature, parent, five foundation docs, current production transport/session/launcher, target reducer, core recording/time/ports, real-Chrome fixtures, final5 decision/README, and `rust-cdp-transport` skill resolved the design surface. No distinct discovery unknown justified violating the caller's no-subdelegate constraint.
- **Rolling Foundation:** code-first. The standing foundation already states receive → ack completion → bounded handoff, distinct clocks, bounded target streams, and explicit gaps. Implementation must update an assertion only if the landed behavior differs; omission alone is not drift.

## Design decisions

- **Acceptance point:** “accepted” means the acknowledged raw frame was successfully placed into that target generation's bounded in-memory queue. Acknowledgement is not acceptance or retention. `received >= acknowledged >= accepted + dropped`; persistence remains a later counter and may fail independently.
- **Ack ordering and frame-number provenance:** parse only the integer `params.sessionId` required by `Page.screencastFrameAck`, timestamp the return, complete the ack, then parse source metadata, inspect payload length, or call `try_send`. The official CDP Page domain describes that field as the screencast frame number; Krometrail preserves it as `source_sequence` and uses increasing values within one real attachment generation as continuity evidence. Scripted candidate traces are not protocol evidence—the retained scripted fixture happens to use a constant value—so they must not invalidate this contract. The opt-in production test must establish increasing values from real Chrome before sequence-discontinuity behavior is accepted. Ack latency is return-to-completion only, matching final5; no receive wait or handoff time is included.
- **Boundedness and aggregate memory:** use a Tokio bounded channel per target generation plus a bounded in-memory gap ledger. Never await channel capacity. Defaults are at most 8 active streams, 4 queued events per stream, and 8 MiB of base64 payload text per event, yielding a 256 MiB aggregate queued-payload ceiling plus fixed per-slot metadata. Hard per-field caps are 32 streams, 16 slots, and 16 MiB, but constructor validation rejects every combination whose checked product exceeds 256 MiB. The fixed ledgers and histograms are budgeted separately and remain O(active streams), and cdpkit's private upstream subscriber queue is explicitly outside this measurable Krometrail bound. Saturation records an explicit `IngestionQueueSaturated` span after acknowledgement; a full queue cannot erase its own loss evidence.
- **Gap ledger:** keep the current saturation span plus a small bounded deque of closed spans. A successful enqueue closes the current saturation span. If the ledger itself reaches its fixed bound, conservatively coalesce adjacent pending spans into a broader range while retaining the exact estimated drop count; this may overstate the uncertain interval but never implies continuity or grows without bound.
- **Target isolation:** a frame queue, worker, acknowledgement loop, sequence tracker, visibility interval, and cancellation token belong to one `(TargetId, attachment_generation)`. Shared sink calls can run concurrently; target ordering is preserved by each target's single worker.
- **Reconnect generation:** connection loss stops all old readers and opens a `BrowserDisconnected` interval per active target. Old-generation callbacks are ignored. The same exact browser target key keeps its `TargetId`; a successfully reconciled attachment gets a higher generation, resets Chrome sequence comparison, starts a new stream only after `Ready`, and closes the interruption at its first observed frame. Missing keys close their interruption at reconciliation/termination.
- **Visibility:** subscribe to `Page.screencastVisibilityChanged` before start. `visible: false` opens `TargetHidden`; `true` or an actually received frame closes it. Repeated signals coalesce. Lack of frames on an otherwise visible static page is not inferred as a gap.
- **Frame metadata:** globally allocated `FrameId` is authoritative identity. The tuple `(SessionId, TargetId, source_sequence, session_time)` remains queryable evidence, while status carries the current attachment generation. Base64 decoding and bounded JPEG/PNG header inspection occur in the target worker after handoff; no general image dependency is added. PNG parsing validates the signature and fixed IHDR dimensions. JPEG parsing walks checked marker lengths only up to a 64 KiB header-scan ceiling and accepts a declared SOF marker; it never decodes pixels. Malformed, unsupported, empty, over-limit, missing-IHDR, or missing-SOF payloads produce `FrameRejected`, not a fabricated frame.
- **Clocks:** sample the session's fixed `SessionOrigin` before capture subscriptions/start and therefore before the first frame. Call injected `MonotonicClock::now()` at each frame return to obtain `ObservedTime`, then normalize against that origin. Observed and normalized session times are nondecreasing (`next >= previous`); equal readings are valid for a coarse or deterministic monotonic clock. Chrome's optional floating-point seconds become `SourceTime` by checked rounded nanoseconds and receive `MissingSourceTime` or `SourceTimestampRounded` warnings as applicable. Source time is never compared to daemon clocks.
- **Downstream seam:** retain the existing infrastructure-free `RecordingSink`; do not invent a second persistence-like port. Workers call `append_frame`/`append_gap`; `BrowserSessionEvent::CaptureGapDeclared` and status snapshots make loss observable independently of future durable implementation.
- **Reducer/effect reconciliation:** the target reducer remains the sole lifecycle writer. Add `SupervisorInput::InitialReconciliationCompleted` so `session.rs` no longer mutates Connecting → Ready by hand. `SupervisorTargetState` stores `CaptureBinding::{Inactive, Active(context), Suspended(context), Terminal}`. After each successful reduction, one `reconcile_capture_bindings` helper compares eligibility/binding and emits exhaustive `StartCapture`, `StopCapture`, `SuspendCapture`, or `ResumeCapture` effects while atomically updating the binding. `CaptureEffectContext` carries `TargetId`, `connection_generation`, `attachment_generation`, and the exact `TransportSessionId`; stop/suspend retain the old context before target transport state is cleared. `StartCapture` is emitted only for a newly eligible Ready/Attached/non-Unknown target, `SuspendCapture` on connection loss, `ResumeCapture` for the same exact key after a higher-generation restored attachment, and `StopCapture` on detach/close/failure/shutdown. `session.rs::apply_effects` executes these effects and does not independently infer capture lifecycle from published events, preventing two competing reconciliation mechanisms.
- **Shutdown:** `session.rs` creates one absolute `ShutdownDeadline` from the configured 5-second default aggregate budget. Cancellation first closes acceptance; capture stop, `Page.stopScreencast`, queue/ledger drain, the one session `flush`, target detaches, `Browser.close`, and managed-process termination each receive only the remaining budget through `timeout_at`/equivalent. Exhaustion skips later graceful waits, performs last-resort process cleanup, emits/returns `ShutdownIncomplete`, and never resets a per-target or per-phase timeout. Drop remains a last-resort abort and cannot claim a flush.
- **Timing status:** each target owns fixed 64-bucket logarithmic histograms for receive-to-ack-completion latency and nonnegative inter-frame observed cadence. Status returns sample count plus nearest-rank p50/p95/p99 bucket upper bounds and exact observed max; no raw sample vector grows with session duration. Deterministic tests check bucket/percentile math, while real-Chrome tests report values without making a cross-platform performance claim.
- **Feature topology and visibility:** `capture` and its Tokio/base64 requirements compile only behind the default `cdpkit-transport` feature. `CaptureConfig` is public because the root crate composes it; the coordinator, dependencies, target context, observer, deadline bridge, errors, stop reasons, and stop/shutdown outcomes are `pub(crate)` and are not re-exported from `krometrail-cdp`. Internal pipeline tests therefore live under `src/capture/`, while the public real-Chrome integration test uses the production connector/port.
- **Privacy:** info logs and status/events contain Krometrail session/target IDs, generation, stable reason/state names, counters, queue measurements, and durations only. Never log frame bytes/base64, event params, Chrome session IDs, browser target keys, titles, URLs, source timestamps, executable/profile paths, or downstream source errors at info level.
- **Configuration:** keep capture configuration adapter-local until a configuration feature owns the external schema. Default to JPEG quality 80, `everyNthFrame = 1`, 8 active streams, queue capacity 4, maximum base64 payload text 8 MiB, gap-ledger capacity 64, 250 ms ack timeout, and one 5-second aggregate shutdown budget. Constructors validate non-zero values, the hard caps and checked aggregate memory product, and JPEG/PNG option compatibility.

## Capture stream state machine

`CaptureStreamState` is one stable registry-backed core enum. The adapter applies only these transitions; status changes are emitted once per actual transition.

| Current state | Input/effect result | Next state | Required evidence/action |
|---|---|---|---|
| `Starting` | start succeeds, visibility visible | `Capturing` | Exact session/generation binding is active. |
| `Starting` | start succeeds, visibility hidden | `Hidden` | Open/coalesce `TargetHidden`. |
| `Starting` | disconnect/suspend | `Suspended` | Fence the generation and open `BrowserDisconnected`. |
| `Starting` | stop/close | `Draining` | Refuse new acceptance and begin bounded drain. |
| `Starting` | ack/start/protocol failure | `Failed` | Publish target-local failure; no handoff after failed ack. |
| `Capturing` | explicit hidden | `Hidden` | Open one hidden interval. |
| `Hidden` | explicit visible or actual frame | `Capturing` | Close the hidden interval. |
| `Capturing` / `Hidden` | disconnect/suspend | `Suspended` | Fence old callbacks before transport replacement. |
| `Suspended` | reducer emits `ResumeCapture` for a higher exact generation | `Starting` | Bind the new transport session; close disconnect only on its first frame. |
| `Capturing` / `Hidden` / `Suspended` | stop/close/failure requiring teardown | `Draining` | Stop acceptance; stop/drain under aggregate deadline. |
| `Draining` | queue and ledger complete | `Stopped` | Outcome reports complete. |
| `Draining` | aggregate deadline expires | `Stopped` | Emit `CaptureStopped`; outcome reports incomplete. |
| Any nonterminal | unrecoverable stream-local error | `Failed` | Preserve truthful counters/gaps; unrelated streams continue. |
| `Stopped` / `Failed` | any late input | unchanged | Ignore by exact generation; terminal states do not restart. |

## Architectural choice

### Option 1: Ack and call the recording sink inline

The event reader would acknowledge and immediately await `RecordingSink::append_frame`. This has the fewest tasks, but storage latency would stall cdpkit's unbounded subscriber and eventually Chrome's limited screencast window. It also makes one target's downstream failure interfere with event draining. Rejected because it violates the central isolation contract.

### Option 2: One session-wide queue and writer

A single bounded queue would simplify flush and statistics. It sacrifices target isolation: a high-rate or stalled target can consume capacity and create drops for unrelated targets, and reconnect generation cleanup becomes global. Rejected even though it uses fewer channels.

### Option 3: Per-target ack reader plus bounded raw-frame worker

Each supervised attachment owns a prompt acknowledgement reader, non-blocking bounded handoff, bounded gap ledger, and one ordered worker. A small coordinator reconciles those streams with target generations and owns session flush. Chosen because it preserves CDP responsiveness and target isolation while reusing the existing transport and recording ports. It adds one task/channel pair per bounded supervised target, which is proportional to the existing reconnect target limit.

### Option 4: Bypass the transport seam with cdpkit typed screencast APIs

This mirrors the qualification spike and is concise, but leaks candidate-specific subscription behavior into production capture and makes deterministic fakes or a fallback transport harder. Rejected. Production capture uses only `CdpTransport::subscribe_named` and `send_raw`; cdpkit remains named solely in `transport/cdpkit.rs`.

## Trickiest unit first: acknowledgement, handoff, and loss accounting

The riskiest unit is the per-target reader/worker boundary. It must prove that every returned frame follows exactly one path after ack completion: bounded acceptance, explicit drop, or explicit rejection, while preserving enough timing to report loss and never waiting for storage or image work.

```rust
// crates/krometrail-cdp/src/capture/mod.rs
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaptureConfig {
    pub format: ImageFormat,
    pub jpeg_quality: Option<u8>,
    pub max_dimensions: Option<PixelDimensions>,
    pub max_active_streams: NonZeroUsize,
    pub queue_capacity: NonZeroUsize,
    pub max_base64_payload_bytes: NonZeroUsize,
    pub gap_ledger_capacity: NonZeroUsize,
    pub ack_timeout: Duration,
    pub shutdown_timeout: Duration,
}

pub(crate) struct CaptureDependencies {
    pub clock: Arc<dyn MonotonicClock>,
    pub ids: Arc<dyn IdSource>,
    pub sink: Arc<dyn RecordingSink>,
}

#[derive(Clone)]
pub(crate) struct CaptureTarget {
    pub session_id: SessionId,
    pub session_origin: SessionOrigin,
    pub target_id: TargetId,
    pub connection_generation: u64,
    pub attachment_generation: u64,
    pub transport_session: TransportSessionId,
}

pub(crate) trait CaptureObserver: Send + Sync {
    fn status_changed(&self, status: TargetCaptureStatus);
    fn gap_declared(&self, gap: CaptureGap);
}

pub(crate) struct CaptureCoordinator { /* bounded target registry and interruption ledger */ }

#[derive(Debug, thiserror::Error)]
pub(crate) enum CaptureError {
    #[error("invalid capture configuration: {0}")]
    InvalidConfig(&'static str),
    #[error("capture transport operation failed")]
    Transport(#[from] TransportError),
    #[error("invalid screencast frame: {0}")]
    InvalidFrame(&'static str),
    #[error("capture task ended")]
    TaskClosed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CaptureStopReason {
    TargetClosed,
    TargetDetached,
    TargetFailed,
    SessionStopping,
    ReconnectExhausted,
    Cancelled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CaptureStopOutcome {
    pub reason: CaptureStopReason,
    pub complete: bool,
    pub abandoned_accepted_frames: u64,
    pub emitted_gap_count: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CaptureShutdownOutcome {
    pub targets: Vec<CaptureStopOutcome>,
    pub flush_attempted: bool,
    pub flush_succeeded: bool,
    pub complete: bool,
}

impl CaptureCoordinator {
    pub(crate) fn new(
        config: CaptureConfig,
        dependencies: CaptureDependencies,
        observer: Arc<dyn CaptureObserver>,
    ) -> Result<Self, CaptureError>;

    pub(crate) async fn start_target(
        &self,
        target: CaptureTarget,
        transport: Arc<dyn CdpTransport>,
    ) -> Result<(), CaptureError>;

    pub(crate) async fn stop_target(
        &self,
        target: &CaptureTarget,
        reason: CaptureStopReason,
        deadline: tokio::time::Instant,
    ) -> CaptureStopOutcome;

    pub(crate) async fn suspend_target(
        &self,
        target: &CaptureTarget,
        at: SessionTime,
    );
    pub(crate) fn statuses(&self) -> Vec<TargetCaptureStatus>;
    pub(crate) async fn shutdown(
        &self,
        session_id: SessionId,
        deadline: tokio::time::Instant,
    ) -> CaptureShutdownOutcome;
}
```

`start_target` performs `subscribe_named(Page.screencastFrame)`, `subscribe_named(Page.screencastVisibilityChanged)`, then `Page.startScreencast`. The reader records receipt, validates only the ack token, and executes:

```rust
let ack_started = clock.now();
transport.send_raw(
    &CommandScope::Session(target.transport_session.clone()),
    "Page.screencastFrameAck",
    json!({ "sessionId": source_sequence }),
).await?;
let ack_completed = clock.now();

// Parsing data/metadata and bounded handoff happen only after ack completion.
let raw = RawScreencastFrame::parse_after_ack(params, observed, session_time)?;
match sender.try_send(raw) {
    Ok(()) => statistics.accept(),
    Err(TrySendError::Full(raw)) => drops.saturated(raw.session_time),
    Err(TrySendError::Closed(raw)) => drops.stopped(raw.session_time),
}
```

The worker drains pending gap spans before later frames, base64-decodes accepted payloads, reads only JPEG/PNG dimensions, builds `EncodedFrame`, and awaits the sink. It never transcodes or decodes pixels. Any sink failure emits `PersistenceRejected`, marks only that stream failed, and leaves other readers/workers live.

## Implementation units and exact files

### Unit 1: Core capture status and adapter-neutral engine

**Story:** `epic-rust-cdp-capture-foundation-bounded-screencast-ingestion-engine`

**Files:**
- `Cargo.toml`
- `Cargo.lock`
- `crates/krometrail-core/src/recording/frame.rs`
- `crates/krometrail-core/src/recording/gap.rs`
- `crates/krometrail-core/src/recording/session.rs`
- `crates/krometrail-core/src/recording/mod.rs`
- `crates/krometrail-core/src/lib.rs`
- `crates/krometrail-cdp/Cargo.toml`
- `crates/krometrail-cdp/src/lib.rs`
- `crates/krometrail-cdp/src/capture/mod.rs` (new)
- `crates/krometrail-cdp/src/capture/pipeline.rs` (new)
- `crates/krometrail-cdp/src/capture/image_header.rs` (new)
- `crates/krometrail-cdp/src/capture/tests.rs` (new, crate-internal tests)

Core contract additions:

```rust
define_stable_enum! {
    pub enum CaptureStreamState {
        Starting => "starting",
        Capturing => "capturing",
        Hidden => "hidden",
        Suspended => "suspended",
        Draining => "draining",
        Stopped => "stopped",
        Failed => "failed",
    }
}

pub struct CaptureStatistics {
    received_frames: u64,
    acknowledged_frames: u64,
    accepted_frames: u64,
    dropped_frames: u64,
    persisted_frames: u64,
    gap_count: u64,
}

pub struct CaptureTimingSummary {
    sample_count: u64,
    p50_nanos: Option<u64>,
    p95_nanos: Option<u64>,
    p99_nanos: Option<u64>,
    max_nanos: Option<u64>,
}

pub struct TargetCaptureStatus {
    target_id: TargetId,
    attachment_generation: u64,
    state: CaptureStreamState,
    statistics: CaptureStatistics,
    queue_capacity: usize,
    queue_depth: usize,
    last_frame_session_time: Option<SessionTime>,
    ack_latency: CaptureTimingSummary,
    frame_cadence: CaptureTimingSummary,
}
```

`CaptureStatistics::new/update` enforce `acknowledged <= received`, `accepted + dropped <= acknowledged`, and `persisted <= accepted` with checked arithmetic. `TargetCaptureStatus::new` rejects zero capacity, depth above capacity, or frame time without a received frame. Extend the single `CaptureGapReason` registry with `FrameRejected`; retain all current distinct reasons.

Add only the workspace `base64` dependency. Gate `pub mod capture` and all capture-only Tokio/base64 dependency activation behind the default `cdpkit-transport` feature so `cargo check -p krometrail-cdp --no-default-features --all-targets` remains compile-real. Implement the bounded JPEG SOF/PNG IHDR parser locally; do not add `image`, a pixel buffer, an image worker pool, or a temporal-vision dependency.

**Acceptance criteria:**

- [ ] Deterministic fake transport proves subscription → start → receive → ack completion → parse/`try_send`; under a full queue and forever-blocked sink, ack completion and ack-histogram recording remain structurally before and independent of payload parsing, queue occupancy, gap-ledger work, and sink progress.
- [ ] Every returned frame that completes ack increments exactly one accepted/dropped path; ack failure hands nothing off and marks only that stream failed.
- [ ] Saturation produces a bounded, explicit `IngestionQueueSaturated` gap with exact estimated count even when the frame channel and sink are blocked; no unbounded side queue exists.
- [ ] The protocol-defined integer frame number is preserved as `source_sequence`; sequence comparison resets at attachment generation boundaries and produces `SourceSequenceDiscontinuity` only within one generation. Scripted fixtures do not claim to prove real Chrome numbering behavior.
- [ ] `SessionOrigin` is sampled before subscriptions/start/first frame. Source, observed, and session clocks remain distinct; observed/session times permit equality and are nondecreasing (`>=`); wall-clock changes are irrelevant; malformed/missing source timestamps cannot reorder frames.
- [ ] Base64 and bounded JPEG SOF/PNG IHDR dimensions are processed only by the worker after acceptance. Empty, malformed, unsupported, over-limit, missing-IHDR, or no-SOF-within-64-KiB payloads produce `FrameRejected`; no pixels are decoded or transcoded and no general image dependency exists.
- [ ] Visibility false/true and first-frame recovery open/close one `TargetHidden` interval without treating visible quiet time as loss.
- [ ] The documented `CaptureStreamState` transition table is exhaustive: invalid and terminal-state transitions are rejected/ignored deterministically, and status events occur once per real transition.
- [ ] Fixed-size ack-latency and frame-cadence histograms produce deterministic sample-count/p50/p95/p99/max summaries and remain constant-memory for arbitrarily long synthetic streams.
- [ ] Config validation enforces per-field caps and a checked aggregate queued-payload ceiling of 256 MiB; boundary tests cover overflow, maximum valid combinations, and over-budget combinations.
- [ ] Only `CaptureConfig` is exported for root composition. Engine/coordinator/dependency/target/observer/error/stop/outcome types remain crate-private and are tested from `src/capture/tests.rs`.
- [ ] Status invariants, stable state/gap names, bounded queue depth, ack timing, and privacy-safe logs have focused tests.
- [ ] Workspace format/check/test/clippy and `cargo check -p krometrail-cdp --no-default-features --all-targets --locked` pass independently before supervised-session wiring.

### Unit 2: Supervised lifecycle, generation, and root composition

**Story:** `epic-rust-cdp-capture-foundation-bounded-screencast-ingestion-supervised-wiring`

**Depends on:** `epic-rust-cdp-capture-foundation-bounded-screencast-ingestion-engine`

**Files:**
- `crates/krometrail-core/src/browser/session.rs`
- `crates/krometrail-core/src/ports/browser.rs`
- `crates/krometrail-core/src/ports/mod.rs`
- `crates/krometrail-cdp/src/targets/model.rs`
- `crates/krometrail-cdp/src/targets/reducer.rs`
- `crates/krometrail-cdp/src/targets/mod.rs`
- `crates/krometrail-cdp/src/session.rs`
- `crates/krometrail-cdp/tests/session_capture.rs` (new)
- `src/app.rs`

Port and event changes:

```rust
pub enum BrowserSessionEvent {
    // existing variants remain
    CaptureStateChanged { status: TargetCaptureStatus },
    CaptureGapDeclared { gap: CaptureGap },
}

pub trait BrowserSessionPort: Send + Sync {
    fn session_id(&self) -> SessionId;
    fn session_origin(&self) -> SessionOrigin;
    // existing compatibility/ownership/profile/state/targets/subscribe/stop
    fn capture_statuses(&self) -> PortFuture<'_, Result<Vec<TargetCaptureStatus>>>;
}

impl ProductionBrowserConnector {
    pub fn with_capture(
        self,
        clock: Arc<dyn MonotonicClock>,
        ids: Arc<dyn IdSource>,
        sink: Arc<dyn RecordingSink>,
        config: CaptureConfig,
    ) -> Self;
}
```

The connector allocates `SessionId` once per successful connection and samples `SessionOrigin` before any capture subscription/start can occur. Supervision-only construction remains available for deterministic launcher/transport tests; the root production composition always calls `with_capture` using the same clock, IDs, and recording sink already held in `RuntimeDependencies`.

Capture reconciliation is reducer-owned rather than a second observer loop. Add `InitialReconciliationCompleted` to replace the direct Ready mutation in `session.rs`, add `CaptureBinding`/`CaptureEffectContext` in `targets/model.rs`, and call one `reconcile_capture_bindings` helper at the end of each successful reduction. The helper emits `StartCapture`, `StopCapture`, `SuspendCapture`, or `ResumeCapture`, each carrying exact target ID, connection generation, attachment generation, and transport session, while updating the binding idempotently. `session.rs::apply_effects` exhaustively executes those variants alongside existing effects. It samples session time only while executing an effect; it does not independently scan/poll published target state to infer capture work. Initial Ready, dynamic attach/probe, reconnect, disconnect, close/detach, and shutdown all flow through that one mechanism. Stop/cancellation drains capture before transport detach/browser close; reconnect does not session-flush.

**Acceptance criteria:**

- [ ] `InitialReconciliationCompleted` replaces the direct Ready mutation; reducer tests exhaustively cover the one `reconcile_capture_bindings` helper and `StartCapture`/`StopCapture`/`SuspendCapture`/`ResumeCapture`. Every effect carries the exact `TargetId`, connection generation, attachment generation, and `TransportSessionId`; `targets/reducer.rs` extends its exhaustive `BrowserSessionEvent` logging match for both capture variants; `session.rs` has an exhaustive effect match with no event-driven/polling reconciliation path.
- [ ] No `Page.startScreencast` occurs while the session is Connecting/Reconnecting or the target is Discovered/Suspended/Unknown; it occurs once after a matching Attached/Ready generation.
- [ ] Two targets start independent scoped subscriptions and commands; saturating/failing one preserves the other's frames, status, and ordering.
- [ ] Disconnect cancels old acceptance, opens `BrowserDisconnected`, rejects late old-generation callbacks, rebuilds the same exact-key target with a higher generation, resets source sequence, and closes the gap on the first new-generation frame.
- [ ] Target closure, visibility intervals, ack failure, persistence rejection, reconnect exhaustion, explicit stop, and dropped `ProductionSession` each have deterministic bounded outcomes without duplicate terminal events.
- [ ] Explicit stop creates one absolute aggregate deadline, first prevents new acceptance, then threads remaining budget through capture stop, all queue/ledger drains, one `RecordingSink::flush(session_id)`, target detaches, `Browser.close`, and process termination. No target or phase resets the timeout; exhaustion emits `CaptureStopped`, returns/records `ShutdownIncomplete`, and falls through to last-resort cleanup rather than hanging or claiming success.
- [ ] `capture_statuses()` is deterministically sorted by `TargetId`; `CaptureStateChanged` is transition-driven rather than per-frame spam; `CaptureGapDeclared` carries no browser key, URL/title, raw params, or payload.
- [ ] Root shares one injected monotonic clock/ID source/sink with the connector, retains the explicit unavailable storage adapter, and does not add a capture command or fake-success store.
- [ ] Existing target/reconnect/doctor tests remain green, core remains runtime/transport-free, and workspace format/check/test/clippy pass independently.

### Unit 3: Real-Chrome fidelity and shutdown evidence

**Story:** `epic-rust-cdp-capture-foundation-bounded-screencast-ingestion-real-chrome-fidelity`

**Depends on:** `epic-rust-cdp-capture-foundation-bounded-screencast-ingestion-supervised-wiring`

**Files:**
- `crates/krometrail-cdp/tests/capture_real.rs` (new)

Use the existing `tests/fixtures/browser/cdp-transport-gate`, Chrome helpers, opt-in `KROMETRAIL_REAL_CHROME_TESTS=1` gate, unique profile-root guard, and exact production cdpkit transport. Implement test-only in-memory/blocking `RecordingSink`, monotonic clock, and ID source in this file; do not add product fixture or support-file ownership.

**Acceptance criteria:**

- [ ] A managed real Chrome target proves the sampled `SessionOrigin` precedes frame subscriptions/start/first receipt, reaches Ready before `Page.startScreencast`, and yields at least 30 non-empty JPEG frames under a bounded timeout. Sink observations have unique `FrameId`, one session/target identity, nondecreasing (`>=`) session/observed times, Chrome source time when supplied, and strictly increasing protocol frame numbers preserved as `source_sequence` within the generation. This real-browser evidence—not the constant scripted candidate trace—grounds sequence-discontinuity handling. Header dimensions, viewport metadata, and positive scale are coherent.
- [ ] A two-page run proves session-scoped frame delivery does not cross `TargetId`, source sequence state, queue status, or gap ownership.
- [ ] Capacity-one with a deliberately blocked sink keeps all received frames promptly acknowledged, bounds accepted depth, reports non-zero drops plus `IngestionQueueSaturated`, then drains or reports unfinished accepted work on stop.
- [ ] One proxy-sever/reconnect cycle rejects the old stream, records `BrowserDisconnected`, restores the same `TargetId` with a higher generation, and captures new frames without comparing source sequence across generations.
- [ ] Managed stop leaves no browser/profile reference; attached stop leaves external Chrome alive. All waits use explicit timeouts and no sleeps for correctness conditions.
- [ ] The existing final5 gate remains unchanged and independently buildable; this test verifies production wiring rather than copying or weakening its thresholds.
- [ ] Opt-in real-Chrome test, workspace format/check/test/clippy, no-default production check, and cdpkit spike regression pass.

## Design review ledger

Updated recipient adjudication of the GLM review:

| Finding | Disposition | Design response |
|---|---|---|
| B1 — reject frame-number/sequence contract | **Rejected (unsupported)** | Official CDP defines `Page.screencastFrame.params.sessionId` / Ack `sessionId` as integer “Frame number.” GLM inspected a scripted candidate trace whose fixture value is constant, not real Chrome. Keep `source_sequence` as the protocol frame number, record that provenance explicitly, and require real-Chrome strictly increasing evidence within a generation. No foundation assertion became false, so no drift item is created. |
| B2 — origin/order semantics | **Accepted** | Sample `SessionOrigin` before subscribe/start/first frame; define monotonic ordering as nondecreasing (`>=`), not strictly increasing. |
| C1 — exhaustive reducer event match | **Accepted** | Wiring story exclusively owns `targets/reducer.rs` with the core event additions, keeping every story compile-real. |
| C2 — no-default topology | **Accepted** | Gate the full capture module and capture-only dependencies behind default `cdpkit-transport`; verify `krometrail-cdp --no-default-features --all-targets`. |
| C3 — shutdown deadline | **Accepted** | One absolute deadline covers capture stop/drain/flush, detaches, `Browser.close`, and process termination using remaining budget only. |
| C4 — lifecycle reconciliation | **Accepted** | Reducer-owned capture binding emits explicit Start/Stop/Suspend/Resume effects with exact transport session and both generations; `session.rs` is the sole effect executor. |
| M1 — ack independence | **Accepted** | Saturation tests prove ack completion and histogram recording structurally precede queue/ledger/sink work, without a timing-threshold claim. |
| M2 — image dependency | **Accepted** | Remove `image`; use checked, bounded PNG IHDR and JPEG SOF parsing after handoff. |
| M3 — timing status | **Accepted** | Add fixed-size ack/cadence histograms and sample-count/p50/p95/p99/max status summaries. |
| M4 — stream transitions | **Accepted** | Add the exhaustive `CaptureStreamState` transition table above and test it. |
| M5 — memory defaults/caps | **Accepted** | Defaults yield a 256 MiB queued-payload ceiling; checked hard caps reject larger aggregate combinations. |
| M6 — private types | **Accepted** | Export only root-required `CaptureConfig`; keep coordinator/error/stop/outcome and related engine types crate-private with internal tests. |

The later cross-platform capture-smoke feature remains responsible for Linux/macOS/high-DPI timing-fidelity qualification. This feature records honest real-browser distributions and proves structural ordering/boundedness only; it does not promote local p50/p95/p99 values into cross-platform guarantees.

## Feature acceptance

- [ ] The three serial stories remain at `stage: implementing`, have disjoint file ownership, and each leaves its declared default/no-default build boundary compiling before the next begins.
- [ ] Official frame-number provenance is preserved, scripted evidence is labeled non-authoritative for numbering, and opt-in real Chrome supplies strictly increasing within-generation evidence before discontinuity claims pass.
- [ ] Ack completion is structurally independent of saturated handoff; capture queues, ledgers, histograms, parser work, and aggregate queued payload remain bounded.
- [ ] Session origin/order, state transitions, reducer effects, generation fencing, private/public visibility, and aggregate shutdown deadline match the contracts above.
- [ ] Status exposes truthful counters plus bounded ack/cadence p50/p95/p99/max summaries; gaps remain explicit for every known post-ack loss/rejection/abandonment path.
- [ ] No `image` dependency, foundation drift item, user-visible command, fake-success store, or cross-platform timing claim is introduced.
- [ ] Default workspace fmt/check/test/clippy, `krometrail-cdp --no-default-features --all-targets`, cdpkit spike regression, and opt-in real-Chrome contract pass in their owning stories.

## Implementation order

1. `...-engine` — land domain status/gap invariants and the transport-neutral per-target pipeline with deterministic tests.
2. `...-supervised-wiring` — integrate only the proven engine with session readiness, target generations, reconnect, stop, events, and root dependencies.
3. `...-real-chrome-fidelity` — validate production behavior against Chrome and the final5-selected adapter without changing production files.

The chain is intentionally serial. Story 1 owns capture/core engine files and gates the entire capture module behind `cdpkit-transport`; it adds no `BrowserSessionEvent` variants, so existing reducer matches continue compiling. Story 2 owns session/port/composition plus `targets/model.rs`, `targets/reducer.rs`, and `targets/mod.rs`, adding capture event variants and the exhaustive reducer/effect handling in the same compile-real stride. Story 3 owns only its new real-browser test. No stories write the same file, and every story includes its own compile-real verification.

## Simplification and elimination pass

- Reuse `CdpTransport`, `RecordingSink`, `MonotonicClock`, `IdSource`, `SessionOrigin`, `CapturedFrame`, `CaptureGap`, and `CaptureStatistics`; do not create cdpkit-specific capture handles, a second sink, a generic event bus, or a storage façade.
- Keep spike capture code isolated and unchanged. Production tests may copy the proven ordering expectation, not import spike contracts or scenarios.
- Use one coordinator and one compact per-target pipeline instead of separate ack, decode, gap, metrics, and persistence services.
- Derive all stable gap/state names from their enum registries and all public statistics from one status snapshot; do not maintain duplicate routing/display lists.
- Do not add a no-op/discarding production sink to make capture look successful before storage exists. The current explicit unavailable adapter remains truthful.
- Retain existing frame warnings that add provenance value. Do not duplicate sequence loss as an unrelated warning-only path: the gap is authoritative, and the following frame may carry `SourceSequenceDiscontinuity` only as local context.
- No existing useful tests are removed. New coverage targets ordering, boundedness, clocks, generation, isolation, and real Chrome; constructors/getters receive only compact invariant tests rather than one test per line.

## Testing

### Stable interface tests

- Core serde/registry/invariant tests protect capture state, statistics, and gap reason contracts consumed later by store/MCP adapters.
- Browser-session fake tests protect session identity/origin, sorted status snapshots, gap/status events, and object-safe port behavior.

### Complex-unit and regression tests

- A scripted transport with an ack completion barrier proves no payload parse, queue attempt, or sink call occurs before ack completion.
- A blocked sink and tiny queue prove bounded memory, explicit capacity loss, target isolation, and exact counter invariants.
- Deterministic clocks prove source/observed/session separation and ack measurement; no wall-clock API appears in capture.
- Reconnect scripts prove generation fencing, exact target identity, disconnect gap closure, and sequence reset.
- Cancellation scripts prove acceptance stops first and one absolute deadline's remaining budget bounds capture stop/drain/flush, target detach, browser close, and process termination without per-phase resets.

### Real-browser tests

- Opt-in Chrome coverage protects assumptions the fake transport cannot: official frame numbers actually increase within a generation, event metadata is coherent, image dimensions/device scale are valid, visibility events and cdpkit session filtering hold, acknowledgement continues under saturation, reconnect rebuilds, and process/profile cleanup completes.
- The test records bounded ack/cadence histogram summaries as diagnostic evidence but has only generous liveness bounds. The next cross-platform feature owns Linux/macOS/high-DPI timing-fidelity CI qualification; this feature does not duplicate or pre-claim that downstream matrix.

### Tests deliberately not added

- No tests for image pixels, transcoding, disk offsets, segment durability, retention, or visual artifacts because those behaviors are out of scope.
- No timing threshold copied from final5 into unit tests. Deterministic tests assert ordering and bounded deadlines; real-browser tests use generous liveness bounds and preserve final5 as the performance qualification evidence.
- No test for every getter, tracing line, or simple constructor branch beyond public invariants.

## Risks and pre-mortem

- **Hidden cdpkit subscriber is still unbounded.** The Krometrail queue cannot bound memory already buffered inside cdpkit. Mitigation: the event reader performs only receipt timestamp, ack, lightweight post-ack parse, `try_send`, and bounded ledger update; ack has a short timeout; real Chrome repeats sustained saturation. Fallback: a demonstrated accumulation or non-cancellable ack failure invalidates cdpkit under the existing selection rules and reopens the owned transport, rather than adding a second buffering layer.
- **Gap reporting can itself be saturated.** A second ordinary channel would reproduce the same loss problem. Mitigation: bounded in-memory coalescing ledger independent of frame capacity, transition-driven observer events, and shutdown drain. Conservative coalescing may broaden uncertainty but cannot imply continuity.
- **Chrome metadata may not equal encoded dimensions on high-DPI/max-dimension paths.** Mitigation: inspect the accepted encoded header in the worker, preserve viewport metadata separately, derive scale only from validated positive values, and verify real Chrome. Fallback: mark `ViewportMetadataIncomplete` and reject incoherent dimensions instead of fabricating scale.
- **Ack succeeds but downstream parse fails.** This is expected protocol semantics: Chrome flow control is released, then Krometrail emits `FrameRejected`; the frame is not accepted/persisted and continuity is explicitly uncertain.
- **Reconnect races old callbacks with new target state.** Mitigation: key every stream/callback by exact `TargetId` plus attachment generation, cancel old readers before replacing connection resources, and ignore late generation updates. Source sequence comparison never crosses generations.
- **Flush deadline cannot guarantee a stuck sink records the terminal gap.** The system can guarantee bounded shutdown and in-memory/event status, not a successful write through a failed port. Mitigation: emit the gap/status before awaiting sink persistence, return `ShutdownIncomplete`, and never claim durable flush. Durable recovery belongs to the store feature.
- **Visibility event semantics differ by Chrome version.** Mitigation: subscribe before start, treat explicit false/true as evidence, close hidden state on an actual frame, and never infer a visible gap from silence. Real Chrome tests protect current supported behavior.
- **Where least certain:** production image/device-scale metadata across macOS high-DPI. The local real-Chrome test validates internal coherence; the dependent cross-platform smoke remains the authority for Linux/macOS high-DPI fidelity and can drive a narrow metadata correction without changing the queue/lifecycle design.
