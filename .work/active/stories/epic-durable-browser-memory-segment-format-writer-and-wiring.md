---
id: epic-durable-browser-memory-segment-format-writer-and-wiring
kind: story
stage: done
tags: [storage, browser]
parent: epic-durable-browser-memory-segment-format
depends_on:
  - epic-durable-browser-memory-segment-format-core-address-contract
  - epic-durable-browser-memory-segment-format-binary-codec
release_binding: 1.0.0
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# SegmentWriter Adapter, Rotation, Flush, and Composition Wiring

## Brief

Deliver the live frame-write path: a `SegmentWriter` adapter that implements `RecordingSink` (frame-only writes), drives bounded rotation by duration and size, seals segments on flush, and is wired into the composition root so the CDP capture stream's frames land durably. Includes a real-write smoke test against a temp directory that round-trips frames back through the codec using only the returned `FrameAddress`es.

## Parent context

- Parent feature: `epic-durable-browser-memory-segment-format`
- This is Unit 3 of three. Depends on Unit 1 (address contract) and Unit 2 (binary codec). It is the story that makes the live capture stream durable.

## Scope

**In scope:**
- `SegmentWriter` implementing `RecordingSink`:
  - `append_frame(&self, frame: EncodedFrame) -> Result<FrameAddress>`: lazily open a segment for `(frame.session_id(), frame.target_id())` on first append; check rotation bounds (age = `frame.session_time() - header.start_session_time`; size = current file length); if either bound is crossed, seal+fsync the current segment and open a new one with a fresh `SegmentId`; encode the `FrameRecord` via the Unit 2 codec; append to a `BufWriter`; flush the buffer; return `FrameAddress { segment_id, byte_offset: <offset of record_kind byte before this append> }`.
  - `append_gap(&self, _gap: CaptureGap) -> Result<()>`: return `Unsupported` with message `"capture-gap persistence is owned by the SQLite metadata feature and is not yet wired"`. Write zero bytes.
  - `flush(&self, session_id: SessionId) -> Result<()>`: for each open segment of `session_id`, write `SealedFooter` (`record_count`, `total_payload`, first/last frame session time, `sealed_observed`), flush, `fdatasync`, close. Segment is immutable after flush.
- `SegmentStoreConfig { directory: PathBuf, rotation: RotationConfig }` and `RotationConfig { max_duration: Duration, max_size: u64 }` with `RotationConfig::suggested()` (120 s, 128 MiB).
- Concurrency: `Mutex<HashMap<(SessionId, TargetId), OpenSegment>>` guards open segments; per-target ordering preserved independently.
- `SegmentWriter::open(config) -> Result<Self>`: create the segments directory if absent; error `PersistenceFailed` if the path is not writable. No recovery (sibling feature).
- Composition wiring in `src/app.rs`: resolve a data directory (`KROMETRAIL_DATA_DIR` env, else a platform-local default with a `tracing::warn!` if it falls back to a relative path); construct `SegmentStoreConfig { directory: <data_dir>/segments, rotation: RotationConfig::suggested() }`; `SegmentWriter::open(...)`; inject as `recording: Arc<dyn RecordingSink>` into `RuntimeDependencies` and `ProductionBrowserConnector::with_capture`. Remove `UnavailableRecordingSink`.
- Real-write smoke test `crates/krometrail-store/tests/segment_writer_smoke.rs` (temp dir).

**Non-goals:**
- Gap persistence (deferred to sqlite-index; `append_gap` returns `Unsupported`).
- Recovery, truncation, sealing of pre-existing open segments on startup (sibling feature). `SegmentWriter::open` creates the directory; it does not scan or seal existing files.
- Retention, eviction, pinning, session deletion (sibling feature).
- Frame-index / metadata writes (sqlite-index owns the index-commit side of the write-order invariant).
- A full configuration system (config file, precedence). The data-dir resolution here is env-or-default; the future config feature substitutes the path.
- Per-frame `fdatasync` (tiered durability: sync at seal/rotate/flush only).

## Files

- `crates/krometrail-store/src/segments/writer.rs` (new)
- `crates/krometrail-store/src/segments/mod.rs` (extend — re-export `SegmentWriter`, `SegmentStoreConfig`, `RotationConfig`)
- `crates/krometrail-store/src/lib.rs` (extend — re-export writer types)
- `src/app.rs` (extend — construct + inject `SegmentWriter`; remove `UnavailableRecordingSink`)
- `crates/krometrail-store/tests/segment_writer_smoke.rs` (new)

## Acceptance criteria

- [x] `SegmentWriter::open` creates the segments directory and returns `PersistenceFailed` when the configured path cannot be used as a writable directory.
- [x] `append_frame` writes a frame record and returns a `FrameAddress` that random-access decodes to the original frame.
- [x] Duration and size bounds both rotate before the triggering frame; the previous segment has a valid sealed footer and the trigger receives a fresh segment ID.
- [x] `flush(session_id)` seals every target segment with accurate record count, payload total, and first/last session times.
- [x] `append_gap` returns the documented `Unsupported` error and leaves file sizes unchanged.
- [x] Two concurrent target tasks produce disjoint segments; returned offsets increase in per-target append order.
- [x] Real-write smoke coverage round trips four JPEG/PNG frames across two targets using only their addresses.
- [x] `build_runtime()` injects `SegmentWriter`, resolves an env-or-platform data directory, propagates startup persistence errors, and removes `UnavailableRecordingSink`; isolated `doctor` succeeds.
- [x] Locked workspace fmt/check/test/clippy gates pass.

## Implementation notes

- Execution capability: highest/raised (autopilot caller), appropriate for filesystem durability, rotation, and root composition wiring.
- Review weight: standard (autopilot/project default); this child checkpoint advances directly to done.
- Files changed: `Cargo.toml`, `Cargo.lock`, `crates/krometrail-store/src/{lib.rs,segments/mod.rs,segments/writer.rs}`, `crates/krometrail-store/tests/segment_writer_smoke.rs`, `src/{app.rs,main.rs}`.
- Tests added: five filesystem integration tests covering directory creation/failure, absolute address reads, both rotation triggers, multi-target flushing/order, footer summaries, and gap write invariance.
- Simplification: removed the production unavailable recording placeholder; one writer and one frame-only codec now serve the capture path.
- Discrepancies from design: because the designed constructor has no monotonic-clock dependency, `sealed_observed` records the last accepted frame's observed time, the latest session-clock evidence available to the writer, rather than inventing an unrelated clock value at seal. A future clock-bearing store composition can tighten that field without changing the v1 layout. Size rotation follows the designed pre-append current-length check, so one record can cross the bound and the next frame triggers rotation.
- Honest partial integration: capture gaps remain metadata-only and return `Unsupported` until the SQLite index feature wires their persistence; no gap persistence success is claimed. Metadata indexing and the power-loss index-commit ordering remain downstream work.
- Adjacent issues parked: none.
- Verification: `cargo fmt --all -- --check`; locked workspace check; 246 workspace tests across 24 suites; locked workspace clippy with warnings denied; isolated `KROMETRAIL_DATA_DIR=<temp> cargo run --locked -- doctor` reported one available browser and created `<temp>/segments`.

## Notes

- The capture stream's behavior on a genuine gap event (hidden target, queue saturation, reconnect) until sqlite-index lands: `append_gap` returns `Unsupported`, the CDP pipeline declares a `PersistenceRejected` gap and fails the target stream. This is the same failure category today's `UnavailableRecordingSink` returns and fires only on real gap events — normal visible-tab capture persists frames end-to-end. Documented in the parent feature body; gap persistence is wired when sqlite-index lands.
- Tiered durability: records are flushed to the OS page cache on every `append_frame` (so the returned `FrameAddress` is process-crash-durable); `fdatasync` (power-loss-durable) runs at seal/rotate/flush. A power-loss crash between syncs can drop the unflushed tail; recovery (sibling feature) truncates it. The write-order invariant between segment-record and index-commit is enforced at the index-commit layer (sqlite-index).
