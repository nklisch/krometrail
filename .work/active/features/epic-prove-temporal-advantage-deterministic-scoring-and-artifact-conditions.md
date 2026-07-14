---
id: epic-prove-temporal-advantage-deterministic-scoring-and-artifact-conditions
kind: feature
stage: drafting
tags: [testing, visual]
parent: epic-prove-temporal-advantage
depends_on: [epic-prove-temporal-advantage-benchmark-corpus-and-manifest-contracts]
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Deterministic Scoring and Artifact Conditions

## Brief

Deliver the CI-safe evaluation harness for the five evidence conditions: final screenshot, uniform storyboard, change-aware storyboard, temporal bundle, and progressive source access. It consumes the committed corpus and ground truth, uses deterministic source sequences or fake capture/storage ports where live infrastructure is unnecessary, and scores temporal-state recall, ordering, region localization, reversal/direction description, uncertainty under gaps, false defects on stable controls, and source-frame traceability. It also validates that artifact outputs and manifests are reproducible for identical inputs and algorithm versions.

This feature owns condition packaging and the structured scorer, not model calls. It must prove that the benchmark can distinguish the conditions and that a reported claim can be traced to exact retained source identities without treating a generated artifact as ground truth. It does not turn a green fake or synthetic run into a real-Chrome capture claim, a model-comprehension claim, or a product-thesis pass.

## Epic context

- Parent epic: `epic-prove-temporal-advantage`
- Position in epic: deterministic evidence foundation — live collection and manual model lanes consume its condition and scoring contracts
- Depends on: `epic-prove-temporal-advantage-benchmark-corpus-and-manifest-contracts`

## Execution boundary

- Runs in ordinary locked Rust CI with no browser installation, network, paid Codex invocation, or external model.
- Uses explicit fake clocks, source frames, gaps, storage/retention seams, and bounded artifact handles only where those adapters already represent the product boundary; it must not hide missing live evidence as a fixture pass.

## Simplification opportunity

- Reuse `temporal-vision` measurements, selection plans, manifests, artifact versions, existing progressive evidence contracts, and store test seams. Do not add a parallel visual algorithm, a second provenance format, a generic statistics framework, or low-value line-coverage tests.

## Foundation references

- `docs/VISUAL-EVIDENCE.md` — Shared Artifact Contract, Temporal Storyboard, Temporal Difference Map, Progressive Detail, and Determinism
- `docs/EVALUATION.md` — Artifact Evaluation Conditions, Visual Interpretation Tasks, Scoring, and Reproducibility
- `docs/ARCHITECTURE.md` — Temporal Visual Crate, Artifact Generation, and Failure Isolation
- `docs/SPEC.md` — Artifact Provenance and Errors and Degraded Operation

<!-- Feature design will define implementation units, interfaces, and focused verification. -->
