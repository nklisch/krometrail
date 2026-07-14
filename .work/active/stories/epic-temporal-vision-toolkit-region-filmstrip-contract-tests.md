---
id: epic-temporal-vision-toolkit-region-filmstrip-contract-tests
kind: story
stage: done
tags: [visual]
parent: epic-temporal-vision-toolkit-region-filmstrip
depends_on: [epic-temporal-vision-toolkit-region-filmstrip-rendering]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Verify Public Region Filmstrip Contract

## Checkpoint

Add public integration coverage in `crates/temporal-vision/tests/filmstrip.rs` and focused colocated tests in `src/filmstrip.rs` plus shared render/encode helpers only where private arithmetic needs direct coverage.

Use browser-free typed-ID RGBA8 frame sequences with hand-checkable small dimensions, markers, and declared gaps. Cover fixed source-image regions, fixed viewport regions with rational scaling, negative and beyond-edge rectangles, fully outside rectangles, deterministic tile thinning, locator selection, padding color, downscale divisibility rejection, tiny render/processing limits, and manifest determinism.

Tests should verify visible metadata through deterministic canvas/glyph layout or decoded pixel checks rather than OCR or large golden image snapshots. Include one tiny stable PNG hash to protect byte determinism, not a corpus of binary fixtures. Confirm normal dependencies remain free of Krometrail, CDP, MCP, browser, UI toolkit, host-font discovery, filesystem, async runtime, GPU, and image-decoder dependencies beyond the shared deterministic PNG encoder.

## Acceptance evidence

- A public caller can generate a region filmstrip from a `FrameSequence` with arbitrary typed IDs and trace every rendered crop to source-frame IDs.
- Source-image and viewport coordinates record distinct semantics and reproduce expected crop/padding plans.
- Partially and fully out-of-bounds regions visibly use padding and never claim missing pixels as source observations.
- Gap warnings, timestamps, signed anchor offsets, locator outline/edge chevrons, selected IDs, omitted count, and `tracking_method: none` are present and manifest-aligned.
- Repeated generation yields identical PNG bytes/hash/manifest for identical source pixels and parameters.
- Invalid viewport mapping, invalid scale, tiny frame/canvas/encoded limits, and impossible region dimensions fail explicitly without large allocations.
- Package/workspace format, locked check, test, and clippy gates pass, with concurrent unowned-file interference reported rather than edited.

## Ordering

Depends on `epic-temporal-vision-toolkit-region-filmstrip-rendering`. This checkpoint verifies the public source-derived evidence contract and should advance directly to `done` once green.

## Implementation notes

- Added `crates/temporal-vision/tests/filmstrip.rs` with arbitrary typed IDs and browser-free exact cases for source-image and rational 2× viewport regions, negative/partial/fully outside geometry, thinning, locator choice, gaps, signed offsets, padding color/hatching, locator chevrons, deterministic bytes/hash/manifest, and honest `tracking_method: none` provenance.
- Boundary coverage rejects contradictory viewport mappings, non-divisible downscales, oversized layout/canvas/PNG requests, source-frame limits, and invalid tile-limit deserialization before artifact return.
- One tiny stable PNG hash protects the deterministic shared encoder seam; decoded-pixel checks protect source/padding separation and visible warning/glyph bands without binary fixtures or OCR.
- Normal dependencies remain unchanged and browser/CDP/MCP/UI/filesystem/runtime/GPU/decoder dependencies were not introduced; `png` remains a dev dependency for decoded-pixel assertions.
- Verification: format/check/test/clippy package gate passed; 41 temporal-vision tests passed across 7 suites.
