---
id: epic-prove-temporal-advantage-agent-debugging-qualification
kind: feature
stage: drafting
tags: [testing, visual, browser, agent-ux]
parent: epic-prove-temporal-advantage
depends_on: [epic-prove-temporal-advantage-manual-multimodal-interpretation]
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-15
---

# Manual Agent Debugging Qualification

## Brief

Measure the end-to-end coding-agent workflow after the interpretation lane is complete: reproduce the fixture behavior, inspect current-state-only versus temporal evidence, identify the visible transient defect, locate the responsible application logic, apply a focused patch, and verify both the final state and temporal behavior without regressing stable controls. The comparison uses the same Linux reference-host source intervals, scenario set, prompt discipline, and evidence provenance so the result measures assistance from temporal evidence rather than fixture selection or prompt drift. MacOS platform completion is not part of this prerequisite.

The feature owns the reportable product-thesis assessment and its narrow claim language, not an automatic bug solver. Manual Codex runs are explicitly authorized and paid only by an operator; the feature does not run them in CI or ask an agent to spend money. It reports pass, fail, or inconclusive only when the required scenarios, conditions, source traceability, and model metadata are complete. A result for Codex does not establish that other model families receive the same benefit.

## Epic context

- Parent epic: `epic-prove-temporal-advantage`
- Position in epic: end-to-end qualification capability — combines the manual interpretation contract,
  declared Linux reference-host evidence, and patch/verification outcomes
- Depends on: manual multimodal interpretation, which carries the required Linux stable-Chrome
  reference-host evidence transitively

## Dependency correction

Agent debugging starts from the completed manual interpretation contract and its exact evidence
packages/source references. It does not depend on the platform feature or on macOS/default-DPI/
high-DPI completion. No direct Linux child edge is needed: the manual feature's exact dependency on
`epic-prove-temporal-advantage-platform-evidence-collection-linux-stable-chrome-reference-host-evidence`
is the semantic live-evidence prerequisite. A future implementation may add a direct Linux edge
only if debugging consumes a distinct Linux manifest not carried by the manual contract; this design
does not invent that redundant dependency.

## Execution boundary

- Manual, authorization-gated, and isolated from normal CI. An incomplete paid run set remains local evidence with an explicit incomplete/inconclusive status.
- The benchmark may establish that temporal evidence helped an evaluated agent on the named fixtures and configurations; it cannot claim automatic diagnosis, deterministic replay, all-browser support, all-model support, or causal attribution from correlated events.

## Simplification opportunity

- Reuse the existing standalone fixtures, browser-control operations, temporal bundle/progressive evidence surfaces, condition packager, and scoring contract. Do not add a second agent protocol, patch orchestration product feature, replay engine, or compatibility layer for hypothetical consumers.

## Foundation references

- `docs/VISION.md` — Success and Product Boundaries
- `docs/SPEC.md` — Current-State Observation, Action Timeline, Temporal Queries, and Exclusions
- `docs/EVALUATION.md` — Debugging Tasks, Agent Metrics, Product-Thesis Assessment, and Model Evaluation Discipline
- `docs/VISUAL-EVIDENCE.md` — Progressive Detail and Non-Diagnostic Posture

<!-- Feature design will define implementation units, interfaces, and focused verification. -->
