---
id: feature-fix-preserve-visibility-wedge
kind: feature
stage: drafting
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
