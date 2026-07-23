---
id: feature-temporal-partial-retention
kind: feature
stage: implementing
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

## Design decisions

1. **One uniform allow_partial clamp across all anchor kinds.** Under
   `RetentionPolicy::AllowPartial`, any candidate range that intersects
   retained bounds resolves to `intersection(candidate, retained)` with the
   existing edge + `partially_captured` warnings; the anchor kind no longer
   participates. Refusal remains only for `require_complete` overshoot and
   wholly disjoint ranges. Per-kind justification from code reading:
   - `session_time` / `wall_clock`: both materialize to an explicit
     `SessionRange` seed (`range.rs:1101-1143`); nothing about the anchor
     makes partial resolution less honest than for interactions, and the
     requested range is preserved on the response either way.
   - `interaction` / `latest_interaction`: current behavior, unchanged. Note
     `LatestInteraction` already seeds as `TemporalRangeAnchorKind::Interaction`
     (`range.rs:1247`), so the old kind-match was effectively two spellings of
     one case.
   - `navigation` / `marker`: point observations expanded by a window
     (`point_range`, `range.rs:968`); structurally identical to interaction
     windows. Clamping is semantically correct.
   - `source_frame`: NOT semantically wrong, just structurally inert — both
     endpoints must be retained frames (`frame_metadata_by_id` returns
     exactly 2 or the resolver refuses, `range.rs:1149-1158`), so the
     candidate is inside retained bounds by construction and the clamp branch
     is a no-op. If eviction races between the endpoint fetch and the
     availability fetch, clamping to what is still retained (with warnings)
     is the honest allow_partial outcome. No special case is needed, so the
     uniform rule stands unnarrowed.
2. **No compatibility path for the old refusal.** Per Current Contract
   Discipline, the "requested interval extends beyond captured source-frame
   bounds" refusal for intersecting allow_partial requests is deleted, not
   flagged or shimmed. `require_complete` behavior is byte-for-byte unchanged.
3. **Fix gap 2 by adding the missing production session-catalog writer**, not
   by weakening `live_session_now`. The resolver, the wall-clock anchor path,
   and the store tests were all written against a catalog that contains
   `RecordingSession` rows; the production writer simply never existed (see
   Root cause). Persisting the row also fixes a second latent production
   break found during root-causing: wall-clock anchors always fail in
   production today ("wall-clock resolution requires complete session
   metadata", `range.rs:1123-1128`) for the same reason.
4. **Cross-boot safety via a startup sweep, not a persisted boot id.** A
   session row surviving a process crash still says `recording`, and its
   origin came from the dead process's `Instant`-based clock — normalizing it
   against this process's clock would be garbage. At storage open, every
   non-`Ended` session row is transitioned `-> Stopping -> Ended` before any
   resolver can run. This keeps `live_session_now`'s lifecycle guard
   sufficient, adds no new persisted field, and matches the existing
   "recovery reconciles before writers exist" doctrine in
   `open_storage_with_budget`. Sweep failure fails startup (storage-integrity
   path, consistent with `recover()`).
5. **Reconnect churn is not persisted.** The catalog row is written
   `Recording` at connect and `Ended` at shutdown; transport
   reconnect transitions stay in-memory. `live_session_now` accepts
   `Starting | Recording | Reconnecting` alike, so the refinement is
   unaffected, and per-hiccup catalog writes buy nothing.
6. **Catalog statistics stay at their start-time default.** No current
   consumer reads `RecordingSession::statistics()`; live statistics remain
   the capture-status stream's job. Revisit only when a consumer appears.
7. **Idle tail: keep `partially_captured`, document it, do not suppress.**
   Bounded-loss-accounting requires that a refinement claiming
   absence-of-change be provable from capture-state evidence. At resolution
   time the durable evidence is: retained frames, declared capture gaps, and
   (post-fix) guarded session-now. A tail with no frames and no declared gap
   on a live session is *usually* an idle page, but a wedged capture stream
   that has not yet declared its gap is indistinguishable from durable
   evidence alone — suppressing the warning could hide real loss. So:
   emission logic keeps `partially_captured`; the already-shipped
   `requested_end_not_yet_elapsed` refinement (which this feature makes
   actually fire) covers the not-yet-elapsed part of a live tail; the
   elapsed idle remainder keeps `partially_captured`, and
   `docs/VISUAL-EVIDENCE.md` gains prose defining `partially_captured` as
   "frames do not cover the full requested interval" — an evidence-coverage
   statement, not a proven-loss statement. No schema change anywhere
   (validated-wire-contracts: `RetentionWarning` shapes untouched).

## Root cause (gap 2)

**Proven: `live_session_now` returns `None` at its first guard because no
production code ever writes a session catalog row.**

The chain, verified by reading the code:

- `live_session_now` (`crates/krometrail-core/src/timeline/range.rs:1440`)
  first does `self.catalog.session(session_id).await?` and returns `None`
  when the row is absent (`range.rs:1445-1447`).
- The production resolver's catalog is `SqliteIndex` directly
  (`RecordingStore::resolve_range`,
  `crates/krometrail-store/src/recording.rs:2167-2174`; wired in
  `src/app.rs:448`).
- `SqliteIndex::session` returns `Ok(None)` unless the `sessions` row has
  non-NULL `record_json`
  (`crates/krometrail-store/src/index/catalog.rs:29-53`).
- The only production writes to `sessions` are `ensure_session`'s
  `INSERT OR IGNORE ... VALUES (?1, NULL)` identity rows, created
  transactionally when targets/frames persist
  (`crates/krometrail-store/src/index/mod.rs:209-220`).
- `RecordingCatalog::put_session` has **zero production callers** — every
  call site is in store tests (`range_resolution.rs`, `sqlite_timeline.rs`,
  `temporal_queries.rs`). `RecordingSession` is never even constructed in
  `src/`, `krometrail-cdp`, or `krometrail-mcp` production code.
- The CDP session supervision mints `session_id` and `session_origin` in
  memory only (`crates/krometrail-cdp/src/session/mod.rs:327-328`) and never
  publishes a `RecordingSession`.

Therefore `requested_end_not_yet_elapsed` can never fire in production, which
exactly matches the shakedown observation (warning present in store tests
that hand-populate the catalog, absent live). The other suspects are
exonerated for this repro: `SessionOrigin::normalize` only fails when the
observed clock reads before the origin (`crates/krometrail-core/src/time.rs:69-75`)
and the resolver and session supervision share one process clock
(`src/app.rs:196-204, 251`); the `session_now < retained.end()` guard is
reachable only after a session row exists. Both stay as doubt-guards.

Corollary bug fixed by the same change: wall-clock anchor resolution
(`range.rs:1123`) requires the same missing row and therefore always fails in
production today.

## Architectural choice

Persist `RecordingSession` from the CDP session supervision through the
existing `RecordingCatalog` port (injected-core-ports: the domain contract
already exists and flows inward; the composition root wires `SqliteIndex`
in). Alternatives rejected:

- *In-memory live-session registry consulted by the resolver*: fixes only the
  refinement, leaves wall-clock anchors broken for retained cross-boot
  evidence, and adds a second session-truth authority beside the catalog.
- *Persisted boot id on `RecordingSession`*: adds a schema concept to solve
  what a startup sweep solves with existing lifecycle transitions
  (`Starting|Recording|Reconnecting -> Stopping -> Ended` are all legal rows
  in the `SessionLifecycle` table, `lifecycle.rs:59-67`).

There are no existing persisted session rows in any production deployment
(nothing ever wrote them), so this is a zero-migration change; test fixtures
already write the current format.

## Implementation Units

### IU1 — Uniform allow_partial clamp (core)

File: `crates/krometrail-core/src/timeline/range.rs`

- Replace `clamp_natural_interaction_range` (line 1660) with:

  ```rust
  fn clamp_partial_range(
      candidate: SessionRange,
      retained: SessionRange,
      options: RangeResolutionOptions,
  ) -> Option<SessionRange> {
      if options.retention != RetentionPolicy::AllowPartial
          || !ranges_intersect(candidate, retained)
      {
          return None;
      }
      Some(intersection(candidate, retained))
  }
  ```

  The `seed` parameter and `Result` wrapper drop out: the anchor-kind match
  is deleted, and `intersection` on intersecting ranges cannot fail (existing
  helper, `range.rs:1722`).
- Update the single call site in `classify_retention` (`range.rs:1586`):
  `clamp_partial_range(candidate, retained, options).ok_or_else(...)` — the
  refusal construction is otherwise unchanged and now fires only for
  `require_complete` overshoot or a disjoint candidate.
- Warning emission (`range.rs:1613-1656`) is untouched; the generalized clamp
  makes `requested_start_before_oldest_retained`,
  `requested_end_after_newest_retained`, `requested_end_not_yet_elapsed`, and
  `partially_captured` reachable for every anchor kind.

Acceptance:
- `session_time`, `wall_clock`, `navigation`, and `marker` requests whose
  edges overshoot retained bounds resolve to the retained intersection under
  `allow_partial` with the same warning set interactions get today.
- `require_complete` overshoot and wholly disjoint requests still refuse with
  the existing messages and recovery advice.
- `cargo test -p krometrail-core` and existing store suites green;
  `bash scripts/check-wire-enum-schemas.sh` clean (no schema drift).

### IU2 — Production session-catalog writer (cdp + app)

Files: `crates/krometrail-cdp/src/session/mod.rs`, `src/app.rs`

- New assembly on `ProductionBrowserConnector` (mirroring `with_capture` /
  `with_browser_events` style):

  ```rust
  pub fn with_session_catalog(
      mut self,
      catalog: Arc<dyn RecordingCatalog>,
      wall_clock: Arc<dyn WallClock>,
      disk_budget: DiskBudgetBytes,
      capabilities: Vec<CapabilityId>,
  ) -> Self
  ```

  stored as `Option<SessionCatalogAssembly>`.
- In `connect()` (`session/mod.rs`, after setup succeeds and immediately
  before `tokio::spawn(run_supervisor(...))`, ~line 467): build
  `RecordingSession::new(session_id, session_origin.observed(),
  wall_clock.now(), setup.compatibility.version.clone(), profile,
  disk_budget, capabilities, every_nth_frame)`, `transition(Recording, None)`,
  `put_session` once. A persistence failure fails the connect explicitly —
  if the index cannot take a session row, frame persistence is broken too.
  Keep the record (`Mutex<RecordingSession>`) plus the assembly handle on
  `SessionShared` for the terminal write.
- At session end — in the supervisor shutdown path where
  `BrowserSessionState::Ended` is committed (covers both driven `stop()` and
  spontaneous browser death; `session/shutdown.rs` / `run_supervisor`):
  `transition(Stopping, None)` then
  `transition(Ended, Some(max(wall_clock.now(), started_at)))`, `put_session`
  best-effort with a `tracing::warn!` on failure (the startup sweep of IU3 is
  the backstop; shutdown reporting must not be blocked on a catalog write).
- `src/app.rs::build_runtime`: create `SystemWallClock` before the connector,
  then chain `.with_session_catalog(Arc::clone(&storage.catalog),
  Arc::clone(&wall_clock), budget, mcp_config-enabled capability ids)` —
  budget and `mcp_config` are already in scope (`app.rs:201-213`).

Acceptance:
- After `start_browser`/`attach_browser`, the index holds a `RecordingSession`
  row with lifecycle `recording`, the minted origin, and `ended_at: None`.
- After `stop_browser` (or browser death), the row reads `ended`, with
  `ended_at >= started_at`.
- A live `latest_interaction` resolve with a future `after_ms` now emits
  `requested_end_not_yet_elapsed` end-to-end; wall-clock anchors resolve in
  production.

### IU3 — Startup sweep of dangling sessions (store + app)

Files: `crates/krometrail-store/src/index/catalog.rs` (or `index/mod.rs`),
`src/app.rs`

- New inherent method (not a port method — no domain consumer):

  ```rust
  impl SqliteIndex {
      pub fn end_dangling_sessions(&self, now: SystemTime)
          -> krometrail_core::Result<u64>
  }
  ```

  Reads every non-NULL `record_json` session, and for each with lifecycle
  != `Ended`: `transition(Stopping, None)` (from `Starting`, `Recording`, or
  `Reconnecting` — all legal) then
  `transition(Ended, Some(max(now, started_at)))`, rewrites the row, returns
  the count. Errors propagate.
- Call from `open_storage_with_budget` (`src/app.rs`, directly after
  `recover(...)`, ~line 411) with the wall clock's `now()`, logging the swept
  count at `info` when non-zero. This runs before any resolver or writer
  exists, so no live session can be swept.

Acceptance:
- A `recording` row left by a crashed process reads `ended` after the next
  storage open, and `live_session_now` returns `None` for it (no cross-boot
  origin is ever normalized against this process's clock).
- Fresh-session flow (IU2) is unaffected: connect happens after storage open.

### IU4 — Foundation-doc prose (SPEC + VISUAL-EVIDENCE)

Files: `docs/SPEC.md` (~line 519), `docs/VISUAL-EVIDENCE.md`

- Rewrite the SPEC natural-anchor paragraph: allow_partial clamping is
  uniform across every anchor kind (any requested range intersecting
  retained capture resolves to the exact intersection with affected-edge and
  `partially_captured` warnings, preserving the requested range and anchor
  identity); refusal remains for `require_complete` and ranges wholly
  disjoint from retention. Delete "Explicit ranges ... remain exact
  failures" — replace in place, no "previously" prose.
- `docs/VISUAL-EVIDENCE.md`: add the `partially_captured` semantics prose
  from Design decision 7 — coverage statement vs. proven loss, and how
  `requested_end_not_yet_elapsed` refines a live not-yet-elapsed tail while
  an elapsed idle tail retains the warning.
- Regenerate `docs/public/llms-full.txt` via `bun run docs:build` (never
  hand-edited).

Acceptance: no stale assertion about interaction-only clamping or exact
explicit-range failure remains in `docs/`.

### IU5 — Tests

See `## Testing` for the exact scenario list; tests land with their units
(IU1 tests with IU1, etc.), and any existing assertion of the old
session_time refusal under allow_partial is updated in place, not preserved.

## Implementation Order

1. **IU1** — core clamp generalization + its unit/store tests. Independent,
   immediately shippable.
2. **IU3** — dangling-session sweep + store tests. Landing the sweep before
   the writer means no window in which a crash can strand a live-looking row.
3. **IU2** — session-catalog writer + cdp/app wiring + supervision tests.
4. **IU4** — docs prose + regeneration, closing the stride code-first.
5. Full gate: `cargo fmt --all -- --check`,
   `bash scripts/check-wire-enum-schemas.sh`,
   `cargo check/test/clippy --workspace --all-targets --locked`.

## Simplification

- `clamp_natural_interaction_range` (kind-matched, seeded, `Result`-wrapped)
  collapses into the 8-line kind-agnostic `clamp_partial_range`; the
  interaction special case and its dead `LatestInteraction` match arm are
  deleted.
- The "extends beyond captured source-frame bounds" refusal narrows to two
  genuinely exceptional cases (require_complete, disjoint) instead of being
  the default for four of six anchor kinds.
- No new port, schema, or persisted field: the fix for gap 2 is a writer for
  a contract that already exists end-to-end (type, port, SQL row, read path,
  tests).
- The `seed` parameter and error plumbing around the clamp disappear.

## Testing

### Unit tests — `crates/krometrail-core/src/timeline/range.rs` tests module

1. `clamp_partial_range_is_anchor_kind_agnostic_and_intersects` — direct:
   allow_partial + intersecting candidate returns the exact intersection
   (left overshoot, right overshoot, both); `require_complete` returns
   `None`; a disjoint candidate returns `None`. (Kind-uniformity is now
   structural — the function cannot see the anchor kind — so this pins the
   policy/intersection contract.)
2. `classify_retention_clamps_session_time_candidates_with_full_warning_set`
   — in-module call: session_time seed overshooting retained end yields
   resolved = intersection plus `requested_end_after_newest_retained` +
   `partially_captured`; with `session_now: Some(now)` where
   `requested.end > now >= resolved.end`, additionally
   `requested_end_not_yet_elapsed`; with `session_now: None`, the refinement
   is absent and retained-truth warnings stand alone.
3. `classify_retention_refuses_disjoint_and_require_complete_overshoot` —
   the two surviving refusal paths keep their messages and captured-bounds
   recovery advice.

### Store tests — `crates/krometrail-store/tests/range_resolution.rs`

Existing fixture (frames at session times 1, 5, 5, 10; origin 0; session row
lifecycle `Starting`; `resolver_at(now)` injectable clock):

4. `session_time_tail_overshoot_resolves_partial_with_not_yet_elapsed` —
   gap 1 + gap 2 regression: session_time anchor `[1, 30]`, allow_partial,
   `resolver_at(12)` → resolved `[1, 10]`; warnings contain
   `requested_end_after_newest_retained`, `partially_captured`, and
   `requested_end_not_yet_elapsed { session_now: 12 }`; requested range
   preserved on the response.
5. `wall_clock_tail_overshoot_resolves_partial` — same shape via the
   wall_clock anchor (start epoch+1ns, end past retained), proving the
   second explicit-interval kind clamps.
6. `navigation_and_marker_windows_clamp_like_interactions` — navigation and
   marker anchors with windows extending past both retained edges resolve to
   the retained intersection with both affected-edge warnings.
7. `disjoint_and_require_complete_requests_still_refuse` — session_time
   `[20, 30]` (allow_partial) refuses not_found; `[1, 30]` under
   require_complete refuses with captured-bounds recovery advice.
8. `absent_session_record_omits_not_yet_elapsed_refinement` — root-cause
   regression that catches the proven cause and the guard family: a fixture
   variant that appends frames but never calls `put_session` (mirroring the
   pre-fix production store) resolves the partial tail with
   `requested_end_after_newest_retained` + `partially_captured` and WITHOUT
   `requested_end_not_yet_elapsed` — pinning the documented degraded
   behavior whenever the session row is missing, ended, or unsound.
9. `end_dangling_sessions_marks_prior_boot_sessions_ended` — IU3: put a
   `recording`-lifecycle row, run `end_dangling_sessions`, read back `ended`
   with `ended_at >= started_at`; a subsequent live-tail resolve omits the
   refinement. Also: an already-`ended` row is untouched and counted zero.
10. `live_elapsed_idle_tail_keeps_partially_captured_without_refinement` —
    Design decision 7 boundary: request `[1, 12]` with `resolver_at(12)`
    (requested end == session_now, last frame at 10) → resolved `[1, 10]`
    with `requested_end_after_newest_retained` + `partially_captured` and no
    `requested_end_not_yet_elapsed` (nothing is claimed about a tail that
    has fully elapsed).

### Supervision tests — `crates/krometrail-cdp/tests/session_supervision.rs`

11. `connect_persists_recording_session_and_shutdown_ends_it` — IU2 with the
    deterministic transport doubles: a capturing fake `RecordingCatalog`
    records `put_session` calls; after connect the row is lifecycle
    `recording` with the session's minted id/origin/every_nth_frame and
    `ended_at: None`; after stop the final write is `ended` with a valid
    `ended_at`. A connect-time `put_session` failure fails the connect.

### Runtime smoke

No change to `tests/rust-runtime-smoke.rs` — the binary contract surface is
untouched.

## Risks

- **False `requested_end_not_yet_elapsed` from a cross-boot origin** if the
  sweep is bypassed (e.g., a future second storage-open path). Mitigations:
  sweep failure fails startup; `live_session_now` keeps its lifecycle,
  normalize, and `session_now < retained.end()` doubt-guards; test 9 pins
  the sweep.
- **Behavioral loosening of explicit ranges**: callers that relied on the
  exact overshoot refusal now receive partial results. This is the feature's
  intent and is allowed under Current Contract Discipline (no supported
  third-party consumers); the requested range and warnings preserve full
  honesty. SPEC prose updates in the same stride.
- **Connect now depends on a catalog write**: a broken index blocks browser
  start. Judged correct — frame persistence would fail immediately anyway,
  and failing early is the more explicit contract.
- **Best-effort terminal write**: a failed `Ended` write leaves a
  live-looking row until next process start (sweep repairs it). Within the
  same process lifetime such a session could still refine warnings while
  actually ended; the `ended_at`/lifecycle in-memory state and the
  `session_now >= resolved.end()` guard bound the exposure, and the row's
  frames stop advancing, so the claim stays consistent with retained truth.
- **Schema-drift gate**: `RetentionWarning` and all wire shapes are
  unchanged; `check-wire-enum-schemas.sh` runs in the gate to prove it.
- **Idle-tail decision may draw future refinement pressure**: if durable
  per-target capture-state evidence lands later, a provable
  absence-of-change refinement can be designed then; nothing in this feature
  forecloses it.
