---
id: epic-temporal-vision-toolkit-region-filmstrip-rendering
kind: story
stage: done
tags: [visual]
parent: epic-temporal-vision-toolkit-region-filmstrip
depends_on: [epic-temporal-vision-toolkit-region-filmstrip-region-plan]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Render, Encode, and Manifest Region Filmstrips

## Checkpoint

Implement deterministic filmstrip rendering in `crates/temporal-vision/src/filmstrip.rs`, using the shared artifact and encoding seam (`artifact.rs`, `encode.rs`, and crate-private render helpers) when present, or introducing that same minimal shared seam if this feature lands first.

Expose `RegionFilmstripLabels`, `RegionFilmstripRenderLimits`, `RegionFilmstripParameters`, `RegionFilmstripArtifact`, and `generate_region_filmstrip`.

`generate_region_filmstrip` starts from a validated `FrameSequence`, calls the existing normalization pipeline with full-frame identity crop/scale and the caller-declared `Rgb8` background, then renders from normalized opaque linear RGB16 pixels. Region crop, padding, and display scale are filmstrip transformations recorded after normalization. Use a separate declared padding color for generated missing-pixel areas.

Render one combined PNG: title/source/gap warning header, full-frame locator panel, chronological crop strip, and bottom time-direction band. Keep annotations outside source pixels. Every crop tile shows session-relative timestamp, signed anchor offset, source-frame ID, explicit padding/`OUTSIDE SOURCE` labeling when applicable, and consistent scale. Locator rendering clips the region outline to the source image and uses visible edge chevrons for out-of-bounds portions. Declared gaps appear in the header and as text-plus-pattern separators between neighboring rendered tiles when their closed timestamp interval intersects a gap.

Construct an `ArtifactManifest` with `ArtifactKind::RegionFilmstrip`, `EvidenceClass::SourceDerived`, and `AlgorithmDescriptor::new("region-filmstrip", "1.0.0")` after encoding so `output_hash` covers the exact bytes. Deterministic manifest parameters must include original region definition, coordinate mapping, resolved source rectangle, selected frame indices, omitted count, locator index, padding color, display scale, tile dimensions, gap warnings, label truncation policy, output layout, PNG settings, and `tracking_method: none`. Use manifest `region` only for an in-bounds fixed source-image rectangle; otherwise rely on the richer parameter record.

## Acceptance evidence

- Locator and chronological strip render into one source-derived PNG with annotation separate from source pixels.
- Every tile uses the same declared crop size and display scale, labels timestamp/anchor offset/source ID, and marks padding visibly.
- Fixed viewport regions render through the recorded source-pixel mapping and do not claim DOM, scroll, node-reference, or logical-element tracking.
- Declared gaps are visible and machine-recorded; no interpolation or stability claim crosses missing evidence.
- Repeated identical input produces identical plan, canvas, PNG bytes, SHA-256, parameters, and manifest.
- Width/height, canvas bytes, encoded bytes, source-frame count, and scale constraints fail explicitly without partial artifacts.
- The implementation adds no UI engine, host-font dependency, browser/CDP/MCP dependency, decode path, filesystem sink, async runtime, GPU path, cache, or inferred-analysis type.

## Ordering

Depends on `epic-temporal-vision-toolkit-region-filmstrip-region-plan`. Rendering consumes the plan and must not reinterpret coordinates or perform tracking.

## Implementation notes

- Added bounded full-frame normalization, exact fixed-region crop/padding scaling, full-frame locator rendering, clipped outlines and edge chevrons, wrapped chronological tiles, explicit gap hatches/text, timestamp/anchor/source labels, and deterministic PNG/SHA-256 output.
- Reused the checked RGB8 canvas, embedded bitmap font, PNG encoder, `EncodedImage`, and `GeneratedArtifact` seam already established by storyboard/difference-map.
- Extended the manifest constructor internally so filmstrips can honestly set `region` only for an in-bounds fixed source-image rectangle rather than inheriting an unrelated sequence analysis region.
- Added a configurable source-frame ceiling through `with_max_source_frames`; processing memory, raster dimensions, canvas bytes, and encoded bytes are bounded before artifact return.
- Filmstrip transformation records include display conversion, fixed crop/padding, optional integer scale, locator/layout/text/PNG parameters, and `tracking_method: none`; locator/layout/text remain manifest parameters rather than being mislabeled as normalization operations.
- Downscaling uses a recorded non-overlapping sRGB8 box average over the complete padded region; upscaling uses nearest-neighbor replication.
- Verification: `cargo check -p temporal-vision --all-targets --locked`; `cargo test -p temporal-vision --locked` (38 passed).
