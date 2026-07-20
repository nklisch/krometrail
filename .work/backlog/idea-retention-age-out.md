---
id: idea-retention-age-out
created: 2026-07-20
updated: 2026-07-20
tags: [storage]
---

Recording retention in `crates/krometrail-store` is purely budget-based — there
are no age, TTL, or expiry concepts anywhere in the store. A grep for
`max_age|age_out|ttl|expire` across the crate returns nothing; the only reclaim
trigger is `DiskBudgetBytes` pressure (`recording.rs:83`).

Observed in the 2026-07-20 sixth shakedown: 7.0 GB of retained frames
(`segment_bytes: 7047424322`) plus 54 MB of index sitting under a 10 GB budget,
accumulated across five prior shakedown sessions. Nothing had aged out because
the ceiling had not been reached.

The problem is not the disk use itself — it is *when* and *what* eviction
chooses. Because reclaim only fires under pressure, it fires at whatever moment
the budget happens to fill, and drops the oldest data at that moment. That is
likely to be mid-investigation, and the thing evicted is chosen by age-at-pressure
rather than by irrelevance.

Sketch of the wanted behavior: an age-based sweep running alongside the budget,
not replacing it — sessions past a retention window get reclaimed on startup and
periodically, independent of pressure. Pinned ranges must survive it; the
exclusion mechanism already exists (`pinned_usage_bytes`).

Open question for design: fixed conservative default window, or configurable? A
knob invites nobody ever tuning it, which argues for one sensible default and no
configuration until someone demonstrates a need.

Origin: 2026-07-20 sixth shakedown against v1.2.6. Related but distinct from
`idea-eval-harness-browser-teardown` — both surfaced as "stale things accumulate"
but they live in different subsystems and should not share a review.
