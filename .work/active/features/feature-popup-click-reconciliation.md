---
id: feature-popup-click-reconciliation
kind: feature
stage: implementing
tags: [browser, side-channel]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-22
updated: 2026-07-22
---

# Popup-opening clicks keep observation and new-page facts

## Brief

A click that opens a popup degrades the entire post-action observation and
misses the new-page postcondition fact. Repro (v1.5.0 shakedown, correlation
`692cc338-ddcc-4303-b10a-71e65f656a84`): clicking a button whose handler
calls `window.open('about:blank', '_blank', 'width=…')` in a
`focus: preserve` session stalled ~6s, returned `status: degraded` with
`page_observation_failed` ("interaction was dispatched but completion
evidence is unavailable") on page/screenshot/snapshot, and the postcondition
reported `new_pages.pages: []` while `signals.window_open_attempts: 1`.
Diagnostics show `browser.target.attached` for the popup landing 3ms AFTER
`mcp.request.completed` — the popup's attachment queued behind the in-flight
Execute, so pull-based reconciliation ran before the target existed, and the
observation phase burned its budget failing. `list_pages` a moment later
shows the popup fully supervised.

Two halves: (a) the new-page fact window closes before a same-click popup
can attach — the exact issue-#14 finding-#9 shape the side-channel feature
targeted; (b) the primary observation should not spend ~6s failing when a
popup steals rendering/visibility. `window_open_attempts` + `cursor_before`
still let a caller recover via `wait_for_page`, but the concise result reads
as "no popup appeared".

## Simplification opportunity

If reconciliation moves to a bounded post-dispatch wait keyed on observed
`window_open_attempts > 0`, the current always-run pull pass may simplify to
one conditional path. No compatibility shims: the postcondition shape stays
as shipped; only the facts get more complete.

## Design decisions

### Diagnosis: where the ~6s went and why the fact was missed

Grounded in the code paths, the repro decomposes into two independent
defects:

1. **The ~6s stall is two serial renderer-silent waits, each burning the
   transport's 3s command timeout** (`with_command_timeout(Duration::from_secs(3))`
   in `crates/krometrail-cdp/src/session/mod.rs` and `src/app.rs`):
   - The release half of the click gesture
     (`crates/krometrail-cdp/src/control/pointer.rs`, `dispatch_gesture_half`)
     waits for the `Input.dispatchMouseEvent` acknowledgement. A stalled
     renderer never answers; at 3s the transport yields
     `TransportError::Timeout`, which the gesture path deliberately tolerates
     as a lost acknowledgement ("treating the input as dispatched").
   - The completion settle probe
     (`crates/krometrail-cdp/src/control/interaction.rs`,
     `complete_interaction`, `Runtime.evaluate "Promise.resolve(true)"`)
     then asks the same silent renderer again, burns the next 3s command
     timeout, and errors. That error is exactly what sets
     `completion_degraded` with the repro's message ("interaction was
     dispatched but completion evidence is unavailable",
     `ErrorCode::PageObservationFailed`), and the degraded path clones that
     error into all three observation parts (`unavailable_observation`) —
     matching the repro's page/screenshot/snapshot triple failure.

   The renderer-side mechanism for the silence is not fully provable from
   this repo: the best-supported hypothesis is that the click handler's
   synchronous `window.open` window-creation path stalls the acting page's
   main thread (the `about:blank` popup shares the opener's process and agent
   cluster, and under `focus: preserve` the popup steals activation/occludes
   the opener). The design below is deliberately robust to the exact
   mechanism: it keys on the observable `Page.windowOpen` signal, not on any
   assumption about why the renderer went quiet.

2. **The new-page fact was missed because the reconciliation pull runs
   exactly once and lost the race.** The session supervisor is a serial
   single-writer loop (`crates/krometrail-cdp/src/session/runtime.rs`,
   `while let Some(command) = commands.recv().await`): pumped `Target.*`
   events become `SupervisorCommand::Input` messages on the *same* mpsc
   channel that the in-flight `SupervisorCommand::Execute` occupies, so the
   popup's `Target.attachedToTarget` cannot reduce into supervised state
   until Execute returns — hence the diagnostic's attach landing 3ms *after*
   `mcp.request.completed`. The pull path (`attach_new_page_facts` →
   `fetch_target_infos` → `Target.getTargets` →
   `apply_target_reconciliation`) exists precisely to bypass that queue and
   read the browser's authoritative inventory, and it works — but it runs a
   single pull at fact assembly, and in the repro the popup target was not
   yet present in the browser's inventory at that instant (creation
   completed in the same window in which the renderer recovered). One pull
   with no retry cannot close a race that the drained
   `window_open_attempts: 1` signal has already announced.

### Decisions

- **D1 — Bounded reconciliation poll keyed on drained signals (half a).**
  Fact assembly keeps the pull-based design but converts the single pull
  into a bounded poll: while the drained record shows
  `window_open_attempts > 0` with no page past `cursor_before` (or
  `download_requests > 0` with no download past the download cursor), keep
  re-pulling at a 50ms interval until the delta is non-empty or the bounded
  ceiling elapses. The first loop iteration *is* today's single pull, so
  non-popup interactions keep exactly one `Target.getTargets` round trip and
  zero added latency.
- **D2 — One shared ceiling, derived from the cooperative deadline.** The
  poll ceiling is `bounded_deadline(context.deadline, SIDE_CHANNEL_RECONCILE_WINDOW)`
  with the existing `SIDE_CHANNEL_RECONCILE_WINDOW = 2s` constant, shared by
  the page and download waits (they poll inside one loop, never stacking
  2s + 2s). No new config knobs; no parallel budget mechanism — the
  batch-timeout `OperationExecutionContext.deadline` caps everything
  cooperatively, exactly as it caps the current single pull.
- **D3 — Stop at first non-empty delta, not at `attempts` count.** Popup
  blockers and browser policy make `window_open_attempts` an upper bound on
  materialized pages. Waiting for `pages.len() >= attempts` would burn the
  full window whenever one attempt was blocked. `cursor_before` +
  `wait_for_page` remains the documented recovery for stragglers.
- **D4 — Honest empty result within the bound.** A popup that never
  materializes finalizes as `signals.window_open_attempts > 0` +
  `new_pages: Some({ cursor_before, pages: [], omitted: 0 })` no later than
  the ceiling. Pull-failure semantics are preserved: if no pull succeeded in
  the phase, `new_pages` stays `None` (reconciliation unavailable — never a
  claim that nothing opened); if at least one pull succeeded, the last
  successful inventory delta is attached.
- **D5 — Popup-stall grace bounds the completion settle, not the gesture
  dispatch (half b).** Once a `Page.windowOpen` signal is observed for the
  acting target after the dispatch fence, the remaining completion wait is
  capped at `POPUP_STALL_COMPLETION_GRACE = 750ms` (matching the
  `NAVIGATION_AWARE_WINDOW` order of magnitude). Grace elapse produces the
  same degraded shape as today (`completion_degraded`,
  `ErrorCode::PageObservationFailed`) with a popup-specific message and a
  recovery pointing at `new_pages.cursor_before` + `wait_for_page`. The
  gesture-dispatch phase is deliberately left at the transport's 3s ack
  bound: cutting the dispatch future externally could abandon the gesture
  mid-sequence (press sent, release never dispatched) and corrupt input
  state, and a lost ack is only distinguishable from a slow-but-healthy
  renderer at the transport timeout. That 3s is the explicitly justified
  residual bound.
- **D6 — Grace arms only on popup evidence.** Without an observed
  `Page.windowOpen`, the settle keeps its current windows — a renderer that
  is slow because of heavy JS (no popup) still gets the full settle budget,
  so healthy-but-slow interactions do not degrade more than today. The
  watcher is a second, independent `WindowOpen` `PageSignalReceiver`
  (broadcast receivers consume independently), with a fenced
  `recv_after(signal_floor)` so a late-delivered signal from a previous
  interaction can never arm the grace.
- **D7 — Signal-unavailable browsers keep today's exact behavior.** When
  `window_open_attempts` is `None` (subscription unavailable), the loop
  exits after its first pull and no grace ever arms: single pull, current
  bounds, unchanged semantics.
- **D8 — Wire shape untouched; no doc-contract changes required.**
  `SideChannelSignals`, `NewPagePostcondition`, `DownloadPostcondition`,
  `MAX_SIDE_CHANNEL_FACTS`, cursors, and error codes are unchanged; only
  completeness and timing improve. SPEC.md describes the postcondition at
  fact level and asserts nothing about single-pull timing, so no foundation
  doc rolls forward; in-code doc comments describing "one bounded
  reconciliation pull" are updated in place.
- **Worst-case latency does not regress.** Popup-stall worst case becomes
  ~3s (release ack, justified) + 0.75s (grace) + ≤2s (poll) ≈ 5.75s versus
  today's ~6.1s — and with complete facts. The typical popup case (renderer
  recovers in tens of ms) resolves the poll on its first or second
  iteration.

## Architectural choice

Keep side-channel reconciliation **pull-based and inside the serial
Execute**, closing the race with a bounded poll, rather than restructuring
the supervisor to process target events concurrently with an in-flight
Execute.

- The serial single-writer loop is a load-bearing project pattern
  (`single-writer-effect-reducer`): deterministic state transitions and an
  explicit effect queue depend on exactly one input being reduced at a time.
  Concurrent event reduction during Execute would re-open every
  target-state race the supervisor exists to prevent, for a benefit the
  bounded poll already delivers.
- Waiting on the queued `Target.attachedToTarget` events is impossible by
  construction — the queue is behind the currently executing command.
- The poll mirrors the proven `wait_for_page` loop (same
  `reconcile_targets_once` machinery, same 50ms cadence, same
  reduce-and-apply idempotence), so no new reconciliation semantics are
  introduced; only the loop's exit conditions are new.
- The completion-grace change stays entirely inside
  `execute_interaction_request_inner`'s existing phase structure: it adds
  one race arm to the completion await and reuses the passive page-signal
  authority. No new ports, no session-layer surface changes.

## Implementation Units

### U1 — Fenced awaitable page signal

**File:** `crates/krometrail-cdp/src/events/signals.rs`

```rust
impl PageSignalReceiver {
    /// Awaits the next signal of this receiver's kind stamped at or after
    /// `floor`. Pre-floor signals (late deliveries from earlier activity)
    /// are skipped. Lag and closure surface as errors so callers disarm.
    pub(crate) async fn recv_after(
        &mut self,
        floor: ObservedTime,
    ) -> Result<(), PageSignalReceiveError>;
}
```

Notes: implemented as the existing `recv` loop plus an
`signal.observed_at >= floor` filter. `Lagged` maps to
`PageSignalReceiveError::Lagged` (callers treat it as "watcher unavailable",
never as "signal observed").

Acceptance:
- Unit tests beside the existing `signals.rs` tests: a queued pre-floor
  signal is skipped; a post-floor signal returns `Ok(())`; other-kind
  signals are ignored; closure yields `Closed`.

### U2 — Popup-stall bounded completion

**File:** `crates/krometrail-cdp/src/control/interaction.rs`

- New module constant:
  `const POPUP_STALL_COMPLETION_GRACE: std::time::Duration = std::time::Duration::from_millis(750);`
- In `execute_interaction_request_inner`, alongside the existing passive
  receivers, subscribe an independent watcher (skip for `HandleDialog`):

```rust
let mut popup_stall_watch = (plan.kind != BrowserOperationKind::HandleDialog)
    .then(|| browser_events.page_signal(&event_binding, PageSignalKind::WindowOpen).ok())
    .flatten();
```

- Replace the completion await (currently
  `tokio::time::timeout_at(bounded_deadline(deadline, INTERACTION_PHASE_WINDOW), completion)`)
  with a race: the existing bounded completion versus, when the watcher
  exists, `popup_stall_watch.recv_after(signal_floor)` followed by
  `tokio::time::sleep(POPUP_STALL_COMPLETION_GRACE)`. If the grace arm wins,
  set `completion_degraded` to a `PageObservationFailed` error with message
  "a window-open attempt left the renderer unresponsive; completion evidence
  is unavailable" and recovery "chain wait_for_page from the postcondition's
  new_pages.cursor_before to observe the opened page, then retry
  observation". The `HandleDialog` no-deadline special case and the
  timeout/blocked classification arms are unchanged.

Notes: cancelling the completion future is safe (it is a `Runtime.evaluate`
probe plus optional event waits; no input state depends on its ack). A
`Page.windowOpen` queued during the dispatch stall arms the watcher
immediately at completion start, so grace counts from completion entry —
bounded either way. The gesture-dispatch phase is intentionally not raced
(D5).

Acceptance:
- With a scripted `Page.windowOpen` before a held settle, completion
  degrades after ~750ms of paused-clock time (not the 10s phase window and
  not a scripted-transport hang), `outcome` stays `Dispatched`, and the
  observation parts carry `PageObservationFailed`.
- Without a popup signal, a held settle follows today's path (phase-window
  bound), proving the grace never arms on popup-free stalls.
- A popup click whose settle answers promptly keeps a fully healthy
  observation (grace arm loses the race).

### U3 — Bounded side-channel fact assembly

**File:** `crates/krometrail-cdp/src/session/operations.rs`

- New constant:
  `const SIDE_CHANNEL_RECONCILE_POLL_INTERVAL: Duration = Duration::from_millis(50);`
  (`SIDE_CHANNEL_RECONCILE_WINDOW` stays 2s and becomes the shared ceiling.)
- Replace `attach_new_page_facts` + the separate `attach_download_facts`
  call with one assembly phase:

```rust
/// Post-dispatch side-channel delta assembly. Pull-based target
/// reconciliation observes the browser inventory directly; when the drained
/// signals announce a window-open or download attempt, the pull repeats on
/// a short interval until the corresponding delta materializes or the
/// bounded ceiling (batch-deadline capped) elapses. Every failure degrades
/// to absent facts — never a claim that nothing opened — and the proven
/// interaction result is never failed or delayed beyond the ceiling.
async fn attach_side_channel_facts(
    result: &mut BrowserOperationResult,
    state: &mut SupervisorState,
    transport: &Arc<dyn CdpTransport>,
    shared: &Arc<SessionShared>,
    baselines: crate::control::InteractionDispatchBaselines,
    deadline: Option<tokio::time::Instant>,
)
```

Loop shape:
1. Read `attempts = record.postcondition.signals.window_open_attempts` and
   `requests = record.postcondition.signals.download_requests` from the
   interaction record (already drained by U2's phase); compute
   `ceiling = crate::control::bounded_deadline(deadline, SIDE_CHANNEL_RECONCILE_WINDOW)`.
2. Each iteration: if the page delta is still pending, run one pull
   (`fetch_target_infos` bounded by the ceiling, then
   `apply_target_reconciliation` unfenced, exactly as today) and recompute
   the `sequence > cursor_before` delta with `opener_matched`; read
   `shared.downloads.begun_after(cursor)` (authority pumps run independently
   of the supervisor loop, so this progresses during Execute).
3. Continue only while `now < ceiling` **and**
   (`attempts.unwrap_or(0) > 0 && page_delta.is_empty()` **or**
   `requests.unwrap_or(0) > 0 && download_delta.is_empty()`); sleep
   `min(POLL_INTERVAL, remaining)` between iterations.
4. Finalize: attach `NewPagePostcondition::from_observed` from the last
   successful pull (or leave `None` if no pull succeeded / fetch timed out
   before ever succeeding — today's failure semantics), and
   `DownloadPostcondition::from_observed` when the session manages
   downloads. A hard pull failure (transport error) exits the loop
   immediately.
- Call site in `execute_non_local_operation` collapses the two attach calls
  into `attach_side_channel_facts(...)`, still ahead of
  `finalize_expectation_note` and `commit_observed_visibility`.
- Update the module doc comments that describe "one bounded reconciliation
  pull" in place.

Acceptance:
- `attempts == 0` / `attempts == None`: exactly one `Target.getTargets`
  command is issued (assert scripted command count) and no poll latency is
  added.
- `attempts > 0`, popup in the second scripted inventory: `new_pages.pages`
  carries the popup with `opener_matched: true` before the ceiling.
- `attempts > 0`, popup never appears: finalizes with the honest
  attempts>0 + empty-pages fact at the 2s ceiling (paused clock), result
  `Ok`/`Dispatched`.
- `requests > 0`, download begin recorded between polls: download fact
  attached within the ceiling; page and download waits share one ceiling
  (no 2s + 2s stacking).
- Under a nearly exhausted batch deadline the poll is capped by
  `context.deadline` and the step still finalizes with a dispatched record
  (extends the batch-timeout-preserves-dispatched-record contract).

### U4 — Deterministic and qualification tests

**Files:** `crates/krometrail-cdp/tests/verified_interactions.rs`,
`crates/krometrail-cdp/tests/waits_and_batches.rs` (batch-deadline case),
`crates/krometrail-cdp/src/events/signals.rs` (unit tests, part of U1).

The full matrix is in `## Testing`. The repro-shaped integration double
(popup signal + held settle + late-arriving inventory) is the capstone and
must pass with paused-clock timing assertions.

### U5 — Gates and sweep

- `cargo fmt --all -- --check`, `bash scripts/check-wire-enum-schemas.sh`
  (wire shape untouched — must be a no-op), `cargo check/test/clippy
  --workspace --all-targets --locked`.
- Verify no foundation doc asserts single-pull timing (checked at design
  time: none does; re-verify at merge).

## Implementation Order

1. **U1** (`recv_after` + unit tests) — no behavior change, unblocks U2.
2. **U2** (popup-stall completion grace + its three acceptance tests) —
   independently shippable; shrinks the stall even before facts improve.
3. **U3** (bounded fact assembly + its acceptance tests) — depends only on
   existing drained signals; lands the missing-fact fix.
4. **U4 capstone** — repro-shaped integration double combining U2 + U3, the
   batch-deadline cap test, and the real-Chrome qualification extension.
5. **U5** — gates and doc-comment sweep.

## Simplification

- `attach_new_page_facts` and `attach_download_facts` merge into one
  assembly phase with one ceiling — the two-call sequence and its duplicated
  record plumbing disappear.
- The poll's first iteration is the old single pull, so there is no
  "legacy path vs poll path" split: one loop, one exit predicate, and the
  brief's "one conditional path" simplification lands (the continuation
  condition is the only popup-conditional code).
- `reconcile_targets_once` / `fetch_target_infos` /
  `apply_target_reconciliation` stay the single shared reconciliation
  machinery for `wait_for_page` and fact assembly; no new variant is added.
- No compatibility shims, aliases, or dual schemas anywhere: the
  postcondition wire shape, error codes, and cursor semantics ship
  unchanged per Current Contract Discipline.

## Testing

Per the layered-cdp-qualification ladder: deterministic doubles carry the
logic, boundary fault injection carries the failure modes, and real-browser
qualification stays an explicit opt-in — popup timing must never be trusted
to real-browser scheduling in CI.

**Deterministic doubles (`ScriptedCdp`, paused tokio clock where timing is
asserted):**
1. *Popup adopted on a later pull:* click with scripted
   `Page.windowOpen` (via `wait_for_command_count("Input.dispatchMouseEvent", 3)`
   then `push_scoped_event`), first `Target.getTargets` response without the
   popup, second with it plus `Target.attachToTarget` → `new_pages.pages`
   has the popup, `opener_matched`, `sequence > cursor_before`; persisted
   record matches the live record (`RecordingEvidenceFake`).
2. *Popup never materializes:* `attempts > 0`, every scripted inventory
   empty → `start_paused` asserts finalization at the 2s ceiling with
   attempts>0 + empty pages + `omitted: 0`, outcome `Dispatched`.
3. *No attempts, no extra pulls:* plain click → exactly one
   `Target.getTargets` (scripted command count), `new_pages` empty-honest as
   today.
4. *Signal source unavailable:* `fail_subscription("Page.windowOpen")` →
   `window_open_attempts: None`, single pull, no grace arming.
5. *Repro-shaped capstone:* `Page.windowOpen` event, settle stalled via
   `hold_method_after("Runtime.evaluate", N)` (N skips the hit-test and
   pre-URL evaluates), popup only in a later scripted inventory → completion
   degrades at ~750ms paused time (not 10s), observation parts carry
   `PageObservationFailed`, and the final record still carries the popup in
   `new_pages` — the exact defect pair from the Brief, closed.
6. *Grace never arms without a popup:* held settle, no window-open event →
   current phase-window behavior preserved.
7. *Download wait:* `Page.frameRequestedNavigation` download-disposition
   signal with the download-begin event delivered between polls → download
   fact within the shared ceiling.
8. *Batch deadline cap:* batch whose deadline expires mid-poll → poll is
   cut at `context.deadline`, step finalizes with a dispatched record and
   honest facts (or `None` when no pull succeeded).

**Fault injection:** `push_failure("Target.getTargets", …)` mid-loop keeps
`new_pages: None` and never fails the click (extends the existing
`reconciliation_failure_degrades_the_delta` test to the poll); watcher
`Lagged`/`Closed` disarms grace without affecting the interaction.

**Unit:** `recv_after` fence tests in `signals.rs` (U1).

**Real-browser qualification (opt-in, `KROMETRAIL_REAL_CHROME_TESTS=1`):**
extend `opt_in_real_chrome_qualifies_download_and_popup_side_channel_facts`
so the same-click `window.open` popup asserts a non-empty
`new_pages` on the click result itself (today it can only qualify the
cursor/wait_for_page recovery), and record observed popup-attach latency in
the test log for future bound tuning.

## Risks

- **Residual miss when the popup materializes after the ceiling.** If the
  renderer stall is causally coupled to our own Execute (unproven), the
  popup may still appear browser-side after the 2s poll ends. Mitigation:
  the honest attempts>0 + empty + `cursor_before` result and
  `wait_for_page` recovery are unchanged, and the real-Chrome qualification
  logs actual attach latency so the window can be tuned with evidence
  rather than guesswork.
- **Grace cutting a settle that would have succeeded.** A popup click on a
  slow-but-recovering renderer may now degrade at 750ms where 3s would have
  produced a healthy observation. Bounded by design: grace only arms on
  popup evidence, degradation preserves the postcondition and recovery, and
  the trade removes a 6s stall from the common popup path.
- **Added latency on blocked popups.** A popup blocker keeps
  `attempts > 0` with no page, costing the full 2s poll. Accepted: bounded,
  popup-only, and offset by the ~2.4s the grace removes from the stall.
- **Repeated `apply_target_reconciliation` effect application.** The poll
  re-runs reduce/apply per iteration; `wait_for_page` already relies on this
  idempotence, and test 1 exercises adoption mid-loop, but any latent
  non-idempotent effect would now run more often. Watch in review.
- **Watcher lag under event storms.** Broadcast lag disarms the grace
  (fallback to current bounds) rather than mis-arming it; counted drains for
  the postcondition are unaffected (independent receivers).
- **The 3s dispatch-ack bound remains.** Explicitly retained (D5): gesture
  integrity requires completing the input command sequence, so the popup
  worst case keeps a ~3s component that cannot be shortened without
  redesigning gesture dispatch. Documented here as the accepted floor.
