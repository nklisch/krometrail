---
id: epic-rust-cdp-capture-foundation-chrome-target-supervision-contracts
kind: story
stage: done
tags: [browser]
parent: epic-rust-cdp-capture-foundation-chrome-target-supervision
depends_on: []
release_binding: 1.0.0
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
- `BrowserSessionEvent::SessionFailed` plus all nine stable errors: `browser_not_found`, `browser_launch_failed`, `browser_process_terminated`, `browser_compatibility_failed`, `profile_in_use`, `target_failed`, `reconnect_exhausted`, `cancelled`, and `shutdown_incomplete`; process termination remains distinct from transport closure/reconnect exhaustion.

Migrate `RecordingSession`'s field, constructor, wire representation, accessor, and tests from `ProfileIdentity` to `ProfileRef`, and migrate its browser fixtures to the complete `BrowserVersion`. This is a downstream contract migration required now so the richer profile/version values compile through existing consumers; it adds no capture behavior.

Own the compile-real root transition atomically with the trait change. Update `UnavailableBrowserConnector` in `src/app.rs` to implement the changed connector, with `installations()` returning an empty list and `connect()` returning stable `browser_not_found` plus browser-installation recovery. Make `doctor` call `installations()` exactly once, never `connect()`, and preserve that stable error when empty. Update `tests/rust-runtime-smoke.rs` to require exit 1, `error[browser_not_found]`, and recovery while rejecting provisional `unsupported`/`browser transport is not available` text. This is a transitional unavailable adapter, not fake success; story 4 replaces its composition in an explicit later edit.

Do not implement cdpkit, filesystem/process behavior, reconnect tasks, production composition, or screencast behavior.

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
- `src/app.rs`
- `tests/rust-runtime-smoke.rs`

## Implementation notes

- Added the infrastructure-free browser domain contracts, including validated installation/product/version/profile values, renderer capability support, session state/events, ownership, stop outcomes, and object-safe event/session/connector ports.
- Migrated `RecordingSession` to `ProfileRef` and the complete runtime `BrowserVersion`, including managed/external serde fixtures.
- Made `TargetLifecycle` exhaustive with `Suspended` restoration edges and terminal-state closure; added the nine browser failure codes with stable display/serde names, safe guidance, and exhaustive adapter-kind mapping.
- Atomically updated the transitional root connector and discovery-only `doctor`; `UnavailableBrowserConnector` returns no installations and stable `browser_not_found` recovery without being called by `doctor`.
- Verification passed independently: `cargo fmt --all -- --check`, `cargo check --workspace --all-targets --locked`, `cargo test --workspace --all-targets --locked` (48 tests), and `cargo clippy --workspace --all-targets --locked -- -D warnings`.

## Acceptance criteria

- [x] Parent Unit 1 signatures and complete installation/product/version/profile types are implemented, validated, and exported; no provisional string-only domain support remains.
- [x] `RecordingSession` serializes and validates `ProfileRef`, with managed and external fixtures, and uses the complete runtime `BrowserVersion` without conflating `BrowserSessionState` and `SessionLifecycle`.
- [x] The lifecycle registry exhaustively enforces: `Discovered -> Attached | Suspended | Closed | Failed`; `Attached -> Recording | Hidden | Suspended | Closed | Failed`; `Recording -> Hidden | Suspended | Closed | Failed`; `Hidden -> Recording | Suspended | Closed | Failed`; `Suspended -> Discovered | Attached | Recording | Hidden | Closed | Failed`; terminal states have no exits. Every unlisted pair is rejected by tests.
- [x] Stable variant registries/serde names, invalid values, duplicate capability entries, managed/attach stop outcomes, session failure events, and event-stream closure have exhaustive tests.
- [x] Adapter errors map to safe structured core errors with retry/recovery guidance; the registry, serde/display, and exhaustive mapping tests cover all nine codes: `browser_not_found`, `browser_launch_failed`, `browser_process_terminated`, `browser_compatibility_failed`, `profile_in_use`, `target_failed`, `reconnect_exhausted`, `cancelled`, and `shutdown_incomplete`. Source/debug strings cannot serialize.
- [x] `UnavailableBrowserConnector` implements the changed traits with empty `installations()` and stable `browser_not_found` from `connect()`; `doctor` calls `installations()` exactly once and never `connect()`.
- [x] The runtime smoke is green in the transitional state: exit 1 with `error[browser_not_found]` and recovery, with no provisional `unsupported`/`browser transport is not available` text.
- [x] `krometrail-core` remains free of cdpkit, CDP, WebSocket, Tokio, URL-parser, filesystem adapter, and process types. `PathBuf` is permitted only as the validated installation executable value.
- [x] Workspace check/tests remain green after this story lands independently; no production connector from later stories is required to compile the changed contracts.
- [x] No capture or screencast contract is added.

## Review (2026-07-13)

**Verdict:** Approve

**Blockers:** none
**Important:** none
**Nits:** none

**Notes:** Fast-lane contract review reran 48 workspace tests and denied-warning clippy; verified ProfileRef, complete browser identities, object-safe ports, exhaustive lifecycle/errors/capabilities, compile-real transitional doctor behavior, and no infrastructure/capture leakage. Verdict: Approve - story verified by implement; fast-lane advance.
