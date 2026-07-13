---
id: epic-durable-browser-memory-segment-format
kind: feature
stage: review
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

## Execution policy and grounding

- **Driver:** direct design under active autopilot `--all`; no user questions, no subagents, no peeragent. All probes were local (`read`, `grep`, `work-view`).
- **Effective worker capability:** highest/raised. This feature settles a versioned on-disk format, a durability ordering, a recovery boundary, and a shared frame-address contract consumed by four sibling features. The cost of getting the byte layout or the address shape wrong ripples through every later storage feature.
- **Effective review weight:** `standard` (project default). No design-time advisory review runs (the caller prohibited subdelegation); feature review remains required after implementation.
- **Dispatch rationale:** direct reads covered the feature brief, the parent epic and its decomposition-risk notes, all five foundation docs, `.agents/rules/agile-workflow.md`, the principles skill, the four sibling feature briefs (sqlite-index, retention, recovery, range-resolution), the archived `epic-rust-cdp-capture-foundation` ref, the current `krometrail-core` surface (`ids.rs`, `error.rs`, `time.rs`, `recording/{frame,gap,session}.rs`, `timeline/observation.rs`, `ports/{recording,timeline}.rs`, the exhaustive port-contract tests in `ports/mod.rs`), the empty `krometrail-store` stub, the composition root (`src/app.rs`, `src/main.rs`), the workspace `Cargo.toml`, the CDP capture pipeline that calls `RecordingSink` (`crates/krometrail-cdp/src/capture/{mod,pipeline}.rs`, including `declare_gap`, `CaptureDependencies`, and the `Ok(())` match arm at `pipeline.rs:801`), and one done sibling feature body (`cross-platform-capture-smoke`) for house style.
- **Rolling Foundation:** additive only. No standing assertion in VISION/SPEC/ARCHITECTURE/EVALUATION is contradicted by this design. The format concretizes `docs/ARCHITECTURE.md § Segment Format` and `§ Crash Recovery` and the SPEC storage-segment/byte-offset fields; it does not change what those documents claim.

## Design decisions

### 1. Frame-address contract lives in `krometrail-core`

```rust
// crates/krometrail-core/src/recording/address.rs (new)

/// Byte offset of a frame record's start within a segment file. Points at the
/// record-kind byte so a reader can parse the full record (header + payload)
/// from this offset without a second seek.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ByteOffset(u64);

impl ByteOffset {
    pub const fn new(value: u64) -> Self { Self(value) }
    pub const fn get(self) -> u64 { self.0 }
}

/// Stable location of a durably-written frame payload. The SQLite index stores
/// one of these per frame; the frame-source read port (owned by sqlite-index)
/// resolves one of these back to encoded bytes. Published here so index,
/// retention, recovery, and range-resolution never re-derive the shape.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct FrameAddress {
    pub segment_id: SegmentId,
    pub byte_offset: ByteOffset,
}
```

`FrameAddress` and `ByteOffset` are exported from `recording::mod` and `lib.rs` alongside the existing recording types. `SegmentId` already exists in `ids.rs`. `ByteOffset` is a plain `u64` newtype (it can legitimately be small — the first record sits right after the segment header — so it is not `NonZero`). The address is `Copy` so it threads through the capture pipeline and the future index-commit layer without allocation.

### 2. `RecordingSink::append_frame` evolves to return `FrameAddress`

The existing port returns `Result<()>`. The address is the durable evidence the index consumes, so it must be returned to the caller:

```rust
// crates/krometrail-core/src/ports/recording.rs (evolved)
pub trait RecordingSink: Send + Sync {
    fn append_frame(&self, frame: EncodedFrame) -> PortFuture<'_, Result<FrameAddress>>;
    fn append_gap(&self, gap: CaptureGap) -> PortFuture<'_, Result<()>>;
    fn flush(&self, session_id: SessionId) -> PortFuture<'_, Result<()>>;
}
```

`append_gap` and `flush` keep their signatures. This is a contract refinement (one return type widens from `()` to `FrameAddress`), not a port split — `RecordingSink` stays the capture pipeline's single focused write surface. The CDP pipeline's `Ok(())` match arm at `capture/pipeline.rs:801` widens to `Ok(_addr)` (the address is currently discarded; sqlite-index will consume it later). The five test fakes (`FakeRecording` in `ports/mod.rs`, `ShutdownTestSink` in `cdp/session.rs`, three `TestSink` impls in `cdp/capture/tests.rs`, `cdp/tests/capture_real.rs`, `cdp/tests/cross_platform_smoke.rs`) synthesize a `FrameAddress` (a monotonic counter for `byte_offset` against a fixed `SegmentId`) so they compile unchanged in behavior.

### 3. `CaptureGap` is metadata — it does **not** live in segment files

This is the load-bearing decision the design pass owed. Segments hold **only frame records**, full stop. `CaptureGap` is persisted through the metadata store (the generic timeline index as `ObservationKind::CaptureGap` plus the structured `CaptureGap` table), both owned by `epic-durable-browser-memory-sqlite-index`. Three authorities converge here:

- `docs/ARCHITECTURE.md § Segment Format` lists a segment's contents as "ordered frame records" with "the encoded image payload" — no gap records.
- `docs/SPEC.md § Disk Budget and Retention` states "Session metadata and artifact indexes are stored separately from frame payloads." A capture gap is metadata.
- The parent epic's design decision: "Dedicated structured tables are added only for records `krometrail-core` already defines (`CaptureGap` today)." A structured SQLite table is the gap's home.

Consequences and the honest partial-implementation:

- The `SegmentWriter` adapter does not serialize `CaptureGap` records. Its `append_gap` does not write to any segment.
- This feature lands **before** the SQLite-index feature (sqlite-index `depends_on` this feature), so the real metadata store is not available yet. The adapter's `append_gap` therefore returns an explicit `Unsupported` error: `"capture-gap persistence is owned by the SQLite metadata feature and is not yet wired"`. This is the same failure category today's `UnavailableRecordingSink.append_gap` returns, so there is no regression — today the entire sink is unavailable; after this feature, frames persist and only gap persistence is unavailable.
- The capture pipeline's existing behavior on `append_gap` error is to declare a `PersistenceRejected` gap and fail the target stream (`capture/pipeline.rs`). That cascade is pre-existing and unchanged; it fires only on genuine gap events (hidden target, queue saturation, reconnect). Normal visible-tab capture declares no gaps and persists frames end-to-end. When sqlite-index lands, gap persistence is wired and the cascade disappears for ordinary gaps.
- The gap-routing **mechanics** (whether sqlite-index exposes a `CaptureGapSink` port, routes through `TimelineStore.append`, and/or adds `observed_time` to `CaptureGap` so a `CaptureGap → TimelineObservation` conversion is lossless) are sqlite-index's design choice. This feature publishes only the **decision** (gaps are metadata, not segment records) and the contract that segments are frame-only. See `## Handoff to downstream features`.

### 4. Segment binary layout — versioned, length-prefixed, CRC-guarded, sealed-footer

A segment file is a contiguous byte sequence:

```text
┌─────────────────────────── SegmentHeader ───────────────────────────┐
│ magic            4 B   b"KTSF"  (Krometrail Segment Format)          │
│ format_version   u16   BE        (starts at 1; forward-only)        │
│ segment_id       16 B  UUID                                        │
│ session_id       16 B  UUID                                        │
│ target_id        16 B  UUID                                        │
│ start_session_time u64 BE ns      (segment-open session time)       │
│ created_observed  u64 BE ns       (segment-open monotonic time)     │
│ rot_max_duration  u64 BE ns       (rotation policy snapshot)        │
│ rot_max_size      u64 BE bytes    (rotation policy snapshot)        │
│ header_crc32      u32             (CRC32 of all header bytes above) │
└─────────────────────────────────────────────────────────────────────┘
┌──────────────────────── FrameRecord (repeated) ─────────────────────┐
│ record_kind      u8    0x01 = frame (gaps reserved, never written)  │
│ header_len       u32   BE        (length of the metadata header)    │
│ payload_len      u64   BE        (length of the encoded image)      │
│ record_crc32     u32             (CRC32 of header_bytes ⊕ payload)  │
│ header_bytes     [header_len]    (per-frame metadata, layout below) │
│ payload_bytes    [payload_len]   (encoded JPEG/PNG, verbatim)       │
└─────────────────────────────────────────────────────────────────────┘
┌──────────────────────── SealedFooter (written once, on seal) ───────┐
│ footer_magic     4 B   b"KTSE"  (Krometrail Segment End)            │
│ segment_id       16 B  UUID      (must match header)                │
│ record_count     u64   BE                                           │
│ total_payload    u64   BE bytes                                    │
│ first_session_t  u64   BE ns      (first frame's session time)      │
│ last_session_t   u64   BE ns      (last frame's session time)       │
│ sealed_observed  u64   BE ns                                       │
│ footer_crc32     u32             (CRC32 of footer bytes above)      │
└─────────────────────────────────────────────────────────────────────┘
```

**Per-frame metadata header (`header_bytes`)** is a fixed-layout, host-independent, big-endian byte sequence carrying every `CapturedFrame` field that varies per frame. `session_id` and `target_id` are **not** repeated per record — they live once in the segment header (a segment is single-session, single-target), which removes per-frame duplication without losing information:

```text
frame_id          16 B  UUID
capture_ordinal   u64   BE
source_time_present u8   (0/1)
source_time       i128  BE   (present only when source_time_present == 1)
observed_time     u64   BE ns
session_time      u64   BE ns
format            u8    (0=jpeg, 1=png; mirrors ImageFormat)
image_w           u32   BE
image_h           u32   BE
viewport_w        u32   BE
viewport_h        u32   BE
device_scale      f64   BE   (IEEE-754; validated finite > 0 on read)
warnings_count    u16   BE
warnings          [warnings_count] u8   (CaptureWarning kind codes)
```

The record is fully self-describing: `header_len` + `payload_len` are read **before** the variable bytes, and `record_crc32` covers the concatenation of header and payload. A reader/recovery scanner can decide "is this record complete and intact?" by reading the three length/checksum fields, checking that `header_len + payload_len` bytes remain, and verifying the CRC — **without parsing the metadata header or scanning the payload**. This is the recoverable-record contract the recovery feature depends on.

**Format version** is `u16`, starts at `1`, and is forward-only. A version mismatch on read is a hard `PersistenceFailed` error — this feature ships no migration. A future v2 lands through explicit version bump + migration story, not by silently widening v1.

**Checksum** is CRC32 (not cryptographic): the threat model is torn writes and bit rot, not adversaries. The implementation adds a CRC32 crate to the workspace (e.g. `crc32fast`); the design names the algorithm and lets the implementor pick the crate.

**Why fixed-layout BE bytes, not bincode/postcard/serde-json.** The metadata header is a fixed-shape, all-numeric record. A hand-rolled big-endian layout needs no serialization dependency, is byte-stable across versions and hosts (every field has an explicit width and offset), and is trivially recoverable (recovery reads length-prefixed fields by offset). Adding a serde-binary format would be reasonable but is not the shortest clear solution for a fixed-shape header.

### 5. `FrameAddress.byte_offset` points at the record-kind byte

The index stores the offset of the `record_kind` byte. The frame-source read port (sqlite-index) seeks to `byte_offset`, reads `record_kind + header_len + payload_len + record_crc32`, then `header_len + payload_len` bytes, verifies the CRC, and returns the `EncodedFrame` reconstructed from the header + payload. Pointing at the record start (not the payload start) means the reader gets the full self-describing record in one seek.

### 6. Rotation: bounded duration **and** bounded size

A segment rotates (seals + opens a new one) when **either** bound is crossed on a frame append:

```rust
pub struct RotationConfig {
    pub max_duration: Duration,   // suggested default: 120 s
    pub max_size: u64,            // suggested default: 128 MiB
}
```

Rotation check fires at the top of `append_frame`: if the open segment's age (`frame.session_time - header.start_session_time`) ≥ `max_duration`, or its current file size ≥ `max_size`, the writer seals the open segment (writes `SealedFooter`, fsyncs, closes) and opens a fresh segment with a new `SegmentId` before appending the frame. The frame that triggers rotation lands in the **new** segment. Defaults are tunable; the exact values are not load-bearing for the design (the rotation *mechanism* is). Configuration wiring (a future config feature) can override them; for now `RotationConfig` is constructed in the composition root.

A segment is single-target: rotation is per `(session_id, target_id)`. The writer maintains one open segment per active target so per-target ordering is preserved independently, satisfying `ARCHITECTURE.md § Capture Tasks` ("ordering is preserved independently per target").

### 7. Durability and the write-order invariant

The SPEC invariant — "metadata does not claim a frame exists until its complete segment record is durable" — is preserved by the address-return contract: `append_frame` returns `FrameAddress` **only after** the record bytes are flushed to the file. The writer uses `BufWriter` for throughput and calls `flush()` (which drains the kernel buffer via the file's `write`) before constructing the address; `fdatasync` (true durability against power loss) is performed at **seal/rotate** and at **flush-on-stop**, not per-frame.

This gives a deliberately tiered durability model:

- **Between seals:** records are in the OS page cache (durable against process crash, not against power loss). A power-loss crash can drop the unflushed tail.
- **At seal/rotate/flush:** `fdatasync` makes every prior record power-loss-durable.

The recovery feature (sibling) handles the gap: on startup it scans open segments, uses the per-record length+checksum to find the last complete record, truncates any incomplete trailing bytes, and seals. The write-order invariant between segment-record-durable and index-commit is enforced at the **index-commit layer** (sqlite-index), which must not commit an index row for a `FrameAddress` whose segment has not been sealed-or-synced if power-loss durability is required for that row. This feature provides the primitive (`flush`/`fdatasync` at seal) and returns the address only after the OS-level flush; the cross-layer crash-mid-write integration test is jointly owned with recovery (see Handoff). Per-frame `fdatasync` would make every frame power-loss-durable but is too slow for 30 fps capture (the `EVALUATION.md` "disk write throughput" metric would suffer); tiered durability is the proportional choice.

### 8. Flush-on-stop seals the open segment

`RecordingSink::flush(session_id)` flushes and `fdatasync`s every open segment for that session, writes the `SealedFooter`, and closes the file. After flush, the segment is immutable (sealed). This satisfies SPEC's "Stopping a session flushes accepted frames and metadata before reporting completion" for the frame-payload half (the metadata half is sqlite-index).

## Architectural choice: where do `CaptureGap` records live?

Three options were weighed.

### Option A — `CaptureGap` as a non-frame record inside the segment stream

Add a `record_kind = 0x02` for gaps and serialize `CaptureGap` into the segment alongside frame records. The writer's `append_gap` would then write a real record; the index would later scan segments to populate the gap table.

**Rejected.** It contradicts `ARCHITECTURE.md § Segment Format` ("ordered frame records" + "the encoded image payload") and SPEC ("metadata stored separately from frame payloads"). It also duplicates gap authority (segment + SQLite) and forces recovery to reconcile gap records in addition to frame records. The parent epic already decided `CaptureGap` gets a structured SQLite table, which is the gap's natural home.

### Option B — `CaptureGap` in the index only; this feature's adapter delegates `append_gap` to an injected `TimelineStore`

Keep `append_gap` on `RecordingSink`; the `SegmentWriter` forwards each gap to an injected `Arc<dyn TimelineStore>` after converting it to an `ObservationKind::CaptureGap` observation. Forward-compatible: when sqlite-index lands, gap persistence "just works" by injecting the real adapter.

**Rejected for this feature.** The `CaptureGap → TimelineObservation` conversion needs `observed_time` (an absolute monotonic clock), which `CaptureGap` does not carry today (it carries only a session-relative `SessionRange`). A lossless conversion therefore requires evolving `CaptureGap` to carry `observed_time`, touching `declare_gap` in the done CDP epic, and picking a conversion home (the segment adapter or a recording facade). That is sqlite-index's design space, not this feature's. Forcing the decision now would either ripple a core-type change through a done epic for this feature's benefit, or put a lossy conversion in the segment adapter.

### Option C (chosen) — `CaptureGap` in the index only; this feature publishes the decision and stubs `append_gap` as a documented deferred operation

Segments are frame-only by contract. The `SegmentWriter` does not serialize gaps. Its `append_gap` returns `Unsupported` with a clear message until the SQLite-index feature wires gap persistence through whatever metadata port it owns (`TimelineStore`, a new `CaptureGapSink`, etc.). The decision is authoritative here; the mechanics land with the feature that owns the metadata store.

Chosen because it respects the foundation docs, matches the parent epic's structured-table decision, keeps this feature focused on the format and frame-write path (its primary deliverables), and avoids rippling a core-type evolution through a done epic prematurely. The cost — the target stream fails on a genuine gap event until sqlite-index lands — is the same failure category today's `UnavailableRecordingSink` already returns for gaps, so no regression; the gap-persistence mechanics are sqlite-index's to design.

## Trickiest unit

The **recoverable-record boundary** is the unit with the most novel risk and the longest reach. The layout must let the recovery feature identify the last complete record after a crash *without parsing payload contents*, while also letting the frame-source read port reconstruct a full `EncodedFrame` from a single `FrameAddress`. The two consumers have opposite directions: recovery scans forward (header → records → footer, truncating the torn tail), the read port seeks random-access to one address. Both depend on the same invariant: `record_kind + header_len + payload_len + record_crc32` precede the variable bytes, and `record_crc32` covers `header_bytes ⊕ payload_bytes`. If the length fields are mis-encoded, or the CRC covers the wrong slice, recovery either over-truncates (losing real frames) or under-truncates (leaving corrupt records the index then claims). The codec story nails this with forward-scan, random-access-read, torn-record, and bit-flip tests before the writer story is allowed to consume it.

## Implementation units

Three child stories, linear dependency chain.

### Unit 1: Core frame-address contract and `RecordingSink` evolution

**Story:** `epic-durable-browser-memory-segment-format-core-address-contract`

**Files:**
- `crates/krometrail-core/src/recording/address.rs` (new) — `FrameAddress`, `ByteOffset`.
- `crates/krometrail-core/src/recording/mod.rs` (extend) — re-export `FrameAddress`, `ByteOffset`.
- `crates/krometrail-core/src/lib.rs` (extend) — pub-use `FrameAddress`, `ByteOffset`.
- `crates/krometrail-core/src/ports/recording.rs` (evolve) — `append_frame -> Result<FrameAddress>`.
- `crates/krometrail-core/src/ports/mod.rs` (extend) — `FakeRecording::append_frame` synthesizes a `FrameAddress` (monotonic counter for `byte_offset` against a fixed `SegmentId`); the existing `recording_port_separates_frames_gaps_and_flush` test is updated to assert the returned address.
- `crates/krometrail-cdp/src/capture/pipeline.rs` (mechanical) — the `Ok(())` match arm at line ~801 widens to `Ok(_addr)` (address discarded; sqlite-index consumes it later).
- `crates/krometrail-cdp/src/session.rs` (mechanical) — `ShutdownTestSink::append_frame` returns a synthesized `FrameAddress`.
- `crates/krometrail-cdp/src/capture/tests.rs` (mechanical) — `TestSink::append_frame` returns a synthesized `FrameAddress`.
- `crates/krometrail-cdp/tests/capture_real.rs` (mechanical) — `TestSink::append_frame` returns a synthesized `FrameAddress`.
- `crates/krometrail-cdp/tests/cross_platform_smoke.rs` (mechanical) — `TestSink::append_frame` returns a synthesized `FrameAddress`.

**Acceptance criteria:**
- [ ] `FrameAddress { segment_id: SegmentId, byte_offset: ByteOffset }` and `ByteOffset(u64)` exist in `krometrail-core`, are `Copy + Eq + Hash + Serialize + Deserialize`, round-trip through serde, and are re-exported from the crate root.
- [ ] `RecordingSink::append_frame` returns `Result<FrameAddress>`; `append_gap` and `flush` are unchanged.
- [ ] Every test fake that implements `RecordingSink` (`FakeRecording`, `ShutdownTestSink`, three `TestSink`s) compiles and returns a non-zero `FrameAddress` whose `segment_id`/`byte_offset` are populated.
- [ ] The CDP capture pipeline's `append_frame` match arm compiles (`Ok(_addr)`); no pipeline behavior changes — frames are still discarded by the pipeline until the index layer consumes the address.
- [ ] `cargo fmt --all --check`, `cargo check --workspace --all-targets --locked`, `cargo test --workspace --all-targets --locked`, `cargo clippy --workspace --all-targets --locked -- -D warnings` pass.
- [ ] The `core_ports_have_no_runtime_or_transport_types` source-scanner test in `ports/mod.rs` still passes (no `tokio`/`sqlite`/`cdp` markers leak into the port module).

### Unit 2: Segment binary format codec

**Story:** `epic-durable-browser-memory-segment-format-binary-codec`

**Depends on:** `epic-durable-browser-memory-segment-format-core-address-contract`

**Files:**
- `Cargo.toml` (workspace `[workspace.dependencies]`) — add a CRC32 crate (e.g. `crc32fast = "1"`).
- `crates/krometrail-store/Cargo.toml` — depend on `krometrail-core`, `serde`, `crc32fast` (workspace); dev-depend on `tempfile`.
- `crates/krometrail-store/src/segments/mod.rs` (new) — module root.
- `crates/krometrail-store/src/segments/header.rs` (new) — `SegmentHeader`, encode/decode, header CRC.
- `crates/krometrail-store/src/segments/record.rs` (new) — `FrameRecord` (encoded form), `encode_frame_record(&CapturedFrame, &[u8]) -> Vec<u8>`, `decode_frame_record(&[u8]) -> Result<DecodedFrameRecord>`, record CRC over `header ⊕ payload`.
- `crates/krometrail-store/src/segments/footer.rs` (new) — `SealedFooter`, encode/decode, footer CRC.
- `crates/krometrail-store/src/segments/wire.rs` (new) — fixed-layout BE primitives (`u16/u32/u64/i128/f64/UUID` read/write at offset) shared by header/record/footer. No serde-binary dependency.
- `crates/krometrail-store/src/segments/scanner.rs` (new) — `scan_complete_records(&[u8]) -> ScanResult { records: Vec<RecordSpan>, trailing: Trailing }` where `Trailing` is `Clean | Incomplete | Corrupt { at }`. This is the forward-scan primitive the writer, the frame-source read port, and recovery all share; it reads only length+checksum fields.
- `crates/krometrail-store/src/lib.rs` (extend) — `pub mod segments;`.

**Acceptance criteria:**
- [ ] `SegmentHeader`, `FrameRecord` (encoded), and `SealedFooter` round-trip byte-for-byte: encode then decode yields equality, and the encoded form is host-independent (all integers big-endian, fixed widths).
- [ ] A `format_version` mismatch on decode returns `PersistenceFailed` with a message naming the expected vs observed version. No silent migration.
- [ ] Header/footer/record CRC mismatch is detected: a single bit-flip anywhere in the covered bytes makes the relevant CRC check fail.
- [ ] `scan_complete_records` on a buffer truncated mid-record returns `Trailing::Incomplete` and the offset of the truncation, **without reading the record's metadata or payload** (the test asserts the scanner consumed only the length+checksum prefix).
- [ ] `scan_complete_records` on a buffer with a complete record followed by a CRC-corrupt record returns the first record as complete and the second as `Trailing::Corrupt { at }` pointing at the corrupt record's start.
- [ ] `scan_complete_records` on a clean buffer ending exactly at a record boundary returns `Trailing::Clean` and all record spans.
- [ ] A `FrameAddress.byte_offset` returned by the scanner, when seeked-and-read by a random-access reader, reconstructs the original `EncodedFrame` (metadata + payload byte-equal).
- [ ] `cargo fmt --all --check`, `cargo check --workspace --all-targets --locked`, `cargo test --workspace --all-targets --locked`, `cargo clippy --workspace --all-targets --locked -- -D warnings` pass.

### Unit 3: `SegmentWriter` adapter, rotation, flush, and composition wiring

**Story:** `epic-durable-browser-memory-segment-format-writer-and-wiring`

**Depends on:** `epic-durable-browser-memory-segment-format-core-address-contract`, `epic-durable-browser-memory-segment-format-binary-codec`

**Files:**
- `crates/krometrail-store/src/segments/writer.rs` (new) — `SegmentWriter` (implements `RecordingSink`), `SegmentStoreConfig { directory: PathBuf, rotation: RotationConfig }`, `RotationConfig { max_duration: Duration, max_size: u64 }`, `OpenSegment` state (per `(SessionId, TargetId)`).
- `crates/krometrail-store/src/lib.rs` (extend) — re-export `SegmentWriter`, `SegmentStoreConfig`, `RotationConfig`.
- `src/app.rs` (extend) — `build_runtime()` constructs `SegmentStoreConfig` from a data-directory path (env `KROMETRAIL_DATA_DIR`, defaulting to a platform-local dir; segments under `<data_dir>/segments/`) with `RotationConfig` defaults, builds `SegmentWriter`, and wires it as the `recording: Arc<dyn RecordingSink>` dependency (replacing `UnavailableRecordingSink`). The `UnavailableRecordingSink` type is removed once nothing else uses it.
- `crates/krometrail-store/tests/segment_writer_smoke.rs` (new) — real-write smoke test against `tempfile::TempDir`.

**`SegmentWriter` shape:**

```rust
pub struct SegmentStoreConfig {
    pub directory: PathBuf,
    pub rotation: RotationConfig,
}

pub struct RotationConfig {
    pub max_duration: Duration,
    pub max_size: u64,
}

impl RotationConfig {
    pub const fn suggested() -> Self { /* 120 s, 128 MiB */ }
}

pub struct SegmentWriter { /* directory, rotation, Mutex<HashMap<(SessionId, TargetId), OpenSegment>> */ }

impl SegmentWriter {
    pub fn open(config: SegmentStoreConfig) -> Result<Self>;     // create dir, no recovery (recovery is a sibling feature)
}

impl RecordingSink for SegmentWriter {
    // append_frame: acquire per-(session,target) open segment (rotate if bound crossed),
    //   encode FrameRecord, write to BufWriter, flush the writer, return FrameAddress.
    // append_gap: return Unsupported ("capture-gap persistence is owned by the SQLite metadata feature").
    // flush: for each open segment of `session_id`, write SealedFooter, fdatasync, close.
}
```

**Behavior:**
- `append_frame`: lazily opens a segment for `(frame.session_id(), frame.target_id())` on first append. Checks rotation bounds (age by `frame.session_time() - header.start_session_time`, size by current file length); if crossed, seals + fsyncs the current segment and opens a new one with a fresh `SegmentId`. Encodes the `FrameRecord` from `frame.metadata()` + `frame.bytes()`, appends to the `BufWriter`, flushes the buffer, and returns `FrameAddress { segment_id, byte_offset: <offset of record_kind byte> }`. The byte offset is the file length before this append.
- `append_gap`: returns `Unsupported` (`KrometrailError::new(ErrorCode::Unsupported, ...)`) with the message above. Documented partial-implementation; gap persistence lands with sqlite-index.
- `flush(session_id)`: for each open segment of that session, writes the `SealedFooter` (with `record_count`, `total_payload`, first/last session time, `sealed_observed`), flushes, `fdatasync`s, closes the file. After flush the segment is immutable.
- Concurrency: a `Mutex<HashMap<(SessionId, TargetId), OpenSegment>>` guards open segments. Per-target appends take the mutex briefly; cross-target appends do not need parallel disk writes (`ARCHITECTURE.md § Capture Tasks` explicitly permits shared disk writing).

**Composition wiring (`src/app.rs`):**
- Resolve a data directory: `KROMETRAIL_DATA_DIR` env var, else a platform default (`dirs`-style; for the foundation, a relative `./krometrail-data` fallback is acceptable with a `tracing` warning — the real configuration system lands separately).
- `SegmentWriter::open(SegmentStoreConfig { directory: <data_dir>/segments, rotation: RotationConfig::suggested() })`.
- Inject as `recording: Arc<dyn RecordingSink>` into `RuntimeDependencies` and into `ProductionBrowserConnector::with_capture`.

**Acceptance criteria:**
- [x] `SegmentWriter::open` creates the segments directory and reports unusable paths as `PersistenceFailed`.
- [x] `append_frame` returns an absolute address that decodes back to the complete original frame.
- [x] Duration and size rotation seal the old segment and place the triggering frame in a fresh segment.
- [x] Session flush seals every target segment with validated footer counts, payload totals, and frame-time bounds.
- [x] Gap writes fail explicitly as deferred SQLite metadata work and change no segment bytes.
- [x] Concurrent target streams remain in disjoint segments with increasing per-target offsets.
- [x] Real-write tests round trip multiple JPEG/PNG frames across two targets solely through stored addresses.
- [x] Root composition injects the real segment writer, propagates startup failures, and removes `UnavailableRecordingSink`; isolated `doctor` succeeds.
- [x] Locked workspace fmt/check/test/clippy gates pass.

## Implementation order and dependencies

```text
Unit 1 (core-address-contract)        depends_on: []
   │   publishes FrameAddress/ByteOffset; widens RecordingSink::append_frame
   ▼
Unit 2 (binary-codec)                  depends_on: [Unit 1]
   │   pure encode/decode/scanner; no I/O; round-trip + corruption tests
   ▼
Unit 3 (writer-and-wiring)             depends_on: [Unit 1, Unit 2]
       SegmentWriter + rotation + flush + app.rs wiring + real-write smoke
```

Linear chain. Unit 1 is the cross-crate contract evolution (core + cdp fakes/pipeline); Unit 2 is the pure, store-local format codec (no I/O, fully deterministic tests); Unit 3 consumes both to deliver the live write path. Splitting the codec (Unit 2) from the writer (Unit 3) keeps the recoverable-record contract — the trickiest unit — reviewable on deterministic byte-level evidence before any filesystem behavior is layered on.

## Simplification and elimination

- **Segments are frame-only.** Rejecting Option A removes a `record_kind` discriminator branch at read time, a parallel gap-record encode/decode path, and recovery's need to reconcile gap records. One record shape, one scan path, one read path.
- **`session_id`/`target_id` live in the segment header, not per record.** A segment is single-session, single-target by construction; repeating these per record would be 32 wasted bytes per frame with no information gain. The frame-source read port reconstructs the full `CapturedFrame` by combining the per-record header with the segment header.
- **Fixed-layout BE wire, no serde-binary crate.** The metadata header is all-numeric and fixed-shape. A hand-rolled `wire.rs` of BE primitives is shorter than introducing `bincode`/`postcard`, is byte-stable across versions, and keeps the store crate's dependency surface minimal.
- **One forward-scan primitive shared by writer, read port, and recovery.** `scan_complete_records` is written once in the codec story; the writer uses it for nothing heavier than offset accounting, the read port uses its record spans for random-access decode, and recovery will reuse it for truncation. No second scanner.
- **CRC32, not SHA.** The threat model is torn writes and bit rot; CRC32 is the proportional choice and avoids hashing large payloads unnecessarily.
- **No parallel addressing scheme.** `FrameAddress` is published once in core; every consumer imports it. The epic's decomposition risk (segment-format ↔ index coupling on the address contract) is mitigated by settling the shape before any consumer lands.
- **`UnavailableRecordingSink` is removed.** Once the real `SegmentWriter` is wired in Unit 3, the placeholder sink is dead code; deleting it leaves the binary simpler.

## Testing

### Deterministic, no filesystem (Unit 2)

- **Round-trip:** encode `SegmentHeader` + N `FrameRecord`s + `SealedFooter`; decode; assert byte-equal and structural equality. Cover JPEG and PNG payloads, present and absent `source_time`, zero and multiple warnings, `device_scale` extremes (1.0, 2.0, fractional).
- **Host-independence:** the encoded bytes are asserted against literal expected byte sequences for at least one canonical header+record+footer, so an accidental host-endian regression is caught.
- **Version guard:** decode with `format_version = 0` and `format_version = 2` both return `PersistenceFailed`; no silent migration.
- **CRC coverage:** bit-flip each of (header bytes, payload bytes, length fields) and assert the relevant CRC fails; bit-flip a byte outside any CRC-covered slice (none exists by construction — every byte is covered) and document.
- **Trailing-record detection:** truncated-mid-record buffer → `Trailing::Incomplete` at the right offset, scanner consumed only the length+checksum prefix (assert the scanner did not read payload bytes). Corrupt-record buffer → `Trailing::Corrupt { at }`. Clean buffer → `Trailing::Clean`.
- **Random-access read:** given a `FrameAddress` from a forward scan, seek-and-decode yields the original `EncodedFrame`.

### Real filesystem, temp dir (Unit 3)

- **Single-target write+read:** write N frames, flush, re-open the sealed file, decode every frame by its `FrameAddress`, assert metadata + payload byte-equal.
- **Rotation by duration and by size:** construct a writer with a tiny `max_size` and assert rotation fires after the bound; construct with a tiny `max_duration` (simulated by injecting frame session times that span the bound) and assert rotation fires. The triggering frame lands in the new segment.
- **Multi-target isolation:** two targets writing concurrently produce two disjoint sealed files; per-target order preserved.
- **Gap no-op:** `append_gap` writes zero bytes (file size invariant) and returns `Unsupported`.
- **Flush seals:** after `flush(session_id)`, the sealed file's footer validates and its `record_count`/`first_session_t`/`last_session_t` match the appended frames.
- **Open failure:** `SegmentWriter::open` against a non-writable path returns `PersistenceFailed`.

### Boundary

- The `core_ports_have_no_runtime_or_transport_types` source-scanner test in `ports/mod.rs` continues to pass after the `append_frame` signature change — no `tokio`/`sqlite`/`cdp` markers leak into the core port module.
- `cargo check -p krometrail-core` passes without the store crate (core remains infrastructure-free).

## Risks

- **Recoverable-record contract is the highest-reach decision.** If the length+checksum layout is wrong, recovery (sibling feature) either over-truncates real frames or under-truncates corrupt tails, and the index then lies about what exists. Mitigation: the codec story nails forward-scan + torn-record + corrupt-record + random-access tests before the writer consumes it; the recovery feature reuses the same `scan_complete_records` primitive.
- **Capture-stream cascade on gap events until sqlite-index lands.** `append_gap` returns `Unsupported`, and the CDP pipeline fails the target stream on gap-persistence error. This is the same failure category today's unavailable sink returns, and fires only on genuine gap events (hidden target, saturation, reconnect) — normal visible-tab capture is unaffected. Documented in the adapter and in this body. Mitigation: gap persistence is wired when sqlite-index lands; the partial state is explicitly time-bounded by the epic's dependency chain.
- **Tiered durability (sync at seal, not per frame) means a power-loss crash can drop the unflushed tail.** This is deliberate (per-frame `fdatasync` is too slow for 30 fps) and recovery is designed to handle it (truncate incomplete trailing records). The write-order invariant between segment-record and index-commit is enforced at the index-commit layer, not here. Mitigation: the cross-layer crash-mid-write integration test is jointly owned with recovery and explicitly named in Handoff.
- **Data-directory configuration is minimal.** The composition root resolves a path from `KROMETRAIL_DATA_DIR` or a platform default; a full configuration system (config file, precedence) is out of scope. Mitigation: `SegmentStoreConfig` takes a `PathBuf`, so the future config feature substitutes the path without touching the writer.
- **CRC32 crate added to the workspace.** Minimal surface (`crc32fast` is a single-purpose, no-dep crate). Mitigation: named as a workspace dependency; the algorithm, not the crate, is the contract.
- **`append_frame` signature change ripples through five test fakes and one pipeline match arm.** All mechanical. Mitigation: the change is consolidated in Unit 1 so the workspace compiles end-to-end after that story.

## Handoff to downstream features

- **`epic-durable-browser-memory-sqlite-index`:** consumes `FrameAddress`/`ByteOffset` from core; stores one `FrameAddress` per frame in the frame-index table; populates the frame-index row from the `EncodedFrame` + the address returned by `append_frame`. Owns gap persistence — pick the routing mechanic (`TimelineStore.append` of an `ObservationKind::CaptureGap`, a dedicated `CaptureGapSink` port, and/or adding `observed_time` to `CaptureGap` for a lossless `CaptureGap → TimelineObservation` conversion) and replace the `SegmentWriter`'s `append_gap` `Unsupported` stub with delegation to that port. Owns the frame-source read port that seeks a `FrameAddress` and decodes via the codec's random-access reader. Owns the index-commit side of the write-order invariant (do not commit a row for a `FrameAddress` whose segment has not been sealed-or-synced if power-loss durability is required for that row).
- **`epic-durable-browser-memory-recovery`:** consumes the recoverable-record layout (`scan_complete_records`, `SealedFooter`); on startup, scans open segments, truncates `Trailing::Incomplete`/`Corrupt` tails, seals segments missing a valid footer, and reconciles the SQLite frame index + usage accounting. Owns the cross-layer crash-mid-write integration test that exercises this format + the sqlite-index write-order invariant. The format-level corruption-detection tests in Unit 2 are the lower half of that evidence; recovery owns the upper (real-crash) half. Recovery is idempotent and trusts segment-file checksums as the byte-level authority; pins in SQLite are trusted across recovery.
- **`epic-durable-browser-memory-retention`:** enumerates **sealed** segments (presence of a valid `SealedFooter`) for oldest-unpinned-first eviction and session deletion. Open segments are not eligible for eviction (only the open segment is mutable; retention tolerates at most one open segment beyond budget per the evaluation). Uses the segment header's `start_session_time` and the footer's `last_session_t` for chronological ordering and range intersection.
- **`epic-durable-browser-memory-range-resolution`:** does not read segments directly; it consumes `FrameAddress`es from the sqlite-index frame-source read port to retrieve source frames for a resolved range.

## Notes

- The `(segment_id, byte_offset)` frame-address contract, the frame-only segment contents decision, and the recoverable-record layout are the three load-bearing settlements. Everything else (rotation defaults, exact CRC crate, data-dir resolution) remains tunable within the constraints above.

## Implementation summary

- Execution capability: highest/raised (autopilot caller), selected because this feature fixes the shared address contract, versioned disk layout, recovery boundary, durability tiers, and production composition.
- Review weight: standard (autopilot/project default). Implementation is complete and this feature is intentionally left at `stage: review`; no self-review was performed.
- Child checkpoints: core address contract, binary codec, and writer/wiring all advanced directly to `done` with per-checkpoint evidence.
- Commits: `4ec4fd1` (core address contract), `a8edd75` (binary codec), `e618437` (writer and composition wiring).
- Production files: core recording address/port exports; mechanical CDP sink consumers; workspace/store manifests; `krometrail-store::segments` v1 header/record/footer/wire/scanner/writer modules; root composition and startup error propagation.
- Verification: locked workspace format and check; 246 tests across 24 suites; locked workspace clippy with warnings denied; isolated data-directory `doctor` reported an available browser and created its segments directory.
- Durability delivered: every append flushes a complete CRC-guarded record before returning its address; rotation and session flush append a sealed footer, flush, `sync_data`, close, and rename the segment from `.open` to immutable `.kts` writer state.
- Honest partial integration: segments contain frames only. Capture-gap persistence remains explicitly unsupported until the dependent SQLite metadata feature wires it; index commits, recovery, retention, and range resolution are not claimed here.
- Discrepancies from design: `sealed_observed` uses the last accepted frame's observed time because the settled `SegmentWriter::open(config)` contract carries no monotonic clock; the writer does not fabricate a timestamp from an unrelated clock. Size rotation follows the specified pre-append current-length rule, allowing one frame record past the threshold before the next append rotates.
- Simplification: the production unavailable recording placeholder is removed; consumers share one core address type, one codec/scanner, and one frame-only writer.
- Adjacent issues parked: none.
