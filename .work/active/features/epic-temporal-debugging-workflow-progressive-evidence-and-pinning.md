---
id: epic-temporal-debugging-workflow-progressive-evidence-and-pinning
kind: feature
stage: drafting
tags: [visual, storage, agent-ux]
parent: epic-temporal-debugging-workflow
depends_on:
  - epic-temporal-debugging-workflow-resolved-temporal-queries
  - epic-temporal-debugging-workflow-artifact-generation-and-cache
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Progressive Evidence Retrieval and Pinning

## Brief

Deliver the focused investigation operations beneath the primary bundle: retrieve an individual generated artifact, list or fetch selected retained source frames, generate a fixed region filmstrip with locator context, and request supported artifact variants without loading a complete recording. Every result remains tied to the same resolved session, target, source identities, gaps, and provenance used by the bundle.

Support the SPEC region forms by mapping declared viewport/source coordinates, a region selected from a source frame, current structured-reference geometry, or a caller mask into the existing temporal-vision contracts without claiming logical element tracking. Add pin and unpin operations that protect the storage segments intersecting the exact resolved range, and report the actual protected range/segments and retention state so agents know what evidence remains available.

This feature owns progressive domain operations and stable evidence handles for later resource presentation. It does not create tracked regions, infer geometry across time, pin derived artifacts as authoritative evidence, expose remote file access, or maintain a second source-frame read or retention path.

## Epic context

- Parent epic: `epic-temporal-debugging-workflow`
- Position in epic: progressive-detail capability — parallel to bundle composition after range and artifact foundations; consumed by MCP resources and focused tools

## Simplification opportunity

- Reuse `FrameSource`, the artifact cache, structured snapshot geometry, and `RetentionStore` directly behind one progressive-evidence service. Do not duplicate frame decoding, resource payload storage, region math, or pin bookkeeping in MCP handlers.

## Foundation references

- `docs/SPEC.md` — Temporal Queries, Regions of Interest, Disk Budget and Retention, and Artifact Provenance
- `docs/ARCHITECTURE.md` — Retention, Temporal Range Resolution, Artifact Generation, and MCP Boundary
- `docs/VISUAL-EVIDENCE.md` — Region Filmstrip and Progressive Detail
