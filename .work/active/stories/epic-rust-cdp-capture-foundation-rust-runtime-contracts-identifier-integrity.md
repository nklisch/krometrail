---
id: epic-rust-cdp-capture-foundation-rust-runtime-contracts-identifier-integrity
kind: story
stage: done
tags: [bug, infra, tests]
parent: epic-rust-cdp-capture-foundation-rust-runtime-contracts
depends_on: []
release_binding: 1.0.0
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Align and harden identifier contracts

## Scope

Resolve identifier drift and collision risks found by the second adversarial review. The authoritative architecture must list identifiers implemented by the foundation and distinguish identifiers intentionally deferred to later browser-control work. Runtime ID allocation must remain collision-resistant across process restarts, and typed-ID tests must derive from the same declaration as production types.

## Requirements

- Roll `docs/ARCHITECTURE.md` forward to include implemented `GapId` and `NavigationId` and clearly mark future `SnapshotGeneration` and `NodeReference` ownership.
- Replace restart-repeating process IDs with collision-resistant UUID generation suitable for persisted session/frame identities.
- Generate exhaustive typed-ID round-trip coverage from the production typed-ID declaration rather than a second list.
- Add tests that independently constructed ID sources do not repeat deterministic sequences.

## Acceptance criteria

- [x] Foundation docs and implemented identifier vocabulary agree.
- [x] New processes do not restart an identical ID sequence.
- [x] Adding a typed ID automatically brings it under exhaustive contract coverage.
- [x] The complete Rust quality gate passes.

## Review origin

Filed from the second GPT-5.6 Sol adversarial feature review.

## Implementation notes

- Files changed: `crates/krometrail-core/src/ids.rs`, `src/app.rs`, `Cargo.toml`, `Cargo.lock`, `docs/ARCHITECTURE.md`, and this story record.
- Tests added: macro-generated exhaustive typed-ID display/parse/Serde round trips; process ID tests for independent-source uniqueness and UUID v4 version/variant.
- Identifier allocation: replaced the restart-repeating atomic sequence with root-owned UUID v4 generation. Core remains randomness-free behind `IdSource`.
- Architecture: implemented IDs now list `GapId` and `NavigationId`; `SnapshotGeneration` and `NodeReference` are explicitly deferred to browser-control boundaries with ownership noted.
- Manifest/lock: enabled the required `uuid` v4 feature; Cargo.lock gained its required `getrandom` dependency.
- Verification: `cargo fmt --all -- --check`, `cargo check --workspace --all-targets --locked`, `cargo test --workspace --all-targets --locked` (40 passed), `cargo clippy --workspace --all-targets --locked -- -D warnings`, and `bun run docs:build` all passed. The configured Rust 1.85 toolchain was unavailable locally, so the separate MSRV command could not run.
- Dispatch: direct local reads and implementation only; no questions or subagents used. Distribution workflow/scripts and `.pi/` were not touched.
- Discrepancies from design: none.
- Adjacent issues parked: none.

## Review (2026-07-12)

**Verdict**: Approve

**Blockers**: none
**Important**: none
**Nits**: none

**Notes**: Fast-lane story review. The orchestrator independently reran formatting, all 40 workspace tests, and locked clippy, and spot-checked the macro-generated contract coverage and UUID v4 root adapter. The identifier findings are resolved. Verdict: Approve - story verified by implement; fast-lane advance.
