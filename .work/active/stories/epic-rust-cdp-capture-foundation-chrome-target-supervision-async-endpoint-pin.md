---
id: epic-rust-cdp-capture-foundation-chrome-target-supervision-async-endpoint-pin
kind: story
stage: review
tags: [browser, security, testing]
parent: epic-rust-cdp-capture-foundation-chrome-target-supervision
depends_on: [epic-rust-cdp-capture-foundation-chrome-target-supervision-architecture-final5]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-12
---

# Reuse same-authority pins and make changed-authority resolution cancellable

## Origin

Final adversarial closure reproduced two remaining endpoint defects. HTTP discovery re-ran the resolver for a returned WebSocket URL even when authority was unchanged, allowing a second loopback address to replace the first pin. For a changed authority, synchronous resolver work blocked the reconnect future from polling deadline, cancellation, stop, or process death.

## Scope

For identical HTTP/WebSocket authority, reuse the already validated HTTP `SocketAddr` exactly and never call the resolver again; only the path may rotate. For genuinely changed authority, use an asynchronous resolver contract (or an equivalently interruptible boundary) so the complete resolution is polled inside reconnect attempt control. System resolution must not block the Tokio worker. Timeout/cancellation/process death may abandon outstanding resolution immediately without committing an endpoint or connection. Keep all-address loopback rejection and pinned dialing.

## Acceptance criteria

- [x] Same-authority `/json/version` responses reuse the first exact pin and call the resolver once, even when a second lookup would return a different loopback address.
- [x] Changed-authority resolution is asynchronous and reconnect deadline/cancellation/process death win promptly against a stalled resolver.
- [x] No endpoint/state/connection is committed after abandoned resolution; mixed/public results remain rejected.
- [x] Deterministic resolver probes, rotating-path real reconnect, workspace/no-default/spike/clippy pass leak-free; no screencast code lands.

## Implementation notes

- `EndpointResolver` now returns an object-safe boxed future; `SystemEndpointResolver` uses Tokio `lookup_host`, while deterministic ports can return ready or delayed futures. Endpoint construction and direct WebSocket validation are consequently asynchronous.
- HTTP discovery reuses the exact initial HTTP `SocketAddr` whenever `/json/version` returns the same host+port authority, including path rotation; unchanged previously pinned WebSocket authorities also reuse their existing pin. Changed authorities resolve only through the reconnect attempt race.
- Added adversarial deadline, cancellation, and managed-process-death tests proving stalled changed-authority resolution cannot reach transport connection or state publication. Existing all-address loopback rejection, pinned dialing, rotating-path proxy, and repeated reconnect coverage remain intact.
- Verification: workspace tests, no-default-features tests/check, workspace denied-warning clippy, and cdp-spike tests pass after the implementation commit; no screencast code was added.

## Review

**Status:** Ready for review

**Blockers:** none known

**Notes:** Implementation is intentionally left at `stage: review`; the implementation commit contains the endpoint contract change, adversarial supervision tests, and work-item transition.
