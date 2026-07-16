---
id: gate-cruft-remove-unconsumed-capture-shutdown-fields
kind: story
stage: review
tags: [cleanup, browser]
parent: null
depends_on: []
release_binding: 1.0.0
gate_origin: cruft
created: 2026-07-15
updated: 2026-07-15
---

# Remove unconsumed capture shutdown result fields

## Confidence
High

## Category
Dead internal result data

## Location
`crates/krometrail-cdp/src/capture/mod.rs:183`

## Evidence

Private `CaptureShutdownOutcome.reason`, `emitted_gap_count`, and `targets` are constructed but never read by production code or tests. Session shutdown consumes only `complete`; existing checks consume the authoritative per-target outcomes before aggregation.

## Removal

Remove the three aggregate fields and construction-only bookkeeping while preserving `complete`, abandonment/timeout/flush counters, authoritative gap emission, per-target stop outcomes used before aggregation, and all shutdown behavior/tests.

## Implementation notes

- Removed the unread stop `reason` and emitted-gap aggregate from `CaptureStopOutcome`, and removed the unread aggregate `targets` field from `CaptureShutdownOutcome`.
- Kept the stop reason parameter because it still controls ordinal lifecycle cleanup; kept per-target `complete` and abandonment outcomes, immediate gap emission, target stop ordering, flush accounting, and final completion aggregation.
- Replaced the unused shutdown outcome vector with a boolean fold over each authoritative per-target stop result.

## Verification

- `cargo test -p krometrail-cdp capture --locked` — 42 passed.
- Full locked workspace and `qualification-support` test variants passed.
- Rust 1.85 fmt, check, and Clippy `-D warnings` passed.

Implementation is complete; this standalone story is left at `stage: review` for one bounded independent review.
