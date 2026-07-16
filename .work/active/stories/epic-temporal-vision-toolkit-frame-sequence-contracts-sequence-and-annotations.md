---
id: epic-temporal-vision-toolkit-frame-sequence-contracts-sequence-and-annotations
kind: story
stage: done
tags: [visual]
parent: epic-temporal-vision-toolkit-frame-sequence-contracts
depends_on: [epic-temporal-vision-toolkit-frame-sequence-contracts-frame-and-geometry]
release_binding: 1.0.0
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Validate Ordered Sequences and Explicit Annotations

## Checkpoint

Implement `crates/temporal-vision/src/sequence.rs` around immutable `FrameSequence<FrameId, MarkerId, GapId, Pixels>`, `Marker<Id>`, `DeclaredGap<Id>`, and inclusive `TimeRange`.

`FrameSequence::new` accepts complete vectors and never sorts or repairs them. Require at least one frame; equality-unique frame IDs; nondecreasing frame timestamps; and common dimensions/pixel format. Equal timestamps preserve exact caller order and IDs never act as hidden tie-breakers. Require nonempty marker kind/label and gap reason strings, equality-unique marker/gap IDs, nondecreasing annotations inside the first/last frame range, and non-overlapping gap ranges. Adjacent gaps may share a boundary. Do not infer gaps from timestamps, IDs, or ordinal arithmetic.

The optional `FrameRegion` must match the common geometry and the optional `BinaryMask` must cover the complete frame dimensions. A borrowed sequence holds borrowed pixels without copying; owned conversion is explicit. Keep uniqueness checks linear so callers do not need `Hash`, `Ord`, `Debug`, or serde bounds solely to analyze frames.

## Acceptance evidence

- Empty, duplicate-ID, out-of-order, mixed-dimension/format, annotation-out-of-range/order, overlapping-gap, invalid-region, and mismatched-mask input fails with stable errors and useful index context.
- Tied frames and markers remain in insertion order through iteration and serialization.
- A valid borrowed sequence performs no pixel copy and can be explicitly converted to owned data.
- Gaps remain declared evidence and divide later calculations; no continuity is inferred.
- Constructors and validated deserialization enforce the same invariants.

## Ordering

Depends on `epic-temporal-vision-toolkit-frame-sequence-contracts-frame-and-geometry`. The authoritative sequence projection must exist before provenance can be built without duplicated caller metadata.

## Implementation notes

- Execution capability: highest/raised (caller-selected) for the authoritative ordering and annotation aggregate.
- Review weight: standard (caller/autopilot).
- Files changed: `crates/temporal-vision/src/sequence.rs` and `crates/temporal-vision/src/lib.rs`.
- Tests added: tied-order preservation, duplicate/out-of-order rejection, touching-gap acceptance, and sequence-region compatibility.
- Simplification: linear equality checks avoid extra ID trait bounds; sequence conversion owns pixels only when explicitly requested.
- Discrepancies from design: added `Timestamp`-consistent `pixel_format()` and explicit `FrameSequence::to_owned()` accessors to satisfy the stated consumer contract; no semantic deviation.
- Adjacent issues parked: none.
- Verification: `cargo test -p temporal-vision --lib --locked` passed (6 tests).
