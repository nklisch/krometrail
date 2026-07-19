---
id: feature-response-evidence-economy-viewport-anchoring
kind: story
stage: done
tags: [agent-ux, browser]
parent: feature-response-evidence-economy
depends_on: [feature-response-evidence-economy-dedupe-projection]
release_binding: null
gate_origin: null
created: 2026-07-19
updated: 2026-07-19
---

# Viewport-anchored post-scroll evidence

Checkpoint for Unit 3 of the parent design: optional `SnapshotNode.document_rect`
populated by one bounded `DOMSnapshot.captureSnapshot` layout pass for scroll and
set-viewport observations only (reusing the frame-query story's DOM-semantics
acquisition); `bounded_targets` and `semantic_outcomes` rank/filter by visual-viewport
intersection when geometry is present, exact current behavior when absent.

## Acceptance
- Post-scroll concise index leads with in-viewport targets; semantic outcomes describe
  in-viewport text (fixture with above/inside-viewport targets).
- Non-scroll operations acquire no DOMSnapshot layout pass (command-recording double).
- Geometry-less snapshots project byte-identical to pre-change output.

## Completion Note

Implemented and verified: geometry-bearing scroll/set-viewport observations now attach bounded
DOMSnapshot layout rectangles to AX nodes and viewport-anchor response ranking; ordinary snapshots
remain geometry-less and avoid the DOMSnapshot layout pass.

## Review-fix note (2026-07-19)

The geometry-less path now has an exact serialized concise/expanded projection regression, proving
optional `document_rect` remains behavior-neutral when absent.
