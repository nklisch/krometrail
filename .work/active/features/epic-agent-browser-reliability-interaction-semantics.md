---
id: epic-agent-browser-reliability-interaction-semantics
kind: feature
stage: drafting
tags: [browser, agent-ux]
parent: epic-agent-browser-reliability
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Consistent interaction semantics

## Brief

Correct GitHub issues #7, #8, and #11 through one coherent interaction contract. Replace-mode fill must clear editable controls, including password inputs, without platform-specific shortcut assumptions or secret exposure. Key chords and named activation keys must use canonical validated spellings and normal Chrome semantics, while distinguishing dispatched input from any subsequently observed DOM effect.

Page-scoped requests should default to the selected page and common interaction options should have safe defaults. Structured references remain usable while their attachment, document, and backing node remain valid rather than becoming stale merely because another observation created a snapshot. Element pointer actions consistently prepare off-screen targets by scrolling, then re-resolve and validate viewport geometry before dispatch.

## Epic context
- Parent epic: `epic-agent-browser-reliability`
- Position in epic: independent runtime feature; its final request examples are consumed by agent-contract guidance.

## Simplification opportunity
- Centralize editable selection, key event construction, and element preparation so fill, click, hover, and drag do not maintain divergent workarounds.

## Foundation references
- `docs/SPEC.md` — browser-control action contract
- `docs/ARCHITECTURE.md` — page control and snapshot authority
