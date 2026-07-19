---
id: feature-response-evidence-economy-dedupe-projection
kind: story
stage: done
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
- [x] Two consecutive actions on an unchanged document project the full index once, then the
  marker; navigation re-emits the full index.
- [x] `snapshot_page` output never deduped.
- [x] Concise `states` carry no `focusable`; expanded unchanged.

## Completion Note

Implemented and verified Unit 1. Session-owned projection memory keys observations by target and attachment generation; automatic post-action projections dedupe unchanged generations, explicit inspection remains novel, and concise state output prunes only `focusable`.
