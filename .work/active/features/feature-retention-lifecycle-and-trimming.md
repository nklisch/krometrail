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
updated: 2026-07-21
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
- **Shared budget: the budget is a total, divided equally across live
  instances.** Every budget decision goes through
  `RecordingStore::effective_budget()` rather than the configured value, and that
  value is `total / live_instances`. The live count comes from `InstanceCensus`
  (`crates/krometrail-store/src/instance.rs`), which enumerates sibling roots and
  probes each with `acquire_existing` — the same primitive that decides
  reclaimability, so "who counts toward the total" and "whose root may be
  reclaimed" can never disagree.
  - *The policy needs a count, not usage.* This is the whole point of the shape.
    A count is exact at the moment it is read, so there is nothing to publish,
    nothing to cache, no staleness window, and no failure path that could hand
    out a grant no peer can see. See "Fourth round outcome" below for the four
    defects that lived in the usage-sharing machinery this replaced.
  - *Sampling cadence: every budget decision, no cache.* A cached count is a
    stale count, which is the defect class being deleted. Measured cost of one
    read: ~3.5 µs alone, ~7 µs with one live peer, ~30 µs with eight — against an
    append that already does a segment write and a SQLite transaction. Not worth
    a cache.
  - *A lone instance gets the whole total* (`total / 1`), so a single-process
    install is unaffected. An undecidable sibling counts as live, which tightens
    this instance rather than letting the total overshoot. Where ownership cannot
    be proved (non-Unix), sibling enumeration returns nothing, so `N = 1` and each
    instance enforces the full configured budget — the cost already documented for
    that platform.
  - *Accepted cost:* two live instances each get `total / 2` even if one is idle,
    and a single write larger than a share is refused however much disk is free.
    Predictability is the deliberate trade.
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
5. `instance.rs` — `InstanceCensus`, the live-instance count that divides one
   total budget. (Superseded a `budget_registry.rs` usage ledger, now deleted.)
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

`tests/shared_budget.rs` covers the properties the equal-division policy actually
has: a lone instance gets the whole total; two live instances each enforce
`total / 2` and neither exceeds it regardless of the other's usage, whether they
grow one after another or concurrently; a frame larger than a share is refused
while one that fits is admitted; a dead root does not count toward `N` and the
survivor regains the whole budget; and an instance that grew while alone stays put
while idle but trims toward its smaller share on its first operation after a peer
joins. Verified to discriminate: forcing `live_instances()` to return 1 fails five
of the six.

## Risks

- **Age-out is now the only path to old data** (reads are instance-scoped), so an
  over-aggressive policy loses evidence with no recovery. Default 7 days.
- **Trim adds an index query per append** below the high-water mark
  (`expired_object_count`). Cheap and indexed, but it is on the hot path.
- **`unixepoch` is wall-clock**, so a large clock step backwards defers age-out
  and a step forwards expires evidence early.
- **Artifact grace is best effort**, not a guarantee; under sustained pressure a
  fresh artifact link can still die. The override is logged.
- **An instance that grew while the live count was lower stays above its current
  share until its next operation.** Reclaim is operation-driven; there is no
  background scheduler. Usage does not grow while idle, and the very next
  operation is judged against the current share, but nothing reclaims the excess
  in the meantime. Stated as such in `docs/SPEC.md` — not softened into a
  convergence claim.
- **Census liveness probing acquires peer locks.** Cheap and non-blocking, but a
  census briefly claims and releases each *dead* peer's lock. Harmless — reclaim
  re-acquires — but it is a side effect of a read-shaped call, and it now happens
  on every budget decision rather than on a throttled accounting pass.
- **Live instances each hold a share even when idle.** Two instances means
  `total / 2` each, so an idle peer does hold capacity it is not using. Accepted:
  the alternative requires exact peer usage at write time, which is not knowable.

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

**Historical.** Everything below describes the usage ledger, which was deleted in
the fourth round. Kept as the record of how the defect was found; none of it
describes current code.

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

**Superseded below.** The two-term closed form recorded here is wrong for
arbitrary join order; see the third review round.

## Third cross-model review round: unrecorded reservations and an unsound bound

**Historical**, for the same reason as the second round: the ledger it analyses no
longer exists. The `11T/6` construction is still worth keeping, because it is why
the equal-share floor could never be reconciled with a hard total bound.

### A ledger write failure still granted a share

`budget_registry.rs` discarded the result of `self.write(&file)` and returned the
generous `total - other_live_usage` grant regardless. The comment called that
"accuracy, not liveness" — but the generous term is only sound *because* the peers
it discounts can read the bytes this instance just reserved. A failed write denies
them exactly that. Two instances in this state each measure themselves against a
peer the ledger reports as empty and each admit a large append against the same
unclaimed capacity, so the combined footprint exceeds the total by construction.
The module header already claimed a failed write degrades to per-instance
enforcement; the code did not do it.

Fixed: `publish` now checks whether the write succeeded and falls back to
`self_only_budget` — the equal share and nothing more — when it did not. The
generous term is dropped; the floor is kept, because for a lone instance the equal
share *is* the configured total, which preserves the standing guarantee that
degraded accounting never blocks capture. Degrade, do not stall.

Regression: `an_unrecorded_reservation_does_not_buy_a_shared_grant` in
`crates/krometrail-store/tests/shared_budget.rs`. Two live instances, 1,000,000
total. The write is made to fail deterministically by occupying the atomic-write
temp path (`.budget-registry.tmp-<uuid>`) with a directory, so no permission
juggling is involved and it behaves the same on every host. Pre-fix the second
instance was granted the whole 1,000,000 against a reservation nobody recorded;
post-fix it is held to 500,000. Quoted failure against the pre-fix build:

    assertion `left == right` failed: a reservation that was never recorded
    must fall back to self-only enforcement
      left: 1000000
     right: 500000

### The documented bound did not survive sequential joins

The closed form `total + (N-1)/N x total + N x total/32` is only correct for a
*fixed* live set. It breaks when instances join over time, because each earlier
instance was granted its share of a **smaller** live set and keeps it. Walking the
reviewer's counter-example with total `T`:

| step | live set | grant | why | combined |
| --- | --- | --- | --- | --- |
| `A` starts alone, fills | 1 | `T` | no peers | `T` |
| `B` joins, fills | 2 | `T/2` | `T - T = 0`, so the equal-share floor wins | `3T/2` |
| `C` joins, fills | 3 | `T/3` | `T - 3T/2 = 0`, floor wins again | `11T/6` |

`11T/6 = 1.833T`, and the documented bound at `N = 3` is
`T + (2/3)T + 3T/32 = 1.760T`. Violated. Every step is exactly what the allocation
policy promises, so this is a documentation defect, not a code defect. Extending
the construction gives `T x (1 + 1/2 + ... + 1/N)` — the harmonic sum, which grows
without limit, though only logarithmically (~`2.9T` at ten instances). Checked
numerically: the formula first fails at `N = 3` and diverges from there.

**Deliberately not fixed by loosening the formula.** No `(N-1)/N`-shaped bound
survives the counter-example, and inventing a looser closed form to fit the code
would be the same mistake again. `docs/SPEC.md` now states the honest thing: the
combined footprint *converges* to the configured total; the instantaneous figure
may exceed it while a growing set of instances is settling, by an amount governed
by join order rather than a fixed multiple; and every source of excess is transient
under three named mechanisms — trimming to the current share on the over-sized
instance's own next append, age-out (which applies regardless of budget, so an
idle instance's evidence still expires), and exit (bytes stop counting immediately,
the root is reclaimed by the next instance to start). The staleness term is
unaffected and is still a real bound: `total/32` per live instance.

The construction is recorded as a construction, not as a proven supremum — it is a
lower bound on the worst case, and that is enough to disqualify the old formula.
The wrong bound is also removed from the `effective_budget` doc comment, and
`sequential_joins_exceed_the_equal_share_overshoot_formula` pins it as a unit test
so the formula cannot quietly return.

### Convergence claim corrected (round 4)

The round-3 wording closed with "every source of excess is transient under
trimming, age-out, and exit". That is false as written. There is no background
trim or age-out scheduler: every reclaim walk runs inside an instance's own
append (`recording.rs:587-610`), enforcement pass, or artifact publication, and
`reclaim` (`recording.rs:895-929`) is only ever reached from those paths.
Age-out is checked on the same operation-driven walks, so it does not expire an
idle instance's evidence "on its own schedule". Exit handling was the only one of
the three that held.

Counter-example, worked to the end in `docs/SPEC.md`: three sequential joins
reach `11T/6`; if all three instances then go idle, usage stays at `11T/6`
indefinitely. It does not grow, and no instance is over-granted from that point
on because each one's next operation is judged against its current share — but
nothing reclaims the excess until some instance does work or a process exits.

Took option (a), correcting the claim rather than adding a periodic trim. A
background scheduler would be a real design change: it needs a timer per store, a
policy for waking an idle process, and interaction with the pin and grace rules,
all to reclaim disk that no one is contending for. An idle instance consumes
nothing further, so operation-driven reclaim is defensible behaviour; the defect
was the documentation claiming more than the code does. `docs/SPEC.md` now says
reclaim is operation-driven, states the idle outcome explicitly, and drops the
convergence claim. A short paragraph in the Reclaim section says the same thing
for age-out generally.

Not restated in softer words: the text says outright that Krometrail does not
claim the combined footprint converges on its own.

### Deferred defect: lock contention granted the full budget — now moot

The ledger mapped a `None` from `BudgetRegistry::publish` — which included lock
contention, not just a failed write — to the full configured budget. Two
instances could both hit contention, both receive unrecorded full grants, and
each append nearly the whole total. Confirmed with a probe test: two contended
instances were jointly granted 2_000_000 against a 1_000_000 total.

**Resolved by deletion, not by a fix.** There is no ledger and therefore no ledger
lock, so there is nothing to contend on and no degraded-grant branch to fall
through. The mechanism is gone.

## Fourth round outcome: the usage ledger deleted

Four consecutive review rounds found four defects, and all four were the same
shape: stale peer usage granting `N x total`; a large append landing after the
staleness check; a failed ledger write still granting; lock contention still
granting the full budget. Every one lived in the usage-sharing machinery, and
each fix added another guard to it.

The root cause is not any of the four. `max(equal_share, total - other_live_usage)`
can only be honoured with every peer's *current* byte usage exactly at the moment
of a write, and instances write independently and publish periodically, so that
number is stale the instant it is read. Separately, the equal-share floor
guarantees each instance `total/N` regardless of what peers hold, so three
sequential joins reach `11T/6` by design. The spec was simultaneously promising a
hard total bound, "a busy instance may use everything idle peers aren't", and "no
instance is starved" — mutually incompatible without a central allocator every
instance must synchronously ask.

**The change: grant `total / live_count`; delete the ledger.** The policy now
needs only the *count* of live instances, which the instance lock files already
give exactly and for free. Nothing is published, nothing goes stale, and there are
no failure paths to get wrong.

Deleted: `crates/krometrail-store/src/budget_registry.rs` (318 lines, including
6 unit tests), its module declaration and `BudgetRegistry`/`BudgetShare` exports,
`BUDGET_SHARE_REFRESH` / `BUDGET_SHARE_REFRESH_DIVISOR` / `BudgetShareCache` and
the `budget_share` cache field, `effective_budget_at`, `effective_budget_for_append`,
`republish_budget_share`, `observed_usage`, and the pending-bytes reservation
threading through `ensure_append_capacity`. `effective_budget()` is now nine
lines. Net: about 480 lines removed, about 45 added.

Documented in `docs/SPEC.md`: each instance enforces `<= total/N` at every write,
so once every instance has performed one operation since the last join, combined
usage is `<= total`. The operation-driven carve-out is kept and stated plainly, as
is the accepted cost — two live instances each get `total/2` even if one is idle.
