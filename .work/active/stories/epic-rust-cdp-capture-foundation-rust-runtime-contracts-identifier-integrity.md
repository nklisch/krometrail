---
id: epic-rust-cdp-capture-foundation-rust-runtime-contracts-identifier-integrity
kind: story
stage: implementing
tags: [bug, infra, tests]
parent: epic-rust-cdp-capture-foundation-rust-runtime-contracts
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Align and harden identifier contracts

## Scope

Resolve identifier drift and collision risks found by the second adversarial review. The authoritative architecture must list identifiers implemented by the foundation and distinguish identifiers intentionally deferred to later browser-control work. Runtime ID allocation must remain collision-resistant across process restarts, and typed-ID tests must derive from the same declaration as production types.

## Requirements

- Roll `docs/ARCHITECTURE.md` forward to include implemented `GapId` and `NavigationId` and clearly mark future `SnapshotGeneration` and `NodeReference` ownership.
- Replace restart-repeating process IDs with collision-resistant UUID generation suitable for persisted session/frame identities.
- Generate exhaustive typed-ID round-trip coverage from the production typed-ID declaration rather than a second list.
- Add tests that independently constructed ID sources do not repeat deterministic sequences.

## Acceptance criteria

- [ ] Foundation docs and implemented identifier vocabulary agree.
- [ ] New processes do not restart an identical ID sequence.
- [ ] Adding a typed ID automatically brings it under exhaustive contract coverage.
- [ ] The complete Rust quality gate passes.

## Review origin

Filed from the second GPT-5.6 Sol adversarial feature review.
