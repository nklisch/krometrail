---
id: feature-window-lifecycle-integrity
kind: feature
stage: implementing
tags: [bug, browser]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-19
updated: 2026-07-19
---

# Survive popup windows, target re-attach, and session end without wedging

## Brief

The 2026-07-19 motion workload found a cascade of lifecycle defects around
`window.open` popups and session teardown (dev build at v1.2.3-19, foreground
managed session):

1. **Popup initial navigation never commits.** `window.open('detail.html',
   'detail', 'width=420,height=300')` opens the OS window, but the target stays
   at an empty URL indefinitely (Chrome `/json/list` showed `url: ""`; one empty
   navigation entry; blank document complete). `waitForDebuggerOnStart` is false
   everywhere, so it is not a debugger hold — the popup's renderer-initiated
   navigation appears to be cancelled during krometrail's attach/reject churn on
   the unrecordable empty-URL target. A manual out-of-band `Page.navigate`
   loaded instantly, after which krometrail supervised the page with the correct
   `opener_target_id` — the window was healthy; supervision works once a
   recordable URL exists.
2. **Opener click hard-fails instead of degrading.** Each popup-opening click
   returned "browser rejected or could not complete the page observation
   command" as a hard error (no interaction record) even though the input
   dispatched. `wait_for_page` then times out because the frozen popup never
   becomes supervisable.
3. **Post-close observation wedge.** After closing the popup page, the opener
   target silently detached/re-attached (attachment generation unchanged) and
   every observation failed with `invalid_input: "CSS size must be finite and
   positive"` plus `browser.compositor.signal_unavailable` at
   `compositor_readiness` — across observe_live and repeated same-origin reloads
   — while the page stayed fully interactive. Only a cross-origin navigation
   (process swap) recovered it. Observation state must re-baseline on re-attach.
4. **Ended sessions block restart.** Closing the last supervised page exits
   Chrome; the dead session (`state: "ended"`) still occupies the singleton slot.
   `start_browser` refuses ("a browser session is already active") and the only
   recovery is a `stop_browser` call that reports the error "browser supervision
   task ended" while actually reaping the slot. Reap ended sessions on
   `start_browser` (or make stop-on-ended a reported success), and give
   last-page-close and ended-session errors recovery guidance naming
   `start_browser`.
5. **Visibility commit-ordering fence** (parked from the first shakedown's
   review): the activation visibility write-back can be overwritten by a stale
   queued visibility event captured before activation but reduced after it. Add
   a monotonic observation sequence (or generation fence) to visibility inputs
   so older observations cannot overwrite newer ones, with a deterministic race
   test. Single-writer reducer remains the sole authority.

Also observed: `evaluate_page` on the adopted popup ran in the stale
`about:blank` execution context (null `document.documentElement` while
`location` shows the new URL); context selection should track the current
document.

Absorbed backlog: `idea-popup-window-lifecycle-wedge`,
`idea-ended-session-slot-reaping`, `idea-visibility-commit-ordering-fence`.
Implementation via peeragent Codex `gpt-5.6-luna` per operator decision
(2026-07-19).

## Simplification opportunity

Items 1–3 likely share one root in target discovery/attach handling of
not-yet-recordable targets; fixing the attach path may collapse the popup freeze
and the re-attach wedge into one mechanism plus a re-baseline rule. The ended
session reap may delete the error-shaped-success path in stop_browser rather
than add a new one.

## Explorer map (verified file:line)

- Discovery/auto-attach: `session/runtime.rs` — event subscription L119-128,
  `Target.setDiscoverTargets` L136-143, `Target.setAutoAttach {autoAttach:true,
  waitForDebuggerOnStart:false, flatten:true}` L144-151, `Target.getTargets` seed
  L152-162; `parse_event` L989; opener key via `parse_target_info` L1032/L1052.
- Effects: Attach → `Target.attachToTarget` L210-236; Detach →
  `Target.detachFromTarget` L237-245; `RestoreSessionDomains` (Page/Runtime/
  Accessibility enable) then `ProbeInitialVisibility` L304-339; probe =
  `Runtime.evaluate document.visibilityState` L340-369.
- Recordability: `targets/model.rs` `is_recordable` L65-68 (`type=="page"` AND
  non-empty URL AND not internal L73-85). Reducer `reconcile_one`
  (`targets/reducer.rs` L277): **unrecordable targets are skipped, never
  attached** (destroyed only if previously known L283-288); a blank target that
  later gains a URL is adopted via `TargetInfoChanged` → attach L296-352.
- Detach-on-failure paths that mark a target Failed and emit Detach:
  `initial_visibility_probe_failed` L457-498 (Detach L484-487), RestoreViewport
  failure (runtime L263-302), `detach_failed` L419-455.
- Opener: captured in `reconcile_one` L290-295, state field `opener_target_id`
  (model.rs L135), reconnect back-fill `resolve_unresolved_openers` L745-760;
  surfaced by `page_contexts` (model.rs L166-197, cursor = next_page_sequence-1).
- wait_for_page: `session/operations.rs` L212-311, match filter
  `next_page_match` L313-328; sequences minted reducer L325-329/L680-684.
- CSS-size error: `krometrail-core/src/browser/observation.rs` `CssSize::new`
  L162-169; fed by `decode_effective_viewport` (`control/viewport.rs` L162-195,
  builds from `cssVisualViewport` at L191-192); `observe_effective_viewport`
  (viewport.rs L96-124) also evaluates `innerWidth/innerHeight` via JS.
- Compositor readiness: `control/pages.rs` `await_compositor_ready` L79-112
  (250 ms, log-only `browser.compositor.signal_unavailable` L102-110).
- Session slot: `krometrail-mcp/src/session.rs` `connect` L95-109 ("already
  active" L99-104), `stop` L149-157; ended state `krometrail-core
  browser/session.rs:37`; "browser supervision task ended" errors
  `krometrail-cdp/src/session/mod.rs` L786-792 (execute), L815-822 (stop, incl.
  cached `stop_result` L806-808); supervisor loop `run_supervisor`
  runtime.rs L766-831.
- Visibility inputs: reducer `visibility_changed` L591-621 (dispatch L127-146;
  reconnect guard drops them L36-58). Producers: initial probe (runtime
  L340-369), screencast reader (`capture/pipeline.rs` L1049-1091 →
  `SessionCaptureObserver::visibility_changed` session/mod.rs L530-543,
  **no generation wrapper** L537-542), reconnect probe (`session/reconnect.rs`
  L268-320), activation write-back (`session/operations.rs`
  `commit_observed_visibility` L945-974 → `commit_supervisor_input`
  L1071-1097). Inputs `VisibilityChanged`/`CaptureVisibilityChanged`
  (model.rs L309-319) carry **no sequence/timestamp**; only
  `ForConnectionGeneration` (model.rs L338-342) exists as an ordering guard and
  visibility producers bypass it.

## Design decisions

- **Popup unit is investigation-first**: the map shows the reducer never
  attaches unrecordable targets, yet Chrome's browser-level
  `setAutoAttach {autoAttach:true}` creates flat sessions for new targets
  independently. The candidate mechanisms to test, in order: (a) the
  unsolicited `Target.attachedToTarget` session for the empty-URL popup is
  left unserviced or detached in a way that starves its initial navigation;
  (b) a Detach-on-failure path fires against it; (c) discovery/reconcile churn
  destroys and re-skips it. The fix must make the popup's own navigation
  commit and adoption happen on `TargetInfoChanged`. Verified against a real
  Chrome opt-in test, not only doubles.
- **Observation failures on pointer/action paths degrade, never hard-fail**:
  the action's success is decided by dispatch, not by the post-action
  observation; a failed observation returns the interaction record +
  diagnostics with an unavailable observation part (same shape create_page
  already uses for hidden-tab screenshots).
- **Zero/invalid layout metrics fall back to the JS-observed size**:
  `observe_effective_viewport` already evaluates `innerWidth/innerHeight`; when
  `Page.getLayoutMetrics` yields a non-finite/non-positive CSS size, use the
  JS-observed geometry and mark the observation degraded with truthful
  recovery ("reload or navigate the page; a cross-origin navigation restores
  observation when same-origin reload does not") — never `retry: never` for a
  navigation-recoverable state.
- **Ended-session reaping happens in `connect`**: when the slot holds a session
  whose supervision task has ended, reap it and proceed with the new start;
  `stop` on an ended session returns success reporting the cleanup instead of
  the "browser supervision task ended" error. The last-page-close warning and
  ended-session operation errors gain recovery text naming `start_browser`.
- **Visibility ordering token = session monotonic time**: extend
  `VisibilityChanged`/`CaptureVisibilityChanged` with the producer's observed
  session time (all four producers have access to the shared session clock);
  the reducer keeps `last_visibility_observed_at` per target and ignores older
  observations. Chosen over a bespoke ordinal because session time already
  exists at every producer and preserves single-writer semantics unchanged.

## Implementation Units

### Unit 1: Popup navigation and adoption
**Files**: `crates/krometrail-cdp/src/targets/reducer.rs`,
`crates/krometrail-cdp/src/session/runtime.rs`,
`crates/krometrail-cdp/tests/page_lifecycle.rs`
**Story**: `story-lifecycle-popup-adoption`

- Reproduce with an opt-in real-chrome test: click a `window.open(url, name,
  features)` button; assert the popup commits its navigation, `wait_for_page`
  with the opener filter returns it, and `list_page_contexts` shows
  `opener_target_id`.
- Root-cause with the candidate list above; fix so unrecordable auto-attached
  sessions are explicitly released without starving the target (and adoption on
  `TargetInfoChanged` attaches with correct opener and page sequence).
- Deterministic reducer coverage: Created(empty url) → InfoChanged(url) →
  attach effect with opener resolved; unsolicited Attached for unrecordable
  target produces a safe explicit effect, not silence.

**Acceptance Criteria**:
- [ ] Real-chrome opt-in test: popup loads its URL unaided and becomes
      supervised with `opener_target_id`; `wait_for_page` matches it.
- [ ] Deterministic reducer tests for the empty-URL create→adopt sequence and
      the unsolicited-attach handling.

### Unit 2: Post-action observation degrades instead of hard-failing
**Files**: `crates/krometrail-cdp/src/control/pages.rs`,
`crates/krometrail-cdp/src/session/operations.rs`,
`crates/krometrail-mcp/src/response.rs`
**Story**: `story-lifecycle-popup-adoption`

- Pointer/action operations whose input dispatched must return
  `status: degraded` with the interaction record, diagnostics correlation id,
  and unavailable observation parts — mirroring the create_page hidden-tab
  shape — instead of the hard error "browser rejected or could not complete
  the page observation command".

**Acceptance Criteria**:
- [ ] A click whose post-action observation fails still returns its
      interaction id, record, and diagnostics block with `status: degraded`.
- [ ] Existing genuine dispatch failures (e.g. target_hidden preflight) remain
      hard errors.

### Unit 3: Layout-metrics fallback and truthful recovery
**Files**: `crates/krometrail-cdp/src/control/viewport.rs`,
`crates/krometrail-core/src/browser/observation.rs`,
`crates/krometrail-mcp/src/response.rs`
**Story**: `story-lifecycle-metrics-fallback`

- When `decode_effective_viewport` receives a non-finite/non-positive
  `cssVisualViewport`, fall back to the JS-observed `innerWidth`/`innerHeight`
  geometry already acquired by `observe_effective_viewport`; only fail when
  both sources are unusable, and then with recovery text naming reload /
  navigation (retry `after_recovery`, not `never`).

**Acceptance Criteria**:
- [ ] Deterministic double: zero-size layout metrics + valid JS size → page
      state available, flagged with a metrics-fallback note.
- [ ] Both-sources-invalid → degraded with the navigation recovery text.

### Unit 4: Ended-session slot reaping
**Files**: `crates/krometrail-mcp/src/session.rs`,
`crates/krometrail-cdp/src/session/mod.rs`
**Story**: `story-lifecycle-session-reaping`

- `connect` reaps a slot whose supervision task ended and proceeds.
- `stop` on an ended session succeeds, reporting cleanup.
- Last-page-close warning and ended-session operation errors carry recovery
  text naming `start_browser`.

**Acceptance Criteria**:
- [ ] start → close last page (session ends) → start succeeds without a manual
      stop (deterministic session-owner test).
- [ ] stop on ended session returns success with a cleanup note.

### Unit 5: Visibility observation ordering fence
**Files**: `crates/krometrail-cdp/src/targets/{model.rs,reducer.rs}`, all four
producer sites (runtime.rs, capture/pipeline.rs + session/mod.rs,
session/reconnect.rs, session/operations.rs)
**Story**: `story-lifecycle-visibility-fence`

- Add observed session time to both visibility input variants; reducer ignores
  observations older than the last accepted one per target.

**Acceptance Criteria**:
- [ ] Deterministic race test: activation commit (t2) followed by a queued
      pre-activation hidden event (t1 < t2) leaves the target visible and
      recording.
- [ ] All four producers stamp the shared session clock.

## Implementation Order
1. Unit 5 (fence — smallest, unblocks nothing but derisks reducer edits)
2. Unit 1 (popup root-cause + fix)
3. Unit 2 (observation degradation)
4. Unit 3 (metrics fallback)
5. Unit 4 (slot reaping)

## Testing
- Real-chrome opt-in tier only for Unit 1's popup commit (cannot be proven with
  doubles); everything else deterministic doubles per layered-cdp-qualification.
- Regression tests double as the acceptance criteria above; no per-line
  coverage beyond them.

## Risks
- The popup freeze may root in Chromium behavior krometrail can only mitigate
  (e.g. releasing unsolicited sessions promptly); the unit's acceptance is
  behavioral (popup loads), so a mitigation that achieves it is acceptable.
- The metrics fallback must not mask real zero-size windows (minimized popups);
  the fallback note keeps the provenance visible.
