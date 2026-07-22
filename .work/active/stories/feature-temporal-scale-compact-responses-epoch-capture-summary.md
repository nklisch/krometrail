---
id: feature-temporal-scale-compact-responses-epoch-capture-summary
kind: story
stage: done
tags: [visual, storage]
parent: feature-temporal-scale-compact-responses
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-21
updated: 2026-07-21
---

# Epoch summary in capture quality

## Checkpoint

Design Unit 2 of `feature-temporal-scale-compact-responses`: a metadata-only
epoch summary makes long ranges describable as epochs + gaps + cadence + counts
without id enumeration.

- `CapturedFrame::same_visual_epoch` in
  `crates/krometrail-core/src/recording/frame.rs` becomes the single
  visual-epoch predicate authority (image, viewport, device-scale-factor bit
  equality); `src/artifacts/epoch.rs::same_epoch` delegates to it.
- `EpochSummary` struct and `CaptureQuality.epochs: Vec<EpochSummary>` in
  `crates/krometrail-core/src/timeline/context.rs`, computed in one O(n) pass
  over the frame metadata `capture_quality` already receives — no decoding, no
  new queries. Domain vec is exact; bounding is the projector's job (Unit 3).

## Acceptance evidence

- One geometry change mid-range yields exactly two `EpochSummary` rows with
  exact per-epoch counts, ranges, and first/last frame endpoints; uniform
  geometry yields one epoch.
- Artifact epoch partitioning is unchanged (existing `src/artifacts` tests stay
  green with the delegated predicate).

## Ordering constraints

Independent of the not-yet-elapsed story; must land before the bounded
projection story, which presents the epoch summary.

## Implementation

Implemented 2026-07-21; full gate green (fmt, wire-enum schema check, check,
test, clippy `-D warnings`).

- `CapturedFrame::same_visual_epoch` added in
  `crates/krometrail-core/src/recording/frame.rs` (image, viewport,
  device-scale-factor bit equality); `src/artifacts/epoch.rs::same_epoch` now
  delegates to it, so artifact partitioning and capture-quality summaries
  share one authority. Existing artifact epoch-partition tests stay green
  with the delegated predicate.
- `EpochSummary` and `CaptureQuality.epochs: Vec<EpochSummary>` in
  `crates/krometrail-core/src/timeline/context.rs`, computed by a single O(n)
  pass (`epoch_summaries`) over the frame metadata `capture_quality` already
  receives — no decoding, no new queries. The domain vector is exact;
  bounding is Unit 3's projector concern.
- Construction sites of `CaptureQuality` literals in tests
  (`src/app.rs`, `src/debug_bundle/tests.rs`) and the MCP handle-flow spy
  JSON gained the `epochs` field.
- Tests: `uniform_geometry_yields_one_exact_epoch` and
  `one_mid_range_geometry_change_yields_exactly_two_epochs` in `context.rs`
  pin exact per-epoch counts, ranges, and first/last endpoints.
