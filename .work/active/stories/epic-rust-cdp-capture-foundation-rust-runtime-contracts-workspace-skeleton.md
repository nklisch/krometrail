---
id: epic-rust-cdp-capture-foundation-rust-runtime-contracts-workspace-skeleton
kind: story
stage: done
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

## Implementation notes

- Files changed: `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `src/main.rs`, and the five member manifests and `src/lib.rs` skeletons under `crates/`.
- Tests added: none; this unit establishes compiling package boundaries only.
- Discrepancies from design: added `workspace.package.version = "0.2.20"` so member manifests can inherit the required package version; added root workspace path declarations for the five internal crates so the composition-root dependency direction is explicit. The toolchain uses the stable channel with rustfmt and clippy components while manifests enforce the minimum Rust 1.85.
- Adjacent issues parked: none.
- Verification: `cargo metadata --no-deps --format-version 1` confirmed the root plus exactly five member crates; a metadata/source scan confirmed root-to-adapter edges, inward adapter dependencies, infrastructure-free core, and Krometrail-free `temporal-vision`. `cargo fmt --all --check`, `cargo check --workspace --all-targets`, `cargo test --workspace --all-targets`, and `cargo clippy --workspace --all-targets -- -D warnings` all passed.
- TypeScript runtime files were not modified or removed.

## Review (2026-07-12)

**Verdict**: Approve

**Blockers**: none
**Important**: none
**Nits**: none

**Notes**: Fast-lane story review. The implementation record reports the complete Cargo quality gate green, and the orchestrator independently reran formatting, check, workspace tests, and clippy successfully. Verdict: Approve - story verified by implement; fast-lane advance.
