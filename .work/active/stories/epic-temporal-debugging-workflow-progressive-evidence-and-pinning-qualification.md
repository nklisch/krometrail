---
id: epic-temporal-debugging-workflow-progressive-evidence-and-pinning-qualification
kind: story
stage: implementing
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
