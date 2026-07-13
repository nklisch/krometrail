---
id: epic-durable-browser-memory-segment-format-binary-codec
kind: story
stage: implementing
tags: [storage, browser]
parent: epic-durable-browser-memory-segment-format
depends_on: [epic-durable-browser-memory-segment-format-core-address-contract]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Segment Binary Format Codec

## Brief

Implement the versioned, length-prefixed, CRC32-guarded segment byte layout as pure encode/decode functions with no filesystem I/O. This story owns the recoverable-record contract — the trickiest unit of the feature — and verifies it with deterministic byte-level tests for round-trip, host-independence, version guarding, CRC coverage, trailing-record detection (incomplete and corrupt), and random-access read. The writer (Unit 3) consumes this codec.

## Parent context

- Parent feature: `epic-durable-browser-memory-segment-format`
- This is Unit 2 of three. Depends on Unit 1 (`FrameAddress` / `ByteOffset`). Unblocks Unit 3 (writer + wiring).

## Scope

**In scope:**
- The segment byte layout described in the feature body:
  - `SegmentHeader` (magic `b"KTSF"`, `format_version: u16` starting at 1, segment/session/target UUIDs, `start_session_time`, `created_observed`, rotation-policy snapshot, `header_crc32`).
  - `FrameRecord` (encoded form): `record_kind: u8` (`0x01` frame; gaps reserved, never written), `header_len: u32`, `payload_len: u64`, `record_crc32: u32` (CRC32 over `header_bytes ⊕ payload_bytes`), then the per-frame metadata header bytes, then the encoded payload bytes.
  - Per-frame metadata header (fixed-layout, big-endian): `frame_id`, `capture_ordinal`, `source_time_present` + conditional `source_time`, `observed_time`, `session_time`, `format`, `image_w/h`, `viewport_w/h`, `device_scale` (f64), `warnings_count` + warning kind codes. `session_id`/`target_id` are NOT repeated (they live in the segment header).
  - `SealedFooter` (magic `b"KTSE"`, segment UUID, `record_count`, `total_payload`, `first/last_session_t`, `sealed_observed`, `footer_crc32`).
- A shared `wire` module of big-endian primitives (`u16/u32/u64/i128/f64/UUID` at offset) used by header, record, and footer — no serde-binary dependency.
- A forward-scan primitive `scan_complete_records(&[u8]) -> ScanResult { records: Vec<RecordSpan>, trailing: Trailing }` where `Trailing` is `Clean | Incomplete { at } | Corrupt { at }`. The scanner reads **only** the length+checksum prefix of each record; it does not parse metadata or payload bytes.
- A random-access reader that, given a buffer and a `FrameAddress.byte_offset`, decodes one `EncodedFrame` (reconstructed `CapturedFrame` metadata — combining the per-record header with the segment-header `session_id`/`target_id` — plus the payload bytes).
- Encode/decode functions for header, record, and footer.
- Add a CRC32 crate (e.g. `crc32fast = "1"`) to the workspace `[workspace.dependencies]` and to `crates/krometrail-store/Cargo.toml`; dev-depend `tempfile` (used by Unit 3, declared here so the manifest is stable).

**Non-goals:**
- Filesystem I/O, the `SegmentWriter` adapter, rotation, flush, composition wiring (Unit 3).
- Recovery logic (open-segment truncation, sealing, index reconciliation — sibling feature). This story provides the scanner primitive recovery will reuse; it does not perform recovery.
- Gap records of any kind (segments are frame-only).
- Per-frame `fdatasync` or durability semantics (Unit 3 and the recovery feature).

## Files

- `Cargo.toml` (workspace `[workspace.dependencies]` — add CRC32 crate)
- `crates/krometrail-store/Cargo.toml` (add `krometrail-core`, `serde`, CRC32 crate; dev-dep `tempfile`)
- `crates/krometrail-store/src/segments/mod.rs` (new — module root)
- `crates/krometrail-store/src/segments/wire.rs` (new — BE primitives)
- `crates/krometrail-store/src/segments/header.rs` (new)
- `crates/krometrail-store/src/segments/record.rs` (new)
- `crates/krometrail-store/src/segments/footer.rs` (new)
- `crates/krometrail-store/src/segments/scanner.rs` (new — `scan_complete_records`, random-access reader)
- `crates/krometrail-store/src/lib.rs` (extend — `pub mod segments;`)

## Acceptance criteria

- [ ] `SegmentHeader`, encoded `FrameRecord`, and `SealedFooter` round-trip byte-for-byte: encode then decode yields structural equality, and the encoded bytes are host-independent (all integers big-endian, fixed widths).
- [ ] At least one canonical header+record+footer sequence is asserted against literal expected bytes, so an accidental host-endian regression or width change is caught.
- [ ] A `format_version` mismatch on decode (e.g. `0` or `2`) returns `PersistenceFailed` naming expected vs observed; no silent migration.
- [ ] Header CRC, footer CRC, and record CRC each fail on a single bit-flip in their covered bytes.
- [ ] `scan_complete_records` on a buffer truncated mid-record returns `Trailing::Incomplete { at }` at the truncation offset, having consumed only the length+checksum prefix (the test asserts the scanner did not read payload bytes).
- [ ] `scan_complete_records` on a buffer with a complete record followed by a CRC-corrupt record returns the first record as complete and the second as `Trailing::Corrupt { at }` pointing at the corrupt record's start.
- [ ] `scan_complete_records` on a clean buffer ending exactly at a record boundary returns `Trailing::Clean` and all record spans.
- [ ] Given a `FrameAddress.byte_offset` from a forward scan, the random-access reader reconstructs the original `EncodedFrame` (metadata field-equal, payload byte-equal).
- [ ] The codec covers the full `CapturedFrame` surface: JPEG and PNG formats; present and absent `source_time`; zero, one, and multiple `CaptureWarning`s; `device_scale` at `1.0`, `2.0`, and a fractional value.
- [ ] `cargo fmt --all --check`, `cargo check --workspace --all-targets --locked`, `cargo test --workspace --all-targets --locked`, `cargo clippy --workspace --all-targets --locked -- -D warnings` pass.
