---
id: epic-rust-cdp-capture-foundation-rust-runtime-contracts
kind: feature
stage: drafting
tags: [browser, infra]
parent: epic-rust-cdp-capture-foundation
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Rust Capture Runtime Foundation

## Brief

Establish the Rust 2024 workspace, composition root, and the core browser-session and recording contracts that the CDP adapter and later storage, control, and temporal capabilities consume. The core owns domain identities, session timing, frame and capture-gap vocabulary, lifecycle states, errors, and infrastructure ports; infrastructure crates depend inward so transport findings can revise port details without reversing dependency direction.

Make Rust the only buildable Krometrail runtime in the same cutover. Remove the TypeScript/DAP implementation and align package entry points, development commands, CI, and current contributor documentation with the Rust workspace only after confirming the remote `v0.2.20` tag preserves the legacy implementation. This feature establishes contracts and runtime ownership, but does not select a production CDP transport, launch Chrome, or ingest real screencast frames.

## Epic context

- Parent epic: `epic-rust-cdp-capture-foundation`
- Position in epic: foundation feature — every other child consumes its workspace and core contracts
- Design decisions inherited: immediate single-runtime Rust cutover; the legacy implementation remains available at remote tag `v0.2.20`

## Foundation references

- `docs/VISION.md` — Local-First Operation and Product Boundaries
- `docs/SPEC.md` — Sessions and Targets, Continuous Visual Capture, and Errors and Degraded Operation
- `docs/ARCHITECTURE.md` — Rust Workspace, Domain Model, Time Model, Dependency Direction, and Technology Decisions
