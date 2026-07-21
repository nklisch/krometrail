---
id: feature-retention-lifecycle-and-trimming
kind: feature
stage: review
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
separately as `feature-multi-instance-store-isolation`. Eviction was
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

## Architectural choice

**One reclaim walk, narrowed by a filter — not a second eviction engine.**
`RecordingStore::reclaim` is the single entry point for budget pressure,
in-session trimming, and age-out. They differ only in the target byte count and
in how the candidate set is narrowed (`SegmentReclaimFilter`); ordering and pin
protection are identical by construction, so age-out cannot drift into subtly
different behaviour.

Reclaim proceeds in tiers, cheapest loss first: derived artifacts (regenerable),
then browser events and segments in retention order. **Tier 0 is reserved for
abandoned instance roots** — nothing live references them, so they are the
cheapest possible reclaim. That tier slots in ahead of the rest without
reshaping the walk.

## Design decisions

- **Age is read from SQLite's own clock, not an injected port.**
  `created_unix_ms` defaults to `CAST(unixepoch('subsec') * 1000 AS INTEGER)` on
  `segments` and `artifacts`, and cutoffs are computed from `now_unix_ms()` — the
  same clock that stamped the rows, so a cutoff can never be compared against a
  different time source. *A future reader will reasonably ask why this bypasses
  the clock port:* the value is a property of the disposable cache ("how long has
  this file been on disk") and carries none of the source/observed/session
  semantics the clock ports exist to keep distinct. Defaulting in SQL also lets
  startup recovery stamp rows identically without threading a clock through the
  recovery path.
- **Item 3, honest artifact lifetime — grace window with liveness override.**
  A segment backing an artifact published within `artifact_grace` (default 15 min)
  is skipped during budget pressure. If *every* remaining segment is so
  protected, the grace is dropped and the eviction proceeds, logging
  `retention.artifact_grace_overridden`. Rationale: protecting artifact files
  alone would not work, because `read_artifact` revalidates source frames, so the
  artifact's *segments* must survive; and an unconditional grace could stall
  capture at a full store, which is worse than losing a fresh evidence link.
  Liveness wins, and the broken promise is reported rather than hidden.
- **Trimming does not flush.** It reclaims already-sealed evidence, so a hot
  append path is never forced into segment rotation.
- **Trim exhaustion latch.** A walk that reclaims nothing sets a latch, so a
  store whose remaining evidence is entirely pinned does not re-walk (and
  re-checkpoint) on every frame. Any later reclamation clears it.
- **Age-out will not touch browser events it has not proven expired.** Events are
  bounded by an expired segment's `retention_sequence`; with no such segment,
  age-out skips events entirely. Unbounded event eviction remains correct only
  under real pressure.
- **Shared budget registry: the budget is a total, not a per-instance
  allowance.** `budget_registry.rs` is a small JSON ledger under
  `instances/.budget-registry.json`, guarded by its own `flock` file. Each
  instance publishes its usage and reads its peers'; every budget decision goes
  through `RecordingStore::effective_budget()` rather than the configured value.
  - *Allocation policy:* `max(equal_share, total - other_live_usage)`. The first
    term lets a busy instance use everything idle peers are not using; the floor
    stops a busy instance being starved by peers that grew first. The floor is
    also what permits the sum to exceed the total, because isolation forbids one
    instance from reclaiming another's data. An instance already over its share
    trims back to it on its own next append, so the combined footprint *settles*
    at the total.
  - *Liveness reuses `acquire_existing`* — the same primitive that decides
    reclaimability — so "who counts toward the total" and "whose root may be
    reclaimed" can never disagree. A dead instance's bytes stop counting exactly
    when tier-0 reclaim becomes able to free them.
  - *The lock is held only for the accounting transaction* (read, prune dead
    entries, write, unlock), never across data I/O, so instances never serialize
    on each other's capture writes. Acquisition is non-blocking: contention
    degrades this pass rather than putting a lock wait on the capture path.
  - *Never blocks capture.* Corrupt ledger, contended lock, failed write, or a
    peer that died mid-transaction all degrade to per-instance enforcement.
    Writes are temp-file-plus-rename, so a death mid-write leaves the previous
    ledger intact. An undecidable liveness probe counts the peer as live, which
    tightens this instance rather than letting the total silently overshoot.
  - *Publish cadence:* invalidated on **both** elapsed time and bytes written —
    `BUDGET_SHARE_REFRESH` (2 s) or growth past `total / 32` since the last
    publish, whichever comes first. Also forced at every `enforce_locked` (flush).
  - **Real overshoot bound, and why the documented one was wrong.** *Revised
    after cross-model review.* The claimed `(live - 1) * total / live` bound
    assumed instances republish often enough for peers to see them. Time alone
    does not guarantee that: `flush` runs at session *stop*, so a live session's
    only budget check is the append path, and a capture pipeline can write far
    more than a share inside a two-second window. Measured on the pre-fix build,
    two instances growing without an intervening flush reached **3 394 431 bytes
    against a 2 000 000 total** — 1.7x, past the documented bound. Making the
    share expire on bytes as well as on time ties staleness to the quantity the
    bound is expressed in: the same scenario now settles at 2 041 968 bytes, an
    overshoot of 41 968 against a 62 500 drift allowance. **The bound as stated
    now:** the combined footprint settles at the total, and may instantaneously
    exceed it by at most `total / 32` per live instance before the next
    accounting transaction pulls it back. Recorded in `docs/SPEC.md`.
  - *Test shape.* `concurrent_instances_share_one_total_budget` filled instances
    one after another, so the first saw an empty ledger exactly once and every
    later one saw a settled ledger — it could not observe concurrency at all. It
    now interleaves round-robin (and passes before and after, which is itself the
    finding: interleaving *with* a flush per frame converges either way). The test
    that actually discriminates is
    `instances_that_never_flush_still_share_one_total_budget`, which reproduces
    the real capture shape — growth with no durability boundary — and fails on the
    pre-fix build with the numbers above.
- **`status()` left the mutation gate.** It is a read, and serialising it behind
  the gate made it wait out whatever eviction was running — the opposite of what
  an agent checking a store under pressure needs. It uses
  `live_usage_snapshot()`, which derives the index class from live pages instead
  of the stored row (which is only written by mutating paths and would read zero
  on a fresh store).
  - **Accepted imprecision, recorded so it is not rediscovered as a bug:**
    `live_usage_snapshot` does *not* run `PRAGMA wal_checkpoint(TRUNCATE)`, so
    pages still sitting in an un-checkpointed WAL are not yet reflected in the
    reported `index_bytes`. Under sustained writes, status can therefore
    under-report total usage by up to the outstanding WAL size until the next
    mutating path checkpoints. This is deliberate: every path that *acts* on the
    budget (`ensure_append_capacity`, `enforce_locked`, `reclaim`) still calls
    `refresh_usage()` and sees checkpointed accounting. Only the read-only status
    projection trades that precision for never blocking.
- **Pin coalescing unified.** `coalesce_protected_ranges` is exported from core
  and the byte-identical copy in `recording.rs` is deleted. `PinState::new` still
  recomputes and compares — **not** redundant, because that check guards
  wire-decoded values arriving from outside the process.
- **`retained_bounds` now orders by `segments.created_unix_ms`, not `rowid`.**
  The bounds are global across sessions, so they need a key that is meaningful
  across sessions. `rowid` is global *insertion* order, which answers a different
  question, and `session_time` is measured from each session's own start, so
  comparing two sessions' session times is meaningless by construction. That is
  the root cause of the shakedown observation where `oldest_retained`
  (`126065361437`) exceeded `newest_retained` (`118028908063`): the endpoints came
  from different sessions and were never comparable.
  - *Chosen authority:* `created_unix_ms`, because it is one wall clock shared by
    every session — the only key here that genuinely orders evidence globally.
    Ties break on `session_time` then `frame_id` for determinism.
  - *Contract made explicit:* each endpoint still carries its own
    session-relative time, since that is the coordinate needed to address the
    frame within its session. Those two values are comparable, and a span between
    them meaningful, **only when both endpoints share a session and target.** The
    query guarantees the endpoints are *ordered*, not that their session times can
    be subtracted. The MCP layer's `comparable_scope` flag is the correct
    presentation of that contract and remains right.
- **Legacy `pin_range`/`unpin_range` and the simpler `PinChange` were NOT
  removed.** They are `RetentionStore` port methods implemented by test doubles
  in `crates/krometrail-cdp/` and `src/progressive/service.rs` — outside this
  task's ownership boundary. Removing them needs a coordinated pass.

## Implementation Units

1. `krometrail-core` — `RetentionLifecycle` (budget, max age, trim high-water,
   artifact grace); `coalesce_protected_ranges` exported.
2. Schema v8 — `created_unix_ms` on `segments` and `artifacts`, plus
   `segment_created_idx` / `artifact_created_idx`.
3. `index/retention.rs` — `SegmentReclaimFilter`, `oldest_reclaimable_segment`,
   `oldest_reclaimable_artifact`, `expired_object_count`, `now_unix_ms`,
   `open_segment_count`, `live_usage_snapshot`.
4. `recording.rs` — `reclaim`/`reclaim_once` tiered walk, `trim_locked`,
   `ReclaimOutcome` observability, gate-free `status()`,
   `verify_recovery_completed`.
5. `budget_registry.rs` (new) — shared lock-protected ledger, liveness pruning,
   allocation policy.
6. `src/app.rs` — retention configuration and the dead-instance reclaim tier.

## Tunables

Documented in `docs/SPEC.md` ("Disk Budget and Retention"):

| Variable | Default | Meaning |
| --- | --- | --- |
| `KROMETRAIL_DISK_BUDGET_BYTES` | 10 GB | Total shared across all live instances. |
| `KROMETRAIL_RETENTION_MAX_AGE_SECS` | 7 days | Age at which evidence expires regardless of budget. `0` disables age-out. |

With reads instance-scoped, the maximum age is the main thing standing between a
user and an ever-growing store, so it is deliberately on by default.

## Testing

`tests/retention_lifecycle.rs`: age-out reclaims while far inside budget; pinned
evidence survives a 30-day backdate; in-session trimming keeps peak usage below
the budget wall and the store `Available`; age-out with no expired segment leaves
events alone. Backdating stored stamps is the deterministic way to exercise a
real-time policy.

`tests/retention_lifecycle.rs::retained_bounds_order_by_wall_clock_not_insertion_order`
drives the shakedown inversion: a session inserted second but backdated to be
wall-clock older. Verified to **fail** against the previous `rowid` query and pass
against the wall-clock ordering, so it distinguishes the two implementations
rather than passing incidentally.

`tests/artifact_store.rs::derived_artifacts_are_evicted_before_the_frames_they_derive_from`
proves the tier ordering rather than its aftermath. The pre-existing
`source_segment_eviction_removes_linked_artifact_before_frames` applied pressure
large enough to evict both and then asserted only that both were gone — which is
equally consistent with the segment going first and the artifact being
*invalidated* as collateral, since `artifact()` reports that as `None` too.
Confirmed by mutation: with the artifact tier disabled in `reclaim_once`, the old
test still passes and the new one fails. The new test applies exactly enough
pressure that losing the artifact suffices, then asserts the source frames
survive.

`tests/shared_budget.rs`: three concurrent instances sharing one total budget use
strictly less than three unshared instances and stay inside the total; a dead instance's bytes stop counting and the survivor regains
the whole budget; a corrupt ledger degrades without blocking capture and is
repaired by the next transaction; registry bookkeeping files are never mistaken
for instance roots. Unit tests in `budget_registry.rs` cover the allocation
policy directly, including that balanced instances sum exactly to the total.

## Risks

- **Age-out is now the only path to old data** (reads are instance-scoped), so an
  over-aggressive policy loses evidence with no recovery. Default 7 days.
- **Trim adds an index query per append** below the high-water mark
  (`expired_object_count`). Cheap and indexed, but it is on the hot path.
- **`unixepoch` is wall-clock**, so a large clock step backwards defers age-out
  and a step forwards expires evidence early.
- **Artifact grace is best effort**, not a guarantee; under sustained pressure a
  fresh artifact link can still die. The override is logged.
- **Shared-budget overshoot is real but bounded.** An instance that grew while
  alone keeps its bytes until it trims, ages out, or exits. Worst case is
  `(live - 1) * total / live` above the configured total.
- **Registry liveness probing acquires peer locks.** Cheap and non-blocking, but
  it means an accounting pass briefly claims and releases each dead peer's lock.
  Harmless — reclaim re-acquires — but it is a side effect of a read-shaped call.

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

## Second cross-model review round: shared-budget bound

The shared-budget overshoot bound was stated but not enforced. The share cache
was invalidated by bytes already written, and the check ran *before* the append,
while `EncodedFrame` carries no payload-size limit. Two consequences, both real:

- a single frame larger than the drift allowance was admitted against a grant
  that never accounted for it;
- the ledger was only ever told what an instance held *before* its write, so two
  instances starting together each sized themselves against a peer the ledger
  reported as empty.

Fixed in `crates/krometrail-store/src/recording.rs`. `budget_share` now takes the
pending write size. It enters the staleness test (`observed + pending` is what is
compared against the last published figure) and it is *reserved* in the ledger at
publish time, so a concurrent peer sees the committed bytes. `trim_locked` takes
the already-computed share rather than recomputing it, so a large append still
costs one registry transaction.

The generous `total - other_live_usage` grant is kept; it was not the cause. The
equal-share floor is likewise kept, and its overshoot is a genuinely separate
term rather than a widening of the drift bound. The two are now stated
separately in `docs/SPEC.md`:

    total + (N-1)/N x total   (equal-share floor, transient)
          + N x total/32      (accounting staleness)

Regression: `one_oversized_frame_cannot_escape_the_shared_bound` in
`crates/krometrail-store/tests/shared_budget.rs`. Two instances, one 6 MB frame
each, 8 MB total. Pre-fix the combined footprint reached 12,804,238 bytes against
a bound of 8,500,000; post-fix the second instance is refused at the equal-share
floor and the combined footprint stays inside the bound.
