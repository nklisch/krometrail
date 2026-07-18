---
id: resilient-compact-temporal-bundles-project-manifests
kind: story
stage: implementing
tags: [agent-ux, visual]
parent: resilient-compact-temporal-bundles
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Project compact manifests

## Checkpoint

Default bundle responses carry compact artifact handles and canonical manifest resource links; generic artifact results and complete retained provenance remain intact.

## Acceptance evidence

- Structured bundle size is bounded without repeated frame-ID arrays.
- Each manifest resource returns exact full provenance under validated evidence scope.
