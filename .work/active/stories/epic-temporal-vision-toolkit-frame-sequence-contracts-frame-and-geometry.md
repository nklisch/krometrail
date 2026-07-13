---
id: epic-temporal-vision-toolkit-frame-sequence-contracts-frame-and-geometry
kind: story
stage: implementing
tags: [visual]
parent: epic-temporal-vision-toolkit-frame-sequence-contracts
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Establish Decoded Frame and Pixel-Geometry Contracts

## Checkpoint

Create the infrastructure-neutral validation boundary and the crate's exact decoded-image vocabulary in `crates/temporal-vision/src/{lib,error,frame,geometry}.rs`.

The public frame is `Frame<Id, Pixels>` with caller-owned ID type and `Pixels: AsRef<[u8]>`. Publish `OwnedFrame<Id> = Frame<Id, Box<[u8]>>` and `BorrowedFrame<'a, Id> = Frame<Id, &'a [u8]>`; both pass through the same constructor. A frame contains only its ID, a caller-clock `Timestamp` in nanoseconds, nonzero `PixelDimensions`, registry-backed `PixelFormat::Rgba8SrgbStraight`, and its pixels. RGBA8 is tightly packed row-major R/G/B/A, sRGB color channels, and straight alpha. Validate exact checked `width * height * 4` length.

Define source-frame pixel geometry with upper-left origin, right/down axes, and half-open rectangles. `PixelRect` uses checked exclusive bounds; `FrameRegion` must fit within frame dimensions. `BinaryMask` is full-frame, row-major, MSB-first, one bit per pixel, exact `ceil(pixel_count / 8)` byte length, with zero unused trailing bits. A region and mask later combine by intersection.

Add the stable `VisionError`/`ErrorCode` boundary and the crate-local enum registry macro. Errors may report a collection index but must not render generic IDs, pixels, paths, or private sources. Public invariant-bearing deserialization reuses constructors.

## Acceptance evidence

- `OwnedFrame` and `BorrowedFrame` expose identical metadata and bytes without codecs, trait objects, async, filesystem, browser, or Krometrail dependencies.
- Zero or overflowing dimensions and every non-exact RGBA8 payload length fail with stable codes.
- Rectangle overflow/out-of-bounds and mask length/padding errors fail deterministically; valid masks return deterministic pixel membership.
- Pixel format and error-code stable names, display, and serde round-trip from their single declarations.
- Focused tests protect constructors and malformed deserialization, not trivial accessors.

## Ordering

This is the first checkpoint. It establishes the types required by ordered sequences and provenance.
