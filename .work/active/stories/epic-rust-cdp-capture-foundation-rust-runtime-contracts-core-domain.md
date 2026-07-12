---
id: epic-rust-cdp-capture-foundation-rust-runtime-contracts-core-domain
kind: story
stage: review
tags: [browser, infra]
parent: epic-rust-cdp-capture-foundation-rust-runtime-contracts
depends_on: [epic-rust-cdp-capture-foundation-rust-runtime-contracts-workspace-skeleton]
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Implement core capture domain contracts

## Scope

Implement the parent feature's Unit 2 exactly in `krometrail-core`: opaque typed IDs; source, observed, and normalized session time; session/target identities; frame metadata and encoded payload; explicit capture gaps; validated lifecycle transitions; timeline observations; and the capability registry.

Stable domain invariants land now. Chrome timestamp interpretation, transport envelopes, storage locations, MCP schemas, and visual-analysis behavior remain out of scope.

## Implementation requirements

- Use private UUID-backed ID newtypes generated from one implementation registry/macro.
- Use checked integer nanoseconds; expose no implicit arithmetic between unrelated clocks.
- Fail fast on time underflow, invalid ranges/dimensions/scale, empty payloads, invalid transitions, and observation payload-kind mismatch.
- Model every known loss as `CaptureGap`; never imply continuity across missing capture.
- Define capability names/defaults/dependencies/subsystems once; `page-state` and `framework-state` are unavailable.
- Keep core free of Tokio and infrastructure dependencies.

## Acceptance criteria

- [ ] Every parent Unit 2 public signature and invariant is implemented or an implementation note records a strictly equivalent safer signature.
- [ ] IDs cannot be interchanged at compile time and round-trip through display/parse/serde.
- [ ] Tests cover time, range, frame, gap, lifecycle, timeline, and capability success/error paths.
- [ ] Frame metadata preserves source, observed, and session time separately.
- [ ] `cargo test -p krometrail-core` and workspace clippy pass.

## Implementation notes

- Files changed: `crates/krometrail-core/src/lib.rs`, `error.rs`, `ids.rs`, `time.rs`, `browser/mod.rs`, `browser/target.rs`, `recording/mod.rs`, `recording/session.rs`, `recording/frame.rs`, `recording/gap.rs`, `lifecycle.rs`, `timeline/mod.rs`, `timeline/observation.rs`, `capabilities/mod.rs`; core dev dependency wiring in `crates/krometrail-core/Cargo.toml`, `Cargo.toml`, and `Cargo.lock`.
- Tests added: 18 colocated core tests covering typed-ID display/parse/serde, time normalization/ranges, target/profile/browser validation, budgets/statistics/session transitions, frame metadata and payloads, explicit gaps, lifecycle tables, timeline payload matching, and capability registry/selection paths.
- Discrepancies from design: Unit 2 signatures use a shared `Result<T, E = KrometrailError>` alias and a deliberately small domain-owned `KrometrailError` (`ErrorCode` plus message). The next ports story extends this same type with structured context/retry/recovery fields rather than introducing a second error vocabulary. Capability enum variants, `ALL`, and definitions are generated from one registry macro; `PageState` and `FrameworkState` remain unavailable. Source sequence zero is accepted because the domain does not assign CDP sequence interpretation before the transport gate.
- Adjacent issues parked: none.
- Verification: dependency `epic-rust-cdp-capture-foundation-rust-runtime-contracts-workspace-skeleton` was confirmed `stage: done`. `cargo fmt --all --check`, `cargo check --workspace --all-targets`, `cargo test --workspace --all-targets`, and `cargo clippy --workspace --all-targets -- -D warnings` all pass; the final core test run passed 18 tests.
