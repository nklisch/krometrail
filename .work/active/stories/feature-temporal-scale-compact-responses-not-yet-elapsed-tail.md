---
id: feature-temporal-scale-compact-responses-not-yet-elapsed-tail
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

## Implementation

Implemented 2026-07-21; full gate green (fmt, wire-enum schema check, check,
test, clippy `-D warnings`).

- `RetentionWarning::RequestedEndNotYetElapsed { requested, newest_retained,
  session_now }` added in `crates/krometrail-core/src/timeline/range.rs`,
  emitted additively inside `classify_retention` immediately after
  `RequestedEndAfterNewestRetained` when a guarded `session_now` is present,
  `session_now >= resolved.end()`, and `requested.end() > session_now`.
- `TemporalRangeResolver` gained an injected `Arc<dyn MonotonicClock>`. The
  guarded current session time lives in `live_session_now`: session must
  exist, have no `ended_at`, and be in an active lifecycle
  (starting/recording/reconnecting — `stopping` is excluded because no future
  frames will arrive); `SessionOrigin::normalize(clock.now())` failure or
  `session_now <` the newest retained frame time (from
  `frame_availability.retained_bounds`) silently drops the refinement.
- `RecordingStore` now requires the clock at construction (`new`,
  `with_budget`, `with_retention`); the composition root
  (`src/app.rs::open_storage_with_budget`) receives the process clock from
  `build_runtime`, live qualification reordered to build its clock before
  storage, and every store/cdp/root-crate test injects a fixed clock helper.
  No default clock exists inside the store (injected-core-ports).
- `src/debug_bundle/header.rs::compose_header` appends a bounded clause when
  the warning is present: the unretained tail is named a not-yet-elapsed
  interval, not evidence loss (approved non-diagnostic vocabulary).
- Wire enum schema check passes (`RetentionWarning` keeps its container
  `rename_all`); SPEC "Temporal Ranges" gained the not-yet-elapsed sentence
  and "Errors and Degraded Operation" the degrade-list line.
- The design's optional exact-failure-message refinement (stating a future
  end in the not-found error) was not implemented — it is marked optional and
  adds a second emission path for the same signal; the warning covers the
  issue #14 finding.
- Acceptance tests in `crates/krometrail-store/tests/range_resolution.rs`
  (`live_session_partial_tail_is_refined_as_not_yet_elapsed`): live emission
  with exact injected `session_now`, guard-failure suppression (now behind
  newest retained), ended-session suppression; plus the header clause test in
  `src/debug_bundle/header.rs`.
