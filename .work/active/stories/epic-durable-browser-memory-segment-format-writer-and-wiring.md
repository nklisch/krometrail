---
id: epic-durable-browser-memory-segment-format-writer-and-wiring
kind: story
stage: implementing
tags: [storage, browser]
parent: epic-durable-browser-memory-segment-format
depends_on:
  - epic-durable-browser-memory-segment-format-core-address-contract
  - epic-durable-browser-memory-segment-format-binary-codec
release_binding: null
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

- [ ] `SegmentWriter::open` creates the segments directory if absent and returns `PersistenceFailed` for a non-writable path.
- [ ] `append_frame` writes a frame record and returns a `FrameAddress` whose `segment_id` matches the open segment and whose `byte_offset` points at the record-kind byte (verifiable by seeking to that offset and decoding the record back to the original `EncodedFrame`).
- [ ] Rotation fires when **either** `max_duration` or `max_size` is crossed: the crossed segment is sealed (valid `SealedFooter`) and a new segment with a fresh `SegmentId` is opened; the triggering frame lands in the **new** segment.
- [ ] `flush(session_id)` seals every open segment of that session: each sealed file ends with a valid `SealedFooter` whose `record_count`, `total_payload`, and `first/last_session_t` match the appended frames.
- [ ] `append_gap` returns `Unsupported` with the documented message and writes zero bytes (file sizes invariant across the call).
- [ ] Two target streams writing to the same `SegmentWriter` produce two disjoint segment files; per-target frame order is preserved within each file.
- [ ] Real-write smoke test: write N frames across two targets to a `tempfile::TempDir`, flush, then re-open each sealed file and decode every frame by its `FrameAddress` back to the original `EncodedFrame` (metadata field-equal, payload byte-equal).
- [ ] `build_runtime()` in `src/app.rs` constructs and injects `SegmentWriter` as `RecordingSink`; `cargo run -- doctor` still works (it touches the sink but does not append); `UnavailableRecordingSink` is removed.
- [ ] `cargo fmt --all --check`, `cargo check --workspace --all-targets --locked`, `cargo test --workspace --all-targets --locked`, `cargo clippy --workspace --all-targets --locked -- -D warnings` pass.

## Notes

- The capture stream's behavior on a genuine gap event (hidden target, queue saturation, reconnect) until sqlite-index lands: `append_gap` returns `Unsupported`, the CDP pipeline declares a `PersistenceRejected` gap and fails the target stream. This is the same failure category today's `UnavailableRecordingSink` returns and fires only on real gap events — normal visible-tab capture persists frames end-to-end. Documented in the parent feature body; gap persistence is wired when sqlite-index lands.
- Tiered durability: records are flushed to the OS page cache on every `append_frame` (so the returned `FrameAddress` is process-crash-durable); `fdatasync` (power-loss-durable) runs at seal/rotate/flush. A power-loss crash between syncs can drop the unflushed tail; recovery (sibling feature) truncates it. The write-order invariant between segment-record and index-commit is enforced at the index-commit layer (sqlite-index).
