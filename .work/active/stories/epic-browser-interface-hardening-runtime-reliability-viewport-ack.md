---
id: epic-browser-interface-hardening-runtime-reliability-viewport-ack
kind: story
stage: done
tags: [browser]
parent: epic-browser-interface-hardening-runtime-reliability
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Verify viewport overrides against authoritative geometry

Accept scrollbar-reduced desktop visual viewports when declared layout/emulation metrics applied exactly, without weakening mobile or true-mismatch validation.

## Implementation notes

- Desktop overrides now acknowledge exact `cssLayoutViewport` geometry while retaining `cssVisualViewport` as the observed content area.
- Capture geometry uses the acknowledged desktop layout dimensions; mobile and cleared targets retain their existing visual/touch checks.
- Deterministic decoder coverage proves scrollbar reduction succeeds only for exact declared layout geometry.
