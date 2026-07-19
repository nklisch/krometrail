---
id: story-lifecycle-metrics-fallback
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

# Layout-metrics fallback and truthful recovery

Unit 3 of the parent design: when Page.getLayoutMetrics yields a non-finite/non-positive CSS size, fall back to the JS-observed innerWidth/innerHeight geometry with a metrics-fallback note; only fail when both sources are unusable, with recovery naming reload/navigation and retry after_recovery.

Acceptance evidence and file targets are defined in the parent feature's
implementation unit; this story is the durable checkpoint for that unit.
