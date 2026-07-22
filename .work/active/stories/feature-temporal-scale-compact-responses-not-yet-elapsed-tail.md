---
id: feature-temporal-scale-compact-responses-not-yet-elapsed-tail
kind: story
stage: implementing
tags: [visual, storage]
parent: feature-temporal-scale-compact-responses
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-21
updated: 2026-07-21
---

# Not-yet-elapsed tail retention state

## Checkpoint

Design Unit 1 of `feature-temporal-scale-compact-responses` (issue #14 finding
#10b): the range resolver distinguishes "requested post-action interval has not
yet elapsed" from actual evidence loss.

- New `RetentionWarning::RequestedEndNotYetElapsed { requested, newest_retained,
  session_now }` in `crates/krometrail-core/src/timeline/range.rs`, emitted
  additively alongside `RequestedEndAfterNewestRetained` when the session is
  live in this process and the requested end exceeds the guarded current
  session time.
- `TemporalRangeResolver` gains an injected `Arc<dyn MonotonicClock>`;
  `RecordingStore` receives the clock at construction (composition root
  `src/app.rs`; fixed clocks in store tests). Guards per the feature design:
  live session only, `session_now >= resolved.end()` and `>=` newest retained
  frame time; on any guard failure the refinement is omitted and resolution is
  unchanged.
- `src/debug_bundle/header.rs` names the tail "not yet elapsed" when the
  warning is present (non-diagnostic language).
- Wire enum schema regenerated (`bash scripts/check-wire-enum-schemas.sh`).
- SPEC roll-forward in the same stride: "Temporal Ranges" sentence and the
  "Errors and Degraded Operation" degrade-list line.

## Acceptance evidence

- Store-level test: live session + interaction after-window beyond newest frame
  and beyond injected now emits both warnings with the exact injected
  `session_now`.
- Ended-session and guard-failure cases emit no new variant and resolve
  unchanged.
- Wire-enum schema check passes.

## Ordering constraints

None; first in the feature's implementation order because it touches wire
schemas early.
