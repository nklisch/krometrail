---
id: epic-rust-cdp-capture-foundation-chrome-target-supervision-real-reconnect
kind: story
stage: done
tags: [browser, testing]
parent: epic-rust-cdp-capture-foundation-chrome-target-supervision
depends_on: [epic-rust-cdp-capture-foundation-chrome-target-supervision-session-supervisor]
release_binding: 1.0.0
gate_origin: null
created: 2026-07-13
updated: 2026-07-12
---

# Prove disconnect and rebuild against real Chrome

## Origin

Feature review found that deterministic reconnect coverage is strong but the parent acceptance criterion explicitly requires real Chrome to prove disconnect/rebuild. The receiver confirms this as the one material current-cycle gap. Other lower-risk review proposals were parked in `idea-harden-session-edge-semantics`.

## Scope

Add an opt-in real-Chrome integration path that keeps an externally owned Chrome process alive while deliberately severing the active CDP transport, then permits the production session supervisor to establish a genuinely new connection through the same eligible attach endpoint. A loopback test proxy or equivalent deterministic transport fault boundary may be used, but the browser, cdpkit connection, target discovery, flat attachment, and rebuilt subscriptions/domain state must be real. Verify exact target-key identity, suspended/restored events, generation change, post-rebuild target commands/events, finite reconnect telemetry, ownership-correct detached stop, and zero process/profile/proxy/test-root leaks. Do not simulate reconnect solely with the scripted transport factory and do not weaken existing deterministic coverage.

## Implementation notes

- Added an opt-in real-Chrome supervision test with a RAII loopback proxy that forwards `/json/version` and WebSocket frames, severs only the active pair, and joins/aborts listener and connection tasks on normal and panic cleanup paths.
- The test launches Chrome outside `ProductionBrowserConnector`, proves the attached stop leaves the external browser usable, verifies the same browser target key and Krometrail `TargetId` across a new attachment generation, observes reconnect/suspended/restored events, and exercises real post-rebuild target commands/events through cdpkit.
- Real Chrome exposed a state-loss edge in the supervisor: a rejected late lifecycle input could leave the task holding a fresh empty state. The supervisor now restores the last committed state on reducer rejection so late old-generation input cannot erase targets before rebuild. The existing deterministic stale-generation reducer assertion remains in place.

## Acceptance criteria

- [x] A real Chrome CDP connection is physically severed while Chrome remains alive, and production supervision reconnects over a new real cdpkit connection.
- [x] Exact target keys preserve `TargetId`; restored attachments use a new generation and stale pre-disconnect events are rejected.
- [x] Post-rebuild discovery/subscriptions/commands work and reconnect state/events are observable.
- [x] Attached stop leaves Chrome alive; all test-owned proxy/process/profile/root resources are removed.
- [x] Full workspace, real Chrome, spike regression, formatting, and denied-warning clippy pass; no screencast code lands.

## Review (2026-07-13)

**Verdict:** Approve

**Blockers:** none
**Important:** none
**Nits:** none

**Notes:** Fast-lane evidence review reran 102 workspace tests and denied-warning clippy; verified a physically severed real cdpkit transport, new physical connection, target/TargetId continuity, generation restoration, late-event state preservation, post-rebuild commands/events, external-browser survival, and zero proxy/process/profile/root leaks. Verdict: Approve - story verified by implement; fast-lane advance.
