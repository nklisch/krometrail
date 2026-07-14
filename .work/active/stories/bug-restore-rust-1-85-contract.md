---
id: bug-restore-rust-1-85-contract
kind: story
stage: review
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

## Implementation notes

- Execution capability: highest, inherited from autopilot because the fix restores a workspace-wide compiler contract and dependency lock rather than one package-local syntax detail.
- Review weight: standard from the autopilot default; as a standalone story this receives the bounded inline review lane and no independent reviewer.
- Dependency correction: `idna_adapter` is locked to 1.1.0, removing the Rust-1.86-only ICU 2.2 family and selecting its Rust-1.85-compatible Unicode mapping dependencies. No direct dependency or workspace MSRV changed.
- Source correction: unstable let-chain expressions were rewritten as direct nested conditions in temporal provenance, browser batch/control/observation/retention validation, process cleanup, network tracking, and session-supervision test code. No compatibility helper, conditional compilation, or behavior change was introduced.
- Verification: `cargo fmt --all -- --check`; `PATH=/home/nathan/.cargo/bin:$PATH cargo +1.85.0 check --workspace --all-targets --locked`; normal locked workspace check/test; and workspace Clippy with `-D warnings` all passed.
- Adjacent SDK finding: official `rmcp` versions 0.12.0 through 2.2.0 do not satisfy the same Rust 1.85 probe under their usable feature sets; exact 0.11.0 is the newest probed official release that passes. SDK selection remains owned by the dependent MCP checkpoint.
