---
id: epic-temporal-vision-toolkit-storyboard-rendering
kind: story
stage: done
tags: [visual]
parent: epic-temporal-vision-toolkit-storyboard
depends_on: [epic-temporal-vision-toolkit-storyboard-selection]
release_binding: 1.0.0
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Render and Encode Storyboard Artifacts

## Checkpoint

Add the public encoded-result contract in `crates/temporal-vision/src/artifact.rs`; bounded layout/raster composition in `src/render.rs` and `src/render/{canvas,font}.rs`; deterministic PNG plus SHA-256 in `src/encode.rs`; a crate-private linear16-to-sRGB8 inverse helper in `src/normalize.rs`; and explicit exports/dependencies in `src/lib.rs`, the workspace `Cargo.toml`, and the crate manifest.

Expose `EncodedImage`, `GeneratedArtifact`, `ArtifactLabels`, `RenderLimits`, `StoryboardParameters`, `StoryboardArtifacts`, and `generate_storyboard`. One call renders the storyboard and an optional before/during/after artifact from one `StoryboardSelection`. Use algorithm `temporal-storyboard` version `1.0.0` for both artifact kinds.

Render a left-to-right strip with equal 240 px preferred / 160 px minimum contain-fit tiles, source aspect ratio preserved, and no border or annotation over source pixels. Keep title/source context, session-relative time, signed anchor offset, source-frame label, selection reasons, assigned marker labels, gap warnings, and `TIME →` in separate high-contrast bands. Assign markers to the first selected tile at or after their timestamp. Show every intersecting gap with text and hatch pattern. Use the checked-in 6×10 printable-ASCII bitmap atlas, deterministic escaped UTF-8, and visibly marked bounded ellipsizing; exact text remains in the manifest.

Convert normalized linear RGB16 to nearest sRGB8 with ties lower and record `linear16-to-srgb8-v1`. Scale through checked integer nearest-neighbor center mapping. Default render limits are 4096×4096, 64 MiB canvas, and 64 MiB encoded bytes; reject before or during allocation/writing when exceeded. Encode fixed RGB8 PNG with no time/text chunks and fixed filter/compression through a bounded writer, then hash the exact returned bytes.

Construct manifests after encoding from the source sequence and the exact render plan. Include source/selected IDs and counts, markers, gaps, ordered normalization + threshold + display conversion, algorithm/version, anchor, tile limit, role mappings, selection reasons and omissions, text/layout/font/scale/PNG settings, output dimensions, and output hash. Visible labels must be derived from the same values.

## Acceptance evidence

- Storyboard and optional orientation reuse one selection, preserve aspect ratio, identify exact source panels, and keep all annotation outside source pixels.
- Title/source, time/offset, IDs, reasons, markers, `TIME →`, and text-plus-pattern `GAP` warnings are visible and agree with provenance.
- Orientation labels exact before/during/after source frames and the during rule/fallback; it never generates or averages content.
- Identical input produces identical canvas, PNG bytes, hash, parameters, and manifest on supported platforms.
- Width/height, minimum tile width, canvas bytes, and encoded bytes are checked; limit failure returns no partial artifact.
- Rendering has no host-font, locale, UI toolkit, scene graph, filesystem, browser, runtime, GPU, strategy, or codec-plugin dependency.

## Ordering

Depends on `epic-temporal-vision-toolkit-storyboard-selection`. The render consumes its exact plan and must not reselect or reinterpret frames.

## Implementation notes

- Added one checked RGB8 canvas, embedded deterministic 5×7-in-6×10 ASCII raster, bounded PNG 0.17.16 encoder, and exact SHA-256 hashing without a renderer/plugin/UI/filesystem abstraction.
- `generate_storyboard` renders the chronological storyboard and optional before/during/after composite from one selection plan; annotations stay outside source pixels and show title/context, time/offset, frame IDs, reasons, markers, time direction, and textual/patterned gap warnings.
- Layout preserves aspect ratio through integer contain-fit and center-mapped nearest-neighbor scaling. Width, height, minimum 160 px panel width, RGB canvas bytes, and encoded bytes fail with `ResourceLimitExceeded` before a partial result escapes.
- Both manifests are built after encoding from authoritative sequence metadata and record algorithm `temporal-storyboard` `1.0.0`, selection/role/omission data, marker buckets, normalization and display conversion, text/layout/font/PNG choices, dimensions, and exact output hash.
- Verification: package formatting, locked all-target tests (29 passed), and locked package Clippy with warnings denied passed.
