---
id: feature-temporal-partial-retention
kind: feature
stage: drafting
tags: [temporal]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-22
updated: 2026-07-22
---

# allow_partial resolves honestly for every anchor kind

## Brief

Two related gaps in allow_partial temporal resolution, both repro'd in the
v1.5.0 shakedown:

1. **session_time anchors never clamp.** `clamp_natural_interaction_range`
   (`crates/krometrail-core/src/timeline/range.rs:1660`) only applies to
   `Interaction | LatestInteraction` anchor kinds, so a `session_time` range
   whose end overshoots retained bounds hard-fails with "requested interval
   extends beyond captured source-frame bounds" even under
   `retention: allow_partial`. An explicit "from t1 until now/future" range —
   the natural way to ask for the recent tail — refuses instead of resolving
   the retained prefix with honest warnings, while the equivalent
   interaction-window overshoot clamps fine.

2. **RequestedEndNotYetElapsed did not fire in a textbook case.** A
   `latest_interaction` resolve with `after_ms: 30000` issued ~1s after the
   interaction (live session, Recording, requested end ~26s beyond session
   now) returned only `requested_end_after_newest_retained` +
   `partially_captured`; the additive `requested_end_not_yet_elapsed`
   refinement (range.rs ~1629) was absent, implying `live_session_now`
   returned None on this path (lifecycle/origin-normalize/retained-bounds
   guard — root cause not yet isolated). Callers therefore cannot
   distinguish "interval not yet elapsed" from evidence loss, which is the
   exact distinction the warning was shipped to make.

Also worth a look while here: an idle page tail (no visual change → no
frames) surfaces as `partially_captured`, which reads as capture loss to a
caller even though the page simply stopped changing.

## Simplification opportunity

If clamping generalizes across anchor kinds, the interaction-only special
case (`clamp_natural_interaction_range`) collapses into one uniform
allow_partial rule, and the "extends beyond captured source-frame bounds"
refusal shrinks to the cases where nothing intersects retention at all.
