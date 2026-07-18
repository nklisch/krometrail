---
id: gate-cruft-remove-unreachable-range-invalidation
kind: story
stage: done
tags: [cleanup]
parent: null
depends_on: []
release_binding: 1.1.0
gate_origin: cruft
created: 2026-07-18
updated: 2026-07-18
---

# Remove unreachable resolved-range session invalidation

## Confidence
Medium

## Category
dead function

## Location
`crates/krometrail-core/src/range_handle.rs:21`

## Evidence

`ResolvedRangeHandles::invalidate_session` has no production caller or public session-deletion callback; only its implementation and tests reference it. Retained evidence is independently revalidated on handle resolution.

## Removal

Remove the trait method, implementations, invalidation-only tests, and `StoredRange.budget_bytes` state used solely for that path. Store the resolved range directly until a real lifecycle callback exists.

## Acceptance criteria

- `ResolvedRangeHandles` has no session-invalidation method and every implementation/fake compiles without it.
- The process authority stores `ResolvedRange` directly; no per-entry `budget_bytes` field or invalidation-only test remains.
- Admission still enforces both entry count and aggregate serialized-byte budget without evicting accepted handles.
- Handle resolution still revalidates exact retained frame order, scope, and availability.

## Implementation plan

- Remove the unused trait method and fake implementations.
- Replace `StoredRange` with direct range storage while retaining aggregate admission accounting.
- Rewrite budget coverage around admission behavior and remove invalidation-only assertions.

## Implementation notes

- Removed the unreachable lifecycle method from the core port and all MCP test authorities.
- Replaced `StoredRange` with direct `ResolvedRange` storage while keeping aggregate serialized-byte admission accounting.
- Kept resolution-time retained-frame revalidation intact and refocused the tests on capacity, budget, scope, exact order, availability, and persistence failures.

## Validation

- `cargo test -p krometrail range_handles::tests --locked -- --nocapture`
- `cargo test --workspace --all-targets --locked`
- `cargo check --workspace --all-targets --locked`
- `cargo clippy --workspace --all-targets --locked -- -D warnings`

## Review

- Verdict: pass; every acceptance criterion is covered by the direct-storage diff and focused range tests.
- Effective implementation size: small. Effective review weight: standard bounded inline standalone-story review.
- No review findings remained after verification.
