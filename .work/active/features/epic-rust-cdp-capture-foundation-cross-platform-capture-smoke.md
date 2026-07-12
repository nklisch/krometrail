---
id: epic-rust-cdp-capture-foundation-cross-platform-capture-smoke
kind: feature
stage: drafting
tags: [browser, testing, infra]
parent: epic-rust-cdp-capture-foundation
depends_on: [epic-rust-cdp-capture-foundation-bounded-screencast-ingestion]
release_binding: null
gate_origin: null
created: 2026-07-12
updated: 2026-07-12
---

# Cross-Platform Capture Fidelity Smoke

## Brief

Provide a minimal real-browser proof that the foundation captures visible transitions with trustworthy timing and loss reporting on supported systems. The smoke exercises current stable Chrome or Chromium on Linux and current stable Chrome on macOS, including a high-DPI configuration, and records browser/protocol identity, frame cadence, distinct clocks, sequence continuity, declared gaps, and shutdown behavior.

Keep this proof intentionally smaller than the evaluation program: use only enough deterministic browser behavior to catch transport, scaling, acknowledgement, and platform regressions in the live frame stream. Duration sweeps, the full visual-defect corpus, artifact comparisons, storage validation, and agent-effectiveness claims remain owned by `epic-prove-temporal-advantage` and its prerequisites.

## Epic context

- Parent epic: `epic-rust-cdp-capture-foundation`
- Position in epic: final foundation gate — validates the complete live-capture path after production ingestion lands
- Design decisions inherited: qualify capture against real Chrome and report unsupported protocol behavior explicitly

## Foundation references

- `docs/VISION.md` — Local-First Operation and Success
- `docs/SPEC.md` — Supported Environment, Continuous Visual Capture, and Exclusions
- `docs/ARCHITECTURE.md` — Observability and Technology Decisions
- `docs/EVALUATION.md` — Capture-Fidelity Evaluation, Timing Integrity, and Cross-Platform Evaluation
