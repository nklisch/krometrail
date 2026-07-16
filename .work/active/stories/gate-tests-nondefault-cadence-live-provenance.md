---
id: gate-tests-nondefault-cadence-live-provenance
kind: story
stage: drafting
tags: [testing, browser, visual]
parent: null
depends_on: []
release_binding: 1.0.0
gate_origin: tests
created: 2026-07-15
updated: 2026-07-15
---

# Test non-default cadence through live qualification provenance

## Priority
Medium

## Value evidence
Item: `configurable-capture-cadence-evaluation-provenance-and-schema`

The live report projects `BrowserStatus.every_nth_frame` into `KrometrailIdentity.capture_config`, but current tests validate bounds/digests by mutating manifest contracts rather than exercising a non-default status value across that projection seam.

## Gap type
Missing cross-boundary provenance regression test.

## Suggested test

Feed a browser status carrying `EveryNthFrame::new(37)` through the browser-free qualification projection and assert the assembled manifest and input digest contain 37. Retain a separate no-session assertion that the explicit non-passing contract seed remains 1. Do not launch Chrome.

## Test location (suggested)
`src/app/live_evaluation.rs` and `src/app/live_evaluation/report.rs`
