---
id: epic-rust-cdp-capture-foundation-chrome-target-supervision-async-endpoint-pin
kind: story
stage: implementing
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

- [ ] Same-authority `/json/version` responses reuse the first exact pin and call the resolver once, even when a second lookup would return a different loopback address.
- [ ] Changed-authority resolution is asynchronous and reconnect deadline/cancellation/process death win promptly against a stalled resolver.
- [ ] No endpoint/state/connection is committed after abandoned resolution; mixed/public results remain rejected.
- [ ] Deterministic resolver probes, rotating-path real reconnect, workspace/no-default/spike/clippy pass leak-free; no screencast code lands.
