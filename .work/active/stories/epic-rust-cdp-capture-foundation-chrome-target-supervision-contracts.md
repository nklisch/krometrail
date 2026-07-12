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

Do not implement cdpkit, filesystem/process behavior, reconnect tasks, or screencast behavior.

## Required files

- `crates/krometrail-core/src/browser/mod.rs`
- `crates/krometrail-core/src/browser/target.rs`
- `crates/krometrail-core/src/browser/session.rs`
- `crates/krometrail-core/src/ports/browser.rs`
- `crates/krometrail-core/src/ports/mod.rs`
- `crates/krometrail-core/src/lifecycle.rs`
- `crates/krometrail-core/src/error.rs`
- `crates/krometrail-core/src/lib.rs`

## Acceptance criteria

- [ ] Parent Unit 1 signatures or strictly equivalent safer forms are implemented and exported.
- [ ] Stable variant registries/serde names, target suspension/restoration transitions, invalid values, duplicate capability entries, managed/attach stop outcomes, and event-stream closure have exhaustive tests.
- [ ] Adapter errors map to safe structured core errors with retry/recovery guidance; source/debug strings cannot serialize.
- [ ] `krometrail-core` remains free of cdpkit, CDP, WebSocket, Tokio, URL-parser, filesystem adapter, and process types.
- [ ] No capture or screencast contract is added.
