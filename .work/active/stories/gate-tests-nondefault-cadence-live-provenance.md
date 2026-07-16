---
id: gate-tests-nondefault-cadence-live-provenance
kind: story
stage: review
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

## Implementation notes

- Extracted the narrow `report::project_capture_config` projection so the live path and browser-free test share the exact status-to-manifest seam.
- Added a `BrowserStatus` fixture carrying `EveryNthFrame::new(37)`, then asserted the assembled manifest serializes 37 and its input digest differs from the default identity.
- Added the no-session (`None`) projection assertion, which preserves the explicit contract seed default of 1.

## Verification

- `cargo test --features qualification-support browser_status_stride_reaches_manifest_identity_and_input_digest --locked` — passed.
- Full locked workspace and `qualification-support` test variants passed.
- Rust 1.85 fmt, check, and Clippy `-D warnings` passed; no Chrome was launched.

Implementation is complete; this standalone story is left at `stage: review` for one bounded independent review.
