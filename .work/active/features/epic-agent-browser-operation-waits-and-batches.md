---
id: epic-agent-browser-operation-waits-and-batches
kind: feature
stage: drafting
tags: [browser, agent-ux]
parent: epic-agent-browser-operation
depends_on: [epic-agent-browser-operation-browser-page-lifecycle, epic-agent-browser-operation-verified-interactions]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Explicit Waits and Ordered Batches

## Brief

Provide deliberate synchronization for elapsed time, text or element state, navigation, page conditions, and explicitly requested network quiet. Then compose lifecycle, navigation, and interaction operations into ordered per-target batches with per-step status, timing, and interaction anchors, stop-on-first-failure by default, opt-in continuation, optional per-step screenshots, and one final live observation.

Batching reuses the exact standalone operation registry, validation, execution, and completion policies rather than implementing a second automation engine. This feature coordinates operations and reports partial outcomes; it does not introduce implicit global network-idle waiting, cross-target action ordering, durable storage, temporal queries, or MCP registration.

## Epic context

- Parent epic: `epic-agent-browser-operation`
- Position in epic: integration capability — consumes both page lifecycle/navigation and verified interaction before the public MCP surface is generated
- Inherited decisions: standalone and batch schemas derive from the same registry, and every batch step remains independently anchored and explainable

## Simplification opportunity

- Make batches a thin sequential coordinator over standalone operations and central completion policies. Remove any temptation to duplicate CDP commands, target resolution, screenshot composition, or error mapping inside a batch-only path.

## Foundation references

- `docs/VISION.md` — Product Thesis and Core Experience
- `docs/SPEC.md` — Current-State Observation, Browser-Control Surface, Batching, and Action Timeline
- `docs/ARCHITECTURE.md` — Interaction Execution and MCP Boundary
- `docs/EVALUATION.md` — Browser-Control Evaluation

<!-- The feature-design pass will fill in interfaces, signatures, and implementation units. -->
