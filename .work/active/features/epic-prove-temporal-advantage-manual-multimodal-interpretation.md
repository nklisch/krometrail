---
id: epic-prove-temporal-advantage-manual-multimodal-interpretation
kind: feature
stage: drafting
tags: [testing, visual]
parent: epic-prove-temporal-advantage
depends_on:
  - epic-prove-temporal-advantage-deterministic-scoring-and-artifact-conditions
  - epic-prove-temporal-advantage-platform-evidence-collection-linux-stable-chrome-reference-host-evidence
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-15
---

# Manual Multimodal Interpretation

## Brief

Run the structured visual-interpretation comparison over the same captured intervals and the five evidence conditions defined by the deterministic harness. The initial supported agent lane is the locally available Codex CLI with explicitly recorded model/version, prompts, tools, evidence condition, artifact parameters, fixture order, seed, raw answer, and scoring rationale. The scorer separates observation from diagnosis and requires the agent to state uncertainty when declared gaps or retention loss prevent a claim.

This feature is deliberately manual and authorization-gated. It must never invoke a paid model from CI, from a normal Rust test, or implicitly while collecting browser evidence. A missing authorization, model, run budget, captured interval, or minimum scenario count produces no result or an inconclusive result; it does not become a green deterministic check. Results are Codex-specific unless a separately authorized, independently identified model family supplies comparable evidence, and no cross-model generality is inferred from one model family.

## Epic context

- Parent epic: `epic-prove-temporal-advantage`
- Position in epic: paid/manual interpretation capability — consumes deterministic conditions, live source evidence, and the shared scorer
- Depends on: deterministic scoring/conditions and the exact Linux stable-Chrome reference-host
  evidence checkpoint

## Dependency correction

Manual interpretation requires one declared, operator-authorized Linux stable-Chrome live evidence
run so every A–E model trial has a real reference-host interval, browser identity, and source
traceability. Its dependency is the exact child story
`epic-prove-temporal-advantage-platform-evidence-collection-linux-stable-chrome-reference-host-evidence`,
not the platform feature or its matrix aggregator. macOS default-DPI/high-DPI evidence and optional
Linux Chromium are separate platform comparisons; they may be absent and leave the matrix
`inconclusive` without delaying this reference-host model lane. No missing run, authorization, or
source interval may be converted into a passing interpretation result.

## Execution boundary

- Requires an operator's explicit authorization and budget before every paid run set; the workflow records authorization state but does not create or assume payment credentials.
- The repository commits prompt/schema/scoring definitions only. Model answers, source frames, generated artifacts, transcripts, manifests, and aggregate results remain local ignored outputs.

## Simplification opportunity

- Reuse the deterministic condition packager, exact artifact manifests, source-frame handles, and structured scoring rubric. Do not add a model-provider abstraction that promises interchangeable semantics, a hidden retry that changes the sample, or an automatic “best answer” selector.

## Foundation references

- `docs/EVALUATION.md` — Artifact Evaluation Conditions, Visual Interpretation Tasks, Model Evaluation Discipline, Product-Thesis Assessment, and Reproducibility
- `docs/VISUAL-EVIDENCE.md` — Evidence Classes, Progressive Detail, Provenance, and Non-Diagnostic Posture
- `docs/VISION.md` — Product Thesis and Visual Evidence

<!-- Feature design will define implementation units, interfaces, and focused verification. -->
