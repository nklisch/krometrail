---
id: epic-rust-cdp-capture-foundation-rust-runtime-contracts-core-invariant-boundaries
kind: story
stage: implementing
tags: [bug, tests, infra]
parent: epic-rust-cdp-capture-foundation-rust-runtime-contracts
depends_on: []
release_binding: null
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

- [ ] Invalid ranges, statistics, ended sessions, gap details, frames, and observation payload-kind pairs cannot enter the domain through direct construction or deserialization.
- [ ] Existing valid serialization round trips remain compatible.
- [ ] The complete Rust quality gate passes.

## Review origin

Filed from the GPT-5.6 Sol Phase 2 adversarial feature review after GLM 5.2 completeness review found the feature otherwise complete.
