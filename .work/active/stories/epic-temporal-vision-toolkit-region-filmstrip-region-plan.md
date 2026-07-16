---
id: epic-temporal-vision-toolkit-region-filmstrip-region-plan
kind: story
stage: done
tags: [visual]
parent: epic-temporal-vision-toolkit-region-filmstrip
depends_on: []
release_binding: 1.0.0
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Resolve Fixed Region Coordinates and Crop Plans

## Checkpoint

Implement the region-filmstrip planning contract in `crates/temporal-vision/src/filmstrip.rs` and export it through `src/lib.rs`.

Expose `RegionCoordinateSpace`, `SignedPixelRect`, `RationalScale`, `ViewportMapping`, `RegionDefinition`, `PaddingInsets`, `FilmstripTilePlan`, `RegionFilmstripPlan`, `FilmstripTileLimit`, and `plan_region_filmstrip`.

Support fixed source-image rectangles and fixed viewport rectangles only. Source-image coordinates are source-frame pixels. Viewport coordinates use caller-declared viewport dimensions and rational X/Y source-pixels-per-viewport-unit scales, converted to source pixels by outward rounding. Validate that the mapping matches the sequence's source dimensions; reject contradictions rather than guessing at device scale.

For each selected source frame, compute the intersection between the resolved signed source rectangle and the frame bounds. Preserve the declared region's logical width/height for every tile; represent missing edges with `PaddingInsets`. Fully out-of-bounds regions are valid all-padding tile plans with `source_rect: None`. The optional locator frame index must exist; when omitted, use the first selected frame at or after the anchor and then the first selected frame.

Tile selection is chronological and deterministic. Select all frames when they fit the limit; otherwise preserve first/final frames and fill remaining slots by integer temporal/source-order coverage, with earlier source index as the final tie-breaker. Record omitted frame count.

## Acceptance evidence

- Fixed source-image and fixed viewport definitions serialize deterministically and retain their coordinate-space labels.
- Viewport mapping uses explicit rational scales, outward rounding, and validation against source dimensions.
- Negative, partially beyond-edge, and fully outside regions produce exact source intersections and padding insets.
- Tile selection respects `FilmstripTileLimit` 1–24, stays chronological, preserves first/final under thinning, and records omitted count.
- Locator defaulting and explicit locator index validation are deterministic.
- The resulting plan contains all coordinate, padding, timestamp, frame-ID, and locator values needed by rendering; rendering does not recalculate region semantics or track logical elements.

## Ordering

First checkpoint. It establishes the evidence semantics that rendering and tests must consume without reinterpretation.

## Implementation notes

- Added the complete fixed source-image/viewport planning contract in `crates/temporal-vision/src/filmstrip.rs` and explicit exports in `src/lib.rs`.
- Viewport conversion uses signed integer floor/ceiling with exact rational scales and rejects mappings whose outward-rounded viewport extent differs from the source dimensions.
- Crop plans preserve signed declarations, exact visible intersections, explicit padding, anchor offsets, gap boundaries, locator choice, and deterministic source-order thinning.
- A one-tile limit selects the first frame when thinning because a single tile cannot preserve both distinct endpoints; limits of two or more preserve first and final.
- Verification: `cargo test -p temporal-vision --lib filmstrip --locked` (2 passed).
