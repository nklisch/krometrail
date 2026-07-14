---
id: epic-prove-temporal-advantage-benchmark-corpus-and-manifest-contracts
kind: feature
stage: drafting
tags: [testing, visual, browser]
parent: epic-prove-temporal-advantage
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Benchmark Corpus and Manifest Contracts

## Brief

Deliver the committed benchmark vocabulary for the temporal-advantage program: deterministic movement-reversal, flicker, transient-layout, DOM-opaque-motion, and stable-control target applications, each with hidden machine-readable ground truth and controlled duration/variant metadata. The fixtures remain standalone browser targets, not a second Krometrail runtime and not framework-state test subjects. Existing browser fixtures, shared local-server patterns, and the current cross-platform smoke fixture should be reused where they fit instead of creating duplicate launch or target abstractions.

Define the benchmark matrix, evidence-condition identifiers, structured interpretation/debugging task prompts, deterministic ordering and seed rules, and the versioned schemas consumed by every later feature. This feature owns the distinction between a known fixture state and a Krometrail observation; ground truth must never be computed from Krometrail measurements. It does not claim that a real Chrome stream captures any duration or that a model understands an artifact.

## Epic context

- Parent epic: `epic-prove-temporal-advantage`
- Position in epic: contract and corpus foundation — every later harness and evidence lane consumes these identifiers and definitions

## Execution boundary

- CI-safe by default: fixture definitions, ground-truth timelines, prompts, schemas, and matrix configuration are committed and runnable without Chrome, network access, paid agents, or a second model family.
- Live Chrome, cross-platform collection, and model execution are separate opt-in consumers. Missing environments remain explicit unavailable evidence; this feature does not provide fallback passes.

## Simplification opportunity

- Reuse the existing standalone fixture conventions, `tests/fixtures/browser/README.md` boundary, shared Chrome test helpers, and versioned evidence-schema style. Do not add framework-state instrumentation, a product CLI command, a second fixture server framework, or compatibility aliases for unpublished benchmark contracts.

## Foundation references

- `docs/VISION.md` — Product Thesis, Core Experience, Local-First Operation, and Success
- `docs/SPEC.md` — Supported Environment, Continuous Visual Capture, Temporal Ranges, and Exclusions
- `docs/ARCHITECTURE.md` — Temporal Visual Crate, Capture Tasks, and Failure Isolation
- `docs/VISUAL-EVIDENCE.md` — Evidence Classes, Capture Gaps, Provenance, and Non-Diagnostic Posture
- `docs/EVALUATION.md` — Benchmark Corpus, Ground Truth, and Visual Interpretation Tasks
- `tests/fixtures/browser/README.md` — target-application boundary

<!-- Feature design will define implementation units, interfaces, and focused verification. -->
