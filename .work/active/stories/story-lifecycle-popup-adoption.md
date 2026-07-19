---
id: story-lifecycle-popup-adoption
kind: story
stage: implementing
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
