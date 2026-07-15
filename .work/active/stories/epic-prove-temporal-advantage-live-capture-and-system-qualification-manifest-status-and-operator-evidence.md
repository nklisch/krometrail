---
id: epic-prove-temporal-advantage-live-capture-and-system-qualification-manifest-status-and-operator-evidence
kind: story
stage: implementing
tags: [testing, infra, visual]
parent: epic-prove-temporal-advantage-live-capture-and-system-qualification
depends_on: [epic-prove-temporal-advantage-live-capture-and-system-qualification-retention-recovery-and-performance]
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
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

- [ ] Canonical manifest bytes round-trip deterministically, include every registered gate exactly
      once, preserve source/observed/session clocks and gap IDs, and contain no private paths,
      payloads, model data, raw page text, credentials, or remote URLs.
- [ ] Status tests cover no-opt-in/no-output, required browser blocked, optional Chromium skipped,
      incomplete/inconclusive, complete fail, complete pass, cleanup failure, and finalization
      failure.
- [ ] A passing manifest cannot contain a missing measurement, unresolved gap, unavailable
      resource, wrong viewport/scale, failed control observation, failed retention/recovery gate,
      or cleanup failure.
- [ ] Output is written only under ignored `target/temporal-evaluation/`; no product command or
      generated documentation is changed.
- [ ] Operator instructions distinguish qualification from authorized live evidence and state
      high-DPI/model/cross-platform/advantage non-claims and required local blockers.
- [ ] Final verification runs the standard Rust gates without launching Chrome; the live path is
      not invoked during design or ordinary tests.

## Ordering

This is the final sequential checkpoint. It depends on all concrete capture, control, retention,
recovery, resource, and latency measurements and makes the feature ready for integrated review.
