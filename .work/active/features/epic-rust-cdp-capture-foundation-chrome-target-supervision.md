---
id: epic-rust-cdp-capture-foundation-chrome-target-supervision
kind: feature
stage: drafting
tags: [browser]
parent: epic-rust-cdp-capture-foundation
depends_on: [epic-rust-cdp-capture-foundation-cdp-transport-gate]
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Chrome Session and Target Supervision

## Brief

Deliver the production browser-session capability around the transport selected by the compatibility gate. Krometrail can discover Chrome, launch an isolated reusable or temporary managed profile, attach to an explicit local endpoint, report browser and protocol compatibility before recording, and close a controlled browser or detach cleanly.

Supervise recordable page targets through flat CDP sessions so target creation, navigation, visibility, closure, and target-local failures remain isolated. Reconnection restores the browser connection and target attachments when safe, while unrecoverable loss ends the session through explicit cancellation and bounded cleanup. This feature owns lifecycle and target continuity, but not frame queueing, persistence, browser actions, or structured page snapshots.

## Epic context

- Parent epic: `epic-rust-cdp-capture-foundation`
- Position in epic: production browser adapter — consumes the qualified transport and supplies supervised target sessions to capture
- Design decisions inherited: transport choice follows real-browser gate evidence rather than library API assumptions

## Foundation references

- `docs/VISION.md` — Local-First Operation
- `docs/SPEC.md` — Browser Lifecycle, Sessions and Targets, and Errors and Degraded Operation
- `docs/ARCHITECTURE.md` — Browser Connection, Target Lifecycle, Failure Isolation, and Observability
