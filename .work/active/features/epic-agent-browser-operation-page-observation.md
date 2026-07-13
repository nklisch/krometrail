---
id: epic-agent-browser-operation-page-observation
kind: feature
stage: drafting
tags: [browser, agent-ux]
parent: epic-agent-browser-operation
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Structured Page Observation

## Brief

Give agents a trustworthy current-page view: a compact accessibility-centered snapshot, generation-scoped actionable references, current URL/title/viewport/navigation state, and requested viewport, full-page, element, or region screenshots. The feature establishes the shared live-observation result used after state-changing operations while preserving screenshot and snapshot provenance to the selected target.

Resolve references through snapshot-local accessibility and DOM metadata, re-check backing-node validity and actionability at use time, and fail stale references with concrete refresh guidance. Explicit CSS selectors and declared coordinate spaces remain weaker escape hatches; JavaScript evaluation and read-only inspection belong here, while input dispatch, navigation, waiting, batching, and MCP registration do not.

## Epic context

- Parent epic: `epic-agent-browser-operation`
- Position in epic: foundation capability — page lifecycle and interaction both consume its snapshot, reference, screenshot, and live-observation contracts
- Inherited decisions: core owns public control contracts; every state-changing operation must return honest post-action evidence; no traditional visual UI or mockups are required

## Simplification opportunity

- Replace the deferred `SnapshotGeneration` and `NodeReference` placeholders with one generation registry and one resolver. Accessibility, DOM geometry, selectors, and coordinates are evidence or fallback target forms, not competing element-identity systems.

## Foundation references

- `docs/VISION.md` — Core Experience
- `docs/SPEC.md` — Current-State Observation and Structured Page Snapshots
- `docs/ARCHITECTURE.md` — Structured Snapshots and References, MCP Boundary, and Failure Isolation
- `docs/EVALUATION.md` — Browser-Control Evaluation

<!-- The feature-design pass will fill in interfaces, signatures, and implementation units. -->
