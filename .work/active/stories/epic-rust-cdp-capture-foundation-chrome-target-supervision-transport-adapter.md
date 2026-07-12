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

`endpoint.rs` owns `LocalCdpEndpoint`: a construct-only-through-validation value containing the normalized loopback HTTP origin, resolved browser WebSocket URL, and redacted display label. It owns no process/profile cleanup. The following managed-launch story consumes this type rather than defining a second endpoint representation.

The workspace adds non-optional `url = "2"` for the endpoint value. The Cargo feature topology is exact: `default = ["cdpkit-transport"]`; that feature enables optional cdpkit 0.4.0, futures-util, serde-json, and required Tokio sync/time support. `cdp-spike` remains opt-in; `cdp-spike-cdpkit = ["cdp-spike", "cdpkit-transport", "dep:libc"]` reuses the workspace pin. Default production modules never import `spike` modules.

No reconnect, launcher/profile ownership, target state machine, fallback transport, or screencast start/ingestion belongs here.

## Required files

- `Cargo.toml`
- `crates/krometrail-cdp/Cargo.toml`
- `crates/krometrail-cdp/src/lib.rs`
- `crates/krometrail-cdp/src/transport/mod.rs` (new)
- `crates/krometrail-cdp/src/transport/cdpkit.rs` (new)
- `crates/krometrail-cdp/src/transport/error.rs` (new)
- `crates/krometrail-cdp/src/compatibility.rs` (new)
- `crates/krometrail-cdp/src/endpoint.rs` (new)
- `crates/krometrail-cdp/tests/support/scripted_cdp.rs` (new)
- `crates/krometrail-cdp/tests/production_transport.rs` (new)
- `crates/krometrail-cdp/tests/compatibility_probe.rs` (new)

## Acceptance criteria

- [ ] No cdpkit type crosses `transport/cdpkit.rs`; the honest event contract is named-event params, not wildcard envelopes, and reconnect remains absent.
- [ ] Browser/session raw sends, flat session routing, event-before-response, additive response fields, malformed responses, close propagation, and two-session isolation are deterministic against an in-process scripted peer without sleeps.
- [ ] HTTP/WebSocket loopback inputs produce `LocalCdpEndpoint`; unsupported schemes, credentials, fragments, TLS/public or non-loopback resolved addresses fail before connection.
- [ ] Chrome, Chromium, and Electron-labelled capable page targets pass the same registry-derived probe; missing requirements and Node-inspector-only endpoints return stable compatibility failures.
- [ ] `browser.compatibility.probed` tracing reports sanitized product, browser/protocol version, endpoint kind, and registry-derived required-capability outcome; it never reports credentials, full URLs, event params, page content, or source/debug error strings.
- [ ] `cargo check -p krometrail-cdp` compiles production cdpkit through the default feature; `cargo check -p krometrail-cdp --no-default-features` compiles the seam without cdpkit. Spike code remains opt-in and its regression test remains green.
- [ ] The probe does not start a screencast.
