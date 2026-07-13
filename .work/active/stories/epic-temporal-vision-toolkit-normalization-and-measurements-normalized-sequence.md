---
id: epic-temporal-vision-toolkit-normalization-and-measurements-normalized-sequence
kind: story
stage: done
tags: [visual]
parent: epic-temporal-vision-toolkit-normalization-and-measurements
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Normalize One Geometry Epoch to Opaque Linear RGB16

## Checkpoint

Implement `crates/temporal-vision/src/normalize.rs` and wire its public exports through `src/lib.rs`. Accept only the existing validated `FrameSequence` of decoded, tightly packed, straight-alpha sRGB RGBA8 frames. Require a caller-declared sRGB `Rgb8` background and normalize every frame into one owned, tightly packed, opaque linear RGB16 working representation.

Use a checked-in 256-entry IEC sRGB-to-linear16 lookup table. Composite each channel in linear light as `(source * alpha + background * (255 - alpha) + 127) / 255`. Apply an optional half-open source-coordinate crop before one whole-number scale. Upscale by nearest-neighbor replication; downscale by exact non-overlapping box average, requiring crop dimensions divisible by the factor. Factors are limited to 1–8 and factor one is canonical identity. Do not interpolate, pad, register, stretch, or combine incompatible geometry epochs.

Transform the sequence region/mask as an analysis domain rather than an implicit crop. Intersect region, mask, and explicit crop; replicate membership when upscaling and include a downscaled output pixel only when every source pixel in its box is included. Reject an empty transformed domain.

Extend the stable error registry with `InvalidScale`, `EmptyAnalysisDomain`, and `ResourceLimitExceeded`. Validate frame count, output pixels per frame, retained RGB16 bytes, optional mask bytes, and all arithmetic before allocating. Default limits are 4,096 frames, 16,777,216 pixels per output frame, and 512 MiB retained output.

Retain the source dimensions/crop, gap ranges, transformed mask, and ordered `NormalizationStep`s in `NormalizedSequence`. Provenance always records color conversion and alpha compositing, then explicit crop and non-identity scaling when present, with exact versions, background, dimensions, factor/kernel, and conservative mask policy.

## Acceptance evidence

- Repeated normalization yields identical packed RGB16 buffers and ordered provenance.
- Exact tiny fixtures protect transparent, opaque, and partial-alpha composition against a non-black background.
- Crop-before-scale, nearest-neighbor upscaling, divisible box downscaling, transformed region/mask behavior, and empty-domain rejection are deterministic.
- Out-of-bounds crop, unsupported/non-divisible scale, checked overflow, and configured/default resource limits fail before output allocation.
- Input remains one validated geometry epoch; no codec, browser, filesystem, async, plugin, GPU, registration, or inferred transform is introduced.

## Ordering

This is the first checkpoint. It establishes the prepared pixels and analysis domain consumed by the direct measurement kernel.

## Implementation notes

- Execution capability: raised/high, selected by the autopilot caller because every downstream artifact consumes these deterministic pixels.
- Review weight: standard (caller); child stories close on verification and the parent feature remains the review boundary.
- Files changed: `crates/temporal-vision/src/normalize.rs`, `src/error.rs`, and `src/lib.rs`.
- Tests added: exact LUT sentinels/checksum, alpha composition, crop/upscale, conservative downscale-domain, scale overflow, and retained-byte bounds.
- Verification: focused library and existing public-contract tests pass; normalization remains browser-free and uses only existing dependencies.
- Simplification: one fixed transform order and one owned RGB16 representation; no configurable pipeline, codec, async, GPU, plugin, registration, or inferred-analysis layer.
- Discrepancies from design: none.
- Adjacent issues parked: none.
