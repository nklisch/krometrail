---
id: epic-temporal-debugging-workflow-resolved-temporal-queries-query-service-composition
kind: story
stage: done
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

## Implementation notes

- Execution capability: highest; retained by caller because root construction and the store mutation order determine whether browser results are immediately queryable and source identities remain coherent.
- Review weight: standard, from the autopilot caller; child checkpoint review is not applicable.
- Files changed: `crates/krometrail-store/src/recording.rs` and `src/app.rs`.
- Tests added/updated: existing store and root composition suites exercise the changed construction; full operation-to-query qualification is carried by the dependent final checkpoint rather than duplicated as a wrapper test here.
- Semantics delivered: `RecordingStore` implements `TemporalQuery` by holding its mutation gate through the existing resolver; root retains one concrete `Arc<RecordingStore>` and projects it as recording, retention, timeline writer, interaction sink, and temporal query; focused catalog/gap/frame reads remain on the shared `SqliteIndex`; `RuntimeDependencies` exposes the query port without changing MCP construction.
- Simplification: root now wires generic timeline writes through the store instead of directly through SQLite, consolidating deletion/retention/evidence/query mutation authority under one gate.
- Discrepancies from design: none. Startup still opens, migrates, and recovers storage before constructing the browser connector or runtime dependencies.
- Adjacent issues parked: none.
- Verification: `cargo fmt --all -- --check`; `cargo check --workspace --all-targets --locked`; store tests (72 passed), root tests (10 passed), and Clippy `-D warnings` for both store and root packages.
