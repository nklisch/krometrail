---
id: epic-temporal-debugging-workflow
kind: epic
stage: drafting
tags: [visual, storage, agent-ux]
parent: null
depends_on: [epic-temporal-vision-toolkit, epic-durable-browser-memory, epic-agent-browser-operation]
release_binding: null
gate_origin: null
created: 2026-07-11
updated: 2026-07-11
---

# Temporal Debugging Workflow

## Brief

This epic delivers Krometrail’s defining agent workflow: operate the browser normally, notice a symptom, inspect the interval around an interaction, receive compact temporal evidence, and progressively retrieve regions or source frames. It integrates retained ranges with the temporal visual crate and exposes the result through context-efficient MCP responses and resources.

Temporal queries resolve natural anchors once, display gaps and retention warnings, generate reproducible cached artifacts, and keep every summary traceable to source evidence. The default bundle combines a simple orientation view, change-aware storyboard, difference map, capture-quality summary, interaction markers, and source references.

This epic does not perform automatic root-cause diagnosis or deterministic replay. The agent remains responsible for interpreting the evidence and deciding what to inspect next.

## Foundation references

- `docs/VISION.md` — Product Thesis, Core Experience, and Success
- `docs/SPEC.md` — Temporal Ranges, Temporal Queries, Regions of Interest, and Artifact Provenance
- `docs/ARCHITECTURE.md` — Temporal Range Resolution, Artifact Generation, MCP Boundary, and Failure Isolation
- `docs/VISUAL-EVIDENCE.md` — Temporal Debug Bundle and Progressive Detail

## Anticipated child features

- Interaction-relative and explicit temporal query contracts
- Krometrail-to-temporal-vision adapter and artifact cache
- Before/during/after, storyboard, and difference-map debug bundle
- Artifact and source-frame MCP resources
- Region-focused filmstrips and progressive source retrieval
- Pinning and retention controls in the investigation workflow
- Capture-quality summaries, gaps, and degraded query responses
- End-to-end temporal investigation scenarios

<!-- The design pass on each child feature will fill in real specifics. -->
