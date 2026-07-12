---
id: epic-rust-cdp-capture-foundation-chrome-target-supervision-transport-adapter
kind: story
stage: implementing
tags: [browser]
parent: epic-rust-cdp-capture-foundation-chrome-target-supervision
depends_on: [epic-rust-cdp-capture-foundation-chrome-target-supervision-contracts]
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Build the production cdpkit transport and compatibility probe

## Scope

Implement Unit 2 of the parent design: production-enable exact cdpkit 0.4.0 behind the owned `krometrail-cdp::transport` seam, validate/resolve explicit loopback endpoints, and derive browser/Electron renderer compatibility from one required-capability probe registry.

No reconnect, launcher/profile ownership, target state machine, fallback transport, or screencast start/ingestion belongs here.

## Required files

- `Cargo.toml`
- `crates/krometrail-cdp/Cargo.toml`
- `crates/krometrail-cdp/src/lib.rs`
- `crates/krometrail-cdp/src/transport/{mod.rs,cdpkit.rs,error.rs}`
- `crates/krometrail-cdp/src/compatibility.rs`
- `crates/krometrail-cdp/src/endpoint.rs`
- `crates/krometrail-cdp/tests/support/scripted_cdp.rs`
- `crates/krometrail-cdp/tests/production_transport.rs`
- `crates/krometrail-cdp/tests/compatibility_probe.rs`

## Acceptance criteria

- [ ] No cdpkit type crosses `transport/cdpkit.rs`; the honest event contract is named-event params, not wildcard envelopes, and reconnect remains absent.
- [ ] Browser/session raw sends, flat session routing, event-before-response, additive response fields, malformed responses, close propagation, and two-session isolation are deterministic against an in-process scripted peer without sleeps.
- [ ] HTTP/WebSocket loopback endpoints normalize safely; unsupported schemes, credentials, and non-loopback resolved addresses fail before connection.
- [ ] Chrome, Chromium, and Electron-labelled capable page targets pass the same registry-derived probe; missing requirements and Node-inspector-only endpoints return stable compatibility failures.
- [ ] The probe does not start a screencast, spike code remains opt-in, and its regression test remains green.
