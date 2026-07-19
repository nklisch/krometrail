---
id: feature-response-evidence-economy-dedupe-projection
kind: story
stage: implementing
tags: [agent-ux]
parent: feature-response-evidence-economy
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-19
updated: 2026-07-19
---

# Dedupe unchanged snapshot projections and prune noise states

Checkpoint for Unit 1 of the parent design: session-owned
`ProjectedSnapshotMemory` keyed by `(target_id, attachment_generation)`, unchanged-
generation marker `{generation, unchanged: true, target_count, omissions}` for automatic
post-action concise/expanded projections only, and `focusable` filtered from concise
`states`. Explicit inspection routes (`snapshot_page`, `observe_live`, `query_page`) are
never deduped.

## Acceptance
- Two consecutive actions on an unchanged document project the full index once, then the
  marker; navigation re-emits the full index.
- `snapshot_page` output never deduped.
- Concise `states` carry no `focusable`; expanded unchanged.
