---
id: epic-temporal-debugging-workflow-progressive-evidence-and-pinning-qualification
kind: story
stage: done
tags: [visual, storage, agent-ux]
parent: epic-temporal-debugging-workflow-progressive-evidence-and-pinning
depends_on:
  - epic-temporal-debugging-workflow-progressive-evidence-and-pinning-progressive-service-and-composition
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Qualify Progressive Evidence Lifetime and Pinning

## Checkpoint

Qualify the complete source/artifact/region/pin path with a real schema-v5 store, exact artifact service, scripted current geometry, tiny budgets, open/sealed segments, overlapping pins, browser events, multiple visual epochs, and deterministic barriers around read/generation/eviction/deletion. This is the integrated acceptance checkpoint for one feature owner, not a separate implementation worker.

## Files

- `src/progressive/tests.rs` (new)
- `crates/krometrail-store/tests/progressive_evidence_store.rs`
- `crates/krometrail-cdp/tests/temporal_evidence.rs`
- `crates/temporal-vision/tests/filmstrip.rs`
- `src/app.rs` tests

## Acceptance evidence

- Frame list/fetch handles and payloads prove all metadata/hash/length/order contracts and exact count/per-item/total limit boundaries.
- Delayed source/artifact reads raced with eviction and session deletion return no stale/partial bytes; corrupt cache invalidates, corrupt source fails, and later weak-handle reads become explicit `NotFound`.
- Region coverage includes in/out-of-bounds source pixels, fractional/negative CSS outward rounding, declared frame rect, current reference, masks, locator/padding, wrong scope/stale generation, multi-epoch rejection, all-zero/tail/wrong-size/oversized masks, and manifest/cache identity.
- Equivalent region and generic requests reuse existing cache/single flight; cancellation/deadline suppresses late publication.
- Pin coverage includes open flush, actual segment overreach, overlap/idempotence/exact unpin, stale/evicted state, all-pinned pause, post-unpin recovery, and concurrent eviction/deletion.
- Pinned source survives while artifacts and v5 browser events remain independently evictable; session deletion removes source/artifact/event/pin state.
- Barrier tests prove no mutation gate spans file read/hash, browser geometry, decode/render, or generation and that frame/event persistence progresses between validation phases.
- Root integration proves one store/generator/progressive service and no MCP surface. Rust 1.85 locked format, workspace all-target check/test, and Clippy with warnings denied pass.

## Ordering

Depends on the composed progressive service. On green verification this child advances directly to `done`; only the parent feature receives the caller's standard independent review.

## Implementation notes

- Execution capability: direct inline feature ownership; the work was a cohesive qualification pass across already-owned progressive, store, artifact, CDP, temporal-vision, and root seams, so an implementation handoff would have duplicated context without creating independent write ownership.
- Review weight: standard from the caller; not applied to this child checkpoint, which advances directly to `done`. The parent is left at `review` for the separate feature review lane.
- Files changed: `src/progressive/{mod.rs,tests.rs}`, `src/artifacts/qualification_tests.rs`, and `crates/krometrail-store/tests/progressive_evidence_store.rs`.
- Tests added/extended: a real current-schema recording fixture using the production `RecordingStore`, artifact service, progressive service, JPEG/PNG source frames, tied times, two visual epochs, browser events, open-segment pins, scripted current geometry, corruption, cancellation, and deletion; deterministic browser-geometry and artifact-publication barriers proving frame/event persistence; and concurrent exact pin/session-deletion serialization without resurrection.
- Simplification: reused the production store, artifact cache/single-flight service, progressive registry/service, browser geometry port, and existing temporal-vision contracts. Added no qualification-only wrapper around storage, no second cache/ledger, no schema inspection, and no production getter.
- Discrepancies from design: none. Typed artifact retrieval invalidates and removes a corrupt cache row before regeneration, so the next generation is a clean `Generated`; the separate cache-lookup corruption test continues to prove `RegeneratedAfterInvalidation` for invalidation discovered during lookup.
- Adjacent issues parked: none.

## Qualification evidence

- Real progressive list/fetch qualification proves exact resolved/request ordering, JPEG/PNG MIME and format, image/viewport/scale, observed/session timing, ordinal, SHA-256, encoded length/payload, and exact count/per-item/total boundaries.
- Real region generation covers in/out-of-bounds source rectangles, fractional negative viewport CSS outward rounding, selected rectangles, masks, locator/padding provenance, one-sample scripted current references, wrong scope, stale geometry, incompatible epochs, invalid mask dimensions/content/size, and mask-sensitive cache identity.
- Equivalent generic and focused region requests share the production artifact cache. Repeated requests hit the same artifact identity; corruption is invalidated source-safely; cancellation/deadline returns before publication. Existing single-flight and late-cancellation tests remain green.
- Real service pinning flushes an open segment, reports physical overreach and global pinned usage, preserves overlapping protection through exact/idempotent unpin, and leaves no source/artifact/event/pin resurrection after session deletion. Tiny-budget, all-pinned pause/recovery, source-only pin scope, artifact eviction, event eviction/tombstones, and weak-handle eviction/corruption tests remain green in the store suite.
- Deterministic barriers prove current geometry and artifact generation/publication do not hold the recording mutation gate while frame and schema-v5 browser-event persistence proceeds. Existing source/artifact snapshot-read barriers prove file/hash work occurs outside the gate with exact final revalidation.
- Root pointer-identity coverage remains green for one concrete `RecordingStore` projected as frame/artifact/retention/progressive authority. `krometrail-mcp`, `build_service`, and MCP registration remain unchanged.
- Geometry qualification is scripted only. No live-Chrome result is claimed.

Rust 1.85 locked gates passed:

- `cargo fmt --all -- --check`
- `cargo check --workspace --all-targets --locked`
- `cargo test --workspace --all-targets --locked`
- `cargo clippy --workspace --all-targets --locked -- -D warnings`
