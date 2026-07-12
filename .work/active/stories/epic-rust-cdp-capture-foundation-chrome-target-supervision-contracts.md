---
id: epic-rust-cdp-capture-foundation-chrome-target-supervision-contracts
kind: story
stage: implementing
tags: [browser]
parent: epic-rust-cdp-capture-foundation-chrome-target-supervision
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Define browser supervision contracts

## Scope

Implement Unit 1 of the parent design: replace provisional browser ports with infrastructure-free typed connection/profile ownership, installation, compatibility capability, session/target state, event subscription, stop outcome, lifecycle, and stable error contracts.

The public shape is the parent design's complete contract, including:

- `ProfileRef::Managed(ProfileIdentity) | External`; `BrowserSessionPort::profile()` returns `&ProfileRef`.
- `BrowserInstallation { executable, source, product, version }`, `BrowserInstallationSource`, `BrowserProduct`, `BrowserProductVersion`, and the complete runtime `BrowserVersion { product, product_version, revision, protocol_version, user_agent, js_version }`.
- `BrowserSessionState` for connector/supervisor connectivity, distinct from recording `SessionLifecycle`.
- `BrowserSessionEvent::SessionFailed` plus stable `browser_not_found` and `browser_process_terminated`; process termination remains distinct from transport closure/reconnect exhaustion.

Migrate `RecordingSession`'s field, constructor, wire representation, accessor, and tests from `ProfileIdentity` to `ProfileRef`, and migrate its browser fixtures to the complete `BrowserVersion`. This is a downstream contract migration required now so the richer profile/version values compile through existing consumers; it adds no capture behavior.

Do not implement cdpkit, filesystem/process behavior, reconnect tasks, or screencast behavior.

## Required files

- `crates/krometrail-core/src/browser/mod.rs`
- `crates/krometrail-core/src/browser/target.rs`
- `crates/krometrail-core/src/browser/session.rs` (new)
- `crates/krometrail-core/src/ports/browser.rs`
- `crates/krometrail-core/src/ports/mod.rs`
- `crates/krometrail-core/src/recording/session.rs`
- `crates/krometrail-core/src/lifecycle.rs`
- `crates/krometrail-core/src/error.rs`
- `crates/krometrail-core/src/lib.rs`

## Acceptance criteria

- [ ] Parent Unit 1 signatures and complete installation/product/version/profile types are implemented, validated, and exported; no provisional string-only domain support remains.
- [ ] `RecordingSession` serializes and validates `ProfileRef`, with managed and external fixtures, and uses the complete runtime `BrowserVersion` without conflating `BrowserSessionState` and `SessionLifecycle`.
- [ ] The lifecycle registry exhaustively enforces: `Discovered -> Attached | Suspended | Closed | Failed`; `Attached -> Recording | Hidden | Suspended | Closed | Failed`; `Recording -> Hidden | Suspended | Closed | Failed`; `Hidden -> Recording | Suspended | Closed | Failed`; `Suspended -> Discovered | Attached | Recording | Hidden | Closed | Failed`; terminal states have no exits. Every unlisted pair is rejected by tests.
- [ ] Stable variant registries/serde names, invalid values, duplicate capability entries, managed/attach stop outcomes, session failure events, and event-stream closure have exhaustive tests.
- [ ] Adapter errors map to safe structured core errors with retry/recovery guidance; `browser_not_found` supports discovery-only doctor, `browser_process_terminated` is distinct from `reconnect_exhausted`, and source/debug strings cannot serialize.
- [ ] `krometrail-core` remains free of cdpkit, CDP, WebSocket, Tokio, URL-parser, filesystem adapter, and process types. `PathBuf` is permitted only as the validated installation executable value.
- [ ] No capture or screencast contract is added.
