---
id: epic-durable-browser-memory-segment-format
kind: feature
stage: drafting
tags: [storage, browser]
parent: epic-durable-browser-memory
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Append-Only Frame Segment Format and Writer

## Brief

Own the physical frame-payload layer of the recording store: a versioned, append-only segment file format with a recoverable record layout, bounded rotation by duration and size, and a sealed-footer immutability boundary. Compressed source frames are appended without transcoding during ingestion, and only the currently-open segment is mutable. Sealed segments become the unit that retention, recovery, pinning, and range reads operate on.

This feature also publishes the frame-address contract — a `(segment_id, byte_offset)` pair that locates a frame payload inside a sealed or open segment — and implements the frame-write path of the recording sink so the live capture stream (produced by `epic-rust-cdp-capture-foundation`) lands durably. It is the foundation feature of the epic: the SQLite index, retention, recovery, and range-resolution features all consume the addressing model and the sealed/open distinction established here.

This feature does not own searchable metadata (the SQLite index), budget accounting, eviction, pinning, or natural-anchor range resolution. It produces durable, addressable, immutable frame payloads and nothing more.

## Epic context

- Parent epic: `epic-durable-browser-memory`
- Position in epic: foundation feature — the segment format and frame-address contract are depended on by every other child. The writer implements the frame-persistence half of the existing `RecordingSink` port.
- Design decisions inherited: storage ports are extended in focused, capability-aligned slices rather than one god-port; compressed frames are stored without transcoding; only the open segment is mutable; sealed segments are immutable; rotation is bounded by duration and size.

## Simplification opportunity

- Replace the in-memory `FakeRecording` test double's assumed write surface with the real segment-write adapter wired through the composition root, so no production path depends on a test fake.
- Publish the frame-address contract once in `krometrail-core` (or as a tiny shared type consumed by core) so the index, retention, and recovery features never re-derive it. Do not invent a parallel addressing scheme per consumer.

## Foundation references

- `docs/VISION.md` — Local-First Operation
- `docs/SPEC.md` — Continuous Visual Capture (storage segment and byte offset fields), Disk Budget and Retention (time-based immutable segments, frame payloads stored separately from metadata)
- `docs/ARCHITECTURE.md` — Recording Store, Segment Format, Frame Ingestion, Crash Recovery
- `docs/EVALUATION.md` — Storage and Retention Evaluation (segment rotation, crash recovery of complete records)

## Scope and honest non-goals

**In scope:**

- A versioned segment container: format version, session and target identity, starting session time, ordered frame records, checksums, and a sealed footer that marks the segment immutable.
- A length-prefixed frame record: a metadata header followed by the encoded image payload, laid out so an incomplete trailing record can be detected and truncated by the recovery feature without scanning payload bytes.
- Segment rotation driven by bounded duration and bounded size; rotation seals the current segment and opens a new one.
- The frame-address contract — `(segment_id, byte_offset)` — published as the stable addressing surface that the index stores per frame.
- The frame-write path of the recording sink: append an `EncodedFrame` to the open segment, returning the frame address; append an explicit `CaptureGap` as a non-frame record (or via the index, decided in this feature's design pass); flush the open segment on session stop.
- Deterministic round-trip and corruption-detection tests for the format, plus a real-write smoke test against a temp directory.

**Non-goals:**

- Searchable metadata, ordering indexes, and per-kind structured tables — owned by `epic-durable-browser-memory-sqlite-index`.
- Budget accounting, pinning, eviction, and session deletion — owned by `epic-durable-browser-memory-retention`.
- Open-segment recovery, truncation, sealing, and index reconciliation on startup — owned by `epic-durable-browser-memory-recovery`.
- Natural-anchor and explicit temporal range resolution — owned by `epic-durable-browser-memory-range-resolution`.
- Transcoding, decoding, or artifact generation during ingestion — explicitly forbidden by SPEC; encoded bytes are stored as received.

## Notes for the design pass

- The `(segment_id, byte_offset)` frame-address contract is the load-bearing coupling point to the SQLite index feature. Settle its shape here, in `krometrail-core` or a tiny shared type, before any consumer lands.
- The recoverable-record layout (length-prefix plus checksum per record) must let the recovery feature identify the last complete record after a crash without parsing payload contents. Coordinate the on-disk record boundary with `epic-durable-browser-memory-recovery`'s expectations.
- Decide whether explicit `CaptureGap` records live inside the segment stream (as non-frame records) or only in the SQLite index. Either is defensible; the decision belongs in this feature's design pass and must keep `CaptureGap` queryable by range.
- The writer must preserve the SPEC invariant that metadata does not claim a frame exists until its complete segment record is durable. The write-order test (segment-record durable before any index commit) is jointly owned with the recovery feature.
