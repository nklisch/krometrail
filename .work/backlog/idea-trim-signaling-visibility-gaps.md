---
id: idea-trim-signaling-visibility-gaps
created: 2026-07-23
updated: 2026-07-23
tags: [temporal, bug]
---

Live small-budget trim/grace exercise (2026-07-23, isolated 1.6.1 instance,
250 MB budget, sustained ~2.4 MB/s capture, 14 in-session trims observed)
verified the 1.6.1 retention mechanics — high-water at exactly 85%, sawtooth
reclaim of one 134 MB segment per pass, boundary advancing, capture never
interrupted, lone-instance census correct — but surfaced two signaling gaps in
the feature-retention-trim-transparency area:

1. **Background grace overrides are practically invisible.** One trim pass
   (log burst 41:22.023-.574) overrode artifact grace 12 times (evicting every
   in-grace artifact, ~170 KB total, before taking the 134 MB segment — the
   only sealed segment also backed the graced artifacts, so "nothing else
   reclaimable" was structurally true and override is per-policy). But
   `grace_override_active` read false at every 12 s status poll: the latch
   (crates/krometrail-store/src/recording.rs:781-799) is cleared by the next
   non-override reclaim (95 ms later here) and by every below-high-water trim
   entry. The causally-bound response warning only exists for MCP-operation-
   triggered reclaims; capture-driven overrides (the common case) surface
   nowhere an agent can see. Direction: make the status fact sticky/anchored —
   e.g. a session-scoped "grace overridden through <session_time>" boundary
   (and/or count) that survives until surpassed, keeping the calm voice.
2. **Fully-trimmed range resolves to a bare not_found.** After trimming past
   the range, `resolve_temporal_range` over session_time {0..10s} failed with
   `error_code: not_found` and no trim-boundary context (correlation
   f079339b-eb06-486e-b6e8-63263ae0a9ec in the exercise instance log). The
   1.6.1 trimmed-through note covers surviving ranges only; the fully-evicted
   case is exactly the "surprise" the signaling decision was meant to prevent.
   Direction: the no-retained-evidence failure names the oldest retained
   boundary and the in-session-trim fact in its structured message/recovery.

Also observed, tuning-note only: with fixed ~134 MB segment sealing, budgets
below ~500 MB behave coarsely (whole-segment sawtooth; first trim structurally
forces grace override because the only sealed segment backs the fresh
artifacts). Segment target size scaling with small budgets may be worth
considering separately.

## Execution assessment — 2026-09-05

Both signaling findings were promoted to
[`story-trim-signaling-visibility`](../active/stories/story-trim-signaling-visibility.md),
marked done for 1.6.2. Current source includes `grace_overridden_through` and
`fully_evicted_range_not_found` with targeted coverage. Do not dispatch a second
implementation from this idea: first rerun the owning regressions, then reconcile
this retained source item under the repository's retention policy. This
assessment inspected the current source and completion record but did not rerun
those tests. The small-budget segment-size note above is still an unmeasured
observation, not another established signaling defect.
