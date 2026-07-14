---
id: epic-temporal-debugging-workflow-temporal-debug-bundle-root-composition
kind: story
stage: implementing
tags: [visual, browser, storage, agent-ux]
parent: epic-temporal-debugging-workflow-temporal-debug-bundle
depends_on:
  - epic-temporal-debugging-workflow-temporal-debug-bundle-bounded-composition-and-degradation
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Wire One Bundle Runtime Authority

## Checkpoint

Compose the completed debug-bundle service at the root over the same concrete recording store, temporal query/context services, and artifact service already used by the runtime. Retain one bundle dependency for future MCP consumption without adding any MCP tool, resource, schema, response, or URI.

## Files

- `src/app.rs`
- `src/debug_bundle/mod.rs`
- existing root composition tests in `src/app.rs`

## Composition

- Project one concrete `Arc<RecordingStore>` as temporal query, generic timeline/interaction evidence, and temporal context; use the existing shared `Arc<dyn ArtifactGeneration>` also consumed by progressive evidence.
- Construct one `TemporalDebugBundleService` with default two-request/20-second limits and retain `Arc<dyn TemporalDebugBundles>` in `RuntimeDependencies`.
- Keep `build_service`, MCP capability/tool/resource registries, generated schemas, stdio handling, progressive operations, and browser connector unchanged.
- Ensure marker/range/context store work completes before visual work; artifact generation retains its existing independent scheduler, cache, single flight, publication, and deletion fences.

## Acceptance evidence

- Pointer identity proves one concrete store behind range, timeline/interaction, context, frame, artifact, retention, and progressive projections.
- Bundle and progressive paths share one exact artifact service/cache/scheduler; no second decoder, worker pool, cache, or artifact store is constructed.
- A controlled blocked artifact generation permits frame and browser-event persistence to acquire the recording mutation gate.
- Runtime owns one future-MCP bundle dependency while `krometrail-mcp`, tools/resources/schemas/URIs, and `build_service` have no diff.

## Ordering

Depends on the complete bounded bundle service. It unblocks integrated qualification. On green verification this child advances directly to `done`.
