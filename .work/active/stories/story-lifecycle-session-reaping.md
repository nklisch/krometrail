---
id: story-lifecycle-session-reaping
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

# Ended-session slot reaping

Unit 4 of the parent design: start_browser reaps an ended session slot and proceeds; stop_browser on an ended session succeeds reporting cleanup; last-page-close warning and ended-session errors gain recovery guidance naming start_browser.

Acceptance evidence and file targets are defined in the parent feature's
implementation unit; this story is the durable checkpoint for that unit.
