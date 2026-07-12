---
id: epic-prove-temporal-advantage
kind: epic
stage: drafting
tags: [testing, visual, browser]
parent: null
depends_on: [epic-temporal-debugging-workflow]
release_binding: null
gate_origin: null
created: 2026-07-11
updated: 2026-07-11
---

# Prove the Temporal Advantage

## Brief

This epic establishes whether Krometrail’s product thesis is true. It delivers the deterministic defect corpus, stable controls, capture-duration sweeps, artifact interpretation comparisons, agent debugging scenarios, control reliability checks, retention validation, and cross-platform measurements defined by the evaluation foundation.

The evidence program compares final screenshots, uniform sampling, change-aware storyboards, temporal bundles, and progressive source access. It records model, browser, operating-system, prompt, artifact, capture, and scoring details so improvements are reproducible rather than anecdotal.

This epic is not a generic test-cleanup container. Its output is the project’s defensible claim boundary: what transient durations are captured, which artifacts help which models, where false interpretations occur, and whether agents debug more successfully with temporal evidence.

## Foundation references

- `docs/VISION.md` — Success
- `docs/SPEC.md` — Supported Environment and Exclusions
- `docs/VISUAL-EVIDENCE.md` — Accessibility and Model Readability
- `docs/EVALUATION.md` — complete evaluation contract and product-thesis gate

## Anticipated child features

- Deterministic movement, flicker, layout, canvas, and stable-control fixtures
- Capture duration sweep and timing-integrity harness
- Artifact-condition comparison and structured visual scoring
- Multimodal interpretation harness across independent model families
- End-to-end agent diagnosis, patch, and verification benchmark
- Browser-control reliability, retention, and performance validation
- Linux and macOS result collection with reproducible manifests

<!-- The design pass on each child feature will fill in real specifics. -->
