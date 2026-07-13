---
id: epic-temporal-vision-toolkit-motion-history-public-contract-tests
kind: story
stage: implementing
tags: [visual]
parent: epic-temporal-vision-toolkit-motion-history
depends_on: [epic-temporal-vision-toolkit-motion-history-rendering]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Motion-History Public Contract and Useful Render Tests

## Scope

Add `crates/temporal-vision/tests/motion_history.rs` protecting the public motion-history
contract end-to-end, plus only the focused colocated private-mechanics tests that earn their
place in `src/motion_history.rs`.

## Public integration fixture

Build a browser-free typed-ID sequence exercising: a translated bright block (decaying
trail), a repeated-traversal pixel changed in two separated segments, one stable interval,
a declared gap that partitions accumulation, an analysis-mask exclusion, and tied timestamps.
Generate the plan and the rendered artifact from normalized pixels.

Assert:

- Exact accumulation values at hand-picked pixels (rank-0 single change, halving at
  `half_life_ranks`, zero at `live_window`, saturating repeated traversal, max-composite
  across the gap).
- Exact `ever_changed` and `outline` masks and `changed_pixel_count`; segment/gap counts.
- Deterministic PNG signature/hash for a tiny fixed raster (committed hash, not a large
  golden image).
- Manifest round trip; `selected_frame_ids == [reference]`;
  `omitted_frame_count == source_frame_count − 1`.
- Visible disclaimer, legend labels, start/end timestamps, and gap warning verified by
  private glyph-layout evidence without OCR or fragile full-image snapshots.

## Focused colocated tests (only where private mechanics need direct coverage)

- Decay curve at boundary ranks; saturating accumulation; max-across-segments compositing.
- Gap-segmentation reset; `ever_changed` survives decay fade-out.
- 4-connectivity outline of corner/edge/interior regions; isolated changed pixel.
- Analysis-mask exclusion from accumulation and outline.
- Checked layout arithmetic (tiny width/height/canvas/encoded limits fail explicitly).

## Acceptance evidence

- A browser-free consumer with arbitrary typed IDs produces a `MotionHistory` source-derived
  artifact through the public API and can trace rendered intensity, outline, and reference
  backdrop to source frame IDs and manifest parameters.
- Repeated tiny render has identical bytes/hash/manifest; decoded PNG dimensions and selected
  pixel colors match the source-derived canvas.
- Tiny limits fail explicitly; tests never allocate near production maxima.
- Normal dependencies remain browser/Krometrail/runtime/UI/font/filesystem/GPU-free and add
  only the shared bounded PNG encoding plus SHA-256 already established by the sibling render
  seam; motion-history introduces no new dependency.
- `cargo fmt -p temporal-vision -- --check`,
  `cargo check -p temporal-vision --all-targets --locked`,
  `cargo test -p temporal-vision --locked`, and
  `cargo clippy -p temporal-vision --all-targets --locked -- -D warnings` pass, with any
  concurrent unowned-file interference reported rather than edited.

## Out of scope

No getter/derive matrix, giant golden image, exhaustive glyph test, exhaustive
decay-parameter sweep, duplicate `FrameSequence` validation, codec-library conformance suite,
browser fixture, visual-diagnosis assertion, inferred-direction assertion, or
benchmark-success claim.
