---
id: truthful-screencast-geometry
kind: feature
stage: drafting
tags: [bug, visual, browser]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Truthful screencast geometry

## Brief

Fix retained frame provenance so visual epochs describe the page's real CSS viewport and device scale rather than Chrome's adaptive screencast encoding dimensions. Manual testing with v1.0.3 held a page at 600×500 CSS pixels and DPR 1, yet one 600×500 encoded frame was recorded as a 300×250 viewport amid otherwise 1200×1000 encoded frames recorded as 600×500. The false transition split a stable five-second interaction into three visual epochs.

The capture pipeline must preserve encoded image dimensions independently from authoritative page geometry across viewport apply, clear, navigation replay, and reconnect. Regression evidence must prove adaptive screencast scaling cannot invent a viewport transition.

## Simplification opportunity

Consolidate capture geometry authority around the already acknowledged target-scoped viewport lifecycle. Remove any inference that treats `Page.screencastFrame` encoding metadata as independently authoritative CSS layout geometry when stronger target state is available.
