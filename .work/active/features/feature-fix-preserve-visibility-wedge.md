---
id: feature-fix-preserve-visibility-wedge
kind: feature
stage: implementing
tags: [bug, browser, agent-ux]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-19
updated: 2026-07-19
---

# Refresh tracked visibility after activation in preserve mode

## Brief

A live shakedown session (Wayland/KDE host, plugin 1.2.x binary) hit a persistent
preserve-focus wedge that the 1.0.5 transient fix (`story-fix-pointer-activation-visibility`)
does not cover. With `start_browser {"focus":"preserve"}` and a page created via
`create_page`, every pointer operation failed with "browser page is hidden and preserve
focus policy did not activate it" — permanently. `activate_page` returned
`change: "activated"` successfully, and an immediate `evaluate_page` probe reported
`document.visibilityState === "visible"` and `hasFocus() === true`, yet `browser_status`
continued to report the target as `lifecycle: "hidden"`, `visibility: "hidden"`, with
`capture: []` (no screencast stream, so no retained temporal evidence for the whole
session). `select_page` re-selection did not clear it. Screenshots, JS evaluation, and
navigation kept working; only pointer input was gated. The only recovery was
`stop_browser` and restart in foreground mode.

The tracked visibility state that gates pointer preflight appears to be derived from a
source (likely screencast/target lifecycle events) that never refreshes after a successful
explicit activation, so the preflight authority contradicts both the activation outcome it
just reported and the document's own state. Related cold-start behavior folded into this
feature: `start_browser` without `initial_url` in preserve mode produced a session with
zero supervised pages (Chrome's own initial tab is not supervised), leaving nothing
selectable and making the first `create_page` land as a hidden background tab.

## Symptom evidence

- `click` → `target_hidden` error, repeatedly, after `activate_page` succeeded.
- `browser_status` (expanded): `pages[0].target.visibility == "hidden"`, `capture: []`.
- `evaluate_page`: `{visibility: "visible", hasFocus: true, hidden: false}` on the same target.
- Diagnostics correlate under session `25affe26-93a1-4490-b668-12c7aa646297` in
  `~/.local/share/krometrail/diagnostics/krometrail.log` (2026-07-19, before 07:15 UTC).

## Simplification opportunity

The fix should converge on one visibility authority (authority-revalidated-handles
pattern): pointer preflight, `browser_status` projection, and capture supervision should
read the same revalidated state instead of three views that can disagree. If activation
success already proves visibility, the preflight gate can trust and update that authority
rather than keeping an independent stale cache.

## Root cause (explorer-verified, file:line)

- Tracked authority: `SupervisedTarget { lifecycle, visibility, .. }`
  (`crates/krometrail-core/src/browser/session.rs:311`), mutated only by the reducer's
  `visibility_changed` (`crates/krometrail-cdp/src/targets/reducer.rs:591-621`).
- Its only inputs: one-shot initial-attach probe
  (`crates/krometrail-cdp/src/session/runtime.rs:340-369`), reconnect probe
  (`session/reconnect.rs:295-314`), and the live screencast
  `Page.screencastVisibilityChanged` reader (`capture/pipeline.rs:1049-1091`).
- `activate_page` (`session/operations.rs:545-590`) calls
  `PageControl::activate_target` (`control/interaction.rs:293-355`), which fronts the tab
  and polls `document.visibilityState` until `"visible"` — then **discards** that result.
  No `SupervisorInput::VisibilityChanged` is ever committed, so tracked state stays
  `Hidden` forever.
- Pointer preflight `prepare_pointer_target` (`control/interaction.rs:275-291`) reads
  `bound.visibility` copied from tracked state at `control/mod.rs:378`; its preserve-mode
  error recovery text says "call activate_page … then retry" — a dead-end loop today.
- Capture eligibility (`targets/reducer.rs:762-871`) requires `visibility == Visible`;
  the only live source that could flip a background tab Visible is the screencast reader,
  which never starts because capture never becomes eligible — circular. Hence
  `capture: []`.
- Cold start: launching without `initial_url` adds no URL arg
  (`launcher/startup.rs:215-226`); Chrome's `chrome://newtab` tab is rejected by
  `is_recordable()`/`is_internal_url` (`targets/model.rs:65-85`) → zero supervised pages,
  no selection; preserve-mode `create_page` then opens `background: true` → hidden.

## Design decisions

- **Write-back mechanism**: reuse the existing `SupervisorInput::VisibilityChanged` rather
  than adding an "activation-proven-visible" variant — activation's visible-document poll
  is the same evidence class as the initial/reconnect probes, and the reducer already
  flips `Hidden → Recording` and re-runs `reconcile_capture_bindings` on it (code
  economy; single writer preserved).
- **Where the commit happens**: in the session operations layer after any successful
  `activate_target` (explicit `activate_page` and implicit foreground activation), via one
  shared helper — the control layer stays reducer-agnostic.
- **Capture restart on activation**: in scope and automatic — once tracked visibility is
  Visible, existing `reconcile_capture_bindings` starts capture. A tab the user actually
  foregrounded is visible; preserve policy governs focus stealing, not capture of
  genuinely visible tabs.
- **Cold start**: launch with an explicit `about:blank` URL when `initial_url` is omitted,
  so the initial tab is recordable, supervised, selected, and visible. Chosen over
  auto-creating a page post-ready (racier, two code paths) and over leaving zero pages
  (violates least surprise; the navigate error fix in feature-failure-surface-clarity is
  a complement, not a substitute).
- **Attach path**: out of scope — preserve policy does not apply to attached browsers
  (per feature-preserve-browser-focus).

## Implementation Units

### Unit 1: Activation visibility write-back
**Files**: `crates/krometrail-cdp/src/control/interaction.rs`,
`crates/krometrail-cdp/src/session/operations.rs`

- Change `PageControl::activate_target` to return the observed final visibility
  (`TargetVisibility::Visible` on success; it already errors on timeout), signature:
  `pub(crate) async fn activate_target(..) -> Result<TargetVisibility>`.
- In `session/operations.rs`, add a helper
  `async fn commit_observed_visibility(&self, target_id: TargetId, visibility: TargetVisibility)`
  that sends `SupervisorInput::VisibilityChanged` through the existing commit channel
  (mirror `commit_supervisor_input(.. SelectTarget ..)` at `operations.rs:990-1016`).
- Call it after successful `activate_target` in the `ActivatePage` handler, and after any
  implicit foreground activation performed during pointer preparation
  (`prepare_pointer_target` foreground branch) — the operation handlers for pointer ops
  know when preparation activated.

**Acceptance Criteria**:
- [ ] After a successful `activate_page` on a previously hidden target,
      `browser_status` reports that target `visibility: "visible"`, lifecycle
      `attached`/`recording` (not `hidden`), within the same operation turn.
- [ ] A pointer click on that target immediately after activation succeeds without a
      second manual activation (preserve mode: `activate_page` then `click` works; the
      recovery text's advice is now truthful).
- [ ] `reconcile_capture_bindings` starts capture for the activated target
      (deterministic double asserts a `StartCapture` effect follows the write-back).

### Unit 2: Recordable cold-start page
**File**: `crates/krometrail-cdp/src/launcher/startup.rs`

- When `request.initial_url` is `None`, append `about:blank` as the URL argument (the
  same literal `create_page` uses at `operations.rs:903`), so the initial tab passes
  `is_recordable()` and initial reconciliation supervises and selects it.

**Acceptance Criteria**:
- [ ] `start_browser` without `initial_url` yields `pages.len() == 1` with a selected
      `about:blank` target in both focus modes (deterministic launcher/reconcile test).
- [ ] Existing initial-visibility Ready gate still holds (probe resolves for the seeded
      tab).

### Unit 3: Regression qualification
**Files**: `crates/krometrail-cdp/src/control/tests.rs`,
`crates/krometrail-cdp/tests/page_lifecycle.rs`

- Deterministic: extend the activation double tests (`control/tests.rs:212-355` area) to
  assert the returned visibility and the reducer write-back → capture-start effect chain;
  today none assert write-back.
- Real-chrome opt-in tier: extend
  `opt_in_real_chrome_preserve_focus_creates_a_background_tab`
  (`tests/page_lifecycle.rs:1225`) with the recovery sequence: create hidden →
  `activate_page` → tracked visibility visible → pointer click succeeds → capture
  status non-empty.

**Acceptance Criteria**:
- [ ] Both tiers cover the wedge sequence; the deterministic tier fails against the
      pre-fix code (true regression test).

## Implementation Order
1. Unit 1 (write-back) — the load-bearing fix.
2. Unit 3 deterministic tests alongside Unit 1 (test-first where practical).
3. Unit 2 (cold start) — independent, small.
4. Unit 3 real-chrome tier last.

## Testing
- Interface: activation → status projection convergence (protects the browser_status
  contract agents rely on).
- Regression: the wedge sequence itself (preserve create → activate → click), failing
  pre-fix.
- No new test for `about:blank` literal beyond the launcher/reconcile assertion; avoid
  duplicating `is_recordable` unit coverage.

## Risks
- **Wayland may refuse to actually raise the window** while `document.visibilityState`
  still reports visible; the write-back would mark Visible and start capture on a tab the
  compositor occludes. Bounded: screencast delivery then reports its own visibility/gap
  state (`CaptureGapReason::TargetHidden`), which is the existing truthful degradation
  path — no corruption, and status converges to the screencast's live signal.
- **Double-commit races** (activation write-back vs. near-simultaneous screencast event)
  serialize through the single-writer reducer; last observation wins, both claim Visible.

## Implementation Notes

Implemented by a peeragent Codex job (`gpt-5.6-luna`, job `20260719T073642Z-07e1e7d9`).
The job's runner died after completing the code (its `.peeragent/` state directory was
destroyed mid-run), so formatting, gate verification, item bookkeeping, and the commit
were completed by the host session. Delivered per design:

- `activate_target` returns the observed `TargetVisibility`; interaction execution
  propagates an `observed_visibility` from pointer preparation.
- `commit_observed_visibility` in `session/operations.rs` routes
  `SupervisorInput::VisibilityChanged` through the existing commit channel after explicit
  `ActivatePage` and implicit foreground activation.
- Cold start seeds `about:blank` when `initial_url` is omitted
  (`launcher/startup.rs`).
- Reducer test `observed_activation_visibility_restarts_capture_for_a_hidden_ready_target`
  plus a full wedge-sequence integration test in `tests/page_lifecycle.rs`
  (create hidden → activate → status Visible → click succeeds → capture reaches
  `Capturing`).

Verification: `cargo fmt --all -- --check`, `cargo check --workspace --all-targets
--locked`, `cargo test --workspace --all-targets --locked`, `cargo clippy --workspace
--all-targets --locked -- -D warnings` — all green. One unrelated pre-existing flaky
test surfaced during verification and was parked as
`idea-flaky-discovery-precedence-test` (fails ~40% on base too; not caused by this
change).
