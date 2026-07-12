---
id: epic-rust-cdp-capture-foundation-rust-runtime-contracts-composition-root
kind: story
stage: review
tags: [browser, infra]
parent: epic-rust-cdp-capture-foundation-rust-runtime-contracts
depends_on: [epic-rust-cdp-capture-foundation-rust-runtime-contracts-core-ports]
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Wire the Rust composition root

## Scope

Implement the parent feature's Unit 4. Make the root `krometrail` package the sole composition root, inject all core ports explicitly, own the async runtime outside core, and establish a truthful executable contract before deleting the legacy runtime.

This story does not select a production CDP adapter or claim browser capture works. `--version` and `--help` work; unavailable operations fail loudly and non-zero.

## Implementation requirements

- Root-only code imports and assembles infrastructure crates.
- `RuntimeDependencies` contains clock, wall clock, IDs, browser, recording, and timeline ports.
- No fake adapter is selected for a normal product operation.
- `main` maps structured failures to concise stderr and stable non-zero exit.
- Help contains no DAP/TypeScript commands.
- Add executable smoke tests for version/help/unavailable behavior.

## Implementation notes

- Files changed: `Cargo.toml`, `Cargo.lock`, `src/main.rs`, `src/app.rs`, `src/cli.rs`, `tests/rust-runtime-smoke.rs`.
- Tests added: executable integration smoke tests for `--version`, `--help`, and the unavailable `doctor` operation.
- Composition: `RuntimeDependencies` injects every core port; the root owns Tokio execution, process clocks, process-local IDs, and explicit unavailable adapters until production infrastructure exists. The unavailable adapters return structured `Unsupported` failures and never claim success.
- CLI: the only product subcommand is `doctor`; help and version are handled by Clap before runtime construction and contain no DAP or TypeScript surface.
- Discrepancies from design: none. The root uses explicit unavailable adapters rather than adding placeholder constructors to infrastructure crates because no production adapters exist yet.
- Dispatch: direct local reads only, per caller instruction; no questions or subagents used.
- Adjacent issues parked: none.
- Dependency verification: `epic-rust-cdp-capture-foundation-rust-runtime-contracts-core-ports` confirmed `stage: done` with `.work/bin/work-view --stage done --paths` before implementation.
- Pre-teardown Rust gate: green — `cargo fmt --all --check`, `cargo check --workspace --all-targets`, `cargo test --workspace --all-targets` (29 passed), and `cargo clippy --workspace --all-targets -- -D warnings` all passed.
- Executable verification: `cargo run -- --version` exits 0 with `krometrail 0.2.20`; `cargo run -- --help` exits 0 with the truthful `doctor`-only surface; `cargo run -- doctor` exits 1 with `error[unsupported]` and a recovery line.

## Acceptance criteria

- [x] `cargo run -- --version` and `cargo run -- --help` exit zero and are truthful.
- [x] An unavailable browser operation fails explicitly rather than faking success or launching Bun.
- [x] Root composition preserves the architecture dependency direction.
- [x] `cargo fmt --all --check`, check, test, and clippy with denied warnings pass across the workspace.
- [x] Record the green pre-teardown gate in this story's implementation notes.
