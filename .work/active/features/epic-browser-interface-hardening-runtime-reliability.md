---
id: epic-browser-interface-hardening-runtime-reliability
kind: feature
stage: drafting
tags: [browser, visual]
parent: epic-browser-interface-hardening
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Reliable Viewport and Capture State

## Brief

Restore responsive viewport presets on real managed Chrome and keep screencast capture alive on nested-frame pages. Desktop responsive overrides currently fail when the visual viewport is reduced by a scrollbar even though Chrome applied the declared emulation width. Separately, a frame-triggered geometry refresh that cannot immediately verify the effective viewport exhausts retries, fails the geometry transition, and terminates useful capture after already acknowledged frames.

Preserve the target-scoped viewport and geometry-fence architecture. Verify declared emulation with the correct CDP metrics, report observed visual content separately, isolate refresh failures as explicit gaps, and prove recovery without weakening frame acknowledgement or retained evidence truth.

## Source findings

- `idea-fix-viewport-preset-regression`
- `idea-fix-frame-envelope-capture`

## UI alignment

No UI surface; this is target-scoped CDP and capture-pipeline reliability.
