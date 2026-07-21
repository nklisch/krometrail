---
id: feature-retention-lifecycle-and-trimming
kind: feature
stage: drafting
tags: [storage, agent-ux, bug]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-20
updated: 2026-07-20
---

# Retention lifecycle, age-out, and in-session trimming

## Brief

Krometrail has no cleanup story. The only reclaim trigger is `DiskBudgetBytes`
pressure (`crates/krometrail-store/src/recording.rs:83`); a grep for
`max_age|age_out|ttl|expire` across the store returns nothing. Evidence therefore
accumulates until it hits the budget wall and stays there forever.

The seventh shakedown (2026-07-20, v1.2.7) found a real store at **9.2 GB
against the 10 GB budget**, accumulated across days of sessions with nothing
ever expiring. `budget_state` still read `available` while the store sat
permanently pinned at ~99%.

**Scope correction.** The terminally-failed capture writer observed in the same
shakedown is *not* part of this feature and was not caused by budget pressure.
Root-cause investigation proved it was a cross-instance defect — a second
`krometrail` process running startup `recover()` against the live data directory
— triggered by the investigator's own diagnostic probe. That defect is scoped
separately as `feature-single-instance-store-ownership`. Eviction was
exonerated: `append_frame` (`recording.rs:1713`) holds `self.mutations` across
`ensure_append_capacity` -> `flush_all` -> `cleanup_to` -> `append_indexable`,
so eviction and sealing are fully serialized within one process, and budget
pressure alone cannot produce the failure.

What *is* in scope here, and is genuinely budget-driven:

**Artifact links die under eviction pressure.** `cleanup_to()`
(`recording.rs:604-611`) evicts a segment together with every artifact derived
from it (`artifacts_for_segment`). On a near-full store this happened within
~25 seconds of publication during the shakedown: `generate_artifacts` returned
resource URIs that `resources/read` then rejected as not found. The agent is
handed an evidence link that is already dying, with no signal that budget
pressure is the reason.

This feature covers three related things that are really one missing lifecycle:

1. **Age-out.** A real retention policy so evidence expires on time as well as
   on size, and a store does not sit permanently pinned at ~99% of budget.
2. **Dynamic in-session trimming.** Reclaim should be possible *during* a live
   session, not only at the budget wall — so a long session trims as it goes
   instead of degrading into permanent near-full pressure.
3. **Honest artifact lifetime.** Either protect freshly published artifacts from
   immediate cascade eviction, or make the expiry explicit to the agent so a
   returned resource link carries a truthful expectation of availability.

## Simplification opportunity

Age-out and budget pressure should share one reclaim path, not become two
parallel eviction engines. `cleanup_to()` already walks artifacts, browser
events, and segments in retention order; an age predicate should feed the same
walk. Per Current Contract Discipline there is no supported third-party consumer,
so the retention configuration shape may be replaced directly rather than
extended with a compatibility alias.

Also fold in, if cohesive:
- `idea-recording-store-operational-edges` — `RecordingStore::status()`
  serializing behind the mutation gate (latency during eviction), and the
  `new`/`with_budget` caller invariant that `recover()` must run first.
- `idea-progressive-pin-contract-cleanup` — the store producer and `PinState`
  validator duplicate the same range-coalescing algorithm; make one
  authoritative without weakening independent invariant checks. Also decide
  whether legacy `RetentionStore::pin_range` / `unpin_range` and the simpler
  recording `PinChange` can be removed now that production uses resolved-range
  pin operations. Pins are load-bearing for this feature — anything pinned must
  survive both age-out and dynamic trimming — so settling the pin contract
  belongs here rather than drifting separately.

## Acceptance

- The reproduced `sealed_segment_publication` / `not_found` / `writer_terminal`
  failure is structurally impossible, with a regression test that drives the
  proven interleaving.
- A terminal writer failure is no longer the outcome of a full store: a store at
  budget either reclaims or degrades with an explicit, recoverable state.
- Age-out reclaims on a configured age policy and shares the existing reclaim
  walk.
- Trimming can reclaim during a live session and is observable to the agent.
- `open_segment_count` accounting reflects reality.
