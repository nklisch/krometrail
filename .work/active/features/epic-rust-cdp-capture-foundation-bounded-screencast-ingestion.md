---
id: epic-rust-cdp-capture-foundation-bounded-screencast-ingestion
kind: feature
stage: implementing
tags: [browser]
parent: epic-rust-cdp-capture-foundation
depends_on: [epic-rust-cdp-capture-foundation-chrome-target-supervision]
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Bounded Screencast Ingestion

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

- Allocate one recording identity and monotonic `SessionOrigin` per production browser session and expose them through the browser-session port.
- Start one screencast only for an exact supervised target attachment after the browser session is `Ready`, the target is `Attached`, its flat transport session is known, and initial visibility has been observed.
- Subscribe to `Page.screencastFrame` and `Page.screencastVisibilityChanged` before `Page.startScreencast`; request every frame (`everyNthFrame: 1`).
- On each frame, timestamp receipt, extract only the acknowledgement token, await successful `Page.screencastFrameAck`, and only then parse metadata or attempt bounded handoff.
- Keep one bounded queue, one drop ledger, one worker, sequence state, and status snapshot per target attachment. A slow or failed downstream target never stalls another target or the CDP event reader.
- Preserve globally unique `FrameId`, `SessionId`, `TargetId`, Chrome screencast sequence, optional Chrome source timestamp, daemon observed time, normalized session time, format, encoded-image dimensions, viewport dimensions, device scale, and capture warnings.
- Emit explicit `CaptureGap` values for queue saturation, malformed/oversized frame rejection, source-sequence discontinuity, hidden-target intervals, browser disconnection, downstream rejection, and bounded shutdown abandonment.
- Stop acceptance before cancellation, best-effort `Page.stopScreencast` only on a live matching generation, drain accepted work under one deadline, and report any abandoned accepted range before the session-level `RecordingSink::flush` attempt.
- Expose per-target generation, state, queue depth/capacity, receipt/ack/accept/drop/persist/gap counters, last-frame time, and acknowledgement timing through `BrowserSessionPort::capture_statuses()` and typed session events.
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
- **Ack ordering:** parse only the integer `sessionId` required by `Page.screencastFrameAck`, timestamp the return, complete the ack, then parse source metadata, inspect payload length, or call `try_send`. Ack latency is return-to-completion only, matching final5; no receive wait or handoff time is included.
- **Boundedness:** use a Tokio bounded channel per target generation plus a bounded in-memory gap ledger. Never await channel capacity. Saturation records an explicit `IngestionQueueSaturated` span after acknowledgement; a full queue cannot erase its own loss evidence.
- **Gap ledger:** keep the current saturation span plus a small bounded deque of closed spans. A successful enqueue closes the current saturation span. If the ledger itself reaches its fixed bound, conservatively coalesce adjacent pending spans into a broader range while retaining the exact estimated drop count; this may overstate the uncertain interval but never implies continuity or grows without bound.
- **Target isolation:** a frame queue, worker, acknowledgement loop, sequence tracker, visibility interval, and cancellation token belong to one `(TargetId, attachment_generation)`. Shared sink calls can run concurrently; target ordering is preserved by each target's single worker.
- **Reconnect generation:** connection loss stops all old readers and opens a `BrowserDisconnected` interval per active target. Old-generation callbacks are ignored. The same exact browser target key keeps its `TargetId`; a successfully reconciled attachment gets a higher generation, resets Chrome sequence comparison, starts a new stream only after `Ready`, and closes the interruption at its first observed frame. Missing keys close their interruption at reconciliation/termination.
- **Visibility:** subscribe to `Page.screencastVisibilityChanged` before start. `visible: false` opens `TargetHidden`; `true` or an actually received frame closes it. Repeated signals coalesce. Lack of frames on an otherwise visible static page is not inferred as a gap.
- **Frame metadata:** globally allocated `FrameId` is authoritative identity. The tuple `(SessionId, TargetId, source_sequence, session_time)` remains queryable evidence, while status carries the current attachment generation. Base64 decoding and JPEG/PNG header inspection occur in the target worker after handoff; malformed, unsupported, empty, or over-limit payloads produce `FrameRejected`, not a fabricated frame.
- **Clocks:** call injected `MonotonicClock::now()` at frame return to obtain `ObservedTime`, then normalize with that session's fixed `SessionOrigin`. Chrome's optional floating-point seconds become `SourceTime` by checked rounded nanoseconds and receive `MissingSourceTime` or `SourceTimestampRounded` warnings as applicable. Source time is never compared to daemon clocks.
- **Downstream seam:** retain the existing infrastructure-free `RecordingSink`; do not invent a second persistence-like port. Workers call `append_frame`/`append_gap`; `BrowserSessionEvent::CaptureGapDeclared` and status snapshots make loss observable independently of future durable implementation.
- **Shutdown:** cancellation first closes acceptance, then sends a bounded best-effort `Page.stopScreencast`, closes queues, drains workers and ledgers within the remaining shared deadline, emits `CaptureStopped` for accepted-but-unfinished work, attempts one session `flush`, and only then lets browser detach/close proceed. Drop remains a last-resort abort that updates status but cannot claim a flush.
- **Privacy:** info logs and status/events contain Krometrail session/target IDs, generation, stable reason/state names, counters, queue measurements, and durations only. Never log frame bytes/base64, event params, Chrome session IDs, browser target keys, titles, URLs, source timestamps, executable/profile paths, or downstream source errors at info level.
- **Configuration:** keep capture configuration adapter-local until a configuration feature owns the external schema. Default to JPEG quality 80, `everyNthFrame = 1`, queue capacity 8, maximum encoded payload 16 MiB, 250 ms ack timeout, and 2 s shutdown/flush budget. Constructors validate all non-zero bounds and JPEG/PNG option compatibility.

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
    pub queue_capacity: NonZeroUsize,
    pub max_encoded_payload_bytes: NonZeroUsize,
    pub ack_timeout: Duration,
    pub shutdown_timeout: Duration,
}

pub struct CaptureDependencies {
    pub clock: Arc<dyn MonotonicClock>,
    pub ids: Arc<dyn IdSource>,
    pub sink: Arc<dyn RecordingSink>,
}

#[derive(Clone)]
pub struct CaptureTarget {
    pub session_id: SessionId,
    pub session_origin: SessionOrigin,
    pub target_id: TargetId,
    pub attachment_generation: u64,
    pub scope: CommandScope,
}

pub trait CaptureObserver: Send + Sync {
    fn status_changed(&self, status: TargetCaptureStatus);
    fn gap_declared(&self, gap: CaptureGap);
}

pub struct CaptureCoordinator { /* bounded target registry and interruption ledger */ }

impl CaptureCoordinator {
    pub fn new(
        config: CaptureConfig,
        dependencies: CaptureDependencies,
        observer: Arc<dyn CaptureObserver>,
    ) -> Result<Self, CaptureError>;

    pub async fn start_target(
        &self,
        target: CaptureTarget,
        transport: Arc<dyn CdpTransport>,
    ) -> Result<(), CaptureError>;

    pub async fn stop_target(
        &self,
        target_id: TargetId,
        reason: CaptureStopReason,
    ) -> CaptureStopOutcome;

    pub async fn suspend_for_disconnect(&self, at: SessionTime);
    pub fn statuses(&self) -> Vec<TargetCaptureStatus>;
    pub async fn shutdown(&self, session_id: SessionId) -> CaptureShutdownOutcome;
}
```

`start_target` performs `subscribe_named(Page.screencastFrame)`, `subscribe_named(Page.screencastVisibilityChanged)`, then `Page.startScreencast`. The reader records receipt, validates only the ack token, and executes:

```rust
let ack_started = clock.now();
transport.send_raw(
    &target.scope,
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
- `crates/krometrail-cdp/tests/capture_pipeline.rs` (new)

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

pub struct TargetCaptureStatus {
    target_id: TargetId,
    attachment_generation: u64,
    state: CaptureStreamState,
    statistics: CaptureStatistics,
    queue_capacity: usize,
    queue_depth: usize,
    last_frame_session_time: Option<SessionTime>,
    last_ack_latency_nanos: Option<u64>,
    max_ack_latency_nanos: Option<u64>,
}
```

`CaptureStatistics::new/update` enforce `acknowledged <= received`, `accepted + dropped <= acknowledged`, and `persisted <= accepted` with checked arithmetic. `TargetCaptureStatus::new` rejects zero capacity, depth above capacity, or frame time without a received frame. Extend the single `CaptureGapReason` registry with `FrameRejected`; retain all current distinct reasons.

Add workspace `base64` and `image` dependencies, with `image` default features disabled and only JPEG/PNG header support enabled. These run only after bounded handoff. Do not add a pixel buffer, image worker pool, or temporal-vision dependency.

**Acceptance criteria:**

- [ ] Deterministic fake transport proves subscription → start → receive → ack completion → parse/`try_send`; a sink that blocks forever cannot delay ack or grow the target queue beyond capacity.
- [ ] Every returned frame that completes ack increments exactly one accepted/dropped path; ack failure hands nothing off and marks only that stream failed.
- [ ] Saturation produces a bounded, explicit `IngestionQueueSaturated` gap with exact estimated count even when the frame channel and sink are blocked; no unbounded side queue exists.
- [ ] Source sequence discontinuities reset at attachment generation boundaries and produce `SourceSequenceDiscontinuity` only within one generation.
- [ ] Source, observed, and session clocks remain distinct; wall-clock changes are irrelevant; malformed/missing source timestamps cannot reorder frames.
- [ ] Base64 and JPEG/PNG dimensions are processed only by the worker after acceptance. Empty, malformed, unsupported, or over-limit payloads produce `FrameRejected`; no pixels are decoded or transcoded.
- [ ] Visibility false/true and first-frame recovery open/close one `TargetHidden` interval without treating visible quiet time as loss.
- [ ] Status invariants, stable state/gap names, bounded queue depth, ack timing, and privacy-safe logs have focused tests.
- [ ] Workspace format/check/test/clippy and `--no-default-features` checks pass independently before supervised-session wiring.

### Unit 2: Supervised lifecycle, generation, and root composition

**Story:** `epic-rust-cdp-capture-foundation-bounded-screencast-ingestion-supervised-wiring`

**Depends on:** `epic-rust-cdp-capture-foundation-bounded-screencast-ingestion-engine`

**Files:**
- `crates/krometrail-core/src/browser/session.rs`
- `crates/krometrail-core/src/ports/browser.rs`
- `crates/krometrail-core/src/ports/mod.rs`
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

The connector allocates `SessionId` and `SessionOrigin` once per successful connection. Supervision-only construction remains available for deterministic launcher/transport tests; the root production composition always calls `with_capture` using the same clock, IDs, and recording sink already held in `RuntimeDependencies`.

After initial reconciliation sets the session `Ready`, and after each successful reconnect transaction commits `Ready`, reconcile capture against exact target keys, transport sessions, and attachment generations. Dynamic target attach/probe completion performs the same reconciliation. Connection loss calls `suspend_for_disconnect` before old connection resources are dropped. Target close/detach cancels only its matching stream. Stop/cancellation drains capture before transport detach/browser close; reconnect does not session-flush.

**Acceptance criteria:**

- [ ] No `Page.startScreencast` occurs while the session is Connecting/Reconnecting or the target is Discovered/Suspended/Unknown; it occurs once after a matching Attached/Ready generation.
- [ ] Two targets start independent scoped subscriptions and commands; saturating/failing one preserves the other's frames, status, and ordering.
- [ ] Disconnect cancels old acceptance, opens `BrowserDisconnected`, rejects late old-generation callbacks, rebuilds the same exact-key target with a higher generation, resets source sequence, and closes the gap on the first new-generation frame.
- [ ] Target closure, visibility intervals, ack failure, persistence rejection, reconnect exhaustion, explicit stop, and dropped `ProductionSession` each have deterministic bounded outcomes without duplicate terminal events.
- [ ] Explicit stop first prevents new acceptance, then drains accepted queues/ledgers and calls `RecordingSink::flush(session_id)` once within the shared deadline; timeout emits `CaptureStopped` covering accepted unfinished work and returns/records incomplete shutdown rather than claiming success.
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

- [ ] A managed real Chrome target reaches Ready before the first `Page.startScreencast`, yields at least 30 non-empty JPEG frames under a bounded timeout, and each sink call observes unique `FrameId`, one session/target identity, increasing session/observed times, Chrome source time when supplied, increasing source sequence within generation, correct JPEG header dimensions, viewport metadata, and positive scale.
- [ ] A two-page run proves session-scoped frame delivery does not cross `TargetId`, source sequence state, queue status, or gap ownership.
- [ ] Capacity-one with a deliberately blocked sink keeps all received frames promptly acknowledged, bounds accepted depth, reports non-zero drops plus `IngestionQueueSaturated`, then drains or reports unfinished accepted work on stop.
- [ ] One proxy-sever/reconnect cycle rejects the old stream, records `BrowserDisconnected`, restores the same `TargetId` with a higher generation, and captures new frames without comparing source sequence across generations.
- [ ] Managed stop leaves no browser/profile reference; attached stop leaves external Chrome alive. All waits use explicit timeouts and no sleeps for correctness conditions.
- [ ] The existing final5 gate remains unchanged and independently buildable; this test verifies production wiring rather than copying or weakening its thresholds.
- [ ] Opt-in real-Chrome test, workspace format/check/test/clippy, no-default production check, and cdpkit spike regression pass.

## Implementation order

1. `...-engine` — land domain status/gap invariants and the transport-neutral per-target pipeline with deterministic tests.
2. `...-supervised-wiring` — integrate only the proven engine with session readiness, target generations, reconnect, stop, events, and root dependencies.
3. `...-real-chrome-fidelity` — validate production behavior against Chrome and the final5-selected adapter without changing production files.

The chain is intentionally serial. Story 1 owns capture/core engine files; story 2 owns session/port/composition files; story 3 owns only its new real-browser test. No stories can write the same file concurrently, and every story includes its own compile-real verification so the workspace remains green at each boundary.

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
- Cancellation scripts prove acceptance stops first and that one shared deadline bounds drain plus flush.

### Real-browser tests

- Opt-in Chrome coverage protects assumptions the fake transport cannot: actual event metadata, image dimensions/device scale, visibility events, cdpkit session filtering, frame cadence under saturation, reconnect rebuilding, and process/profile cleanup.
- The next cross-platform feature owns Linux/macOS/high-DPI CI qualification. This feature runs the production real-Chrome contract locally/opt-in without duplicating that downstream matrix.

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
