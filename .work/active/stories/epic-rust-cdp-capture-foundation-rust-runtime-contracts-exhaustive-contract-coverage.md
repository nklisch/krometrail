---
id: epic-rust-cdp-capture-foundation-rust-runtime-contracts-exhaustive-contract-coverage
kind: story
stage: implementing
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

- [ ] Every stable lifecycle pair, gap reason, and error code is covered.
- [ ] Adding a variant requires updating one authoritative declaration rather than parallel test lists.
- [ ] The complete Rust quality gate passes.

## Review origin

Filed from the GPT-5.6 Sol Phase 2 adversarial feature review.
