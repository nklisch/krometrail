---
id: epic-agent-surface-simplification-persistence-recovery
kind: feature
stage: done
tags: [browser, storage, diagnostics]
parent: epic-agent-surface-simplification
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Recoverable segment publication and actionable capture failures

## Brief

Repair the reproducible capture failure at the 120-second segment-rotation boundary. A completed sealed-file rename followed by directory-sync failure currently becomes a permanently latched writer error, poisoning every later browser session in the MCP process. Classify that publication failure as recoverable when writer state is known, retain terminal latching for ambiguous partial writes, and prove the next append can proceed safely.

Carry the first privacy-safe persistence operation/category through capture status, diagnostics, and structured shutdown recovery. Replace the bare degraded stop outcome with enough closure quality, failed phase, capture cause, and recovery guidance for an agent to distinguish restart-required terminal state from successfully closed degraded evidence.

## Epic context

- Parent epic: `epic-agent-surface-simplification`
- Position in epic: independent recording reliability and diagnostic feature

## Simplification opportunity

Use the store's already-safe operation plus `ErrorKind` classification as one inward failure contract. Delete generic error discards and bare degraded enums rather than adding a second diagnostic side channel.

## Foundation references

- `docs/SPEC.md` — Continuous Visual Capture and Failure Semantics
- `docs/ARCHITECTURE.md` — Segment Format, Crash Recovery, Diagnostics and Observability

## Design decisions

- **Recovery boundary**: Only a failed directory sync after the sealed-file rename is recoverable in place — the writer has already removed the open segment from memory and the filesystem namespace has one sealed path, so the next append can create a new segment without reusing ambiguous offsets. Every write, flush, file sync, rename, and initial-publication failure remains terminal.
- **Failure authority**: `KrometrailError` carries one optional typed `PersistenceFailure` containing the store operation, bounded `std::io::ErrorKind` category, and writer recoverability. Capture and shutdown retain that value rather than building a second diagnostics side channel or parsing error text.
- **Capture lifecycle**: A persistence rejection still terminally fails the affected capture stream and declares a gap; `WriterUsable` means a later session may record through the same MCP process, not that replaying the rejected frame is safe.
- **Shutdown result**: Replace `BrowserStopOutcome`'s scalar variants with a structured result containing closure, quality, first failed phase, first capture failure, and recovery. A successfully released browser/profile is reported as closed even when evidence closure was degraded; `shutdown_incomplete` remains an error only when managed authority remains.
- **Diagnostic privacy**: Operation and category are closed enums. No frame bytes, paths, page data, raw source error, or unbounded OS message crosses the store boundary or enters logs.
- **Dispatch rationale**: Direct-read design across the bounded segment-writer, capture-pipeline, shutdown, and MCP projection seams; child stories are dependency checkpoints for distinct storage, capture, and lifecycle acceptance evidence.

## Architectural choice

### Chosen: typed persistence failure carried through the existing error path

Classify persistence failures where the store still owns the filesystem operation and `std::io::Error`. Add the bounded classification to `KrometrailError`, let `SegmentWriter::execute` latch only failures classified `WriterTerminal`, and preserve the same first error inside capture status. Shutdown then combines its own first failed phase with the capture failure already owned by the coordinator. This keeps one inward error path, makes the recoverability decision at the only boundary with enough state to make it safely, and removes the scalar stop result.

### Rejected: special-case the directory-sync message in capture

The capture adapter could inspect the current message and decide that `sync the sealed-segment publication` is retryable. This is small but makes storage state a stringly-typed outward concern, loses `ErrorKind`, and cannot safely decide whether the rename completed. It also leaves the writer terminal latch unchanged.

### Rejected: reopen or reconstruct the writer after every persistence error

Automatic recovery could scan segment files and recreate worker state after any failure. That optimizes availability at the cost of guessing after partial footer writes, file syncs, or renames. Startup recovery remains the correct boundary for ambiguous storage state; only the proven post-rename state recovers in process.

## Implementation Units

### Unit 1: Bounded persistence failure contract

**Files**: `crates/krometrail-core/src/error.rs`, `crates/krometrail-core/src/lib.rs`
**Story**: `epic-agent-surface-simplification-persistence-recovery-classify-writer-publication-failures`

```rust
define_stable_enum! {
    pub enum PersistenceOperation {
        SegmentDirectoryPreparation => "segment_directory_preparation",
        OpenSegmentCreation => "open_segment_creation",
        OpenSegmentPublicationSync => "open_segment_publication_sync",
        FrameRecordAppend => "frame_record_append",
        FrameRecordFlush => "frame_record_flush",
        SealedSegmentFooterWrite => "sealed_segment_footer_write",
        SealedSegmentFileSync => "sealed_segment_file_sync",
        SealedSegmentPublication => "sealed_segment_publication",
        SealedSegmentPublicationSync => "sealed_segment_publication_sync",
        SegmentWriterWorker => "segment_writer_worker",
        FrameIndex => "frame_index",
        GapIndex => "gap_index",
        SessionFlush => "session_flush",
    }
}

define_stable_enum! {
    pub enum PersistenceFailureCategory {
        NotFound => "not_found",
        PermissionDenied => "permission_denied",
        AlreadyExists => "already_exists",
        Interrupted => "interrupted",
        ResourceBusy => "resource_busy",
        StorageFull => "storage_full",
        ReadOnlyFilesystem => "read_only_filesystem",
        InvalidData => "invalid_data",
        Unavailable => "unavailable",
        Other => "other",
    }
}

define_stable_enum! {
    pub enum PersistenceRecoverability {
        WriterUsable => "writer_usable",
        WriterTerminal => "writer_terminal",
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct PersistenceFailure {
    operation: PersistenceOperation,
    category: PersistenceFailureCategory,
    recoverability: PersistenceRecoverability,
}

impl PersistenceFailure {
    pub const fn new(
        operation: PersistenceOperation,
        category: PersistenceFailureCategory,
        recoverability: PersistenceRecoverability,
    ) -> Self;
    pub const fn operation(&self) -> PersistenceOperation;
    pub const fn category(&self) -> PersistenceFailureCategory;
    pub const fn recoverability(&self) -> PersistenceRecoverability;
}

pub struct KrometrailError {
    pub code: ErrorCode,
    pub message: NonEmptyText,
    pub context: ErrorContext,
    pub retry: RetryAdvice,
    pub recovery: Option<NonEmptyText>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persistence: Option<PersistenceFailure>,
}

impl KrometrailError {
    pub fn with_persistence(mut self, failure: PersistenceFailure) -> Self;
}
```

**Implementation notes**:

- `PersistenceFailureCategory` is the public bounded projection of `std::io::ErrorKind`; do not serialize `ErrorKind`, raw messages, or paths. Match the listed stable categories and collapse everything else to `Other`.
- Non-I/O writer failures use an explicit category (`Unavailable` for worker/channel loss, `InvalidData` for encode or checked-arithmetic invariants) rather than pretending to have an OS category.
- Replace the current source-compatibility defaults in `KrometrailError` construction and serialization tests directly. This epic explicitly permits the current schema to change without aliases or legacy decoding.

**Acceptance criteria**:

- [ ] A serialized persistence error exposes only stable code, message, operation, category, recoverability, retry, recovery, and bounded identity context.
- [ ] Deserialization rejects invalid enum values and does not accept a superseded unclassified persistence shape.
- [ ] Error debug/serialization tests prove paths and raw OS messages cannot enter the public failure.

---

### Unit 2: Recoverable post-rename publication failure

**Files**: `crates/krometrail-store/src/segments/writer.rs`, `crates/krometrail-store/tests/segment_writer_smoke.rs`
**Story**: `epic-agent-surface-simplification-persistence-recovery-classify-writer-publication-failures`

```rust
impl WorkerState {
    fn execute<T>(
        &mut self,
        operation: impl FnOnce(&mut Self) -> krometrail_core::Result<T>,
    ) -> krometrail_core::Result<T>;
}

fn io_error(
    operation: PersistenceOperation,
    error: std::io::Error,
    recoverability: PersistenceRecoverability,
) -> KrometrailError;

fn store_error(
    operation: PersistenceOperation,
    category: PersistenceFailureCategory,
    recoverability: PersistenceRecoverability,
    message: &'static str,
) -> KrometrailError;
```

**Implementation notes**:

- `WorkerState::execute` reads `error.persistence.recoverability`. Cache the first error only for `WriterTerminal`; return `WriterUsable` errors to that command without poisoning later commands. A missing persistence classification at this worker boundary is an invariant violation and latches terminally.
- In `seal_segment`, all footer write/flush/file-sync and rename errors are `WriterTerminal`. Only the directory sync performed after `fs::rename(open, sealed)` returns `WriterUsable` because the removed in-memory entry and published sealed path form a known state.
- Keep the failed append failed: rotation occurs before the incoming frame is appended, so retrying inside the worker would risk duplicating or reordering a caller-owned frame. The next caller/session opens a distinct segment.
- Replace free-form action strings in `io_error` with enum operations. Keep messages plain and generated from operation/category; never append `error` display text.

**Acceptance criteria**:

- [ ] Injected failure on the post-rename directory sync returns `SealedSegmentPublicationSync`, the mapped category, and `WriterUsable`.
- [ ] A subsequent append for the same or a new session succeeds, writes a new open segment, and can be flushed and read without duplicate frame identifiers or offsets.
- [ ] The previously renamed sealed segment remains readable and is not reopened or renamed again.
- [ ] Injected footer write, file sync, rename, and initial open-publication sync failures remain `WriterTerminal`; a later command returns the exact first classified error and performs no filesystem mutation.

---

### Unit 3: First capture cause survives status and logs

**Files**: `crates/krometrail-core/src/recording/session.rs`, `crates/krometrail-core/src/recording/mod.rs`, `crates/krometrail-core/src/lib.rs`, `crates/krometrail-cdp/src/capture/pipeline.rs`, `crates/krometrail-cdp/src/capture/mod.rs`, `crates/krometrail-cdp/src/capture/tests.rs`
**Story**: `epic-agent-surface-simplification-persistence-recovery-propagate-capture-failure-cause`

```rust
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CaptureFailure {
    stage: CaptureFailureStage,
    cause: KrometrailError,
}

impl CaptureFailure {
    pub fn new(stage: CaptureFailureStage, cause: KrometrailError) -> Result<Self>;
    pub const fn stage(&self) -> CaptureFailureStage;
    pub const fn cause(&self) -> &KrometrailError;
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
    every_nth_frame: EveryNthFrame,
    #[serde(skip_serializing_if = "Option::is_none")]
    failure: Option<CaptureFailure>,
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
        failure: Option<CaptureFailure>,
    ) -> Result<Self>;
    pub const fn failure(&self) -> Option<&CaptureFailure>;
}

impl StreamRuntime {
    fn fail(&self, failure: CaptureFailure);
    fn fail_at(&self, stage: CaptureFailureStage);
}
```

**Implementation notes**:

- Replace `failure_stage: Option<CaptureFailureStage>` with `failure: Option<CaptureFailure>`; delete `new_with_failure_stage` and derive the stage from `failure` everywhere.
- `worker_loop` passes the exact first `append_frame` or `append_gap` error into `CaptureFailure`. Do not discard `Err(_)`. Non-persistence failures use a bounded stage-specific `CaptureFailed` cause created by `fail_at`.
- First failure wins in `RuntimeState`. Later gap-flush or shutdown failures cannot overwrite the operation/category that stopped capture.
- Emit `capture.pipeline.failed` fields `failure_stage`, `cause_code`, and, when present, `persistence_operation`, `persistence_category`, and `persistence_recoverability`. The structured fields are enums; never log `?error`, message text, paths, or frame/page content.
- A failed frame still produces one `PersistenceRejected` gap. If persisting that gap also fails, retain the frame-persistence cause as first failure while shutdown reports incomplete evidence flush separately.

**Acceptance criteria**:

- [ ] A classified store rejection appears byte-for-byte in `TargetCaptureStatus.failure.cause`, including operation, category, and writer recoverability.
- [ ] Capture status remains failed and current-state control remains usable.
- [ ] Later failures do not replace the first capture cause.
- [ ] Privacy tests prove capture logs and serialized status contain no source error, path, raw frame input, or page content.

---

### Unit 4: Structured closure and recovery result

**Files**: `crates/krometrail-core/src/browser/session.rs`, `crates/krometrail-core/src/browser/mod.rs`, `crates/krometrail-core/src/lib.rs`, `crates/krometrail-core/src/ports/browser.rs`, `crates/krometrail-cdp/src/session/shutdown.rs`, `crates/krometrail-cdp/src/session/mod.rs`, `crates/krometrail-cdp/src/session/runtime.rs`, `crates/krometrail-cdp/src/session/reconnect.rs`
**Story**: `epic-agent-surface-simplification-persistence-recovery-report-structured-shutdown-recovery`

```rust
define_stable_enum! {
    pub enum BrowserClosure {
        ManagedBrowserClosed => "managed_browser_closed",
        Detached => "detached",
    }
}

define_stable_enum! {
    pub enum ShutdownQuality {
        Clean => "clean",
        Degraded => "degraded",
    }
}

define_stable_enum! {
    pub enum ShutdownFailurePhase {
        CaptureStopDrainFlush => "capture_stop_drain_flush",
        BrowserEventDrainFlush => "browser_event_drain_flush",
        TargetDetach => "target_detach",
        BrowserClose => "browser_close",
        ProcessTerminate => "process_terminate",
        DeadlineComplete => "deadline_complete",
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct BrowserStopOutcome {
    closure: BrowserClosure,
    quality: ShutdownQuality,
    #[serde(skip_serializing_if = "Option::is_none")]
    failed_phase: Option<ShutdownFailurePhase>,
    #[serde(skip_serializing_if = "Option::is_none")]
    capture_failure: Option<CaptureFailure>,
    #[serde(skip_serializing_if = "Option::is_none")]
    recovery: Option<NonEmptyText>,
}

impl BrowserStopOutcome {
    pub fn new(
        closure: BrowserClosure,
        quality: ShutdownQuality,
        failed_phase: Option<ShutdownFailurePhase>,
        capture_failure: Option<CaptureFailure>,
        recovery: Option<NonEmptyText>,
    ) -> Result<Self>;
}
```

**Implementation notes**:

- Move the current private shutdown quality and string phases into the core typed result. `ShutdownReport` retains `remaining` only for deciding whether to return `shutdown_incomplete`; when no authority remains, map it to `BrowserStopOutcome` with the typed phase and first capture failure.
- Capture shutdown returns its first capture failure alongside `flush_attempted`, `flush_succeeded`, and `complete`. This is a read of coordinator state, not a second failure recorder.
- Recovery is derived once: `WriterUsable` → start a new browser session before relying on temporal history; `WriterTerminal` → restart the Krometrail MCP process, then start a new session; non-persistence capture failure → inspect status/diagnostics and start a new session. A degraded close caused only by detach/event cleanup reports the exact failed phase and phase-appropriate recovery.
- Delete `ManagedBrowserClosedDegraded`, `BrowserStopOutcome::ALL/as_str`, and every scalar equality assertion. The current contract is the structured object.

**Acceptance criteria**:

- [ ] Clean managed and attached stops return structured `clean` outcomes.
- [ ] A released managed browser with capture degradation returns `closure: managed_browser_closed`, `quality: degraded`, the exact failed phase, first capture cause, and concrete recovery.
- [ ] `WriterUsable` and `WriterTerminal` produce different recovery actions; neither says a degraded-but-closed browser process remains.
- [ ] Remaining managed process/profile authority still returns `shutdown_incomplete` rather than a successful degraded stop.

---

### Unit 5: Agent-facing response and regression coverage

**Files**: `crates/krometrail-mcp/src/response.rs`, `crates/krometrail-mcp/src/server.rs`, `crates/krometrail-mcp/src/session.rs`, `crates/krometrail-mcp/tests/schemas.rs`, `docs/SPEC.md`, `docs/ARCHITECTURE.md`
**Story**: `epic-agent-surface-simplification-persistence-recovery-report-structured-shutdown-recovery`

```rust
fn capture_failed_warning(status: &TargetCaptureStatus) -> KrometrailError;

pub(crate) fn map_lifecycle_result<T: Serialize>(
    tool: &str,
    value: T,
) -> Result<MappedResult, ResponseInvariantError>;
```

**Implementation notes**:

- Build the `capture_failed` warning from `CaptureFailure`, copying its persistence classification into the warning and selecting recovery from recoverability. Do not reduce it back to a stage-only sentence.
- Concise browser status includes the complete bounded `failure` object for failed streams. Healthy concise status remains small.
- `stop_browser` remains a succeeded tool result when browser/profile authority was released, but its structured result communicates degraded evidence closure. MCP diagnostics attach automatically because the lifecycle mapper must mark a degraded `BrowserStopOutcome` as a degraded response rather than treating every serializable lifecycle value as success.
- Roll foundation assertions forward in place: post-rename directory-sync failure is recoverable for later sessions; ambiguous persistence failures require MCP restart; stop reports closure, phase, capture cause, and recovery.

**Acceptance criteria**:

- [ ] Concise failed status and degraded stop responses include persistence operation/category/recoverability and concrete recovery without requiring a log read.
- [ ] Automatic diagnostics still add correlation ID and log path, while the actionable cause lives in the structured result/warning.
- [ ] MCP schemas contain the new structured failure and stop result and contain no `managed_browser_closed_degraded` scalar variant.
- [ ] A regression test drives injected post-rename sync failure through store → capture → status → stop, then records and flushes a frame in a fresh session through the same writer.

## Implementation order

1. `epic-agent-surface-simplification-persistence-recovery-classify-writer-publication-failures` — establish the typed store contract and prove safe writer reuse.
2. `epic-agent-surface-simplification-persistence-recovery-propagate-capture-failure-cause` — preserve that contract as the first capture failure.
3. `epic-agent-surface-simplification-persistence-recovery-report-structured-shutdown-recovery` — expose closure quality, cause, and recovery through lifecycle/MCP surfaces and roll docs forward.

## Simplification

- Delete the unconditional writer terminal latch; retain one direct branch on typed recoverability.
- Delete free-form store operation strings and raw `ErrorKind` formatting from public messages.
- Delete `TargetCaptureStatus::new_with_failure_stage`, the standalone optional stage field, and stage-only response reconstruction.
- Delete `ManagedBrowserClosedDegraded`, private duplicate shutdown-quality vocabulary, stringly failed phases, and scalar stop-result tests.
- Do not add a diagnostics registry, store-error cache, recovery daemon, retry loop, or historical response decoder.

## Testing

- Store regression tests inject failures at the filesystem phase boundary because only that layer can prove whether rename completed and writer state is reusable.
- Capture interface tests use a sink that returns a classified persistence error and protect first-cause retention, failure/gap accounting, and privacy-safe logging.
- Shutdown tests protect the distinction between closed degraded evidence and incomplete authority cleanup, plus writer-usable versus restart-required guidance.
- One MCP seam test protects the full user-visible reproduction and same-process next-session recovery. Existing stage-only serialization, scalar stop enum, and duplicate message-substring tests are removed.

## Advisory review

- **Effective review weight**: `standard` (autopilot caller default).
- **Design-time pass**: Fresh-context dispatch was attempted but unavailable because the shared agent concurrency limit was already occupied. Per the non-blocking design-review policy, implementation proceeds with direct evidence; the required standard single-pass feature review remains part of normal feature closure.

## Risks

- **Riskiest assumption**: A failed directory `fsync` after successful rename leaves a safe in-process namespace on supported macOS/Linux hosts. The design does not claim power-loss durability for that publication; it only claims the live worker can create a separate next segment without ambiguous offsets. The injected regression must inspect paths and read both segments to validate this narrower claim.
- **Terminal misclassification**: Marking a pre-rename or partial-write failure usable could corrupt later evidence. The implementation defaults every unclassified/missing classification to terminal and grants `WriterUsable` at exactly one post-rename call site.
- **Cause drift**: Gap persistence during teardown could overwrite the initiating frame-persistence cause. First-failure storage in `RuntimeState` and shutdown's read-only capture snapshot prevent replacement.
- **False clean shutdown**: Process/profile release can succeed while frame flush fails. Quality and closure are orthogonal fields, and MCP maps degraded quality to a degraded tool envelope even though stop itself completed.
- **Schema breadth**: Adding persistence details to every error would increase healthy output if projected unconditionally. The optional field appears only on classified failures, and concise healthy status is unchanged.

## Implementation notes

- Execution capability: high; durability and recovery semantics cross the filesystem writer, capture concurrency, shutdown authority, MCP diagnostics, and public documentation.
- Review weight: standard (caller/project default); feature is intentionally left at `stage: review` for the independent pass.
- Files changed: bounded persistence errors and capture/stop contracts in core; segment writer and recording sink classification in store; first-cause capture and structured shutdown in CDP; concise status, degraded stop, warnings, and schemas in MCP; current foundation docs; three child stories.
- Tests added/removed: fault injection proves post-rename writer reuse and terminal replay; typed cause/status/privacy tests; structured closure/recovery tests; MCP projection and schema tests. Obsolete stage-only and scalar outcome assertions were removed.
- Simplification: one typed inward error path now replaces generic error discards, unconditional writer poisoning, stage-only capture state, duplicate shutdown vocabulary, scalar degraded outcomes, and response-side cause reconstruction.
- Discrepancies from design: generated schemas are runtime-derived rather than checked-in artifacts; the integration regression is intentionally layered across private fault seams instead of exposing production injectors.
- Adjacent issues parked: none.
- Integrated verification: workspace all-target check and strict clippy for all touched crates passed; every focused persistence, capture, shutdown, concise-status, degraded-response, and schema regression passed. A prior broad CDP lib run had 175 passing tests and four sandbox-blocked local-socket tests, with no feature failures.

## Review

The single standard fresh-context pass found one blocker: a persistence failure first raised during target drain or final session flush was reduced to a boolean. Commit `2933fae` preserves the typed cause with precedence from pre-stop status through drain and final flush, and adds a rejecting-flush regression for the exact classification and restart recovery. The blocker was adjudicated and fixed; no second review pass was run.
