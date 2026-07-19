---
id: runtime-observation-hardening
kind: feature
stage: drafting
tags: [browser, agent-ux]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Harden live browser observation under real public-site load

## Brief

Resolve the manual cross-surface findings from Krometrail 1.1.1 as one cohesive patch: restore reliable responsive viewport acknowledgement, keep screencast acknowledgement healthy during frame-heavy navigation, expose privacy-bounded viewport and CDP facts in failure diagnostics, rank compact snapshots around actionable content rather than raw preorder, and add an explicit interaction-only snapshot projection for callers that need the smallest targeting surface.

The work preserves the stable canonical snapshot, target-scoped viewport lifecycle, immediate-ack/bounded-handoff capture contract, and existing compact/full/omit meanings. New presentation behavior must derive from canonical acquisition and must not weaken reference authority, loss accounting, or diagnostic privacy.

## Simplification opportunity

Consolidate viewport acknowledgement facts in the existing observation result instead of reconstructing them at error/log boundaries, and centralize snapshot node selection so compact and interaction-only projections share one deterministic ranking implementation rather than separate traversal paths.
