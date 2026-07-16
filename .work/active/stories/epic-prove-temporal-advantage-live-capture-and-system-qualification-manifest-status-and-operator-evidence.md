---
id: epic-prove-temporal-advantage-live-capture-and-system-qualification-manifest-status-and-operator-evidence
kind: story
stage: done
tags: [testing, infra, visual]
parent: epic-prove-temporal-advantage-live-capture-and-system-qualification
depends_on: [epic-prove-temporal-advantage-live-capture-and-system-qualification-retention-recovery-and-performance]
release_binding: 1.0.0
gate_origin: null
created: 2026-07-14
updated: 2026-07-15
---

# Finalize honest live qualification evidence

## Checkpoint

Assemble the complete canonical `RunManifest`, gate/status outcomes, non-claims, cleanup evidence,
and operator boundary after all scenario measurements exist. Make the output useful for later
scoring/evidence collection without pretending that system qualification is model or
cross-platform evidence.

## Exact implementation

Add the final assembler and output tests under `src/app/live_evaluation/report.rs` and
`crates/temporal-evaluation/tests/live_qualification.rs`. Build the manifest only from canonical
benchmark/matrix definitions, observed browser identity, production capture rows, source interval
and gap records, production retention/artifact/cache metadata, control outcomes, resource samples,
latency measurements, and cleanup observations. Enforce the registered `QualificationGateId::ALL`
order and the existing status/failure precedence. A complete below-threshold gate is `fail`;
missing source, unavailable resource, unresolved gap, retention loss, or unsupported environment is
`inconclusive`/`blocked`; optional Linux Chromium absence is `skipped`; only complete passing rows
and gates produce `pass`.

Write one canonical `run-manifest.json` atomically below
`target/temporal-evaluation/live/<browser-product>/<run-id>/`. Keep output filenames/paths out of
manifest identity except for safe relative artifact references already authorized by the existing
privacy contract. Do not write a second live result schema. Add the fixed non-claims for no model
call, no remote/paid service, default-DPI-only capture, no high-DPI conclusion, no cross-platform
conclusion, no model effectiveness, no temporal-advantage uplift, and no production-scale
stability claim. Explicitly record whether the run was code/harness qualification or operator-
authorized live capture.

Implement the final cleanup guard and output boundary: capture stops/flushed, browser target and
managed profile close, fixture server stops, lock releases, temporary store/staging data is
removed or recovery failure is recorded, then the manifest is atomically finalized. Reopening a
written manifest must prove all cleanup and privacy invariants. If finalization fails, preserve a
safe error report in the ignored output root and do not claim a passing result.

Add operator documentation to the existing temporal-evaluation README only for this capability's
actual test-only opt-in and blockers; do not add product CLI examples or edit generated
`docs/public/llms-full.txt`. State that a local `pass` qualifies only the declared configuration
and that the existing high-DPI evidence gap remains owned by platform evidence.

## Acceptance evidence

- [x] Canonical manifest bytes round-trip deterministically, include every registered gate exactly
      once, preserve source/observed/session clocks and gap IDs, and contain no private paths,
      payloads, model data, raw page text, credentials, or remote URLs.
- [x] Status tests cover no-opt-in/no-output, required browser blocked, optional Chromium skipped,
      incomplete/inconclusive, complete fail, complete pass, cleanup failure, and finalization
      failure.
- [x] A passing manifest cannot contain a missing measurement, unresolved gap, unavailable
      resource, wrong viewport/scale, failed control observation, failed retention/recovery gate,
      or cleanup failure.
- [x] Output is written only under ignored `target/temporal-evaluation/`; no product command or
      generated documentation is changed.
- [x] Operator instructions distinguish qualification from authorized live evidence and state
      high-DPI/model/cross-platform/advantage non-claims and required local blockers.
- [x] Final verification runs the standard Rust gates without launching Chrome; the live path is
      not invoked during design or ordinary tests.

## Ordering

This is the final sequential checkpoint. It depends on all concrete capture, control, retention,
recovery, resource, and latency measurements and makes the feature ready for integrated review.

## Implementation notes

- Execution capability: inline feature-owner implementation; this final checkpoint joined existing
  production measurement records without adding a second runtime, store, or result schema.
- Review weight: standard integrated parent-feature review; this child advanced directly to `done`.
- Files changed: `src/app/live_evaluation/report.rs`, `src/app/live_evaluation.rs`,
  `crates/temporal-evaluation/src/{lib.rs,manifest.rs,matrix.rs}`,
  `crates/temporal-evaluation/tests/{contracts.rs,live_qualification.rs}`,
  `docs/evidence/temporal-evaluation/v1/{README.md,run-manifest.schema.json}`.
- Tests added or strengthened: canonical assembler and gate-order/status-precedence coverage;
  required-browser blocked and optional-Chromium skipped assembly; missing measurement, gap,
  resource, control, retention/recovery, and cleanup non-passing invariants; atomic round-trip and
  safe finalization-error output; fixed non-claims and explicit code/harness versus authorized-live
  evidence mode.
- Simplification: removed the prior duplicate finalization implementation from the composition
  module and kept one report authority for status aggregation, cleanup finalization, safe output
  paths, and atomic publication.
- Discrepancies from design: the existing `RunManifest` was extended in place with a typed
  `qualification.evidence_mode` so the output explicitly distinguishes code/harness qualification
  from operator-authorized live capture; no parallel schema was introduced.
- Adjacent issues parked: none.

## Verification evidence

- Rust 1.85 locked fmt, default workspace check/test/clippy, qualification-support workspace
  check/test/clippy, and qualification-support CDP check/test/clippy all passed.
- Final default verification reported 704 passing tests and 1 ignored; qualification-support
  verification reported 717 passing tests and 2 ignored. No live variables were enabled, ignored
  live tests were not invoked, and Chrome was not launched.
- The actual operator-authorized live qualification has **not** been run; no production browser
  identity, live source frames, live gaps, resource samples, latency measurements, or live pass
  are claimed by this implementation.
- `.work/bin/work-view` remains the intentional 772736-byte user modification and was not
  checked out, overwritten, staged, or committed.
