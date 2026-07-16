---
id: epic-prove-temporal-advantage-deterministic-scoring-and-artifact-conditions-deterministic-ci-qualification-and-non-claims
kind: story
stage: done
tags: [testing, visual, storage, infra]
parent: epic-prove-temporal-advantage-deterministic-scoring-and-artifact-conditions
depends_on: [epic-prove-temporal-advantage-deterministic-scoring-and-artifact-conditions-canonical-result-records-and-traceability]
release_binding: 1.0.0
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Qualify deterministic scoring without broadening claims

## Checkpoint

Wire the final browser-free CI qualification around the package, scorer, thresholds, and canonical
result records. Use deterministic source records, an injected fake monotonic clock, existing
artifact/provenance/cache descriptors, and existing `RecordingStore` progressive/retention seams
where persistence truth is needed. Do not create a fake Chrome pass or a benchmark-specific store,
renderer, cache, or CLI.

## Qualification surface

Add focused test-only support under `crates/temporal-evaluation/tests/support/` and
`crates/temporal-evaluation/tests/qualification.rs`, plus a thin existing-store seam test at
`crates/krometrail-store/tests/temporal_evaluation_qualification.rs` if cross-crate validation is
required. The suite proves:

- fake-clock call count, host wall clock, filesystem order, and parallel completion order do not
  alter interval/package/scorer/result bytes;
- all A–E packages share one source interval and exact evidence budgets;
- B's uniform slots differ from a deliberately change-aware C authority snapshot without a new
  image algorithm;
- one-field source, order, clock, gap, retention, artifact-output/manifest hash, cache key,
  source/parameter/epoch hash, adapter version, generator version, or cache-schema mutation
  invalidates the expected package/result;
- D preserves the existing bundle range/outcomes and E adds only bounded progressive records;
- gap, eviction, corruption, unavailable retrieval, partial bundle, and mixed skipped-row fixtures
  remain explicit non-passing states; and
- clean checkout generation of definition/schema/run-manifest/result-schema/result-sample files
  is byte-identical, with no generated VitePress docs or ignored run evidence committed.

Synthetic answers may equal hidden truth to exercise exact scoring and threshold arithmetic, but
all emitted qualification records are `evidence_layer=DeterministicCi`,
`thesis_eligibility=NotEligible`, and carry the complete fixed non-claim registry. They cannot
satisfy any live capture, platform, model interpretation, debugging, or product-thesis claim.

## Acceptance evidence

- [ ] Locked Rust fmt/check/test/clippy gates pass without Chrome, network, paid model, external model, or product CLI execution.
- [ ] Existing fake-clock/source/store seams prove deterministic ordering, gap/retention propagation, cache/version identity checks, and no stale or partial accepted claim.
- [ ] A skipped manifest is accepted only when every row is `Skipped` with row-level optional-unavailability failure; mixed rows fail.
- [ ] No test uses an artifact or measurement as hidden truth, infers loss from ordinals, or treats a synthetic result as a thesis pass.
- [ ] No performance stopwatch, model-quality assertion, new visual algorithm, new provenance format, or generic statistics framework is added.

## Ordering

This is the final implementation checkpoint. Child stories advance directly to `done` after green
verification; only the parent feature receives feature-level review.

## Implementation notes

- Execution capability: inline feature-owner implementation; the qualification write set is a
  browser-agnostic test support module, one integration suite, a narrow scorer failure mapping,
  and the versioned evidence README.
- Review weight: standard default; this child checkpoint advances directly to `done` and does not
  enter review.
- Files changed: `crates/temporal-evaluation/tests/support/mod.rs`,
  `crates/temporal-evaluation/tests/qualification.rs`,
  `crates/temporal-evaluation/src/scoring.rs`, and
  `docs/evidence/temporal-evaluation/v1/README.md`.
- Tests added: 9 qualification tests covering fake-clock/source ordering, A–E budgets and
  authority preservation, change-aware versus uniform selection, identity mutation rejection,
  non-passing evidence states, skipped-row closure, deterministic-CI non-claims, privacy, and
  clean generation of all six committed definition/manifest/result artifacts.
- Simplification: qualification uses existing package/artifact/cache identity contracts and
  existing store semantics rather than adding a fake browser, renderer, store, cache, provenance
  format, stopwatch, or CLI path. Corrupt evidence now retains its specific `CorruptSource`
  failure code and recovery instead of being flattened into retention loss.
- Discrepancies from design: no new cross-crate store test was necessary; the existing
  `RecordingStore` progressive, artifact-integrity, retention, and recovery suites already cover
  those seams, while this crate remains infrastructure-free and consumes only their bounded
  authority projections.
- Adjacent issues parked: none.
- Verification: Rust 1.85 locked `fmt`, full workspace `check`, full workspace `test` (705 passed,
  1 ignored), and full workspace `clippy -D warnings` passed. Generator identity checks reproduced
  definition, definition schema, run manifest, run-manifest schema, result sample, and result
  schema byte-for-byte without touching `docs/public/llms-full.txt` or tracked run output.
