---
id: epic-rust-cdp-capture-foundation-rust-runtime-contracts-composition-root
kind: story
stage: implementing
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

## Acceptance criteria

- [ ] `cargo run -- --version` and `cargo run -- --help` exit zero and are truthful.
- [ ] An unavailable browser operation fails explicitly rather than faking success or launching Bun.
- [ ] Root composition preserves the architecture dependency direction.
- [ ] `cargo fmt --all --check`, check, test, and clippy with denied warnings pass across the workspace.
- [ ] Record the green pre-teardown gate in this story's implementation notes.
