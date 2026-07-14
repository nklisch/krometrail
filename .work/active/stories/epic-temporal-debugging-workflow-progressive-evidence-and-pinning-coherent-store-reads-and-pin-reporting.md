---
id: epic-temporal-debugging-workflow-progressive-evidence-and-pinning-coherent-store-reads-and-pin-reporting
kind: story
stage: implementing
tags: [visual, storage, agent-ux]
parent: epic-temporal-debugging-workflow-progressive-evidence-and-pinning
depends_on:
  - epic-temporal-debugging-workflow-progressive-evidence-and-pinning-contracts-and-region-semantics
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Make Evidence Reads and Pins Coherent

## Checkpoint

Make `RecordingStore` the production `FrameSource` as well as artifact/retention authority. Implement source and artifact reads with metadata snapshot, out-of-gate bounded file I/O/hash validation, and final in-gate revalidation so eviction or session deletion during a read cannot return stale or partial bytes. Preserve authoritative cache-hit validation and distinguish missing from invalidated derived artifacts.

Upgrade exact pin/unpin/query reporting over the existing pins, segment links, segment metadata, and usage rows. Pin flushes open session segments and atomically proves the expected ordered `ResolvedRange` frames before linking segments; unpin is exact/idempotent and reports post-budget-enforcement overlap/availability truth. No schema, reader, cache, or pin ledger is added.

## Files

- `crates/krometrail-store/src/recording.rs`
- `crates/krometrail-store/src/index/{frames.rs,artifacts.rs,retention.rs}`
- `crates/krometrail-store/src/artifacts/mod.rs`
- `crates/krometrail-store/tests/{artifact_store.rs,retention_small_budget.rs}`
- `crates/krometrail-store/tests/progressive_evidence_store.rs` (new)

## Acceptance evidence

- Encoded frame/artifact reads return exact scoped rows/links/content or explicit `NotFound`/`EvidenceInvalidated`; source corruption remains `PersistenceFailed`.
- Controlled eviction/deletion races prove no already-invalidated payload or partial list escapes and no mutation gate spans file reads or hashing.
- Frame order, scope, metadata, hashes, lengths, and source links are revalidated; list hashes reuse the bounded encoded read path without a persisted frame-hash column.
- Pin flush/revalidation prevents empty or partly stale pins, while returned segment bounds expose segment-granular overreach.
- Overlap, repeated pin/unpin, exact unpin, coalescing, concurrent eviction/deletion, paused-budget recovery, and final availability/status are deterministic.
- Pin tables protect source segments only. Existing artifact deletion and v5 browser-event eviction remain independent and session deletion removes every authority.

## Ordering

Depends on the public contracts checkpoint. Current reference geometry can proceed afterward, but progressive service composition waits for both store and browser seams.
