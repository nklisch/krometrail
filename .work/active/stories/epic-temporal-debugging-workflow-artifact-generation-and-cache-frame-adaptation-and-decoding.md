---
id: epic-temporal-debugging-workflow-artifact-generation-and-cache-frame-adaptation-and-decoding
kind: story
stage: implementing
tags: [visual, storage]
parent: epic-temporal-debugging-workflow-artifact-generation-and-cache
depends_on: [epic-temporal-debugging-workflow-artifact-generation-and-cache-artifact-schema-and-publication]
release_binding: null
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