---
id: gate-cruft-remove-unreachable-range-invalidation
kind: story
stage: drafting
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
