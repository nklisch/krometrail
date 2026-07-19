---
id: feature-visibility-emission-ordering
kind: feature
stage: review
tags: [browser, bug]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-19
updated: 2026-07-19
---

# Visibility fence emission-side ordering

## Brief

Cross-model review finding (minor, adjudicated as parkable) on
`feature-window-lifecycle-integrity`: the visibility ordering fence stamps
observed session time when the screencast visibility event is dequeued from the
transport subscription (`capture/pipeline.rs` visibility reader →
`SessionCaptureObserver::visibility_changed`), not when Chrome emitted it. A
hidden event sitting in the subscription channel while an activation write-back
commits gets a post-activation stamp and passes the fence — the original
overwrite race survives, confined to the transport-queue window (small, and
self-healing via the running screencast re-emitting Visible). Chrome supplies
no timestamp on `Page.screencastVisibilityChanged`, so closing it fully needs
an emission-side ordering token: stamp at the transport event pump before
fan-out, or sequence all visibility-bearing transport events through one
ordered path.

Also in scope: producers use `session_time().unwrap_or(SessionTime::ZERO)`,
which would permanently fence a producer whose clock normalization fails (only
possible pre-origin) — prefer dropping the observation with a diagnostic over
stamping zero.

## Simplification opportunity

If stamping moves to the transport event pump, the dequeue-side stamping in the
visibility reader becomes dead and should be removed rather than kept as a
fallback.

Origin: `.work/backlog/idea-visibility-fence-emission-stamp.md`.

## Architectural choice

Move the visibility observation stamp to the earliest point our process can
observe the event, and make stamp-failure drop-with-diagnostic instead of
`SessionTime::ZERO`. Full emission-side ordering is not reachable: Chrome sends
no timestamp on `Page.screencastVisibilityChanged` and the pinned external
`cdpkit` transport exposes buffered `JsonStream`s without receipt stamps, so
the residual window is cdpkit's internal channel dwell plus task scheduling.
This feature eliminates the dwell *our* code adds (stamping late inside the
observer after other work, and batch-dwell where the reader's processing of
event N inflates the stamp of already-queued event N+1) and documents the
residual as accepted, mitigated by the running screencast re-emitting Visible.
Alternatives rejected: patching/forking cdpkit for receipt stamps (pinned
external dependency, disproportionate), and probing Chrome for visibility after
activation (same dequeue-side comparison problem, extra round trip).

## Design decisions
- **Stamp acquisition point**: first statement after `events.next()` returns in
  the capture pipeline's `visibility_reader`, before any observer/transition
  work. The stamp is threaded through `CaptureObserver::visibility_changed`,
  which stops re-sampling time at try_send.
- **Stamp failure policy**: if session-time normalization fails (pre-origin
  only), drop the visibility observation with a `tracing` diagnostic (bounded:
  target id + event name only) instead of stamping ZERO. A pre-origin
  visibility event has no ordering meaning and must not permanently fence the
  producer.

## Implementation Units

### Unit 1: Stamped visibility observations
**Files**: `crates/krometrail-cdp/src/capture/mod.rs` (`CaptureObserver`
trait), `crates/krometrail-cdp/src/capture/pipeline.rs` (`visibility_reader`,
`StreamRuntime`), `crates/krometrail-cdp/src/session/mod.rs`
(`SessionCaptureObserver`)

```rust
// capture/mod.rs
fn visibility_changed(
    &self,
    target_id: TargetId,
    visibility: TargetVisibility,
    observed_at: SessionTime,          // NEW: stamped at dequeue by the reader
);
```

`StreamRuntime` gains a session-time stamper (injected clock + session origin,
or a `Arc<dyn Fn() -> krometrail_core::Result<SessionTime>>`-shaped port
matching how the pipeline already receives its collaborators). The reader
stamps immediately on dequeue; on stamp failure it logs and `continue`s without
calling the observer. `SessionCaptureObserver::visibility_changed` forwards the
received stamp into `SupervisorInput::CaptureVisibilityChanged` and into
`observe_visibility` (keeping one stamp for both sinks) and loses its
`unwrap_or(SessionTime::ZERO)`.

**Acceptance Criteria**:
- [x] Deterministic double: two visibility events queued before the reader
      runs get stamps sampled at their own dequeue, and a supervisor
      write-back stamped between them fences exactly the earlier one.
- [x] Stamp-failure double: normalization error → no supervisor input, no
      ZERO stamp, diagnostic emitted.
- [x] No remaining `unwrap_or(SessionTime::ZERO)` on the visibility path.

### Unit 2: Remove dequeue-side stamping remnants
**File**: `crates/krometrail-cdp/src/session/mod.rs`

After Unit 1, `SessionCaptureObserver` no longer needs
`browser_events.session_time()` for visibility; remove the dead sampling and
keep `observe_visibility`'s stamp consistent with the forwarded one (extend
`observe_visibility`'s `None` time parameter to `Some(observed_at)` if its
signature already accepts an optional time — it does:
`observe_visibility(target_id, None, visibility)`).

**Acceptance Criteria**:
- [x] Browser-event visibility records carry the same stamp the fence saw.

## Implementation Order
1. Unit 1
2. Unit 2

## Testing
- Interface tests at the capture-pipeline seam with deterministic doubles
  (base tier of layered-cdp-qualification); the fence semantics in
  `targets/reducer.rs` already have coverage and are unchanged.

## Risks
- Residual race window (cdpkit channel dwell) survives by design; the item
  body records it as accepted with the screencast re-emission mitigation. If
  it ever bites in practice, the escalation path is a cdpkit feature request
  for receipt-stamped events.

## Implementation notes

- Execution capability: host implementation, because the capture reader,
  observer, browser-event authority, and deterministic capture doubles form one
  cohesive ordering boundary.
- Review weight: standard, project default.
- Files changed: `crates/krometrail-cdp/src/capture/mod.rs`,
  `crates/krometrail-cdp/src/capture/pipeline.rs`,
  `crates/krometrail-cdp/src/capture/tests.rs`,
  `crates/krometrail-cdp/src/events/domain.rs`,
  `crates/krometrail-cdp/src/session/mod.rs`, and
  `crates/krometrail-cdp/src/session/runtime.rs`.
- Tests added: dequeue-order stamping and pre-origin stamp-failure doubles;
  browser-event visibility receives the forwarded session stamp through the
  existing event authority path.
- Simplification: removed observer-side clock sampling and the visibility-path
  ZERO fallback; the same stamped value now feeds the fence and event record.
- Discrepancies from design: the existing event ingress accepts observed-clock
  values, so the session stamp is converted back through the session origin at
  the authority boundary; overflow drops the record rather than resampling.
- Adjacent issues parked: none.
- Verification: `cargo fmt --all` and
  `CARGO_TARGET_DIR=/tmp/krometrail-target cargo test -p krometrail-cdp
  capture::tests::visibility_ --locked` passed.
