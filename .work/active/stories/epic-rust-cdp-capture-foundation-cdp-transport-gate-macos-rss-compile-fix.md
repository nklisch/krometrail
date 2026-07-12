---
id: epic-rust-cdp-capture-foundation-cdp-transport-gate-macos-rss-compile-fix
kind: story
stage: done
tags: [bug, browser, infra, testing]
parent: epic-rust-cdp-capture-foundation-cdp-transport-gate
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Compile the macOS RSS sampler

## Symptom

GitHub run [29198801356](https://github.com/nklisch/krometrail/actions/runs/29198801356) cannot compile the macOS cdpkit gate. `crates/krometrail-cdp/src/spike/chrome_harness.rs::process_rss` reports E0277 because `.parse::<u64>()?` attempts to use a `Result` residual in a function returning `Option<u64>`.

## Root cause

The macOS-only sampler parsed `ps` output inline with the `?` operator before converting the `Result` to `Option`; Linux compilation did not exercise that target-gated function, so the incompatible residual remained unnoticed until the macOS runner compiled it.

## Fix approach

Move decimal macOS RSS parsing and KiB-to-byte normalization into a target-neutral pure helper using `.ok()?`, then have the macOS sampler call that helper. Preserve checked KiB-to-byte conversion and return `None` for malformed, non-UTF-8, or overflowing values.

## Regression test

`crates/krometrail-cdp/src/spike/chrome_harness.rs` tests valid whitespace-delimited KiB input, malformed input, and overflow locally on Linux. A static source assertion verifies that the macOS sampler uses the target-neutral helper and cannot reintroduce the incompatible inline `.parse::<u64>()?` expression. The existing macOS CI gate remains the authoritative target compile check; no unsupported Linux cross-toolchain is installed or used.

## Acceptance criteria

- [x] macOS `process_rss` compiles by using the target-neutral parser helper.
- [x] Valid macOS RSS remains normalized from KiB to bytes with checked multiplication.
- [x] Linux default, spike, and cdpkit tests plus clippy pass without fabricated macOS execution or evidence.
- [x] The macOS evidence story records this run, root cause, fix story, and an exact-SHA manual rerun requirement.

## Implementation notes

- Execution capability: direct inline implementation; the bug is isolated to one spike sampler and its local regression coverage, with no ownership or sequencing uncertainty.
- Review weight: standard, from the project default; implementation stops at `stage: review` as explicitly requested.
- Files changed: `crates/krometrail-cdp/src/spike/chrome_harness.rs`, this story, and the macOS evidence story blocker.
- Test added: target-neutral RSS parsing/normalization cases and a static macOS sampler contract assertion in `chrome_harness.rs`; the new test was first run red before the helper existed.
- Verification: `cargo fmt --all --check`; default workspace tests/clippy; `cdp-spike` tests/clippy; and `cdp-spike-cdpkit` tests/clippy all pass with `--locked` and `-D warnings` where applicable. The macOS runner itself was not fabricated or executed locally.
- Adjacent issues parked: none.

## Review (2026-07-12)

**Verdict**: Approve

**Blockers**: none
**Important**: none
**Nits**: none

**Notes**: Fast-lane bug review. The orchestrator independently reran twelve cdpkit/spike targets and clippy and verified target-neutral parse/overflow coverage plus the macOS branch source guard. The conditional compile failure is fixed. Verdict: Approve - story verified by implement; fast-lane advance.
