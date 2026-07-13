---
id: epic-rust-cdp-capture-foundation-bounded-screencast-ingestion-supervised-wiring
kind: story
stage: implementing
tags: [browser]
parent: epic-rust-cdp-capture-foundation-bounded-screencast-ingestion
depends_on: [epic-rust-cdp-capture-foundation-bounded-screencast-ingestion-engine]
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Wire capture to supervised target generations

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

Allocate one `SessionId` and `SessionOrigin` at connection establishment. Initial capture starts only after the browser session is `Ready`, a target is `Attached`, its exact flat `TransportSessionId` is known, and initial visibility is not `Unknown`. Dynamic attachment and reconnect use the same reconciliation path; do not hide `Page.startScreencast` inside target attach/domain restore effects.

Before replacing/dropping connection resources, stop old readers and call `suspend_for_disconnect`. A reconnected exact browser target key preserves `TargetId`, advances attachment generation, resets source-sequence comparison, and closes `BrowserDisconnected` on its first new-generation frame. Ignore every late callback whose generation no longer matches. Missing, detached, closed, or failed targets stop only their own stream.

Explicit stop/cancellation first prevents acceptance, then best-effort stops live matching screencasts, drains accepted queues and gap ledgers under one shared deadline, emits `CaptureStopped` for accepted unfinished work, and calls `RecordingSink::flush(session_id)` once before browser detach/close. Reconnect never session-flushes. A failed/blocked sink reports incomplete shutdown rather than fake success.

Implement a `CaptureObserver` in `session.rs` that publishes transition-driven `CaptureStateChanged` and explicit `CaptureGapDeclared` through the existing bounded subscriber registry. Per-frame counters remain queryable status, not event spam. Extend subscriber logging matches without exposing browser keys, URLs, titles, raw params, payloads, or source errors.

Root composition shares the existing `MonotonicClock`, `IdSource`, and explicit `RecordingSink` with `ProductionBrowserConnector::with_capture`. Retain `UnavailableRecordingSink`; do not add a discarding sink, fake store, command, or persistence implementation. Supervision-only connector construction remains for focused launcher/transport tests, while `build_runtime` is capture-configured.

## Required files

- `crates/krometrail-core/src/browser/session.rs`
- `crates/krometrail-core/src/ports/browser.rs`
- `crates/krometrail-core/src/ports/mod.rs`
- `crates/krometrail-cdp/src/session.rs`
- `crates/krometrail-cdp/tests/session_capture.rs` (new)
- `src/app.rs`

These files are exclusive to this story's implementation wave. Do not edit the engine files or the later real-Chrome test.

## Acceptance criteria

- [ ] Browser-session core fakes implement and verify unique session identity, fixed origin, object-safe sorted capture status snapshots, capture state events, and explicit gap events without runtime/transport types entering core.
- [ ] No `Page.startScreencast` occurs in Connecting/Reconnecting, for Discovered/Suspended/Unknown targets, or before the exact target's flat session exists; each Ready/Attached generation starts exactly once.
- [ ] Subscriptions are established before start, and initial session Ready is truthful: capture reconciliation has either started every eligible stream or published a target-local capture failure.
- [ ] Two scripted targets use isolated session scopes, queues, sequence trackers, status, and gaps; blocking/failing one leaves the other accepting and persisting frames.
- [ ] Dynamic attach/close affects only that exact key. Connection loss cancels old acceptance before resource replacement, opens `BrowserDisconnected`, rejects late old-generation callbacks, and closes the interruption on the first valid restored frame.
- [ ] A restored exact key keeps `TargetId` and advances generation; a missing/new key closes/creates rather than URL/title matching; sequence comparison never crosses generations.
- [ ] Visibility events feed both capture status and the existing target visibility reducer without starting a second screencast or producing duplicate hidden gaps.
- [ ] Explicit stop prevents new acceptance first, drains or reports all accepted work under one deadline, invokes one session flush, then detaches/closes Chrome. Reconnect does not flush; target close does not flush the whole session.
- [ ] Flush/worker timeout emits observable `CaptureStopped`, leaves statistics truthful, returns/records `ShutdownIncomplete`, and never blocks browser cleanup indefinitely.
- [ ] `capture_statuses()` is sorted by `TargetId`; state events are transition-only; gap/status logs and events follow the parent privacy allowlist.
- [ ] Root uses the shared clock/IDs/sink, preserves explicit unavailable persistence, and adds no user-visible command or fake success.
- [ ] Existing target reducer, reconnect, process/profile, doctor, runtime smoke, default/no-default, and spike tests stay green; workspace format/check/test/clippy pass independently.

## Execution

- Effective worker: `highest`.
- Depends on the engine because session wiring must not outrun the ack/backpressure proof.
- Review weight: `standard` at the parent feature; story verification may fast-advance.
