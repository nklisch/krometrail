---
id: epic-durable-browser-memory-recovery-fault-injection
kind: story
stage: done
tags: [storage, browser, testing]
parent: epic-durable-browser-memory-recovery
depends_on: [epic-durable-browser-memory-recovery-engine]
release_binding: 1.0.0
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Cross-Layer Fault-Injection Qualification

## Checkpoint

Prove the recovery engine against realistic crash aftermaths using the real `SegmentWriter`, real `SqliteIndex`, real `IndexedRecordingSink`, and a `tempfile::TempDir`. Tests simulate what a crash or power loss leaves behind — orphan payloads, truncated open tails, sealed-but-unreconciled segments, missing files, corrupt headers — by direct file and index manipulation, then assert `recover` restores a consistent store. This is the cross-layer crash-mid-write integration evidence the segment-format feature explicitly handed to recovery.

## Ordering

Second and final checkpoint. It depends on the recovery engine and exercises the seal-and-reconcile boundary across the segment-format and sqlite-index features.

## Files

- `crates/krometrail-store/tests/recovery.rs` (new) — integration tests. No power-loss simulation (impossible without root); tests honestly exercise the observable aftermaths of crashes by manipulating files and index rows directly.

## Acceptance evidence

Each test simulates one realistic crash aftermath and asserts recovery restores consistency:

- **Orphan payload (record-before-index crash):** append a frame's segment record via `SegmentWriter::append_indexable` (which returns a `FrameWriteCommit` but does not commit an index row), then `recover` → the missing frame row is inserted and the frame is queryable through `FrameSource::frames_by_id`. Proves the common process-crash direction (insert).
- **Dangling tail (truncated open segment):** index N frames through `IndexedRecordingSink` (open segment), then truncate the `.open` file mid-last-record to simulate a power-loss-dropped tail, then `recover` → the tail is removed, the segment is sealed, and the dangling index row for the truncated frame is removed; earlier frames stay queryable. Proves the opposite direction (remove).
- **Idempotence:** after any recovery above, `recover` again returns an all-zero `RecoveryReport` and the index is unchanged (re-querying the same frames yields identical results).
- **Crash-during-recovery (sealed-but-unreconciled):** create an orphan payload via `append_indexable`, then manually seal the `.open` file (rename to `.kts` and append a valid `SealedFooter`) to simulate a previous recovery that sealed the file but crashed before inserting its rows, then `recover` → the sealed segment is reconciled and the missing row is inserted. Proves recovery processes sealed segments too, not only open ones.
- **Fatal corruption quarantine:** flush a session, corrupt a sealed segment's header bytes (flip a byte inside the first `SEGMENT_HEADER_LEN`), then `recover` → the file is renamed `<id>.corrupt`, its frame rows are gone from the index, and `recover` returns `Ok` with `segments_quarantined == 1`. A second `recover` skips the `.corrupt` file.
- **Missing file (dangling segment):** flush a session, delete the `.kts` file, then `recover` → that segment's frame rows and registration are removed while other sessions are untouched.
- **Pins trusted across recovery:** insert a `pins` row and a `pin_segments` row for a surviving segment (direct SQL), then `recover` → both rows survive unchanged.
- **Usage reconciliation:** flush a session, delete its segment-class `usage` row (direct SQL), then `recover` → the usage row is restored and matches the segment's file size.
- **Empty open segment:** write a `.open` file containing only a valid `SegmentHeader` (no records, no footer), then `recover` → it is sealed as `record_count = 0`, registered as sealed, and indexed with zero frames; the report records `open_segments_sealed == 1`.
- **End-to-end reopen:** index frames through `IndexedRecordingSink`, drop the sink without flushing, re-open the `SqliteIndex`, `recover`, and assert every frame is queryable through `FrameSource`; a second `recover` is a no-op.
- **Open-segment report:** write to two distinct targets without flushing (two open segments), then `recover` → `report.open_segments_sealed == 2`.
- **Asymmetric invariant in one pass:** a fixture that produces an orphan payload on one segment and a dangling row on another in the same store, then a single `recover` → the orphan is inserted and the dangling row removed; both segments are queryable for their surviving frames.

Locked workspace `cargo fmt --all --check`, `cargo check --workspace --all-targets --locked`, `cargo test --workspace --all-targets --locked`, and `cargo clippy --workspace --all-targets --locked -- -D warnings` pass.

## Implementation notes

- Tests construct a `SqliteIndex` and `SegmentWriter` against a `tempfile::TempDir` exactly as the composition root does, then drive `recover` directly. The composition-root wiring is covered by the engine checkpoint; this suite targets the engine through its public `recover` entry point.
- To simulate a crash aftermath, manipulate files and rows directly: `std::fs::read`/`write`/`rename`/`remove_file` for segment file states, and direct `rusqlite` statements (via `index.connection()` in test-only helpers or a small test-side connection) for index row setup. Do not add production API surface to set up fault fixtures.
- Reuse the `IndexedRecordingSink::append_frame` / `SegmentWriter::append_indexable` / `SegmentWriter::flush_indexable` seams already covered by tests in `recording.rs` and `maintenance.rs` to produce realistic on-disk states before perturbing them.
- The orphan-payload and dangling-row tests together are the cross-layer evidence the segment-format handoff named; keep them paired so the asymmetric invariant is visible in one read of the suite.

## Implementation notes

- Execution capability: highest (autopilot caller), selected because this suite is the release evidence for the record-before-index asymmetry and recovery idempotence.
- Review weight: standard (caller/project default); child checkpoints do not self-review, and the parent feature will stop at `review` for independent review.
- Files changed: `crates/krometrail-store/tests/recovery.rs`.
- Tests added: 12 real-filesystem qualification cases covering orphan insertion, torn-tail removal, sealed-but-unreconciled resume, fatal quarantine, missing-file cleanup and pin-link cascade, surviving-pin trust, usage restoration and stale-row removal, empty segments, unflushed multi-target reopen/open count, both asymmetric directions in one pass, damaged sealed-footer repair, operational error mapping, and unreadable-root shutdown mapping.
- Simplification: fixtures use the real writer/index/facade and direct aftermath manipulation; no production fault hook, recovery journal, power-loss simulation, or widened test-only API was added.
- Discrepancies from design: the idempotence test snapshots logical index rows rather than raw SQLite/WAL bytes, because SQLite may change physical page/WAL representation without a logical mutation. The stronger contract asserted is identical segment/frame/timeline/usage metadata plus an all-zero second report.
- Verification: focused store suite passed (49 tests across 7 suites); store Clippy passed with warnings denied; locked workspace gates and isolated startup are the parent feature's integrated gate.
- Adjacent issues parked: none.
