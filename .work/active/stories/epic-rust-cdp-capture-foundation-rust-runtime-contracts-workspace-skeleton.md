---
id: epic-rust-cdp-capture-foundation-rust-runtime-contracts-workspace-skeleton
kind: story
stage: implementing
tags: [browser, infra]
parent: epic-rust-cdp-capture-foundation-rust-runtime-contracts
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Establish the Rust workspace skeleton

## Scope

Create the Rust 2024 root package and exactly five member crates specified by `docs/ARCHITECTURE.md`: `krometrail-core`, `krometrail-cdp`, `krometrail-store`, `krometrail-mcp`, and `temporal-vision`. Centralize package metadata and third-party dependency declarations in the root workspace, track the application lockfile, and make every crate compile before any legacy runtime removal.

Do not implement domain behavior, select a CDP transport, or delete TypeScript in this story. The parent feature's Unit 1 is the exact file/signature design.

## Implementation requirements

- Root `Cargo.toml` is both `[package] name = "krometrail"` and `[workspace]` owner, edition 2024, minimum Rust 1.85.
- Member manifests inherit workspace package metadata and dependencies with `{ workspace = true }`.
- Add `rust-toolchain.toml`; commit `Cargo.lock`.
- `krometrail-core` imports no infrastructure crate or async runtime.
- `temporal-vision` imports no Krometrail crate.
- Skeleton public modules contain no speculative CDP/storage/MCP/visual APIs.

## Acceptance criteria

- [ ] `cargo metadata --no-deps` reports the root and exactly five members.
- [ ] `cargo check --workspace --all-targets` passes before teardown.
- [ ] Dependency direction matches the parent design.
- [ ] Cargo owns the product package/version; no Bun fallback is introduced.
