---
id: epic-temporal-vision-toolkit-difference-map-panel-rendering
kind: story
stage: done
tags: [visual]
parent: epic-temporal-vision-toolkit-difference-map
depends_on: [epic-temporal-vision-toolkit-difference-map-change-accumulation]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Three-Panel Layout, Assembly, and Manifest

## Checkpoint

Assemble the bounded accumulation into one deterministic three-panel composite image (reference, change frequency, change timing) with fixed layout, legends, time labels, repeated-change indicator, and a visible gap warning, then project it into a reproducible `ArtifactManifest` and the public `render_difference_map` entry point.

## Files

- `crates/temporal-vision/src/difference_map.rs` (continued: layout, panel drawing, manifest, public entry point)
- `crates/temporal-vision/src/lib.rs` (explicit exports for `DifferenceMapArtifact` and `render_difference_map`)

## Surface (exact signatures)

```rust
pub(crate) struct DifferenceMapLayout {
    image: PixelDimensions,
    header: PixelRect,
    reference_panel: PixelRect,
    frequency_panel: PixelRect,
    timing_panel: PixelRect,
    reference_label: PixelRect,
    frequency_label: PixelRect,
    timing_label: PixelRect,
    legend: PixelRect,
}
impl DifferenceMapLayout {
    pub(crate) fn new(panel: PixelDimensions) -> Result<Self>;
}

pub struct DifferenceMapArtifact<ArtifactId, FrameId, MarkerId, GapId> {
    manifest: ArtifactManifest<ArtifactId, FrameId, MarkerId, GapId>,
    rendered: RenderedArtifact,
}
impl<A, F, M, G> DifferenceMapArtifact<A, F, M, G> {
    pub fn manifest(&self) -> &ArtifactManifest<A, F, M, G>;
    pub fn rendered(&self) -> &RenderedArtifact;
}

pub fn render_difference_map<A, F, M, G, P>(
    artifact_id: A,
    sequence: &FrameSequence<F, M, G, P>,
    normalized: &NormalizedSequence<F>,
    parameters: DifferenceMapParameters,
) -> Result<DifferenceMapArtifact<A, F, M, G>>
where
    F: Clone + Eq,
    M: Clone + Eq,
    G: Clone + Eq,
    P: AsRef<[u8]>;
// A carries the same bounds `ArtifactManifest`'s first type parameter requires
// (currently none beyond what the caller supplies).
```

## Layout (fixed, all checked arithmetic)

Constants in this module: outer margin and inter-panel gap `16`, header height `56`, panel label height `28`, legend height `120`, section gap `12`.

- `image.width  = 2·MARGIN + 3·panel.width  + 2·INTER_PANEL_GAP`
- `image.height = 2·MARGIN + HEADER + SECTION_GAP + LABEL + panel.height + SECTION_GAP + LEGEND`

Every rectangle is a pure function of `panel` plus these constants.

## Rendering steps

1. Validate `parameters.reference_frame_index < normalized.frames().len()` and that `normalized.dimensions()` and frame count agree with `sequence`; otherwise `InvalidParameter`.
2. Build `DifferenceMapData` via `DifferenceAccumulators::accumulate`. Verify the layout canvas RGBA byte length `≤ limits.max_output_bytes`.
3. Allocate `Canvas` filled with `background`.
4. Draw the header band: `TEMPORAL DIFFERENCE MAP`, range start/end offsets, and a `TIME →` direction indicator.
5. Draw the reference panel from `normalized.frames()[reference_frame_index].linear_rgb16()` via the integer luminance kernel → opaque grayscale RGBA8.
6. Draw the frequency panel from `DifferenceMapData::frequency_value` scaled by the active mode's image-wide maximum; render the mode-specific legend with the numeric maximum.
7. Draw the timing panel: for each non-repeated changed pixel map `timing_offset / range_duration` through the named palette (integer interpolation); repeated-change pixels get the fixed indicator color; unchanged pixels get `background`. Render the palette legend with numeric start, midpoint, and end offsets.
8. Render the repeated-change indicator swatch; render a visible `GAP` warning band iff `sequence.gaps()` has a gap intersecting the range.
9. Encode via `RenderedArtifact::encode_png`.
10. `normalization` = `normalized.normalization_steps()` ++ `[parameters.measurement.provenance_step()?]`.
11. `parameters` manifest block records `frequency_mode`, `time_palette`, effective separation, `reference_frame_index`, palette stop table, layout constants, and encoding format.
12. `ArtifactManifest::from_sequence(artifact_id, ArtifactKind::DifferenceMap, EvidenceClass::SourceDerived, AlgorithmDescriptor::new("temporal-difference-map", "v1")?, sequence, vec![reference_frame_id], normalization, parameters, layout.image, rendered.output_hash())`.
13. Return `DifferenceMapArtifact { manifest, rendered }`.

## Acceptance evidence

- Composite dimensions equal `DifferenceMapLayout`'s computed `image` and the manifest's `output_dimensions`.
- The reference panel is the grayscale luminance of the chosen reference frame; frequency and timing panels align pixel-for-pixel with it.
- Frequency brightness follows the active `FrequencyMode`; its scale maximum appears in the legend.
- The timing panel uses the named palette with numeric start/midpoint/end labels; repeated-change pixels use the indicator color; the legend lists the indicator and the effective separation.
- A visible gap warning appears iff a declared gap intersects the range; gap-crossing pairs contribute nothing to any panel.
- Identical inputs produce byte-identical PNG output and an identical manifest; the manifest's `selected_frame_ids` is exactly the reference frame and its counts, range, annotations, region, mask, normalization, and hash are internally consistent.

## Ordering constraints

Depends on `change-accumulation`. The public-contract-tests story depends on this one.

## Implementation notes

- Execution capability: raised/high; deterministic rendering and provenance form a public evidence contract.
- Review weight: standard (autopilot caller).
- Files changed: `crates/temporal-vision/src/difference_map.rs`, `crates/temporal-vision/src/render.rs`, `crates/temporal-vision/src/lib.rs`.
- Tests added/removed: no low-value image snapshot; existing package tests remain green and the public end-to-end regression belongs to the next checkpoint.
- Simplification: one fixed RGB8 three-panel layout reuses storyboard's canvas, font, encoder, `GeneratedArtifact`, and manifest construction path; no panel framework or duplicate artifact type was added.
- Discrepancies from design: the public `DifferenceMapArtifact` is a type alias for the shared `GeneratedArtifact`, so callers use `image()` and the manifest-owned output hash rather than a second `rendered()` wrapper. Canvas modules became crate-visible for reuse; storyboard behavior is unchanged.
- Adjacent issues parked: none.
