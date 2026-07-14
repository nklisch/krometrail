---
id: epic-temporal-debugging-workflow-mcp-investigation-surface
kind: feature
stage: drafting
tags: [agent-ux, visual, browser, storage]
parent: epic-temporal-debugging-workflow
depends_on:
  - epic-temporal-debugging-workflow-temporal-debug-bundle
  - epic-temporal-debugging-workflow-progressive-evidence-and-pinning
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# MCP Temporal Investigation Surface

## Brief

Expose the completed temporal workflow through MCP: one debug-bundle tool as the primary range or interaction entry point, plus focused tools for source frames, region filmstrips, artifact variants, verbose browser-event detail, and pin state. Derive names, capability membership, schemas, routing, and annotations from the existing capability/operation contract patterns so disabling temporal vision removes its tools without affecting ordinary browser control.

Present compact summaries and a context-sized primary image through the established response envelope, while full-resolution artifacts and source frames are readable through durable MCP resources tied to retained evidence. Resource reads enforce the same session/target, provenance, retention, and stable-error rules as focused tools; eviction or deletion becomes an honest unavailable-resource result rather than a stale file leak.

Qualify the integrated local agent workflow from browser interaction anchor through resolved range, bundle generation/cache, correlated warnings/events, region or source drill-down, pinning, and resource retrieval using deterministic local scenarios. This is product capability qualification, not paid multimodal evaluation; remote transports, automatic diagnosis, replay, cross-session comparison, page/framework state, and the separate thesis benchmark remain out of scope.

## Epic context

- Parent epic: `epic-temporal-debugging-workflow`
- Position in epic: final integration capability — presents the primary bundle and progressive evidence after both domain paths are complete

## Simplification opportunity

- Extend the current dynamic MCP router, response mapper, stdio service, and one capability registry instead of creating a temporal-only server or handwritten schema mirror. Replace the router's control-only registration assumption with per-capability contributions and add one resource authority rather than exposing raw filesystem paths.

## Foundation references

- `docs/VISION.md` — Core Experience and Local-First Operation
- `docs/SPEC.md` — Capabilities, Temporal Queries, Errors and Degraded Operation, and Local Data and Telemetry
- `docs/ARCHITECTURE.md` — Capability Registry and MCP Boundary
- `docs/VISUAL-EVIDENCE.md` — Temporal Debug Bundle and Progressive Detail
- `docs/EVALUATION.md` — Artifact Evaluation Conditions and Reproducibility
