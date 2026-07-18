---
id: epic-agent-browser-ergonomics-viewport-intent
kind: feature
stage: drafting
tags: [agent-ux, browser]
parent: epic-agent-browser-ergonomics
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Viewport intent and presets

## Brief

Add intention-revealing responsive-CSS and mobile-device presets that materialize into the existing explicit target-scoped viewport override. Return preset/intent provenance and warn when observed layout geometry differs materially from the requested visual viewport, especially when missing page viewport metadata produces Chrome's 980px mobile layout.

Custom metrics and clear retain their stable meanings. Presets do not change user agent, browser identity, or the lifecycle-complete reconnect/rollback authority.

## Epic context

- Parent epic: `epic-agent-browser-ergonomics`
- Position in epic: independent additive viewport contract

## Simplification opportunity

Materialize all presets through `ViewportMetrics` and derive guidance from the already observed `PageState`; do not add a parallel emulation state machine.

## Foundation references

- `docs/SPEC.md` — Viewport emulation
- `docs/ARCHITECTURE.md` — Target Lifecycle
