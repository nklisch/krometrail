---
id: bug-restore-rust-1-85-contract
kind: story
stage: implementing
tags: [bug, infra, testing]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Restore the Rust 1.85 Contract

## Brief

The MCP SDK qualification exposed that the current workspace no longer satisfies its declared Rust 1.85 contract before MCP is added. The committed lock selects ICU/idna packages declaring Rust 1.86, and current source uses let-chain syntax rejected by Rust 1.85.

Restore `cargo +1.85.0 check --workspace --all-targets --locked` without raising the workspace MSRV. Keep dependency and syntax corrections behavior-preserving, preserve the normal stable-toolchain gates, and leave SDK selection to the blocked MCP checkpoint once the baseline is truthful again.

## Simplification opportunity

Replace unstable let chains with direct nested conditions rather than introducing compatibility helpers or conditional compilation. Select one Rust-1.85-compatible transitive dependency set in the committed lock instead of adding direct ICU dependencies.

## Acceptance evidence

- The committed lock contains a Rust-1.85-compatible `idna_adapter`/ICU family selected through existing direct dependency constraints; no new direct ICU dependency or workspace MSRV change is introduced.
- Every current let-chain rejected by Rust 1.85 is rewritten without changing validation, lifecycle, capture, or test behavior.
- `PATH=/home/nathan/.cargo/bin:$PATH cargo +1.85.0 check --workspace --all-targets --locked` passes.
- `cargo fmt --all -- --check`, locked workspace check/test, and Clippy with warnings denied pass on the normal toolchain.

## Origin

Promoted from `idea-restore-rust-1-85-contract` after MCP checkpoint qualification reproduced the declared-MSRV failure. This is a current implemented-work blocker, not future MCP scope.
