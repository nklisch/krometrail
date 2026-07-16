---
id: epic-rust-cdp-capture-foundation-rust-runtime-contracts-core-invariant-boundaries
kind: story
stage: done
tags: [bug, tests, infra]
parent: epic-rust-cdp-capture-foundation-rust-runtime-contracts
depends_on: []
release_binding: 1.0.0
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Enforce core invariants at every construction boundary

## Scope

Close the deep-review blocker that allows invariant-bearing domain aggregates to be constructed or deserialized without their validated constructors. Protect ranges, capture statistics, sessions, frames, gaps, and timeline observations at both Rust and Serde boundaries while preserving ergonomic read access.

## Requirements

- Make invariant-bearing fields private and expose intentional getters and validated mutation APIs.
- Add validated constructors for frame metadata and capture statistics.
- Route deserialization through validation with custom implementations or validated wire representations.
- Implement meaningful frame validation or remove the misleading no-op validation path.
- Preserve stable serialized field names where the public contract already exists.
- Add malformed-Serde tests for every protected invariant and compile/runtime evidence that direct invalid construction is unavailable.

## Acceptance criteria

- [x] Invalid ranges, statistics, ended sessions, gap details, frames, and observation payload-kind pairs cannot enter the domain through direct construction or deserialization.
- [x] Existing valid serialization round trips remain compatible.
- [x] The complete Rust quality gate passes.

## Review origin

Filed from the GPT-5.6 Sol Phase 2 adversarial feature review after GLM 5.2 completeness review found the feature otherwise complete.

## Implementation notes

- Files changed: `crates/krometrail-core/src/validation.rs`, `time.rs`, `browser/target.rs`, `recording/session.rs`, `recording/frame.rs`, `recording/gap.rs`, `timeline/observation.rs`, `ports/mod.rs`, and `lib.rs`.
- Tests added: malformed-wire rejection and valid serde round trips for ranges, statistics, sessions, dimensions/scale factors/frames, gaps, and timeline observations; atomic statistics mutation and meaningful frame/observation timestamp validation.
- Invariant boundaries: invariant-bearing aggregate fields are private; public constructors/getters and validated mutation (`CaptureStatistics::update`, `RecordingSession::set_statistics`/`transition`) are the only construction/update paths. A shared `deserialize_validated` adapter routes each public wire representation through its domain constructor/validator while preserving existing serialized field names.
- Discrepancies from design: frame validation now rejects normalized session timestamps later than observed timestamps; this is the minimum meaningful clock-order invariant consistent with the three-clock model. Supporting validated value objects (`BrowserVersion`, `ProfileIdentity`, `PageTarget`, `DiskBudgetBytes`, `PixelDimensions`, and `DeviceScaleFactor`) also received serde validation because protected aggregates contain them.
- Compatibility: valid serialized shapes and field names remain unchanged; `CapturedFrame` construction now uses its validated constructor and read access uses getters.
- Adjacent issues parked: none.
- Verification: `cargo fmt --all --check`, `cargo check --workspace --all-targets`, `cargo test --workspace --all-targets` (37 passed), and `cargo clippy --workspace --all-targets -- -D warnings` all pass. Direct invalid aggregate field construction is unavailable because the invariant-bearing fields are private; malformed serde tests provide runtime boundary evidence.

## Review (2026-07-12)

**Verdict**: Approve

**Blockers**: none
**Important**: none
**Nits**: none

**Notes**: Fast-lane review follow-up. The implementation records 37 workspace tests; the orchestrator independently reran formatting, all 34 core tests, and locked workspace clippy successfully. The feature-review blocker is addressed at direct-construction and Serde boundaries. Verdict: Approve - story verified by implement; fast-lane advance.
