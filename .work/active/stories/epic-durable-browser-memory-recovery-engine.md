---
id: epic-durable-browser-memory-recovery-engine
kind: story
stage: drafting
tags: [storage, browser]
parent: epic-durable-browser-memory-recovery
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Recovery Engine, Reconcile Helpers, and Composition Wiring

## Checkpoint

Land the startup consistency pass described in the parent feature: a top-level `krometrail-store::recovery` module that scans the segments directory, seals open segments, repairs damaged sealed segments, reconciles the SQLite frame index in both directions (insert orphan-payload rows, remove dangling rows), quarantines fatal corruption, reconciles segment-class usage, and returns a `RecoveryReport`. Add the recovery-specific SQL helpers in a new `index::reconcile` sibling module, wire one call into the composition root after both stores open and before capture, and cover the pure seal/reconcile decisions with deterministic in-module tests.

## Ordering

First checkpoint. It owns the engine and the composition hook; the cross-layer fault-injection suite depends on it.

## Files

- `crates/krometrail-store/src/recovery.rs` (new) — `RecoveryReport`, `recover(&SqliteIndex) -> Result<RecoveryReport>`, the four-phase orchestrator (discover / seal open / reconcile / usage), private `classify_tail` / footer-input / seal-or-repair / quarantine helpers, the `QUARANTINED_SEGMENT_EXTENSION = "corrupt"` constant, and in-module unit tests for the pure classify/seal-decision/footer-input logic.
- `crates/krometrail-store/src/index/reconcile.rs` (new) — `StoredSegment`, `IndexedFrame`, `list_segments_tx`, `indexed_offsets_tx`, `upsert_recovered_frame_tx` (SELECT guard over the existing `index_frame_tx`), `list_segment_usage_keys_tx`.
- `crates/krometrail-store/src/index/mod.rs` (one additive line) — `pub(crate) mod reconcile;`.
- `crates/krometrail-store/src/lib.rs` (extend) — `pub mod recovery;` and `pub use recovery::{RecoveryReport, recover};`.
- `src/app.rs` (extend) — call `krometrail_store::recovery::recover(index.as_ref())?` inside `open_storage` after `SqliteIndex::open` and `SegmentWriter::open` and before `IndexedRecordingSink::new`; log the report with `tracing::info!` carrying `open_segments_sealed`, `segments_repaired`, `segments_quarantined`, `frames_recovered`, `frames_removed`.

## Acceptance evidence

- `recover(&SqliteIndex)` runs discover → seal → reconcile → usage and returns a `RecoveryReport`; calling it again on the already-recovered store returns an all-zero report and mutates nothing (idempotence proof).
- An open segment whose last record is torn (`Trailing::Incomplete { at }`) or corrupt (`Trailing::Corrupt { at }`) is truncated at `at`, sealed with a fresh `SealedFooter` whose `record_count`, `total_payload`, `first_session_time`, `last_session_time`, and `sealed_observed` match the surviving records (or the header defaults for a 0-record segment), renamed `.open` → `.kts`, file `sync_data`'d, and the directory synced.
- An orphan payload (complete segment record with no index row) is recovered by inserting the missing frame row plus its `ObservationKind::Frame` timeline observation through `upsert_recovered_frame_tx`; a dangling index row (record absent or corrupt) is removed. Both directions are exercised in-module.
- A header-corrupt segment (`scan_complete_records` returns `Err`, or `SegmentHeader::decode` fails) is renamed to `<id>.corrupt`, the directory is synced, and its frame rows + segment registration + segment usage row are removed; `recover` still returns `Ok` with `segments_quarantined == 1`.
- An index-referenced segment whose `.kts` file is absent is removed via `remove_frame_rows(segment_id, None)` then `remove_segment(segment_id)`; other segments are untouched.
- Segment-class `usage` rows match the reconciled `segments` table after recovery; stale segment usage keys (segment id no longer present) are removed.
- `pins` and `pin_segments` rows for surviving segments are unchanged across recovery; a removed segment's `pin_segments` rows cascade-clear via `ON DELETE CASCADE` while its `pins` row survives.
- Filesystem mutations (truncate, append footer, `sync_data`, rename, sync directory) happen outside any SQLite transaction. Per reconciled segment, missing-record decode happens before the per-segment insertion transaction opens; the insertion transaction upserts the sealed registration before any frame row (FK target).
- Operational failures map to `ErrorCode::PersistenceFailed`. The segments-directory-unreadable path maps to `ErrorCode::ShutdownIncomplete`. No new `ErrorCode` variant is introduced; no source error, path, SQL text, or driver detail reaches the public error.
- Locked workspace `cargo fmt --all --check`, `cargo check --workspace --all-targets --locked`, `cargo test --workspace --all-targets --locked`, and `cargo clippy --workspace --all-targets --locked -- -D warnings` pass. Isolated `doctor` reports a browser installation and creates `index.sqlite3` plus `segments/` with recovery running at startup.

## Implementation notes

- Reuse the segment-format scanner (`scan_complete_records`, `read_frame_at`), footer/header codecs, and path helpers (`sealed_segment_path`, `open_segment_path`). Do not introduce a second scanner.
- Reuse the sqlite-index maintenance primitives (`remove_frame_rows`, `remove_segment`, `update_usage`, `remove_usage`) and `register_segment_tx` / `index_frame_tx`. Do not widen them.
- Keep recovery-specific SQL inside `index::reconcile`. The codec stays private to `index`; `reconcile` is inside the module and reaches it the same way `frames`/`segments`/`timeline` do.
- Dangling-tail removal is one `remove_frame_rows(segment_id, Some(truncate_point))` call (contiguous tail). The anomalous clean-segment-with-stray-dangling case rebuilds via `remove_frame_rows(segment_id, None)` plus re-insert. Whole-segment removal composes `remove_frame_rows(segment_id, None)` with `remove_segment`.
- Build the reconciled `SegmentRegistration` with `state: Sealed`, `relative_path: <id>.kts`, `file_bytes` = post-seal file length, `payload_bytes` = sum of surviving `record.payload_len`, `record_count` = surviving records, `end_time: Some(last_session_time)`. Upsert it before inserting frame rows so the FK is satisfied.
- `recover` reads the segments directory from `index.segments_directory()`. Discovery classifies by extension and validates the filename stem parses as the segment UUID; non-conforming files (residual write probes, WAL/SHM in the parent dir) are skipped.
