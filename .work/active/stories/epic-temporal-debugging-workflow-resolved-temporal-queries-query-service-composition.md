---
id: epic-temporal-debugging-workflow-resolved-temporal-queries-query-service-composition
kind: story
stage: implementing
tags: [storage, browser, agent-ux]
parent: epic-temporal-debugging-workflow-resolved-temporal-queries
depends_on: [epic-temporal-debugging-workflow-resolved-temporal-queries-operation-persistence-ordering]
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Compose the coherent temporal query service

## Checkpoint

Implement `TemporalQuery` on `RecordingStore` by running the core `TemporalQueryService`/existing resolver under the store mutation gate, then root-wire the same concrete store as recording, retention, timeline writer, interaction sink, and temporal query authority. Do not add MCP routes or persistence behavior.

## Required contract

- The guarded query composes one `SqliteIndex` through the existing catalog/frame/gap/timeline/interaction read ports and returns the existing `ResolvedRange`.
- Frame append, marker append, interaction projection, retention eviction, session deletion, and range resolution share one mutation order; a returned range cannot already name identities removed mid-resolution.
- Root retains `Arc<RecordingStore>` until all trait-object projections are wired and injects its interaction sink into `ProductionBrowserConnector`.
- `RuntimeDependencies` exposes `Arc<dyn TemporalQuery>` for later application consumers; current MCP construction and router remain unchanged.
- Storage migration/recovery still completes before browser operations or query service availability.

## Acceptance evidence

- [ ] Root composition proves no no-op/memory anchor source is production wired and all services point to the same store/index authority.
- [ ] A held query blocks concurrent eviction/session deletion until the exact range has resolved.
- [ ] Migration/open failure prevents browser dispatch and temporal query construction.
- [ ] MCP source contains no sink call, SQL, temporal route, resource, or copied range contract.

## Ordering

Depends on the CDP persistence fence and store implementation. It completes the production path consumed by qualification and later temporal features.
