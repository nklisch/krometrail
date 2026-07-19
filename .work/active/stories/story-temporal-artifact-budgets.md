---
id: story-temporal-artifact-budgets
kind: story
stage: implementing
tags: [browser]
parent: feature-temporal-range-artifact-economy
depends_on: [story-temporal-resolve-range]
release_binding: null
gate_origin: null
created: 2026-07-19
updated: 2026-07-19
---

# Artifact budgets follow generator consumption

Unit 2 of the parent design: storyboard fit budgeting and decode bounded by tile selection; per-generator source-frame gate so an exhaustive generator's refusal cannot zero out a selection-bounded one; reproduce then fix the region-filmstrip full-frame normalization charge. Regressions: 44-frame FitLimits storyboard, 367-frame bundle default, 87-frame small-region filmstrip.

Acceptance evidence and file targets are defined in the parent feature's
implementation unit; this story is the durable checkpoint for that unit.

## Completion Notes

Implemented selection-bounded storyboard and region-filmstrip planning with
full source provenance, per-generator refusal boundaries, and cropped-region
normalization plus a one-frame full locator normalization.
