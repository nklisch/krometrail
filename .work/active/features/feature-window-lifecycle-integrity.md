---
id: feature-window-lifecycle-integrity
kind: feature
stage: drafting
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
