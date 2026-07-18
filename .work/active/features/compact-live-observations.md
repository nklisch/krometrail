---
id: compact-live-observations
kind: feature
stage: drafting
tags: [agent-ux, browser]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Compact live observations

## Brief

Keep automatic post-action observations useful without saturating an agent's context or repeating indistinguishable degradation. Creating a Wikipedia page with v1.0.3 returned a 403-node accessibility snapshot and roughly 48,000 tokens when the caller primarily needed the new target identity and current page state. Clicking a JavaScript-alert fixture correctly degraded observation while the dialog was open, but repeated the exact same `page_observation_failed` warning three times at both the response and diagnostic-log boundaries.

Bound automatic observation snapshots while preserving explicit drill-down through the snapshot tool and accurate omission accounting. Coalesce equivalent top-level observation warnings, or retain component identity when warnings are meaningfully distinct, so repetition always carries information.

## Simplification opportunity

Centralize automatic-observation response policy instead of letting each action or observation component independently expand snapshots and append equivalent warnings. Reuse the snapshot model's existing `omitted_node_count` and the shared response composer.
