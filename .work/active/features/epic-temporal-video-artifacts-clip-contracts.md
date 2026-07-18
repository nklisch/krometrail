---
id: epic-temporal-video-artifacts-clip-contracts
kind: feature
stage: drafting
tags: [visual, agent-ux, security]
parent: epic-temporal-video-artifacts
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Temporal video clip contracts

## Brief

Define the deterministic, browser-independent contract that turns one resolved retained range into a bounded temporal-video presentation plan. It covers real-time and model-optimized timing, ordered source mapping, explicit gap slates, visual-epoch boundaries, output ceilings, encoder input, result identity, and typed video provenance. The same canonical plan must drive encoding and the manifest so a held frame or gap can never be represented differently across those surfaces.

This is the shared foundation for the process adapter and retained generation service. It does not discover or launch FFmpeg, publish bytes, register MCP tools, upload to providers, or add video responsibilities to `temporal-vision`'s still-image manifest.

## Epic context

- Parent epic: `epic-temporal-video-artifacts`
- Position in epic: foundation feature — the FFmpeg runtime and retained generation service consume its contracts independently

## Simplification opportunity

- Reuse the existing resolved-range, source-frame, gap, visual-epoch, cancellation, artifact-identity, and validated-wire-contract vocabulary; introduce only the video-specific timing and encoder provenance needed to keep the still visual crate process-free.

## Foundation references

- `docs/VISION.md` — Visual Evidence and Product Boundaries
- `docs/SPEC.md` — Temporal Queries, Artifact Provenance, and Exclusions
- `docs/ARCHITECTURE.md` — Artifact Generation, Capability Registry, and Dependency Direction
- `docs/VISUAL-EVIDENCE.md` — Temporal Video Clip, Capture Gaps, and Provenance
- `docs/EVALUATION.md` — Optional video conditions and Temporal video evaluation

## Parent decisions inherited

- Both presentation policies ship under one versioned deterministic plan.
- Video provenance is typed without importing process or codec concerns into `temporal-vision`.
- Numeric ceilings are hard server boundaries; callers may only request values within them.
- No UI surfaces or mockups apply.

