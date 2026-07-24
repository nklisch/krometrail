---
id: feature-retention-trim-transparency
kind: feature
stage: done
tags: [store]
parent: null
depends_on: []
release_binding: 1.6.1
gate_origin: null
created: 2026-07-23
updated: 2026-07-23
---

# Retention trim correctness and transparency

## Brief

In-session retention trimming evicted freshly generated artifacts while every
agent-visible signal said retention was healthy. Found during the v1.6.0
shakedown under sustained ~100 fps WebGL ingest (~18 MB/s, 25k frames,
4.16 GB stored of a 10 GB configured budget). Grounding against the code shows
two of the three surprises are defect-shaped:

- **Phantom-instance budget halving.** `effective_budget()` divides the
  configured budget by census `live_instances()`. The session enforced
  85% × (10 GB ÷ 2) = 4.25 GB because the census counted a second live
  instance — almost certainly the pre-restart server whose recording cache the
  startup log had already reclaimed as abandoned
  (`retention.instance_reclaimed`). A lone session should trim at 8.5 GB.
  `browser_status` also reports the configured budget (10 GB) while enforcing
  the effective one (5 GB), which is actively misleading.
- **Hollow artifact grace.** `DEFAULT_ARTIFACT_GRACE` (15 min) exists "so a
  returned resource link is not already dying when the agent receives it", but
  the strictly oldest-first reclaim walk hits the segments backing fresh
  artifacts first (agents naturally derive artifacts from the oldest retained
  window — investigate what just happened, keep recording) and the
  `artifact_grace_overridden` path then evicts them anyway. Observed: artifacts
  1–4 minutes old evicted; `usage.artifact_bytes` ended at 0.
- **No trimming signal.** `RecordingBudgetState` only knows
  Available/PausedBudget; continuous in-session trimming surfaced nowhere —
  `budget_state: "available"`, `eviction_blocked: false` throughout.

Also folded in (same surface, found same pass): `resolve_temporal_range`'s
`capture_quality.capture_status.at_range_start/at_range_end` always echo the
session-initial all-zero status block (session_time ~11 ms) instead of the
capture status at those range bounds.

Eviction throughput itself is excellent (trims interleaved with zero capture
disruption; drops stayed at 0.5% attributed queue blips) — the fixes here are
correctness and transparency, not performance.

## Strategic decisions

- **Grace policy — skip and reclaim newer**: the reclaim walk treats artifact
  grace as an ordering exception: skip graced artifacts and their backing
  segments and reclaim the next-oldest instead. Override grace only when
  nothing else is reclaimable (true emergency), and surface that override as an
  explicit response warning. — Makes the documented grace real in the common
  agent pattern.
- **Budget split — fix staleness, keep equal split**: root-cause and fix the
  stale live-instance census count (an instance whose cache was reclaimed as
  abandoned must not still count as live); keep the deliberate equal-split
  policy; surface `effective_budget` and `live_instances` in `browser_status`
  so the enforced number is visible. — Equal split stays for predictability
  (prior review history), the bug and the opacity go.
- **Signaling — informational, not alarming**: `browser_status` gains a
  trimming/pressure state plus effective budget and instance count; temporal /
  artifact / query responses note active trimming calmly with a concrete
  how-far-back reference (the trimmed-through boundary / oldest-retained
  session time or index) so range work is never surprised. Tone: a factual
  note, not a warning klaxon — most sessions on ordinary pages will never hit
  the limit. Recovery text may name `pin_resolved_range` where relevant.

## Architectural choice

Five goals, but two of them (census, capture-status echo) are self-contained and
three (grace ordering, status transparency, trim-aware temporal notes) share one
signaling substrate: the `RetentionStatus` / `RetentionWarning` wire contracts.
The design keeps the two independent fixes independent and routes the three
signaling goals through one substrate so the same enforced numbers, trim state,
and trimmed-through boundary reach every surface identically.

### Root-cause grounding (verified against code, not just the Brief)

- **Census phantom (goal 1).** The Brief's "second live instance" is the census's
  monotonic floor, not a live-count error on the fresh path.
  `InstanceCensus::live_instances` (`crates/krometrail-store/src/instance.rs:673`)
  returns the *fresh* `count_live()` whenever enumeration succeeds, and
  `count_live` already claims each sibling lock, so a departed peer is proven dead
  and the count descends correctly — the existing tests
  `a_dead_instance_root_does_not_count_toward_the_live_set` and
  `a_retained_directory_handle_enumerates_after_permissions_change`
  (`crates/krometrail-store/tests/shared_budget.rs`) pin exactly that. The defect
  is `proved_live` (`instance.rs:576`): it is a *monotonic maximum*
  (`fetch_max`, "only ever rises") consulted only when an enumeration *fails*
  (`instance.rs:674`). A transient startup overlap (restart while the predecessor's
  `flock` has not yet released, or a plugin/standalone coexistence) latches `2`.
  Startup already reclaims abandoned roots *before* the census is built
  (`src/app.rs:403` runs `reclaim_abandoned_instances` ahead of
  `InstanceCensus::new` at `:434`), so the clean-directory baseline is right — but
  once `proved_live` has latched `2`, any later enumeration hiccup under sustained
  ~100 fps load (a `dup`/`fdopendir` failure under FD pressure →
  `entry_names` returns `None` → path fallback also fails → `count_live` returns
  `None`) resurfaces the stale `2` for the rest of the session. That is the
  observed persistent `total/2`. The floor never *descending* is the whole bug: a
  successful enumeration that claims a formerly-live sibling is positive proof it
  is gone, so the floor may safely track a freshly-proven *lower* count.

- **Hollow grace (goal 2).** `oldest_reclaimable_artifact`
  (`crates/krometrail-store/src/index/retention.rs:384`) takes no grace parameter
  and orders by `start_time_be` (session time of the derived window), while the
  grace filter (`artifact_grace_since_unix_ms`, `created_unix_ms >= grace_since`)
  is applied *only* to the segment query (`:262`). Tier 1 of `reclaim_once`
  (`crates/krometrail-store/src/recording.rs:929`) therefore evicts the oldest
  artifact regardless of grace; because agents derive artifacts from the *oldest*
  retained window, that oldest artifact is precisely the fresh one. Once the
  artifact row is gone the backing segment is no longer "backing a recent
  artifact", so the segment-tier grace no longer protects it either. Grace is thus
  bypassed entirely for the common pattern. The existing override
  (`recording.rs:946`) only ever fired for segments and so masked the fact that
  the artifact tier never honored grace at all.

- **No trim signal (goal 3).** `RecordingBudgetState`
  (`crates/krometrail-core/src/recording/retention.rs:113`) is Available /
  PausedBudget only; continuous high-water trimming (`trim_locked`,
  `recording.rs:709`) is invisible, and `RetentionStatus.configured_budget`
  reports the *configured* total (`recording.rs:617`) while enforcement uses
  `effective_budget()` (`recording.rs:828`) — the actively-misleading gap.

- **capture_status echo (goal 5).** `capture_status_changed` fires only on a
  capture-state *transition* (`crates/krometrail-cdp/src/capture/pipeline.rs:433`
  returns early when `next == state.state`), and the emitted
  `TargetCaptureStatus` (`crates/krometrail-core/src/recording/session.rs:365`)
  is a full snapshot — statistics, queue depth, cadence — frozen at that instant.
  `capture_status_evidence` (`crates/krometrail-core/src/timeline/context.rs:1258`)
  sets `at_range_start` from `samples.at_or_before()` and `at_range_end` from the
  last in-range transition (falling back to `at_range_start`). For a steadily
  capturing session the only transition is the session-start
  Idle→Capturing at ~11 ms, so both bounds echo that snapshot with all counters
  at ~0. The store retains *transitions*, not per-frame status; the honest bound
  reading is the *state in effect* plus *when it took effect*, never the frozen
  counters.

### Options considered for the shared signaling substrate

**Option A — new bespoke top-level response field on each temporal/artifact/query
tool.** Cleanest provenance (global store state, not range-relative), but adds a
new wire object and schema surface to 3–4 tools and duplicates the
"trimmed-through boundary" concept that `RetentionWarning` already expresses via
its `oldest_retained` fields. Rejected as heavier than the contract warrants.

**Option B — extend the existing `RetentionWarning` enum and inject the note at
the MCP handler.** `RetentionWarning`
(`crates/krometrail-core/src/timeline/range.rs:448`) already rides
`CaptureQuality.retention_warnings`, already carries `oldest_retained` in several
variants, and is already the "retention affected this evidence" channel. Its one
awkwardness is provenance: the core range resolver computes the existing variants
from request-vs-retained and does not know the store's live trim state. Resolve
that not by teaching the resolver about the store, but by *injecting* the trim /
override note at the MCP tool handler, which already fetches session status
alongside the resolved result (`call_resolve_temporal_range` →
`capture_health(sessions)`, `crates/krometrail-mcp/src/registry.rs:610`). The
handler pushes the variant into the result's `retention_warnings` *before*
projection, so the existing count/serialize path (`response.rs:1739`, `:1818`)
stays consistent. **Chosen** — reuses the existing shape end-to-end (the stated
preference), keeps the core resolver clean, and keeps counts honest.

**Option C — surface everything only through `browser_status` and make agents
poll.** Rejected: the locked decision requires the temporal/artifact/query
responses themselves to carry the note so range work "is never surprised";
polling a second tool is exactly the surprise it removes.

### Chosen shape

1. `RetentionStatus` (core) gains `effective_budget: u64`, `live_instances: u64`,
   `trim_state: RecordingTrimState`, and `grace_override_active: bool`. A new
   `RecordingTrimState { Steady, Trimming }` mirrors `RecordingBudgetState`'s serde
   shape (`#[serde(rename_all = "snake_case")]`, so it is an explicitly-tagged
   contract the wire-enum guard accepts, not a bare identifier-publishing derive).
   `budget_state` (the hard pause) stays orthogonal: a store can be `Available`
   *and* `Trimming` at once.
2. The store fills these in `status_from_snapshot` from the census and the
   high-water threshold it already computes.
3. `browser_status` projects all four onto both the Full path (serialized
   directly) and the Concise/Expanded `ConciseRetentionStatus`.
4. `RetentionWarning` gains `InSessionTrimmingActive { oldest_retained }` and
   `ArtifactGraceOverridden { oldest_retained }`; the temporal/artifact/query MCP
   handlers inject them from the fetched `RetentionStatus`.
5. Census: `proved_live` becomes a *last-successfully-proven* count (store on
   success, fall back on failure) rather than an all-time maximum. This descends
   on positive proof, is still fail-closed on enumeration failure (falls back to
   the last real count, never an optimistic 1), and keeps every existing
   shared-budget test green.
6. capture_status: `at_range_start`/`at_range_end` become a bound-honest shape
   carrying `{ state, established_at, attachment_generation }` only; `transitions`
   keeps the full points (each honest at its own time).

## Implementation Units

Grounded file paths and Rust signatures. Cargo builds are owned by a concurrent
job; do not run them — these are derived from reading the code.

### Unit 1 — Census staleness fix (story A)

`crates/krometrail-store/src/instance.rs`

- Rename `proved_live: AtomicU64` → `last_proven_live: AtomicU64` and rewrite its
  doc: it is the last *successfully proven* live count, the fail-closed fallback
  for a failed enumeration, and it tracks proven counts in *both* directions.
- `live_instances(&self) -> u64` (`:673`): on the success branch replace
  `self.proved_live.fetch_max(live, Ordering::Relaxed)` with
  `self.last_proven_live.store(live, Ordering::Relaxed)`; keep `live` as the
  return. Failure branch unchanged: `self.last_proven_live.load(...).max(1)`.
- `new()` seeding (`:604`) unchanged in structure: first `count_live()` seeds the
  field; a construction-time enumeration failure still stores
  `ASSUMED_LIVE_INSTANCES_WITHOUT_EVIDENCE`. (Because `store` now overwrites, the
  first successful post-construction count corrects a stale assumption too.)
- Update the module/field comments that assert "only ever rises" / "narrows"
  (`:570-575`) to the descend-on-proof semantics. Keep the
  `ASSUMED_LIVE_INSTANCES_WITHOUT_EVIDENCE` never-enumerated behavior and its
  rationale intact.

No wire change, no schema regen. Equal split preserved (this only fixes the
divisor `N`).

### Unit 2 — RetentionStatus signaling substrate + browser_status (story B)

`crates/krometrail-core/src/recording/retention.rs`

- New enum:
  ```rust
  #[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
  #[serde(rename_all = "snake_case")]
  pub enum RecordingTrimState { Steady, Trimming }
  ```
- `RetentionStatus` gains fields `effective_budget: DiskBudgetBytes` (matching
  `configured_budget`), `live_instances: u64`, `trim_state: RecordingTrimState`,
  `grace_override_active: bool`. Extend `RetentionStatus::new(...)`, the `Wire`
  struct, its validated `Deserialize`, and `empty()` (effective = configured,
  live = 1, trim = Steady, grace = false).
- New validation invariants in `new()`: `effective_budget <= configured_budget`;
  `live_instances >= 1`. Do *not* couple `trim_state` to `budget_state` — they are
  independent.

`crates/krometrail-store/src/recording.rs`

- `status_from_snapshot(&self, snapshot, state)` (`:602`): compute
  `let effective = self.effective_budget();`
  `let live = self.live_instances();`
  `let high_water = self.retention.trim_high_water_bytes(effective);`
  `let trim_state = if snapshot.usage.total_bytes()? >= high_water { Trimming } else { Steady };`
  and read the latched `grace_override_active` (see Unit 3). Pass all four into
  `RetentionStatus::new`. `configured_budget` stays `self.retention.budget()`.
- `status()` (`:2420`) does not take the mutation gate; `live_instances()` /
  `effective_budget()` are lock-free census reads (already how `enforce_locked`
  reads them), so status stays non-blocking.

`crates/krometrail-mcp/src/response.rs`

- `ConciseRetentionStatus` (`:110`) gains `effective_bytes: u64`,
  `live_instances: u64`, `trim_state: RecordingTrimState`,
  `grace_override_active: bool`. Populate in `map_browser_status` Concise/Expanded
  arm (`:717`). The Full arm serializes `RetentionStatus` directly, so the new
  core fields appear automatically; `project_retained_bounds` (`:769`) is
  unaffected (it only rewrites the retained endpoints).

`crates/krometrail-mcp/src/session.rs`, `crates/krometrail-mcp/src/server.rs`,
`crates/krometrail-core/src/ports/mod.rs` — update every `RetentionStatus::new` /
`::empty` construction site (server.rs `:544`, `:2533`; empty sites server.rs
`:874`,`:2611`, session.rs `:409`, ports/mod.rs `:189`, response.rs test `:3599`).

Schema: `RecordingTrimState` and the new fields flow through the schemars-derived
tool response schema. Run `bash scripts/check-wire-enum-schemas.sh` (the new enum
is `rename_all`-tagged, so it passes as an explicit contract). Update the
schema.rs structural assertions if they enumerate retention fields; the
temporal-evaluation canonical artifacts do not reference these types, so no
regeneration there.

### Unit 3 — Grace as an ordering exception + override warning (story C)

`crates/krometrail-store/src/index/retention.rs`

- `oldest_reclaimable_artifact(&self, excluded, created_before_unix_ms,
  artifact_grace_since_unix_ms: Option<i64>)`: add the grace parameter and a
  `AND (?3 IS NULL OR created_unix_ms < ?3)` clause so a graced (recently
  published) artifact is skipped and the next-oldest reclaimable artifact is taken
  instead. Update the thin callers `oldest_artifact` / `oldest_artifact_excluding`
  (`:369`,`:373`) to pass `None`.

`crates/krometrail-store/src/recording.rs`

- `reclaim_once` (`:920`): thread `filter.artifact_grace_since_unix_ms` into the
  artifact-tier call. Restructure the override so it wraps *both* tiers: attempt
  the normal artifact→segment(+event) selection with grace applied; only if that
  selects nothing, `under_pressure` is true, and grace was active, retry the whole
  selection with grace dropped and set `outcome.artifact_grace_overridden = true`.
  Keep the single unified walk (the `reclaim` loop at `:883` is untouched) — this
  is still one ordered walk; grace is a candidate-set narrowing, exactly as the
  `SegmentReclaimFilter` doc already frames it. Preserve absolute pin protection
  (pins are excluded in the SQL unconditionally and are never part of the grace
  path).
- Add a latched `grace_override_active: StdMutex<bool>` on `RecordingStore`
  (mirror `trim_exhausted`, `:179`). Set it true whenever a reclaim outcome has
  `artifact_grace_overridden`; clear it when a reclaim makes progress *without*
  override, or when `trim_locked` finds usage back below high-water. Read it in
  `status_from_snapshot` (Unit 2). Keep the existing
  `retention.artifact_grace_overridden` tracing event.

`crates/krometrail-mcp/src/registry.rs` + `response.rs`

- `generate_artifacts` path: when its capacity reclaim
  (`ensure_staged_artifact_capacity` → `cleanup_to`) overrode grace, the response
  carries `RetentionWarning::ArtifactGraceOverridden`. Simplest coherent wiring:
  the handler reads the post-operation `RetentionStatus.grace_override_active`
  (already latched) and injects the warning — the same injection path Unit 4 uses —
  so append-path and artifact-path overrides surface identically without a bespoke
  return channel from `cleanup_to`.

### Unit 4 — Trim-aware temporal/artifact/query notes (story D)

`crates/krometrail-core/src/timeline/range.rs`

- `RetentionWarning` (`:448`) gains:
  ```rust
  InSessionTrimmingActive { oldest_retained: SessionTime },
  ArtifactGraceOverridden { oldest_retained: SessionTime },
  ```
  `oldest_retained` is the trimmed-through boundary — the oldest retained session
  time for the response's scope. Both are calm, factual variants (no klaxon text;
  tone lives in the plugin/docs, not the enum).

`crates/krometrail-mcp/src/registry.rs`

- Minimal coherent set: `resolve_temporal_range` (`call_resolve_temporal_range`,
  `:573`), `generate_artifacts`, and `query_browser_events`. `temporal_debug_bundle`
  composes resolve + context and inherits the note through the same result shape.
- After the operation resolves, fetch the `sessions` retention status (the handler
  already calls `capture_health(sessions)`; add a sibling
  `retention_status(sessions)` read). When `trim_state == Trimming`, push
  `InSessionTrimmingActive { oldest_retained }`; when `grace_override_active`, push
  `ArtifactGraceOverridden { oldest_retained }`, into the result's
  `capture_quality.retention_warnings` before `map_temporal_range_resolution_result`
  / the artifact / event mapper runs, using `RetentionStatus.oldest_retained` for
  the scope. The concise `retention_warning_count` (`response.rs:1739`) then counts
  them naturally.
- Omit both notes when the scope has no `oldest_retained` (empty store) — there is
  no honest boundary to name, so no note.

### Unit 5 — capture_status echo honesty (story E)

`crates/krometrail-core/src/timeline/context.rs`

- Replace the bound projections in `CaptureStatusEvidence` (`:481`):
  ```rust
  pub struct CaptureStatusBound {
      pub state: CaptureStreamState,
      pub established_at: SessionTime,      // session time of the transition in effect
      pub attachment_generation: u64,
  }
  pub struct CaptureStatusEvidence {
      pub at_range_start: Option<CaptureStatusBound>,
      pub at_range_end: Option<CaptureStatusBound>,
      pub transitions: Vec<CaptureStatusPoint>, // unchanged; each honest at its own time
  }
  ```
- `capture_status_evidence` (`:1258`): build the bounds from the establishing
  transition's `state` / `session_time` / `attachment_generation` (drop the
  counter-bearing `TargetCaptureStatus`). `at_range_end` still falls back to
  `at_range_start`'s bound when no in-range transition exists. Ordering/consistency
  checks and the `CaptureQualityWarning` logic are unchanged. Do **not** synthesize
  counters for the bound — omission of the frozen snapshot is the honesty fix.
- Update the MCP response projection of `capture_status` and any schema.rs
  assertion that reaches into `at_range_start.status`. Run
  `bash scripts/check-wire-enum-schemas.sh` (no new bare enum; `CaptureStreamState`
  already tagged).

## Implementation Order

Dependency spine: A and E are independent leaves; B is the shared substrate that C
and D build on.

1. **Story A — census** (`feature-…-census-staleness`). No wire change; unblocks
   B's correct `live_instances`.
2. **Story B — retention status substrate + browser_status**
   (`feature-…-status-transparency`). depends_on A. Lands the wire fields, the
   trim state, and the schema regen — the single wire-schema chokepoint.
3. **Story C — grace ordering + override warning**
   (`feature-…-grace-ordering`). depends_on B. Uses B's `grace_override_active`
   status and the override warning surfacing.
4. **Story D — trim-aware temporal notes** (`feature-…-temporal-trim-note`).
   depends_on B. Injects the trimming/override warnings at the MCP handlers.
   (C and D both depend on B and may proceed in parallel after it; D does not
   depend on C — it reads the same latched status.)
5. **Story E — capture_status echo honesty** (`feature-…-capture-status-echo`).
   Independent; may land any time. Separate wire change from B, so schedule it so
   the two schema regens do not collide (sequence E after B, or before A — just
   not concurrent with B).

Child stories are warranted here: B is a shared wire/schema checkpoint that two
downstream goals build on, and getting B's schema regeneration settled before C/D
avoids re-regenerating twice. A and E are genuinely separable leaves. Five
stories, one per goal, with an explicit `depends_on` chain.

## Simplification

- **One override channel, not two.** Rather than returning the grace-override flag
  up through `cleanup_to` → `ensure_staged_artifact_capacity` → the artifact
  handler *and* separately through the append path, the store latches
  `grace_override_active` once and every surface (browser_status, temporal notes,
  generate_artifacts) reads that one latch. No new return plumbing through the
  reclaim call stack.
- **Trim state is derived, not stored.** `trim_state` is computed in
  `status_from_snapshot` from `usage >= high_water(effective)`; it needs no new
  state machine or event — the high-water threshold already exists
  (`trim_high_water_bytes`, retention.rs:191).
- **Reuse the existing warning shape** (Option B) instead of a new response object,
  and reuse the existing `capture_health` fetch point for the status read rather
  than a new port round-trip.
- **Census fix is a one-line semantic change** (`fetch_max` → `store`) plus a
  rename and doc rewrite; no new fields, no ordering change (startup reclaim
  already precedes census construction).
- **Bound-honest capture status drops a field rather than adds machinery** — the
  simplest honest projection is to stop reporting what the store never measured.

## Testing

Smallest useful, behavior-pinning. No line-coverage padding.

- **Census (store unit / `tests/shared_budget.rs`).**
  - New: a census that proved `2` (peer live), then the peer exits and one
    successful enumeration returns `1`, must report `1` even on a *subsequent*
    forced-enumeration-failure — i.e. the last-proven floor descended. This is the
    regression the current monotonic max fails. Build on the existing
    permissions-fault harness (`a_failed_census_does_not_widen_a_share`) but drop
    the peer *before* breaking enumeration.
  - Keep green: `a_dead_instance_root_does_not_count_toward_the_live_set`,
    `two_live_instances_each_enforce_half_the_total`,
    `three_live_instances_each_enforce_a_third_of_the_total`,
    `a_failed_census_does_not_widen_a_share` (peer still present → still `2`),
    `a_census_that_never_enumerated_does_not_grant_the_whole_total`.
  - Assert the locked outcomes directly: reclaimed/departed abandoned instance not
    counted → lone survivor `live_instances() == 1` and gets the full total; two
    genuinely-live instances still split (`== 2`).

- **Grace ordering (store unit).**
  - Publish an artifact derived from the oldest retained window, then drive budget
    pressure: the fresh artifact and its backing segment survive while an
    older *non-graced* object is reclaimed instead (next-oldest taken).
  - Emergency override: pin/consume everything else so only graced objects remain,
    then one more unit of pressure drops grace, evicts the graced object, and sets
    `grace_override_active` (assert the flag and the `retention.artifact_grace_overridden`
    event still fires).
  - Pins beat grace and pressure absolutely: a pinned graced segment is never
    evicted even in the override path.

- **Status transparency (store + mcp).**
  - `status()` on a store past high-water reports `trim_state == Trimming`,
    `effective_budget == configured / live`, `live_instances == N`; below
    high-water reports `Steady`.
  - `browser_status` Concise and Full both expose `effective_bytes`,
    `live_instances`, `trim_state`, `grace_override_active`; the Full path keeps
    `retained_bounds` projection.

- **Trim-aware notes (mcp).**
  - With the store `Trimming`, `resolve_temporal_range` /
    `query_browser_events` / `generate_artifacts` carry
    `InSessionTrimmingActive { oldest_retained }` with the oldest-retained session
    time; `Steady` carries none. After a grace override, the same responses carry
    `ArtifactGraceOverridden`. Empty store → no note (no honest boundary).

- **capture_status echo (core unit, context.rs `mod tests`).**
  - A range 20 s into a steady-capturing session whose only transition is the
    session-start Idle→Capturing reports `at_range_start.state == Capturing` and
    `established_at == 11 ms` (the transition time) and carries **no** counter
    snapshot — pinning that the frozen all-zero statistics are gone.
  - A range spanning a mid-session Capturing→Paused transition reports that
    transition in `transitions` (full point, honest at its time) and the correct
    bound state at each end.

- **Schema (mcp).** `bash scripts/check-wire-enum-schemas.sh` clean; the
  schemars-derived tool-response schema tests in `schema.rs` updated for the new
  retention fields / capture-status bound shape and asserting structure
  (byte/shape equality where those tests already assert it).

## Risks

- **Single-writer reducer invariant (`single-writer-effect-reducer`).** All new
  store state (`grace_override_active`) is set under the same paths the mutation
  gate already serializes (`reclaim`, `trim_locked`), mirroring `trim_exhausted`.
  It is *read* in `status()` without the gate, exactly as `budget_state` and the
  census already are — a lock-free bool read, no torn state. Do not read it under
  a different lock ordering than `trim_exhausted`.

- **Durability barriers from the 1.6.0 store work must not regress.** The reclaim
  restructure (Unit 3) changes only *candidate selection order*, never the
  deletion/journal path: `remove_objects` → `prepare_deletion` →
  `checkpoint_truncate` → stage → finalize is untouched, so the prepared-journal
  ordering that survives power loss is preserved (see
  `.work/active/features/feature-perf-store-ingestion-accounting.md` if the
  barrier details are needed). Grace filtering is a `WHERE`-clause narrowing on
  read queries only.

- **Census safety trade (last-proven floor).** Descending the floor on a proven
  lower count reopens one narrow window the monotonic max closed absolutely: a
  peer that *joins* during a sustained enumeration outage is unseen until the
  outage clears, so this instance may briefly enforce `total` while the newcomer
  (whose own census *can* see this live instance) conservatively enforces
  `total/2` — combined ≤ `3T/2`. That is exactly the transient overshoot the SPEC
  already documents and accepts (docs/SPEC.md §"The carve-out, stated honestly"),
  and it self-corrects on the next successful enumeration. It is strictly better
  than permanently stranding every restarted lone instance at half budget. Named
  and accepted; call it out for reviewers so the SPEC prose is re-read, not
  silently contradicted.

- **Wire-schema coordination.** Two independent wire changes (B: retention fields;
  E: capture-status bound). Sequence their schema regenerations so they do not
  collide (Implementation Order step 5). Both are additive to agent consumers, but
  E *removes* the counter snapshot from `at_range_start`/`at_range_end` — a
  deliberate contract change under the current-contract-discipline (no shim; the
  old frozen-counter shape had no honest consumer). Ensure any doc/example that
  showed `at_range_start.status.statistics` is updated.

- **Trim-note provenance.** The MCP-handler injection reads a `RetentionStatus`
  taken *after* the operation resolved, so the note reflects store state at
  response time, not at range-resolution time. That is the intended, honest
  reading ("trimming is active right now, oldest retained is here"); do not
  back-date it into the resolver.

- **`effective_budget <= configured_budget` invariant.** With the census fixed a
  lone instance has `effective == configured`; the new `RetentionStatus::new`
  validation must allow equality and only reject `effective > configured`
  (which would signal a census/division bug worth failing closed on).

## Implementation notes

- A complete implementation is in progress in dependency order. Checkpoint A
  changed `crates/krometrail-store/src/instance.rs` and added a focused census
  regression; its store checks passed. Stages remain unchanged per the host
  instruction, and no commit was created.
- Checkpoint B changed the core retention status contract, store status
  derivation, MCP status projection, and checkpoint tests. The wire-enum schema
  check and affected core/store/MCP checks passed; C and D now consume the
  shared status fields.
- Checkpoint C changed `crates/krometrail-store/src/index/retention.rs` and
  `crates/krometrail-store/src/recording.rs`: grace now narrows candidate
  selection, overrides only after the normal unified walk is empty under
  pressure, and latches the agent-visible override state. The focused grace
  ordering test passed.
- Checkpoint D changed `crates/krometrail-core/src/timeline/range.rs` and
  `crates/krometrail-mcp/src/registry.rs`: trim/grace notes are injected at
  the MCP response boundary using the oldest retained session time. The
  boundary/empty-scope test passed.
- Checkpoint E changed `crates/krometrail-core/src/timeline/context.rs`,
  `crates/krometrail-core/src/timeline/mod.rs`, `crates/krometrail-core/src/lib.rs`,
  and `crates/krometrail-store/tests/range_context.rs`: range bounds now echo
  only state, establishing time, and attachment generation. Generated MCP
  schema tests and the no-counter serialization assertion passed.

### Checkpoint file manifest

Implementation files changed per checkpoint (the allowed child-story note
files are recorded separately below):

- A — `crates/krometrail-store/src/instance.rs`,
  `crates/krometrail-store/tests/shared_budget.rs`.
- B — `crates/krometrail-core/src/recording/retention.rs`,
  `crates/krometrail-core/src/recording/mod.rs`,
  `crates/krometrail-core/src/lib.rs`,
  `crates/krometrail-core/src/progressive.rs`,
  `crates/krometrail-store/src/recording.rs`,
  `crates/krometrail-store/tests/shared_budget.rs`,
  `crates/krometrail-mcp/src/response.rs`,
  `crates/krometrail-mcp/src/server.rs`, `src/progressive/service.rs`.
- C — `crates/krometrail-store/src/index/retention.rs`,
  `crates/krometrail-store/src/recording.rs`.
- D — `crates/krometrail-core/src/timeline/range.rs`,
  `crates/krometrail-mcp/src/registry.rs`.
- E — `crates/krometrail-core/src/timeline/context.rs`,
  `crates/krometrail-core/src/timeline/mod.rs`,
  `crates/krometrail-core/src/lib.rs`,
  `crates/krometrail-store/tests/range_context.rs`.

The shared files listed in more than one checkpoint carry changes from each
listed checkpoint; no standalone generated JSON artifact exists for these
schemars-derived MCP contracts, so the repository's generated-schema tests
were run sequentially after B and E.

Final gate: passed with `cargo fmt --all -- --check`,
`bash scripts/check-wire-enum-schemas.sh`, locked workspace check/tests, and
locked workspace clippy (`-D warnings`). The first sandbox-only test attempt
was blocked by loopback permission in four existing CDP tests; the same gate
passed with the required loopback permission. No commit was created and all
item stages remain unchanged.

## Review fixes

- Retention warning boundaries now require the retained point's session and target to match the response scope; mismatched boundaries are omitted. The same scoped injection is applied to temporal debug bundles. Coverage includes the MCP retention-note boundary test and the full workspace suite.
- Artifact grace overrides are returned from the publication reclaim walk through `ArtifactPublish` and `ArtifactGenerationResult`, so generate-artifacts warnings use operation provenance rather than the global status latch. The store-level `pinned_grace_override_is_reported_by_the_publishing_operation` regression pins this outcome.
- Retention status derives live instances, effective budget, and trim high-water from one census snapshot. Shared-budget status and workspace tests pass.
- `docs/SPEC.md` now documents fallback to the last successfully proven count, including that a later proven departure can lower it, while enumeration failure remains fail-closed.
- Added store-level grace-ordering coverage: a fresh artifact's oldest backing segment is skipped while newer reclaimable segments are evicted; pinned grace is overridden only when it is the remaining candidate and reports through the publication; and pinned trim exhaustion short-circuits repeated walks without hanging.
- Verification passed: `cargo fmt --all -- --check`, `bash scripts/check-wire-enum-schemas.sh`, locked workspace check, locked workspace tests, and locked workspace clippy with `-D warnings`.

## Review

Cross-model static review (gpt-5.6-sol): five material findings and one
coverage gap, all accepted and fixed in-cycle — trim-note temporal boundaries
now scoped to the response's session/target (cross-session times omitted),
temporal_debug_bundle carries the same retention notes, the grace-override
warning is causally bound to the operation that forced it rather than a global
latch read, one census observation derives live_instances, effective_budget,
and the trim threshold together, SPEC updated to the last-proven census
fallback with the residual restated, and store-level end-to-end reclaim
regressions added (grace skip-ordering, emergency override reporting,
trim-exhausted latch). Reviewer confirmed the low-level deletion mechanics —
pins, unified walk, durability barriers, set-based eviction, accumulator
reconciliation — intact. Full workspace gate re-verified green by the host
after the fixes (75 suites, 0 failures).
