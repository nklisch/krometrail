---
id: epic-durable-browser-memory-sqlite-index-indexed-recording
kind: story
stage: done
tags: [storage, browser]
parent: epic-durable-browser-memory-sqlite-index
depends_on: [epic-durable-browser-memory-sqlite-index-timeline-catalog]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Indexed Recording and Address-Backed Frame Reads

## Checkpoint

Turn `SegmentWriter` into a payload primitive reporting `FrameWriteCommit` and sealed/open `SegmentRegistration` values, then add `IndexedRecordingSink` in `crates/krometrail-store/src/recording.rs`. Under one mutation gate, append and flush the complete CRC-guarded segment record first, then atomically upsert segment/frame/generic-frame metadata. `append_gap` routes directly to structured SQLite persistence and never touches segment bytes. Remove the raw segment writer's `RecordingSink` implementation and its temporary `Unsupported` gap path.

Implement `FrameSource` in `index/{frames,segments}.rs`. Add `segments::read_frame_from<R: Read + Seek>` so id/range reads query addresses, release SQLite, seek only the record, verify CRC and identity/context, and tolerate the `.open`→`.kts` rename race. Id reads preserve request order; range reads use capture ordinal.

## Ordering

Depends on the working catalog/timeline/gap adapter. This is the load-bearing cross-resource ordering checkpoint.

## Acceptance evidence

- No SQLite frame/timeline claim occurs before the complete segment append returns.
- One immediate transaction contains segment registration, frame metadata/address, and generic frame observation.
- Fault injection after segment append but before SQL commit leaves a readable orphan record and zero frame/timeline rows; the opposite dangling-claim state is impossible through the facade.
- Rotation and session flush publish correct sealed registrations; SQL failure prevents successful append/flush reporting.
- Gap append writes zero segment bytes and is immediately queryable losslessly.
- Open/sealed reads by ids and range use `FrameAddress`, bounded seeks, shared CRC decoding, context validation, and deterministic order.
- Concurrent targets preserve append→index order without lock inversion; locked workspace gates pass.

## Implementation notes

- `SegmentWriter` is now a frame-payload primitive that reports active/sealed registrations with each complete append and every session flush; it no longer implements the domain recording port or owns a gap stub.
- `IndexedRecordingSink` serializes mutations, awaits the bounded segment worker first, then commits segment/frame/generic-frame metadata in one immediate SQLite transaction. The only cross-resource failure asymmetry is a recoverable orphan payload.
- Structured gap persistence bypasses segment bytes and uses the atomic gap/timeline adapter.
- `FrameSource` resolves stored addresses under the SQLite lock, releases it, performs one bounded seek/read through the shared CRC codec, retries the sealed name across rename races, and validates frame/session/target identity.
- Tests cover open and sealed reads, id request order, capture-ordinal range order, rotation metadata, concurrent targets, lossless gap routing, missing ids, and a forced post-append SQL failure with a directly readable orphan and no dangling index claim.
- Verification: 30 store tests passed; store Clippy passed with warnings denied.
