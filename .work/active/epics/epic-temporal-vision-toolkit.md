---
id: epic-temporal-vision-toolkit
kind: epic
stage: drafting
tags: [visual]
parent: null
depends_on: [epic-rust-cdp-capture-foundation]
release_binding: null
gate_origin: null
created: 2026-07-11
updated: 2026-07-11
---

# Temporal Vision Toolkit

## Brief

This epic delivers the browser-agnostic Rust crate that turns timestamped image sequences into compact temporal visual evidence. The crate owns generic frame and marker inputs, direct visual-change measurements, deterministic representative-frame selection, artifact rendering, and reproducible provenance.

The toolkit remains independent of Chrome, CDP, Krometrail storage, MCP, DOM state, and framework types. Its outputs distinguish source frames, source-derived transformations, and inferred analysis so callers can trace every visual claim back to authoritative evidence.

This epic does not own browser capture, persistent sessions, agent tool schemas, logical element tracking, or automatic diagnosis. Those responsibilities remain in Krometrail or in separately labeled inferred-analysis extensions.

## Foundation references

- `docs/VISION.md` — Visual Evidence and Reusable Temporal Vision
- `docs/ARCHITECTURE.md` — Temporal Visual Crate and Artifact Generation
- `docs/VISUAL-EVIDENCE.md` — complete artifact and provenance contract
- `docs/EVALUATION.md` — Artifact Evaluation Conditions and Artifact-Specific Evaluation

## Design decisions

- **Processing model:** Expose an immutable batch-sequence API that generates measurements and artifacts on demand. Streaming and rolling analysis remain caller responsibilities until measured workloads demonstrate that a stateful crate API is necessary.
- **Browser-agnostic proof:** Validate reuse through deterministic synthetic and non-browser frame-sequence examples inside the crate. A real Silas integration is not part of this epic and can consume the crate later without shaping its initial public surface.

## Anticipated child features

- Generic frame-sequence, marker, gap, region, and provenance contracts
- Deterministic visual-change measurements and normalization
- Representative-frame selection and temporal storyboard rendering
- Temporal difference-map rendering
- Region filmstrip rendering
- Motion-history experiments and source-versus-inference boundaries

<!-- The design pass on each child feature will fill in real specifics. -->
