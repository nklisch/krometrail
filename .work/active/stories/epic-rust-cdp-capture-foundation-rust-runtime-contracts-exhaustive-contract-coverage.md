---
id: epic-rust-cdp-capture-foundation-rust-runtime-contracts-exhaustive-contract-coverage
kind: story
stage: done
tags: [tests, infra]
parent: epic-rust-cdp-capture-foundation-rust-runtime-contracts
depends_on: [epic-rust-cdp-capture-foundation-rust-runtime-contracts-core-invariant-boundaries]
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Complete exhaustive core contract coverage

## Scope

Fulfill the feature design's exhaustive contract-test requirement after invariant-bearing types are sealed. Test every lifecycle transition pair and every stable gap-reason and error-code variant from authoritative variant sets rather than sampling representative values.

## Requirements

- Define authoritative iterable variant sets where the enums do not already expose one.
- Table-test every valid and invalid session and target lifecycle transition.
- Round-trip every `CaptureGapReason` and `ErrorCode` through Serde and verify stable names.
- Keep the variant list single-sourced with the production enum/registry so new variants cannot silently escape coverage.

## Acceptance criteria

- [x] Every stable lifecycle pair, gap reason, and error code is covered.
- [x] Adding a variant requires updating one authoritative declaration rather than parallel test lists.
- [x] The complete Rust quality gate passes.

## Review origin

Filed from the GPT-5.6 Sol Phase 2 adversarial feature review.

## Implementation notes

- Files changed: `crates/krometrail-core/src/lib.rs`, `crates/krometrail-core/src/lifecycle.rs`, `crates/krometrail-core/src/recording/gap.rs`, `crates/krometrail-core/src/error.rs`.
- Authoritative registries: a shared stable-enum declaration generates `ALL` and stable Serde names for `CaptureGapReason` and `ErrorCode`; the lifecycle declaration generates `ALL`, `TRANSITIONS`, and names for both lifecycle types.
- Tests added: exhaustive Cartesian-pair tests for all session and target lifecycle states, registry-closure checks, and Serde/name round trips for every gap reason and error code (including structured errors).
- Dependency readiness: verified with `.work/bin/work-view --stage done --paths`; `epic-rust-cdp-capture-foundation-rust-runtime-contracts-core-invariant-boundaries` was present in the done set.
- Exact quality gate: `cargo fmt --all -- --check`; `cargo check --workspace --all-targets --locked`; `cargo test --workspace --all-targets --locked`; `cargo clippy --workspace --all-targets --locked -- -D warnings` — all passed.
- Discrepancies from design: none.
- Adjacent issues parked: none.

## Review (2026-07-12)

**Verdict**: Approve

**Blockers**: none
**Important**: none
**Nits**: none

**Notes**: Fast-lane review follow-up. The orchestrator independently reran formatting, all 35 core tests, and locked workspace clippy successfully. Acceptance boxes were aligned with the already-recorded green implementation evidence. Verdict: Approve - story verified by implement; fast-lane advance.
