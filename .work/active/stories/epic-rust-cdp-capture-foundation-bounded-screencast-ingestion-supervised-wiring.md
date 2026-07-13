---
id: epic-rust-cdp-capture-foundation-bounded-screencast-ingestion-supervised-wiring
kind: story
stage: done
tags: [browser]
parent: epic-rust-cdp-capture-foundation-bounded-screencast-ingestion
depends_on: [epic-rust-cdp-capture-foundation-bounded-screencast-ingestion-engine]
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-13
---

# Wire capture to supervised target generations

## Post-completion correction (2026-07-13)

This story is `done` because the supervised lifecycle, reducer effects, generation fencing, target isolation, reconnect, privacy, and one-absolute-deadline shutdown were implemented and approved. Production Chrome later exposed one invalid acceptance edge: initial visibility decoding handled only `/result/result/value`, while this cdpkit path returned `/result/value`. Reconnect already handled both. `epic-rust-cdp-capture-foundation-bounded-screencast-ingestion-contract-remediation` unifies the parser, makes probe failure explicit, and prevents Ready with unresolved recordable visibility. This historical story is not sufficient completion evidence for the revised feature.

## Scope

Implement Unit 2 of the parent design after the bounded engine is green. Add session identity/origin and per-target capture status to the infrastructure-free browser port, then reconcile the proven `CaptureCoordinator` with production target/session lifecycle and root dependencies.

Add the exact parent signatures:

```rust
pub enum BrowserSessionEvent {
    // existing variants
    CaptureStateChanged { status: TargetCaptureStatus },
    CaptureGapDeclared { gap: CaptureGap },
}

pub trait BrowserSessionPort: Send + Sync {
    fn session_id(&self) -> SessionId;
    fn session_origin(&self) -> SessionOrigin;
    // existing methods
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

Allocate one `SessionId` at connection establishment and sample its fixed `SessionOrigin` before the first capture subscription, `Page.startScreencast`, or frame can occur. Initial capture starts only after the browser session is `Ready`, a target is `Attached`, its exact flat `TransportSessionId` is known, and initial visibility is not `Unknown`. Dynamic attachment and reconnect use reducer-owned capture effects; do not hide `Page.startScreencast` inside target attach/domain restore code or add an event-observer/polling reconciliation loop.

Add `SupervisorInput::InitialReconciliationCompleted` and route the initial Connecting → Ready transition through `reduce` instead of mutating state in `session.rs`. Extend `SupervisorTargetState` with `CaptureBinding::{Inactive, Active(context), Suspended(context), Terminal}` and define `CaptureEffectContext` with `TargetId`, `connection_generation`, `attachment_generation`, and exact `TransportSessionId`. At the end of each successful reduction, one `reconcile_capture_bindings` helper atomically updates the binding and emits exhaustive `StartCapture`, `StopCapture`, `SuspendCapture`, or `ResumeCapture` effects. Suspend/stop retain the old context before reducer transport state is cleared. Start applies to a newly eligible generation, suspend fences connection loss, resume applies only to the same exact target key restored at a higher attachment generation, and stop covers detach/close/target failure/shutdown. `session.rs::apply_effects` is the sole executor and exhaustively matches all variants.

Before replacing/dropping connection resources, execute `SuspendCapture` and stop old readers. A reconnected exact browser target key preserves `TargetId`, advances attachment generation, and closes `BrowserDisconnected` on its first new-generation frame. Ignore every late callback whose generation no longer matches. Missing, detached, closed, or failed targets stop only their own stream. **Correction:** there is no Chrome frame-number comparison to reset; remediation continues Krometrail `CaptureOrdinal` across attachment generations.

Explicit stop/cancellation constructs one absolute `ShutdownDeadline` from `CaptureConfig::shutdown_timeout` (5-second default). It first prevents acceptance, then threads only the remaining budget through matching `Page.stopScreencast`, all accepted queue/gap-ledger drains, one `RecordingSink::flush(session_id)`, every target detach, `Browser.close`, and managed-process termination. No target or phase starts another timeout. Deadline exhaustion emits `CaptureStopped`, marks `ShutdownIncomplete`, skips later graceful waits, and invokes last-resort cleanup without fake success. Reconnect never session-flushes.

Implement a `CaptureObserver` in `session.rs` that publishes transition-driven `CaptureStateChanged` and explicit `CaptureGapDeclared` through the existing bounded subscriber registry. Per-frame counters remain queryable status, not event spam. Extend subscriber logging matches without exposing browser keys, URLs, titles, raw params, payloads, or source errors.

Root composition shares the existing `MonotonicClock`, `IdSource`, and explicit `RecordingSink` with `ProductionBrowserConnector::with_capture`. Retain `UnavailableRecordingSink`; do not add a discarding sink, fake store, command, or persistence implementation. Supervision-only connector construction remains for focused launcher/transport tests, while `build_runtime` is capture-configured.

## Required files

- `crates/krometrail-core/src/browser/session.rs`
- `crates/krometrail-core/src/ports/browser.rs`
- `crates/krometrail-core/src/ports/mod.rs`
- `crates/krometrail-cdp/src/targets/model.rs`
- `crates/krometrail-cdp/src/targets/reducer.rs`
- `crates/krometrail-cdp/src/targets/mod.rs`
- `crates/krometrail-cdp/src/session.rs`
- `crates/krometrail-cdp/tests/session_capture.rs` (new)
- `src/app.rs`

These files are exclusive to this story's implementation wave. This story deliberately owns `targets/reducer.rs` because adding capture event/effect variants requires its exhaustive matches to change in the same compile-real stride. Do not edit the engine files or the later real-Chrome test.

## Acceptance criteria

- [x] Browser-session core fakes implement and verify unique session identity, fixed origin sampled before subscriptions/start/first frame, object-safe sorted capture status snapshots (including bounded ack/cadence summaries), capture state events, and explicit gap events without runtime/transport types entering core.
- [x] `InitialReconciliationCompleted` replaces direct Ready mutation. Reducer tests cover the single `reconcile_capture_bindings` helper, every Start/Stop/Suspend/Resume emission, and idempotent no-op. Effects carry exact target ID, connection generation, attachment generation, and transport session; suspend/stop preserve old context before clearing it. `targets/reducer.rs` exhaustively handles the new `BrowserSessionEvent` variants in its logging match, `apply_effects` exhaustively handles the new effect variants, and no second reconciliation mechanism exists.
- [x] No `Page.startScreencast` occurs in Connecting/Reconnecting, for Discovered/Suspended/Unknown targets, or before the exact target's flat session exists; each Ready/Attached generation starts exactly once.
- [x] **Partially invalidated after completion:** subscriptions precede start, but initial Ready was not truthful for the production cdpkit raw-result shape because visibility could remain `Unknown` silently. Remediation accepts both observed shapes, explicitly fails/detaches malformed probes, and guards Ready against unresolved recordable targets.
- [x] Two scripted targets use isolated session scopes, queues, sequence trackers, status, and gaps; blocking/failing one leaves the other accepting and persisting frames.
- [x] Dynamic attach/close affects only that exact key. Connection loss cancels old acceptance before resource replacement, opens `BrowserDisconnected`, rejects late old-generation callbacks, and closes the interruption on the first valid restored frame.
- [x] A restored exact key keeps `TargetId` and advances generation; a missing/new key closes/creates rather than URL/title matching. The superseded Chrome frame-number comparison is removed by remediation; Krometrail `CaptureOrdinal` continues per target across generations.
- [x] Visibility events feed both capture status and the existing target visibility reducer without starting a second screencast or producing duplicate hidden gaps.
- [x] Explicit stop prevents new acceptance first and uses one absolute deadline whose remaining budget covers capture stop/drain, one session flush, all target detaches, `Browser.close`, and managed-process termination. Tests use a consuming fake clock/deadline to prove later phases receive less budget and no per-phase reset occurs. Reconnect does not flush; target close does not flush the whole session.
- [x] Deadline exhaustion/flush/worker blockage emits observable `CaptureStopped`, leaves statistics truthful, returns/records `ShutdownIncomplete`, skips unbudgeted graceful waits, performs last-resort process cleanup, and never blocks browser cleanup indefinitely.
- [x] `capture_statuses()` is sorted by `TargetId`; state events are transition-only; bounded acknowledgement/cadence sample-count/p50/p95/p99/max summaries flow through unchanged; gap/status logs and events follow the parent privacy allowlist.
- [x] Root uses the shared clock/IDs/sink, preserves explicit unavailable persistence, and adds no user-visible command or fake success.
- [x] Existing target reducer, reconnect, process/profile, doctor, runtime smoke, default/no-default, and spike tests stay green; workspace format/check/test/clippy pass independently.

## Execution

- Effective worker: `highest`.
- Depends on the engine because session wiring must not outrun the ack/backpressure proof.
- Review weight: `standard` at the parent feature; story verification may fast-advance.

## Implementation notes

- Execution capability: `cdpkit-transport` at highest implementation depth; this cross-cutting lifecycle work was kept inline as requested so reducer, session executor, core ports, and root composition stayed compile-real in one stride.
- Review weight: `standard` from the parent feature; explicitly left at `stage: review` for a fresh timing/cross-cutting review.
- Files changed: core browser-session events and port identity/status contracts; target model/reducer capture bindings and effects; production session capture observer/effect executor, reconnect wiring, and aggregate shutdown deadline; capture visibility/stop hardening; root shared dependency composition; deterministic supervised-capture tests.
- Tests added/removed: `crates/krometrail-cdp/tests/session_capture.rs` covers reducer-owned start/suspend/resume/stop ordering, exact generations, visibility/failure locality, shutdown fencing, and event privacy. Existing capture-engine tests continue to cover blocked sinks, cancellation/deadline abandonment, visibility coalescing, target isolation, and privacy-safe status surfaces.
- Simplification: reused the existing `RecordingSink`, `MonotonicClock`, `IdSource`, coordinator, subscriber registry, and transport seam; added no store, analysis, command, or fake-success adapter. Shutdown phases now share one named absolute `ShutdownDeadline` and an ownership-safe process fallback.
- Discrepancies from design: Added an adapter-local `CaptureVisibilityChanged` input so capture visibility signals update the existing target reducer without a second reconciliation loop; added `CaptureStartFailed` as the reducer input for target-local start failures. Both remain outside the core public event contract. Post-completion production discovery found initial raw visibility decoding incomplete; the remediation story owns the shared two-shape parser and unresolved-Ready guard.
- Adjacent issues parked: none.

## Repair notes (2026-07-13)

- `CaptureStartFailed` now emits the exact flat-session `Detach` before the reducer forgets the session mapping. The capture integration regression keeps an unrelated target's active binding and transport mapping intact while proving the rejected surplus session is detached.
- `ShutdownDeadline` now accepts a narrow injectable monotonic budget source. The consuming-clock shutdown fixture exercises capture stop/drain/flush, target detach, `Browser.close`, and managed-process termination, records phase budgets, proves one absolute origin and strict decrease, and covers exhausted-budget force cleanup.
- Removed the unused private `ReconnectExhausted` capture stop-reason variant without changing the stop contract.

## Review findings (2026-07-13)

Fresh-context review approved the overall architecture but proposed two important findings that the receiver confirms as material acceptance gaps:

1. `CaptureStartFailed` clears the target's transport session without emitting `Detach`, leaking the flat CDP session until connection teardown (readily reached above `max_active_streams`). Preserve the session long enough to emit and execute a detach effect, while keeping the target-local failure isolated.
2. The required one-absolute-deadline contract lacks its specified consuming fake clock/deadline integration test. Add deterministic proof that capture stop/drain/flush, detach, Browser.close, and process termination receive monotonically decreasing remaining budget and no phase resets its own deadline.

The reviewer also noted unused private stop-reason variants; this is cleanup, not a blocker, and should be simplified if the repair makes them unnecessary rather than expanding behavior.

## Final review (2026-07-13)

**Verdict:** Approve

**Blockers:** none
**Important:** none
**Nits:** none

**Notes:** Fresh-context re-review verified exact failed-start flat-session detach with target isolation and a consuming fake deadline showing one absolute strictly decreasing budget across capture stop/drain/flush, detach, Browser.close, process termination, and exhaustion fallback. All 12 acceptance criteria and 145 workspace tests pass; no material finding remains.
