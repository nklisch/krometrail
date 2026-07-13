---
id: epic-rust-cdp-capture-foundation-chrome-target-supervision-real-reconnect
kind: story
stage: implementing
tags: [browser, testing]
parent: epic-rust-cdp-capture-foundation-chrome-target-supervision
depends_on: [epic-rust-cdp-capture-foundation-chrome-target-supervision-session-supervisor]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-12
---

# Prove disconnect and rebuild against real Chrome

## Origin

Feature review found that deterministic reconnect coverage is strong but the parent acceptance criterion explicitly requires real Chrome to prove disconnect/rebuild. The receiver confirms this as the one material current-cycle gap. Other lower-risk review proposals were parked in `idea-harden-session-edge-semantics`.

## Scope

Add an opt-in real-Chrome integration path that keeps an externally owned Chrome process alive while deliberately severing the active CDP transport, then permits the production session supervisor to establish a genuinely new connection through the same eligible attach endpoint. A loopback test proxy or equivalent deterministic transport fault boundary may be used, but the browser, cdpkit connection, target discovery, flat attachment, and rebuilt subscriptions/domain state must be real. Verify exact target-key identity, suspended/restored events, generation change, post-rebuild target commands/events, finite reconnect telemetry, ownership-correct detached stop, and zero process/profile/proxy/test-root leaks. Do not simulate reconnect solely with the scripted transport factory and do not weaken existing deterministic coverage.

## Acceptance criteria

- [ ] A real Chrome CDP connection is physically severed while Chrome remains alive, and production supervision reconnects over a new real cdpkit connection.
- [ ] Exact target keys preserve `TargetId`; restored attachments use a new generation and stale pre-disconnect events are rejected.
- [ ] Post-rebuild discovery/subscriptions/commands work and reconnect state/events are observable.
- [ ] Attached stop leaves Chrome alive; all test-owned proxy/process/profile/root resources are removed.
- [ ] Full workspace, real Chrome, spike regression, formatting, and denied-warning clippy pass; no screencast code lands.
