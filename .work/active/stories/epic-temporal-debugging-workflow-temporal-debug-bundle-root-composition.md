---
id: epic-temporal-debugging-workflow-temporal-debug-bundle-root-composition
kind: story
stage: done
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

## Implementation notes

- Execution capability: highest-capability cohesive inline ownership, continuing the feature's one-owner baseline. Direct reads covered the existing `build_runtime` composition and the `StorageDependencies` projection pattern.
- Review weight: standard from the caller; not applicable at this checkpoint because it is a child story and advances directly to done after verification.
- Files changed:
  - `src/app.rs` — imported `TemporalDebugBundles`/`TemporalDebugEvidenceStore`/`BundleWorkLimits`/`TemporalDebugBundleService`; added `temporal_debug_bundles: Arc<dyn TemporalDebugBundles>` to `RuntimeDependencies`; constructed one `TemporalDebugBundleService` in `build_runtime` projecting the same concrete `Arc<RecordingStore>` for temporal query, timeline/interaction evidence, and temporal context, and cloning the shared `Arc<dyn ArtifactGeneration>`; touched the new dependency in the Doctor command; updated the `doctor_is_discovery_only` test fixture; added two new composition tests.
  - `src/debug_bundle/mod.rs` — consolidated the `pub(crate) use` re-exports under `#![allow(dead_code, unused_imports)]` so `app.rs` can access `BundleWorkLimits` and `TemporalDebugBundleService`; moved the inline policy/trait-alias tests into `tests.rs` (no code logic changed).
- Tests added (2 new):
  - `bundle_composition_shares_one_store_and_one_artifact_service` — pointer-identity proof that `storage.temporal_queries`, `storage.temporal_context`, and the `TemporalDebugEvidenceStore` projection all point at the same concrete `RecordingStore`; and that the `Arc<dyn ArtifactGeneration>` cloned to both progressive and bundle paths is the same pointer.
  - `blocked_artifact_generation_permits_frame_persistence` — controlled barrier test using a real `RecordingStore` with one appended frame, a blocking spy `ArtifactGeneration`, and a spy `TemporalContextQuery`; proves that range resolution completes and releases the mutation gate before artifact work begins, allowing a second frame append to acquire the gate while generation is blocked.
- Simplification: the bundle service is constructed with `Arc::clone` of existing dependencies — no new store, artifact service, decoder, cache, or scheduler is created. The `TemporalDebugEvidenceStore` trait alias projects the one concrete store without a facade. MCP `build_service`, tool/resource/schema registries, and stdio handling have no diff.
- Discrepancies from design: none.
- Adjacent issues parked: none.

## Verification

- Rust 1.85: `cargo fmt --all -- --check` passed.
- Rust 1.85 workspace: `cargo clippy --workspace --all-targets --locked -- -D warnings` passed.
- Rust 1.85 workspace: `cargo test --workspace --all-targets --locked` passed (72 root, 101 core, 34 store, plus all other crate tests).
- Rust 1.85 workspace: `cargo check --workspace --all-targets --locked` passed.
- MCP diff: `git diff --name-only HEAD -- crates/krometrail-mcp/` is empty; `build_service`, tools/resources/schemas/URIs unchanged.
