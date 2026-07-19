---
id: feature-response-evidence-economy-staleness-images
kind: story
stage: implementing
tags: [agent-ux, visual]
parent: feature-response-evidence-economy
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-19
updated: 2026-07-19
---

# Staleness-triggered auto-images and tall full-page guidance

Checkpoint for Unit 2 of the parent design: `browser_inline_image_default` adds
`Scroll | SetViewport | ActivatePage` to the default-on set; full-page screenshots taller
than 8192 px carry one guidance warning naming element/region capture and viewport
scrolling; SPEC.md "Routine operations remain pixel-light" sentence and skill text gain
the staleness exception.

## Acceptance
- Registry default test extended: the three operations default on; explicit
  `inline_images: false` suppresses.
- >8192px full-page capture succeeds with exactly one guidance warning; shorter carries
  none.
- SPEC.md + skill text updated; `docs/public/llms-full.txt` regenerated.
