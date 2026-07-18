---
id: resilient-compact-temporal-bundles-guide-captured-bounds
kind: story
stage: done
tags: [agent-ux, visual]
parent: resilient-compact-temporal-bundles
depends_on: []
release_binding: 1.0.4
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Guide requests to captured bounds

## Checkpoint

Never-captured range failures expose exact captured bounds, concrete adjusted-request recovery, and retry-after-recovery advice without silently changing `AllowPartial` semantics.

## Acceptance evidence

- Future-edge requests carry requested context plus captured start/end values.
- Existing eviction-edge behavior remains unchanged.

## Implementation notes

- Execution capability: frontier implementation; the stable range/error contract and retention edge semantics warranted exact regression coverage.
- Review weight: standard (project default; child checkpoint closes directly without independent review).
- Files changed: `crates/krometrail-core/src/timeline/range.rs`, `crates/krometrail-store/tests/temporal_queries.rs`.
- Tests added/removed: strengthened the temporal-query retention regression to assert the original requested context, exact `0..800000000` captured bounds recovery, and `after_recovery` advice for an `AllowPartial` future-edge request; no tests removed.
- Simplification: enriched the existing range authority and error value; no bundle-specific preflight or silent clamping was introduced.
- Discrepancies from design: none.
- Adjacent issues parked: none.

## Verification

- `cargo test -p krometrail-store --test temporal_queries --locked` — passed, 3 tests.
- `cargo test -p krometrail-core timeline::range --lib --locked` — passed, 7 focused tests.
- `cargo fmt --all -- --check` — passed.
- `git diff --check` — passed.
