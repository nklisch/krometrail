---
id: epic-agent-browser-reliability-capture-outcomes
kind: feature
stage: drafting
tags: [browser, storage, agent-ux]
parent: epic-agent-browser-reliability
depends_on: [durable-agent-diagnostics]
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Truthful capture and operation outcomes

## Brief

Correct GitHub issues #1, #2, and #9 by reporting browser mutation, live observation, and retained temporal capture as distinct facts. Preserve the concrete capture failure stage in durable diagnostics and surface unhealthy retained capture on subsequent operations without making current-state browser control depend on the recording pipeline.

Successful navigation or input must remain successful when only post-operation evidence degrades, with the evidence failure attached as a warning and correlated diagnostic. Automatically returned screenshots must be captured at a bounded compositor-ready boundary and remain distinguishable from retained screencast frames. A clean shutdown of an already-failed stream must not rewrite historical capture failure as cleanup failure; the managed-session feature owns the final lifecycle result.

## Epic context
- Parent epic: `epic-agent-browser-reliability`
- Position in epic: consumes durable diagnostic correlation and produces the outcome contract later documented by agent guidance.

## Simplification opportunity
- Consolidate page-operation and interaction evidence classification around one result projection instead of per-operation success rewriting.

## Foundation references
- `docs/SPEC.md` — browser-control and temporal-evidence contracts
- `docs/ARCHITECTURE.md` — capture pipeline, operation execution, and MCP projection
- `docs/VISUAL-EVIDENCE.md` — source-frame and live-screenshot distinction
