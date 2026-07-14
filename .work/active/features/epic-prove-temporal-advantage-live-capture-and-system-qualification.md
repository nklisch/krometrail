---
id: epic-prove-temporal-advantage-live-capture-and-system-qualification
kind: feature
stage: drafting
tags: [testing, browser, storage]
parent: epic-prove-temporal-advantage
depends_on: [epic-prove-temporal-advantage-benchmark-corpus-and-manifest-contracts, epic-prove-temporal-advantage-deterministic-scoring-and-artifact-conditions]
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Live Capture and System Qualification

## Brief

Qualify the production browser-control, capture, timeline, artifact, and storage path against the committed corpus on an explicitly enabled local Chrome/Chromium installation. The harness covers the Evaluation contract's duration sweep, source-versus-observed timing, explicit capture gaps, movement-sequence evidence, control reliability, retention and recovery behavior, resource usage, and temporal-query/artifact latency. It reuses the existing production connector, opt-in real-browser gate, managed-profile cleanup checks, capture status, and retention authorities rather than building a benchmark-only browser runtime.

This is an opt-in live-evidence capability, not a CI prerequisite. It records a complete result or an honest blocked/inconclusive state when Chrome, a supported platform, a required fixture, or a required observation is unavailable. A deterministic harness result from the preceding feature can validate the scorer and contracts, but cannot substitute for a missing live source stream, hide a gap, or satisfy the 100 ms/95% and 50 ms/80% capture envelope.

## Epic context

- Parent epic: `epic-prove-temporal-advantage`
- Position in epic: production-path qualification — produces captured source intervals for platform and manual-agent evidence
- Depends on: corpus/manifest contracts and deterministic condition/scoring contracts

## Execution boundary

- Requires explicit operator opt-in for local real Chrome/Chromium and never runs a paid model or remote service.
- The current `cross_platform_smoke` and `capture_real` surfaces are prerequisites and references, not thesis evidence: their non-claims remain in force and their existing high-DPI absence is not converted into a pass.

## Simplification opportunity

- Extend existing real-browser locks, fixture servers, capture configuration snapshots, timing/gap contracts, `RecordingStore` retention/recovery tests, and production resource measurements. Do not add a product capture command, a second retention ledger, host-speed thresholds not in the evaluation contract, or a compatibility shim around unavailable Chrome behavior.

## Foundation references

- `docs/EVALUATION.md` — Capture-Fidelity Evaluation, Browser-Control Evaluation, Storage and Retention Evaluation, and Performance Evaluation
- `docs/SPEC.md` — Continuous Visual Capture, Browser-Control Surface, Disk Budget and Retention, and Degraded Operation
- `docs/ARCHITECTURE.md` — Frame Ingestion, Capture Tasks, Retention, Artifact Generation, and Observability
- `docs/VISUAL-EVIDENCE.md` — Capture Gaps and Determinism
- `docs/evidence/cross-platform-smoke/v1/README.md` — existing opt-in lane and honest non-claims

<!-- Feature design will define implementation units, interfaces, and focused verification. -->
