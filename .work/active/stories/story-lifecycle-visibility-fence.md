---
id: story-lifecycle-visibility-fence
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

# Visibility observation ordering fence

Unit 5 of the parent design: stamp observed session time on both visibility input variants at all four producers; reducer ignores observations older than the last accepted per target; deterministic race test proves an activated target stays visible and recording when a stale queued hidden event arrives late.

Acceptance evidence and file targets are defined in the parent feature's
implementation unit; this story is the durable checkpoint for that unit.

## Completion Notes

Both visibility input variants now carry shared session observation time at all
four producer sites, and the single-writer reducer ignores stale observations.
The deterministic activation ordering race passes.
