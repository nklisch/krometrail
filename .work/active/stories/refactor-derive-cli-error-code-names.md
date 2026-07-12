---
id: refactor-derive-cli-error-code-names
kind: story
stage: done
tags: [refactor, infra]
parent: null
depends_on: []
release_binding: null
gate_origin: refactor-design
created: 2026-07-12
updated: 2026-07-12
---

# Derive CLI error-code names from the core registry

## Brief

`crates/krometrail-core/src/error.rs:14-27` already declares every `ErrorCode` and its stable boundary name through `define_stable_enum!`, which generates `ErrorCode::as_str()`. `src/main.rs:60-73` independently repeats the complete variant-to-name mapping in `error_code_name`.

Replace the root CLI's duplicate `error_code_name` registry with the core-owned `ErrorCode::as_str()` value. Keep retry-advice handling out of scope: unlike error codes, it does not currently have a core stable-name accessor, and expanding this surgical refactor into a public enum/macro redesign would be churn.

**Source lens**: missing abstraction / single source of truth

**Rationale**: removes a complete duplicate variant mapping at the process boundary so error serialization and CLI rendering derive from the same declaration.

**Black-box classification**: pure refactor. The CLI must emit byte-for-byte identical stable error-code names; no command, exit status, error semantics, or public serialized representation changes.

## Acceptance criteria

- [x] `src/main.rs` renders error codes through `ErrorCode::as_str()` and no longer contains `error_code_name` or a second exhaustive `ErrorCode` mapping.
- [x] `cargo fmt --all -- --check` passes.
- [x] `cargo test --workspace --all-targets --locked` passes, including `tests/rust-runtime-smoke.rs`'s exact `error[unsupported]` contract.
- [x] `cargo clippy --workspace --all-targets --locked -- -D warnings` passes.

## Implementation notes
- Files changed: `src/main.rs`; this story file.
- Tests added: none; existing workspace coverage verifies the unchanged CLI error spelling.
- Discrepancies from design: none.
- Adjacent issues parked: none.
- The duplicate root mapping was removed; `report_error` now calls the core-owned `ErrorCode::as_str()` accessor. Verification was direct-read only as requested.

## Risk and rollback

**Risk**: Low. Both paths currently return the same static names, and the smoke test verifies the externally visible unsupported-error spelling.

**Rollback**: Revert the implementation commit to restore the local match helper.

## Discovery notes

- Scope: the first five-story Rust foundation batch surfaces only — root `Cargo.toml` and workspace topology, `src/`, all of `crates/krometrail-core/`, skeleton crate manifests/libs, and `tests/rust-runtime-smoke.rs`; deleted legacy TypeScript paths and `.pi/` were excluded.
- Dispatch: direct-read only, as explicitly requested; no questions or subagents.
- Value: medium — small implementation, but it removes a full growing-variant duplicate at an external reporting boundary.

## Review (2026-07-12)

**Verdict**: Approve

**Blockers**: none
**Important**: none
**Nits**: none

**Notes**: Fast-lane story review. The orchestrator independently reran the 41-test locked Rust gate and confirmed the exact CLI error contract remains green. Verdict: Approve - story verified by implement; fast-lane advance.
