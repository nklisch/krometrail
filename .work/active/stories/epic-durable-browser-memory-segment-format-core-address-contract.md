---
id: epic-durable-browser-memory-segment-format-core-address-contract
kind: story
stage: implementing
tags: [storage, browser]
parent: epic-durable-browser-memory-segment-format
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Core Frame-Address Contract and RecordingSink Evolution

## Brief

Publish the `FrameAddress` / `ByteOffset` contract in `krometrail-core` and evolve `RecordingSink::append_frame` to return `Result<FrameAddress>`. This is the foundational contract slice for the whole epic: every downstream feature imports `FrameAddress`, and the live capture stream must be able to hand the durable address of each frame to the future index layer. No segment bytes are written in this story — it is the type-and-port contract only.

## Parent context

- Parent feature: `epic-durable-browser-memory-segment-format`
- This is Unit 1 of three. It unblocks Unit 2 (codec) and Unit 3 (writer + wiring).

## Scope

**In scope:**
- `FrameAddress { segment_id: SegmentId, byte_offset: ByteOffset }` and `ByteOffset(u64)` in a new `crates/krometrail-core/src/recording/address.rs`, re-exported from `recording/mod.rs` and the crate root.
- `RecordingSink::append_frame` widens its return type from `Result<()>` to `Result<FrameAddress>` in `crates/krometrail-core/src/ports/recording.rs`. `append_gap` and `flush` are unchanged.
- Update every test fake that implements `RecordingSink` so the workspace compiles:
  - `FakeRecording` in `crates/krometrail-core/src/ports/mod.rs` — `append_frame` returns a synthesized `FrameAddress` (monotonic `byte_offset` counter against a fixed `SegmentId`); the `recording_port_separates_frames_gaps_and_flush` test asserts the address is populated.
  - `ShutdownTestSink` in `crates/krometrail-cdp/src/session.rs`.
  - `TestSink` in `crates/krometrail-cdp/src/capture/tests.rs`.
  - `TestSink` in `crates/krometrail-cdp/tests/capture_real.rs`.
  - `TestSink` in `crates/krometrail-cdp/tests/cross_platform_smoke.rs`.
- Mechanical update to the CDP capture pipeline's `append_frame` match arm at `crates/krometrail-cdp/src/capture/pipeline.rs` (~line 801): `Ok(())` → `Ok(_addr)`. The address is discarded by the pipeline (the index layer that consumes it does not exist yet); no pipeline behavior changes.
- A core contract test: `FrameAddress` and `ByteOffset` round-trip through serde; `ByteOffset::new(0)` is valid (first record sits right after the segment header); the type is `Copy`.

**Non-goals:**
- The segment binary format, codec, checksums, sealed footer (Unit 2).
- The `SegmentWriter` adapter, rotation, flush, real writes, composition wiring (Unit 3).
- Any change to `append_gap` semantics, `CaptureGap`, or gap routing (deferred to sqlite-index per the feature design).
- Consuming the returned `FrameAddress` anywhere in the capture pipeline (that wiring lands with sqlite-index).

## Files

- `crates/krometrail-core/src/recording/address.rs` (new)
- `crates/krometrail-core/src/recording/mod.rs` (extend — re-export)
- `crates/krometrail-core/src/lib.rs` (extend — pub-use)
- `crates/krometrail-core/src/ports/recording.rs` (evolve signature)
- `crates/krometrail-core/src/ports/mod.rs` (extend `FakeRecording`, update its test)
- `crates/krometrail-cdp/src/capture/pipeline.rs` (mechanical match-arm widen)
- `crates/krometrail-cdp/src/session.rs` (mechanical — `ShutdownTestSink`)
- `crates/krometrail-cdp/src/capture/tests.rs` (mechanical — `TestSink`)
- `crates/krometrail-cdp/tests/capture_real.rs` (mechanical — `TestSink`)
- `crates/krometrail-cdp/tests/cross_platform_smoke.rs` (mechanical — `TestSink`)

## Acceptance criteria

- [ ] `FrameAddress { segment_id: SegmentId, byte_offset: ByteOffset }` and `ByteOffset(u64)` exist in `krometrail-core`, derive `Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize`, round-trip through serde, and are re-exported from the crate root.
- [ ] `ByteOffset::new(0).get() == 0` (zero is a valid offset — the first record sits immediately after the segment header).
- [ ] `RecordingSink::append_frame` returns `Result<FrameAddress>`; `append_gap` and `flush` signatures are byte-identical to before.
- [ ] Every `RecordingSink` test fake compiles and returns a populated `FrameAddress` (non-default `segment_id`, monotonically increasing `byte_offset` per fake).
- [ ] The CDP capture pipeline's `append_frame` match arm compiles (`Ok(_addr)`); no pipeline behavior changes.
- [ ] The `core_ports_have_no_runtime_or_transport_types` source-scanner test in `ports/mod.rs` still passes.
- [ ] `cargo fmt --all --check`, `cargo check --workspace --all-targets --locked`, `cargo test --workspace --all-targets --locked`, `cargo clippy --workspace --all-targets --locked -- -D warnings` pass.
