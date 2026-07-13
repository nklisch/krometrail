---
id: epic-agent-browser-operation-verified-interactions
kind: feature
stage: drafting
tags: [browser, agent-ux]
parent: epic-agent-browser-operation
depends_on: [epic-agent-browser-operation-page-observation]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Verified Browser Interactions

## Brief

Let agents act on the observed page through reference-first click, fill/type, key input, selection, hover, drag, scroll, file upload, and dialog operations, with declared coordinate-space fallback for DOM-opaque content. Each operation creates an interaction record before dispatch, validates its target at the last responsible moment, applies action-appropriate completion, and returns an explicit post-action live observation and timeline anchor.

Use one action registry to drive variants, validation, routing, sanitized interaction parameters, and stable display instead of bespoke public contracts per action. This feature owns standalone input execution and explicit stale/no-op failures; page lifecycle, caller-requested waits, ordered batching, durable interaction persistence, and MCP handlers remain with their owning features or epics.

## Epic context

- Parent epic: `epic-agent-browser-operation`
- Position in epic: sibling consumer of `epic-agent-browser-operation-page-observation`; combines with page lifecycle before waits and batches are integrated
- Inherited decisions: snapshot references are primary, selectors and coordinates are weaker declared fallbacks, and silent or guessed success is forbidden

## Simplification opportunity

- Route all interaction variants through one registry and executor shape with narrowly action-specific CDP mechanics and completion policies. Avoid one service, schema, error taxonomy, and result envelope per input command.

## Foundation references

- `docs/VISION.md` — Product Thesis and Core Experience
- `docs/SPEC.md` — Current-State Observation, Structured Page Snapshots, Browser-Control Surface, and Action Timeline
- `docs/ARCHITECTURE.md` — Structured Snapshots and References and Interaction Execution
- `docs/EVALUATION.md` — Browser-Control Evaluation

<!-- The feature-design pass will fill in interfaces, signatures, and implementation units. -->
