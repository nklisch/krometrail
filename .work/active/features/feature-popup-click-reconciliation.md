---
id: feature-popup-click-reconciliation
kind: feature
stage: drafting
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
