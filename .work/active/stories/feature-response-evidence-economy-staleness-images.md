---
id: feature-response-evidence-economy-staleness-images
kind: story
stage: done
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
- [x] Registry default test extended: the three operations default on; explicit
  `inline_images: false` suppresses.
- [x] >8192px full-page capture succeeds with exactly one guidance warning; shorter carries
  none.
- [x] SPEC.md + skill text updated; `docs/public/llms-full.txt` regenerated.

## Completion Note

Implemented and verified Unit 2. Purpose-sensitive defaults now include staleness-prone scroll, viewport, and activation observations; tall decoded screenshots emit one bounded recovery warning without changing capture limits; SPEC, plugin skill text, and generated full documentation are current.

## Review-fix note (2026-07-19)

Direct capture tests now assert exactly one guidance warning above 8192px and none at the threshold.
