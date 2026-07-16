---
id: epic-durable-browser-memory-retention-removal-engine
kind: story
stage: done
tags: [storage, browser]
parent: epic-durable-browser-memory-retention
depends_on: [epic-durable-browser-memory-retention-index-contracts]
release_binding: 1.0.0
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Crash-Safe Removal Engine and Retention Policy

## Checkpoint

Evolve `IndexedRecordingSink` into one `RecordingStore` coordinator over the existing writer/index. Add the bounded blocking removal worker, shared mutation gate, open-segment sealing command, range pin/unpin, deterministic artifact/segment eviction, paused-budget state/notification, startup journal replay, and complete session deletion exactly as designed in the parent feature.

## Ordering

Depends on the SQLite retention contracts; filesystem staging cannot be implemented safely before the journal and candidate transactions exist.

## Acceptance evidence

- One coordinator serializes append/index/pin/delete/evict; no compatibility wrapper or second store authority remains.
- Pinning seals and protects every intersecting segment; ordinary eviction never removes protected data.
- Cleanup prunes regenerable artifacts safely, then evicts oldest unpinned sealed segments, and reports/enforces the one-open-segment tolerance.
- Every prepare/stage/metadata/unlink failure remains accounted and replayable; reopen converges without dangling metadata.
- Session deletion removes all source/event/artifact/index/pin/usage data and prevents later writes from resurrecting the id.
- Filesystem work is bounded and off the async executor; cancellation semantics match accepted-vs-unpolled mutations.

## Implementation notes

- Replaced `IndexedRecordingSink` with one `RecordingStore` coordinator and a single async mutation gate; append/index, pinning, eviction, explicit deletion, and flush now share one ordering authority.
- Added a bounded dedicated removal worker with durable `.trash/<batch>` staging, directory sync, idempotent unlink/finalize, and constructor-time forward replay for both prepared and metadata-removed journal phases.
- Added deterministic artifact-first/oldest-unpinned cleanup, mixed-source provenance invalidation, exact range pin/unpin with segment sealing, all-pinned paused state plus generation wakeup, one-open-segment status tolerance, and destructive session deletion with resurrection prevention.
- SQLite usage refresh checkpoints prior WAL growth and reports a bounded fresh-frame slack, avoiding self-referential accounting growth while retaining physical index accounting.
- Verification includes mixed-source artifact invalidation, replay from both journal boundaries, tiny-budget pause/unpin/evict/resume, scoped session deletion, package tests (55), and warning-free package Clippy.
