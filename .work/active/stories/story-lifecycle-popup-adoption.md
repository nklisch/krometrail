---
id: story-lifecycle-popup-adoption
kind: story
stage: done
tags: [browser]
parent: feature-window-lifecycle-integrity
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-19
updated: 2026-07-19
---

# Popup navigation and adoption + post-action observation degradation

Units 1-2 of the parent design: root-cause and fix the window.open popup initial-navigation starvation (real-chrome opt-in test proves the popup loads unaided, becomes supervised with opener_target_id, and wait_for_page matches it; deterministic reducer tests for empty-URL create-then-adopt and unsolicited-attach handling), and convert post-action observation failures on dispatched interactions from hard errors into degraded responses carrying the interaction record and diagnostics.

Acceptance evidence and file targets are defined in the parent feature's
implementation unit; this story is the durable checkpoint for that unit.

## Completion Notes

Reducer coverage now retains unsolicited auto-attached popup sessions and
adopts them when a recordable URL arrives, preserving opener identity. The
post-dispatch observation path remains degraded-safe. Deterministic coverage
passes, including the session-scoped `Runtime.runIfWaitingForDebugger` release.
The confirmed Chrome 149 root cause is that browser-level auto-attach suspends a
popup's initial `window.open` navigation while falsely reporting
`waitingForDebugger:false`; releasing the pending session unblocks the commit.
The opt-in live Chrome test was rerun with its environment gate enabled but
Chrome exited 133 during sandbox startup (`Crashpad setsockopt:
Operation not permitted`) before the popup assertions could run.
