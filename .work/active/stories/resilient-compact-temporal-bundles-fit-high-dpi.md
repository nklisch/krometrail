---
id: resilient-compact-temporal-bundles-fit-high-dpi
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

# Fit high-DPI default bundles

## Checkpoint

Default `FitLimits` admits the reproduced 53-frame 2400×1410 interval while preserving all frames and the fixed combined request cap; larger work fails with actionable progressive-evidence recovery.

## Acceptance evidence

- Deterministic scale and cache/manifest identity are verified.
- Peak reservation remains within the combined scheduler budget.

## Implementation notes

- Execution capability: frontier implementation; the memory-lifecycle proof spans planning, normalization, scheduling, single-flight publication, and bundle degradation.
- Review weight: standard (project default; child checkpoint closes directly without independent review).
- Files changed: `src/artifacts/epoch.rs`, `src/artifacts/generators.rs`, `src/artifacts/scheduler.rs`, `src/artifacts/service.rs`, `src/artifacts/service_tests.rs`, `src/artifacts/single_flight.rs`, `src/artifacts/tests.rs`, `src/debug_bundle/error.rs`, `src/debug_bundle/service.rs`, `src/debug_bundle/tests.rs`.
- Tests added/removed: added the exact 53-frame 2400×1410 planning regression, deterministic canonical-parameter check, fixed-cap reservation proof, 55-frame pre-allocation rejection, and bundle recovery assertion; no tests removed.
- Simplification: published/cached results now shed encoded payload bytes at the in-flight coordination boundary and retain only response metadata, making the scheduler's peak-output reservation match the sequential generator lifecycle.
- Discrepancies from design: the designed bounded output reservation is implemented as the maximum per-generator-group output ceiling rather than the sum of every request output ceiling, because generator groups execute and publish sequentially; retaining only metadata after publication makes that the truthful peak-memory bound.
- Adjacent issues parked: none.

## Verification

- Red regression: the 53-frame case failed under the prior 512 MiB decoded ceiling with `decoded sequence bytes exceed the configured limit`.
- `cargo test --bin krometrail artifacts::service_tests::default_limits_fit_reproduced_high_dpi_sequence_with_fixed_combined_budget --locked` — passed; exact decoded bytes `717408000`, scale `down(2)`, peak reservation `1053544864`, combined cap `1073741824`.
- `cargo test --bin krometrail artifacts::service_tests::fit_limits_materializes_smallest_exact_divisor_in_manifest --locked` — passed.
- `cargo test --bin krometrail debug_bundle::tests::bundle_resource_limits_recommend_shorter_or_progressive_evidence --locked` — passed.
- `cargo test --bin krometrail artifacts:: --locked` — passed, 24 tests; 2 manual performance qualifications ignored by declaration.
