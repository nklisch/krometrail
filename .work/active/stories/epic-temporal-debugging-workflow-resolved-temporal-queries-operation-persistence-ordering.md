---
id: epic-temporal-debugging-workflow-resolved-temporal-queries-operation-persistence-ordering
kind: story
stage: implementing
tags: [storage, browser, agent-ux]
parent: epic-temporal-debugging-workflow-resolved-temporal-queries
depends_on: [epic-temporal-debugging-workflow-resolved-temporal-queries-durable-anchor-index]
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Fence browser success on durable evidence

## Checkpoint

Wire one `InteractionEvidenceSink` into the production CDP session and centralize evidence projection in the shared non-batch operation path. Persist every returned state-changing anchor before standalone publication or the next batch step, without moving persistence into MCP or action-family handlers.

## Required contract

- `ProductionBrowserConnector::with_interaction_evidence` injects the sink; state-changing work without one fails before dispatch while read-only work remains available.
- Page-operation results persist their existing anchor and no fabricated record; browser-action results persist `InteractionResult::anchor()` plus the exact existing record.
- Only successful explicit navigate/reload/back/forward results mint a `NavigationId` and project a completion-time navigation point.
- Batch children recurse through the same persistence fence; the outer batch is not projected again, and existing parent-batch IDs remain record references rather than new interaction rows.
- Persistence failure after a browser effect returns `PersistenceFailed` with interaction context, retry `Never`, and inspect-before-repeat recovery. It never becomes a successful degraded result or an automatic action retry.

## Acceptance evidence

- [ ] A delayed sink proves standalone result publication waits for commit; read-only requests never call it.
- [ ] A two-step batch proves step 2 cannot dispatch before step 1 evidence commits.
- [ ] A failed sink produces a failed step and default stop-on-failure behavior without duplicate rows or replayed input.
- [ ] Exhaustive result handling projects every page/action variant exactly once and navigation only for successful explicit navigation controls.

## Ordering

Depends on the durable store implementation so the integration is designed against the real transaction boundary, not a temporary production cache.
