---
id: epic-durable-browser-memory-sqlite-index-maintenance-qualification
kind: story
stage: done
tags: [storage, browser]
parent: epic-durable-browser-memory-sqlite-index
depends_on: [epic-durable-browser-memory-sqlite-index-indexed-recording]
release_binding: 1.0.0
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Maintenance Primitives, Composition, and Qualification

## Checkpoint

Add store-local retention/recovery primitives in `index/maintenance.rs`: `remove_frame_rows(segment_id, from_offset)`, `remove_segment`, `remove_artifact`, `update_usage`, and `remove_usage`, with the exact types and semantics in the parent feature. Frame removal deletes matching generic frame observations in the same transaction; segment removal refuses while frame rows remain.

Replace root `UnavailableTimelineStore` with one shared `SqliteIndex` at `<data_dir>/index.sqlite3`; share it as timeline/catalog/gap/frame ports and compose it with `SegmentWriter` as `IndexedRecordingSink` for CDP capture. Runtime construction opens/migrates the index before capture can start. Finish deterministic file-backed qualification covering persistence, concurrency, cancellation, failures, and future consumer seams.

## Ordering

Final checkpoint. It depends on indexed recording so maintenance and composition qualify the production shape, not a test-only adapter.

## Acceptance evidence

- Partial-tail/all-frame removal returns exact frame ids and removes paired timeline rows transactionally; segment deletion refuses live references and becomes idempotent after cleanup.
- Artifact row removal and usage upsert/removal preserve foreign keys and full `u64` byte counts without SQLite signed overflow.
- Root shares one migrated index across focused ports, injects only `IndexedRecordingSink` into capture, and deletes `UnavailableTimelineStore`.
- Open/migration/WAL failure prevents runtime construction; flush/index failure cannot be reported as success.
- Contention stops at configured busy timeout; a future dropped before polling performs no mutation, and a polled transaction completes or rolls back without an await point.
- File-backed qualification covers reopen persistence, equal-time ordering, open→sealed reads, missing/corrupt payload failures, and redaction-by-omission of raw browser-event content.
- `cargo fmt --all -- --check`, locked workspace check/test, and locked clippy with warnings denied pass.

## Implementation notes

- Added store-local frame, segment, artifact, and usage maintenance primitives. Frame-tail removal deletes paired generic frame observations in the same immediate transaction; segment removal refuses live frame references and all deletion primitives are safely repeatable after success.
- Usage accounting preserves full-domain `u64` byte counts as fixed-width blobs for all declared usage classes. Artifact deletion and segment/frame maintenance retain foreign-key enforcement.
- Root composition now opens and migrates `<data_dir>/index.sqlite3` before constructing capture, shares one `SqliteIndex` behind timeline/catalog/gap/frame ports, and injects only `IndexedRecordingSink` into CDP capture. `UnavailableTimelineStore` and the segment writer's unsupported gap path are absent.
- Qualification proves lossless gap routing through production composition, orphan-safe post-append index failure, explicit post-flush index failure, bounded open/sealed address reads, missing/corrupt payload failures, external-writer busy timeout, cancellation-before-poll, transaction completion/rollback after polling, migration refusal, catalog/timeline ordering, and redaction by omission.
- Because unrelated browser-control and `work-view` edits were active in the primary tree, final gates ran in a clean detached worktree containing exactly this checkpoint plus the four committed SQLite checkpoints. Locked workspace format/check, 311 tests, and Clippy with warnings denied passed. Isolated `doctor` found one browser installation and created both `index.sqlite3` and `segments/` under a temporary data directory.
- Deviations: none. No sibling-owned payload types or raw browser-event storage were introduced.
