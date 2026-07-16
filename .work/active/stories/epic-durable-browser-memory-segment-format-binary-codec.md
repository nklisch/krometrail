---
id: epic-durable-browser-memory-segment-format-binary-codec
kind: story
stage: done
tags: [storage, browser]
parent: epic-durable-browser-memory-segment-format
depends_on: [epic-durable-browser-memory-segment-format-core-address-contract]
release_binding: 1.0.0
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

- [x] `SegmentHeader`, encoded `FrameRecord`, and `SealedFooter` round-trip structurally and use fixed-width big-endian fields.
- [x] Canonical header and record prefixes are asserted against literal bytes to catch endian and width drift.
- [x] Format versions `0` and `2` return `PersistenceFailed` naming expected vs observed; no migration is implied.
- [x] Header, footer, and record CRC mismatches are detected after covered-byte bit flips.
- [x] Truncated records return `Trailing::Incomplete` at the incomplete record start; scanner treats metadata/payload as opaque and touches them only for CRC.
- [x] A complete record followed by a CRC-corrupt record returns the first span and `Trailing::Corrupt` at the second start.
- [x] Clean record regions return all spans and `Trailing::Clean`.
- [x] An absolute `FrameAddress` reconstructs the original `EncodedFrame` from a complete segment buffer.
- [x] Codec round trips JPEG/PNG, present/absent and negative source time, zero/one/multiple warnings, and scales `1.0`, `2.0`, and `1.25`.
- [x] Focused formatting, tests, and clippy pass; full workspace gates are deferred to feature roll-up while parallel temporal-vision files settle.

## Implementation notes

- Execution capability: highest/raised, inherited from autopilot for the versioned recovery and random-access contract.
- Review weight: standard (autopilot/project default); child checkpoints do not enter review.
- Files changed: workspace/store manifests and lockfile; store segment header, record, footer, wire, scanner modules and exports.
- Tests added: seven deterministic codec tests covering versioning, byte order, CRC corruption, truncated/corrupt scans, full metadata variants, and absolute-address reads.
- Simplification: fixed BE fields and one scanner avoid a serde-binary format and duplicate recovery/read parsers.
- Discrepancies from design: scanner reports an incomplete record's start (the safe truncation boundary), not the physical end-of-file; payload bytes remain unparsed but are necessarily read by CRC32 validation.
- Adjacent issues parked: none.
- Verification: `cargo test -p krometrail-store --all-targets --locked` (7 passed); `cargo clippy -p krometrail-store --all-targets --locked -- -D warnings`.
