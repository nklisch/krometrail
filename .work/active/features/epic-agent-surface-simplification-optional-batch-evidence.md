---
id: epic-agent-surface-simplification-optional-batch-evidence
kind: feature
stage: drafting
tags: [agent-ux, browser]
parent: epic-agent-surface-simplification
depends_on: [epic-agent-surface-simplification-response-detail]
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Optional batch step screenshot evidence

## Brief

Make batch step screenshot evidence genuinely optional across the core result, CDP execution, and MCP projection. When screenshots are disabled or a step is never attempted, omit the screenshot field. When screenshots were requested and capture failed, retain the structured unavailable/error evidence. Prove disabled mode issues no per-step screenshot acquisition.

## Epic context

- Parent epic: `epic-agent-surface-simplification`
- Position in epic: canonical batch-semantics consumer of the new response shape

## Simplification opportunity

Delete fabricated `Unsupported` screenshot observations, placeholder helpers, imports, and repeated branches. Model absence directly instead of teaching the projector to hide a false domain outcome.

## Foundation references

- `docs/SPEC.md` — Batching
- `docs/ARCHITECTURE.md` — Interaction Execution and MCP Boundary
