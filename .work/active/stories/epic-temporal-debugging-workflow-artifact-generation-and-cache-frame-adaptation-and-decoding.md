---
id: epic-temporal-debugging-workflow-artifact-generation-and-cache-frame-adaptation-and-decoding
kind: story
stage: done
tags: [visual, storage]
parent: epic-temporal-debugging-workflow-artifact-generation-and-cache
depends_on: [epic-temporal-debugging-workflow-artifact-generation-and-cache-artifact-schema-and-publication]
release_binding: 1.0.0
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Adapt Encoded Frames into Visual Epochs

## Checkpoint

Read the exact `ResolvedRange.frame_ids` once through the existing `FrameSource`, validate order/scope/metadata, force-decode current JPEG/PNG screencast formats, and produce deterministic `OwnedFrameSequence` values partitioned into maximal visual epochs. Preserve session time, tied capture order, straight PNG alpha, opaque JPEG alpha, declared gaps, and caller markers without inferring continuity or silently resizing.

Use exact `image = 0.25.9` with only JPEG/PNG features; its crate metadata declares Rust 1.85, while 0.25.10 requires 1.88. Reject unsupported precision/color outputs, orientation/profile transforms, metadata/decode dimension disagreement, malformed data, and decode bombs explicitly.

## Files

- root `Cargo.toml` and `Cargo.lock`
- `src/artifacts/{mod.rs,decode.rs,epoch.rs}` (new)
- `tests/fixtures/artifacts/{chrome-rgb.jpg,chrome-rgba.png,malformed.jpg,bomb-header.png}` (new bounded fixtures)
- focused adapter unit tests

## Acceptance evidence

- Real JPEG and PNG fixtures decode to exact expected RGBA8; JPEG alpha is 255 and PNG straight alpha is unchanged.
- Wrong declared format, malformed/truncated payloads, dimension mismatch, unsupported bit depth, overflow, and bomb headers fail before unbounded allocation.
- Mixed JPEG/PNG common geometry stays one epoch; image/viewport/exact device-scale changes split maximal contiguous epochs.
- Frame IDs/timestamps preserve resolved capture order, including ties.
- Gaps are clipped, sorted, and preserve IDs/reasons/estimated loss; markers preserve typed identity, labels, time, and equal-time declaration order.
- No second frame reader or decoded-frame cache is introduced.

## Ordering

Depends on the artifact store contract so source fingerprints and publication revalidation use one shape. The bounded service consumes these epoch inputs.

## Implementation notes

- Execution capability: highest; decoder allocation safety and exact provenance adaptation are cache-integrity boundaries.
- Review weight: standard from the autopilot caller; child checkpoints do not receive independent review.
- Files changed: root `Cargo.toml`/`Cargo.lock`, `src/artifacts/{mod.rs,decode.rs,epoch.rs,tests.rs}`, and `tests/fixtures/artifacts/{chrome-rgb.jpg,chrome-rgba.png,malformed.jpg,bomb-header.png}`.
- Tests added: exact real-fixture JPEG RGBA output with opaque alpha, exact straight-alpha PNG bytes, forced-format rejection, malformed/truncated and metadata-dimension rejection, 16-bit rejection, dimension/pixel/overflow/bomb limits, mixed-format common epochs, exact viewport/device-scale epoch splits, tied frame/marker ordering, clipped gap loss metadata, source mismatch/order loss, and cooperative cancellation.
- Verification: `cargo fmt --all`; root all-target check; root all-target tests (18 passed); root all-target Clippy with `-D warnings` (green).
- Decoder semantics: exact `image = 0.25.9` with JPEG/PNG features only; stored format selects the decoder; persisted dimensions/pixels/RGBA bytes are checked before decode and image crate width/height/allocation limits are installed; decoder-reported dimensions and 8-bit color type are checked before allocation; gray/gray-alpha/RGB/RGBA expand explicitly to straight RGBA8, with JPEG alpha forced opaque and PNG alpha copied unchanged. No format sniffing, EXIF orientation, profile transform, premultiplication, resize, or precision reduction occurs.
- Epoch semantics: returned frames must exactly match resolved IDs/order/session/target/time, capture ordinals strictly increase while tied session times remain ordered, and maximal epochs split only on image/viewport dimensions or exact device-scale bits. Gaps are only declared resolved gaps clipped inclusively per epoch; markers sort by time then caller declaration position.
- Simplification: adaptation reuses the existing `FrameSource` result shape and produces temporal-vision's exact owned sequence plus the core store source fingerprint; no reader or decoded cache was added.
- Discrepancies from design: decoder/adaptation limits are small internal value objects in this checkpoint and will be projected from the root `ArtifactWorkLimits` in the service checkpoint. Adapter modules remain test-compiled until that production service consumes them, preventing dead production scaffolding between sequential commits.
- Adjacent issues parked: none.
