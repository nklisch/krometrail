---
id: epic-agent-surface-simplification-bounded-temporal-bundles
kind: feature
stage: drafting
tags: [agent-ux, visual]
parent: epic-agent-surface-simplification
depends_on: [epic-agent-surface-simplification-response-detail]
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Bounded temporal bundle defaults

## Brief

Default `temporal_debug_bundle` generation to the one visual epoch containing the effective artifact anchor, with deterministic nearest/earlier selection when the anchor lies between spans. Preserve the full resolved range, gaps, and epoch provenance while generating at most the meaningful outputs for that selected epoch. Let agents explicitly request `all` epochs when investigating geometry transitions. Generic explicit artifact generation retains its all-epoch behavior.

Integrate with concise/expanded/full responses: concise publishes the primary handle/resource and exact outcome/epoch omission counts, expanded publishes every generated compact handle/resource, and full exposes complete structures. Do not read inline artifact bytes unless images were requested.

## Epic context

- Parent epic: `epic-agent-surface-simplification`
- Position in epic: temporal acquisition and presentation consumer of response detail

## Simplification opportunity

Delete frozen v1 bundle-policy version fields/tests, default epoch/output Cartesian work, singleton low-information generation, and default artifact read-then-discard I/O while retaining canonical retained-resource authority.

## Foundation references

- `docs/VISUAL-EVIDENCE.md` — Input Sequence and artifact provenance
- `docs/SPEC.md` — Temporal Query and Artifact Operations
- `docs/EVALUATION.md` — Condition D temporal bundle
