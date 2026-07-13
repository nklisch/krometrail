---
id: epic-temporal-vision-toolkit-difference-map
kind: feature
stage: implementing
tags: [visual]
parent: epic-temporal-vision-toolkit
depends_on: [epic-temporal-vision-toolkit-normalization-and-measurements]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Temporal Difference Map

## Brief

This feature renders a temporal difference map that shows where pixels changed during an interval and when those changes occurred.

The artifact contains three coordinated panels: a reference source frame for spatial context, a change-frequency panel showing how often each pixel or region changed, and a change-timing panel showing when observed changes occurred. The frequency panel records whether brightness represents count, magnitude, or normalized frequency. The timing panel uses a declared time palette with numeric start, midpoint, and end labels. Pixels that change repeatedly across widely separated moments receive a repeated-change indicator rather than a falsely precise single timestamp.

Thresholded change detection and gap handling are reused from the normalization-and-measurements feature. The output includes legends for frequency, timing, and repeated-change indicators, and a visible warning when the source interval contains declared capture gaps.

This feature does not diagnose why a region changed, track logical elements, or infer motion direction. It exposes spatial and temporal change patterns as a source-derived artifact.

## Epic context

- Parent epic: `epic-temporal-vision-toolkit`
- Position in epic: independent artifact feature — parallel to storyboard after measurements land

## Simplification opportunity

- Render the three panels into one combined image with a simple fixed layout rather than building a composable panel engine.
- Use the same thresholded pixel-difference metric as storyboard selection rather than introducing a separate change model.
- Keep the time palette small and deterministic; additional palettes can be added later without changing the core contract.

## Foundation references

- `docs/VISUAL-EVIDENCE.md` — Temporal Difference Map, Visual-Change Measurements, Capture Gaps
- `docs/EVALUATION.md` — Difference-map evaluation criteria

## Design decisions

- **Reuse the canonical change model.** The frequency and timing panels reuse the exact thresholded weighted-linear-RGB change test from `measure.rs` (`weighted_square = 13_933·dr² + 46_871·dg² + 4_732·db²`, changed iff `weighted_square > noise_floor² · 65_536`). A new `pub(crate)` classifier is extracted from `measure_pixels` so the renderer and the measurement kernel share one source of truth — there is no second change metric in this feature.
- **Reuse the canonical gap rule.** Adjacent-pair measurability uses the same intersection test as `measure_pair` (`gap.start ≤ later.timestamp && gap.end ≥ earlier.timestamp`). A `pub(crate)` gap-counter helper is extracted so the renderer never re-derives gap semantics. Pairs that cross a declared gap contribute nothing to any accumulator.
- **Reference panel = normalized reference frame, grayscale.** The reference panel renders the chosen reference frame's linear-RGB luminance (the existing integer `linear_luminance` kernel from `measure.rs`) as an opaque sRGB-ish byte. This keeps the crate integer-only (no inverse color LUT, no `f64`), preserves the spatial-context purpose of the panel, and stays visually consistent with the grayscale frequency panel. Color reference and a true linear→sRGB inverse LUT are deferred future enhancements; the manifest's `source_frame_ids` always point to the full-color source frames for closer inspection.
- **Frequency mode is caller-declared.** A `FrequencyMode` registry selects whether brightness encodes `Count`, `Magnitude`, or `NormalizedFrequency`. The legend reports the active mode and the image-wide maximum so the scale is unambiguous.
- **Timing panel is magnitude-weighted, range-relative.** For each changed pixel the change time is the later frame's timestamp offset from the sequence range start, weighted by its `weighted_square`. The weighted average offset is normalized by the sequence range and mapped through a named integer palette. Using range-relative offsets (not absolute timestamps) keeps the per-pixel `weighted_time_sum` bounded by range duration.
- **Repeated-change is span-based.** A pixel is a repeated change iff it has at least two above-threshold events and the span between its first and last change offset meets a caller-declared separation. Consecutive changes have small span; widely separated bursts have large span, so the rule distinguishes them without inventing a clustering or tracking model. The effective separation defaults to `max(1 ns, range_duration / 4)` when the caller leaves it unset.
- **Fixed three-panel layout, no panel engine.** The renderer hardcodes one layout (header band, three labeled panels, legend band) computed from the normalized output dimensions plus fixed integer constants. There is no `Panel` trait, no composable panel graph, and no layout negotiation.
- **Integer-only color and palette math.** The time palette is a fixed stop table interpolated with `u32` arithmetic; the frequency scale and inverse-sRGB-ish luminance use integer math. The crate's no-`f64`-in-derived-metrics discipline is preserved; rendering color is display state, not a measurement, but is kept integer anyway for cross-platform byte determinism.
- **Deterministic PNG encoding.** The renderer emits PNG via the `png` crate with pinned encoder settings (fixed filter and compression). `OutputHash` is SHA-256 of the exact encoded bytes. The format and encoder profile are recorded in the manifest so a settings change is a visible algorithm-version bump.
- **New crate dependencies are rendering-only.** `sha2` (already a workspace dep) and `png` (new workspace dep) are added as normal dependencies of `temporal-vision`. The contract/normalize/measure modules gain no new dependency; only the new `render` and `difference_map` modules use them. The crate's pre-rendering independence (serde + thiserror) is unchanged for the contract and measurement surfaces.
- **`selected_frame_ids` = reference frame.** The manifest's selected frame is the single reference frame shown in the reference panel; all frames contribute to accumulation but only the reference is selected for display. `omitted_frame_count` is therefore `source_count − 1`.
- **No structured per-pixel output.** The public output is the manifest plus the rendered image. Per-pixel frequency/timing/repeated data is internal; a structured-output API is deferred until evaluation shows a caller need.
- **No UI surface.** This is a browser-agnostic Rust rendering contract with no screen or flow, so no mockups apply.
- **Dispatch rationale.** Direct reading covered the full implemented crate (`error`, `frame`, `geometry`, `sequence`, `provenance`, `normalize`, `measure`) and both foundation docs. No exploratory agent was needed; this feature is one cohesive implementation owner with four sequential checkpoints.

## Architectural choice

### Chosen: one renderer module plus a shared rendering seam

Add three new modules:

- `render.rs` — the shared rendering seam for every artifact renderer: the `ImageEncoding` registry, `RenderedArtifact` (encoded bytes + lazy SHA-256), a `Canvas` RGBA8 framebuffer with checked drawing primitives, a deterministic PNG encoder pinned to one filter/compression profile, and a minimal bitmap font for labels.
- `font.rs` — a small fixed bitmap glyph set (uppercase letters, digits, punctuation, arrow) covering exactly the labels this and the other artifact renderers need.
- `difference_map.rs` — the feature surface: `FrequencyMode`, `TimePalette`, `DifferenceMapParameters`, `DifferenceMapLimits`, the per-pixel accumulation kernel, `DifferenceMapData`, the fixed `DifferenceMapLayout`, the three-panel assembly, manifest construction, and the public `render_difference_map` entry point and `DifferenceMapArtifact` result.

`measure.rs` gains two small `pub(crate)` helpers (`classify_pixel_change`, `intersecting_gap_count`) extracted verbatim from `measure_pixels`/`measure_pair`. The extraction is behavior-preserving; the existing measurement outputs and tests do not change.

This keeps the change model and gap semantics canonical (one source of truth), isolates all encoding/font/drawing infrastructure behind a reusable seam the sibling artifact features consume, and leaves the contract and measurement surfaces untouched.

### Rejected: a composable panel framework

A `Panel` trait with pluggable reference/frequency/timing panels would generalize the layout, but the difference map has exactly one fixed three-panel composition. A framework would add abstraction, indirection, and test surface for no current consumer, and would directly contradict the brief's "no panel framework" constraint.

### Rejected: a separate change metric for the difference map

Computing per-pixel change with a different threshold or distance would create a second source of truth that could drift from the measurement kernel and from storyboard selection. Reusing the canonical weighted-square test keeps the difference map, the measurements, and storyboard selection on one versioned metric.

### Rejected: color reference panel via `f64` inverse sRGB

A true linear→sRGB inverse (analytic or 64 KB LUT) would restore reference color but introduces either `f64` into a proudly integer crate or a large table. The grayscale luminance reference preserves spatial context and the integer discipline; full color is a deferred, separately versioned enhancement.

## Tricky unit first: bounded per-pixel change accumulation

The load-bearing unit is the per-pixel accumulator that folds every measurable adjacent comparison into frequency, magnitude-weighted timing, and repeated-change state without floating point, unbounded memory, or numeric wrap. It must:

- visit each adjacent pair in declaration order, skipping pairs that cross a declared gap (canonical rule) and pixels outside the transformed analysis mask;
- classify each in-domain pixel with the canonical weighted-square threshold (`classify_pixel_change`), accumulating into struct-of-arrays indexed by output pixel position;
- keep every accumulator in fixed-width integers with checked arithmetic that fails explicitly (`ResourceLimitExceeded`) rather than wraps;
- bound total accumulator memory by `pixel_count · ACCUMULATOR_BYTES_PER_PIXEL ≤ limits.max_accumulator_bytes`, checked before the first allocation;
- produce, for each pixel, the data the three panels and the repeated-change indicator need: change count, magnitude sum, comparable-pair count, magnitude-weighted time sum, first/last change offset.

If this kernel is correct and bounded, the panel rendering and manifest assembly above it are mechanical.

## Implementation units

### Unit 1: Shared rendering foundation

**Files:**
- `crates/temporal-vision/src/render.rs` (new)
- `crates/temporal-vision/src/font.rs` (new)
- `crates/temporal-vision/src/lib.rs` (add modules and explicit exports)
- `crates/temporal-vision/Cargo.toml` (add `sha2` and `png`)
- `Cargo.toml` (add `png` to `[workspace.dependencies]`)

**Story:** `epic-temporal-vision-toolkit-difference-map-rendering-foundation`

```rust
// render.rs
stable_registry! {
    /// Encoded image format produced by artifact renderers.
    pub enum ImageEncoding {
        Png => "png",
    }
}

/// An encoded artifact image plus its deterministic output hash.
///
/// The hash is SHA-256 of `bytes` and is computed lazily so the renderer can
/// embed it in a manifest without forcing a second hashing pass on the consumer.
pub struct RenderedArtifact {
    encoding: ImageEncoding,
    bytes: Box<[u8]>,
    hash: OutputHash,
}

impl RenderedArtifact {
    /// Encode an opaque RGBA8 framebuffer as PNG with a pinned encoder profile.
    pub(crate) fn encode_png(rgba: &[u8], width: u32, height: u32) -> Result<Self>;

    pub const fn encoding(&self) -> ImageEncoding;
    pub fn bytes(&self) -> &[u8];
    pub const fn output_hash(&self) -> OutputHash;
}

/// A checked RGBA8 framebuffer for deterministic image assembly.
pub(crate) struct Canvas {
    dimensions: PixelDimensions,
    rgba: Vec<u8>,
}

impl Canvas {
    pub(crate) fn new(dimensions: PixelDimensions, background: [u8; 4]) -> Result<Self>;
    pub(crate) const fn dimensions(&self) -> PixelDimensions;
    pub(crate) fn as_rgba(&self) -> &[u8];
    pub(crate) fn fill_rect(&mut self, rect: PixelRect, color: [u8; 4]) -> Result<()>;
    pub(crate) fn put_pixel(&mut self, x: u32, y: u32, color: [u8; 4]) -> Result<()>;
    /// Blit a `panel_w × panel_h` slice of RGBA8 pixels at `origin`.
    pub(crate) fn blit_rgba(
        &mut self,
        origin: (u32, u32),
        panel: PixelDimensions,
        rgba: &[u8],
    ) -> Result<()>;
    /// Draw `text` with the bitmap font at `origin` in `color`.
    pub(crate) fn draw_text(
        &mut self,
        origin: (u32, u32),
        text: &str,
        color: [u8; 4],
    ) -> Result<()>;
    /// Horizontal gradient swatch between two colors, used by legends.
    pub(crate) fn draw_gradient(
        &mut self,
        rect: PixelRect,
        start: [u8; 4],
        end: [u8; 4],
    ) -> Result<()>;
}

// font.rs
pub(crate) const GLYPH_WIDTH: u32 = 6;
pub(crate) const GLYPH_HEIGHT: u32 = 8;

/// Returns the fixed bitmap for one supported glyph, or `None` for unsupported
/// characters (the caller renders a space). Covers A–Z, 0–9, space, and the
/// punctuation/symbols the artifact labels require (`. : + - / % >`).
pub(crate) const fn glyph(character: char) -> Option<&'static [&'static [u8; 8]]>;
```

PNG encoding uses the `png` crate with `Compression` and `FilterType` pinned to fixed values documented inline; changing either is an algorithm-version bump recorded in provenance. The framebuffer passed to the encoder is exactly `width · height · 4` opaque RGBA8 bytes; all dimension arithmetic is checked and rejects overflow with `ResourceLimitExceeded`. SHA-256 is computed once in `encode_png` via `sha2::Digest` and stored alongside the bytes.

The font is a hand-authored 6×8 monochrome set. Unsupported characters render as a blank glyph; the renderer upper-cases label input so the supported set is sufficient. The glyph tables are `const` and have no runtime initialization.

**Acceptance criteria:**
- [ ] Identical RGBA8 inputs produce byte-identical PNG output across repeated `encode_png` calls, and `output_hash()` equals an independently computed SHA-256 of `bytes()`.
- [ ] `Canvas` fill/blit/text/gradient operations are deterministic and reject out-of-bounds rectangles with `InvalidRegion`.
- [ ] The glyph set renders every character used by the difference-map labels; unsupported characters degrade to a blank cell rather than panicking.
- [ ] `cargo tree -p temporal-vision --edges normal` adds only `png`, `sha2`, and their transitive pure-Rust dependencies.

### Unit 2: Change-model extraction and bounded accumulation

**Files:**
- `crates/temporal-vision/src/measure.rs` (extract two `pub(crate)` helpers; refactor `measure_pixels`/`measure_pair` to call them — behavior-preserving)
- `crates/temporal-vision/src/difference_map.rs` (new domain types and accumulation kernel)
- `crates/temporal-vision/src/lib.rs` (module and explicit exports)

**Story:** `epic-temporal-vision-toolkit-difference-map-change-accumulation`

```rust
// measure.rs (new pub(crate) helpers; existing public API unchanged)
pub(crate) struct PixelChange {
    pub changed: bool,
    pub weighted_square: u128,
}

/// Canonical thresholded weighted-linear-RGB change test.
/// `weighted_square = 13_933·dr² + 46_871·dg² + 4_732·db²`; changed iff it
/// strictly exceeds `noise_floor² · 65_536`. Identical arithmetic to the
/// existing `measure_pixels` loop, now shared with the difference map.
pub(crate) fn classify_pixel_change(
    before: &[u16; 3],
    after: &[u16; 3],
    parameters: MeasurementParameters,
) -> Result<PixelChange>;

/// Number of declared gaps whose inclusive range intersects the inclusive
/// timestamp interval of an adjacent pair. Identical rule to `measure_pair`.
pub(crate) fn intersecting_gap_count(
    gap_ranges: &[TimeRange],
    earlier: Timestamp,
    later: Timestamp,
) -> usize;
```

```rust
// difference_map.rs
stable_registry! {
    /// What the change-frequency panel encodes as brightness.
    pub enum FrequencyMode {
        Count => "count",
        Magnitude => "magnitude",
        NormalizedFrequency => "normalized_frequency",
    }
}

stable_registry! {
    /// Named deterministic palette for the change-timing panel.
    pub enum TimePalette {
        Spectral => "spectral",
    }
}

/// Bounded working-memory limits for one difference-map render.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DifferenceMapLimits {
    max_accumulator_bytes: NonZeroUsize,
    max_output_bytes: NonZeroUsize,
}
impl DifferenceMapLimits {
    pub const fn new(
        max_accumulator_bytes: NonZeroUsize,
        max_output_bytes: NonZeroUsize,
    ) -> Self;
}
impl Default for DifferenceMapLimits; // 256 MiB / 256 MiB

/// Deterministic rendering choices for one temporal difference map.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DifferenceMapParameters {
    reference_frame_index: usize,
    frequency_mode: FrequencyMode,
    time_palette: TimePalette,
    repeated_change_separation: Option<Timestamp>,
    measurement: MeasurementParameters,
    background: Rgb8,
    limits: DifferenceMapLimits,
}
impl DifferenceMapParameters {
    pub const fn new(
        reference_frame_index: usize,
        frequency_mode: FrequencyMode,
        time_palette: TimePalette,
        repeated_change_separation: Option<Timestamp>,
        measurement: MeasurementParameters,
        background: Rgb8,
        limits: DifferenceMapLimits,
    ) -> Self;
    // read-only accessors; the effective separation is resolved against the
    // sequence range inside `render_difference_map`, not here.
}

/// Internal struct-of-arrays accumulator (crate-private).
/// Per output pixel, folded across all measurable adjacent pairs.
const ACCUMULATOR_BYTES_PER_PIXEL: usize = 48;
pub(crate) struct DifferenceAccumulators {
    dimensions: PixelDimensions,
    analysis_mask: Option<BinaryMask>,
    change_count: Box<[u32]>,          // above-threshold events
    comparable_count: Box<[u32]>,      // in-domain, non-gap events
    magnitude_sum: Box<[u64]>,         // Σ weighted_square (also the timing weight)
    weighted_time_sum: Box<[u128]>,    // Σ later_offset · weighted_square
    first_change_offset: Box<[u64]>,   // ns offset from range start
    last_change_offset: Box<[u64]>,    // ns offset from range start
}

impl DifferenceAccumulators {
    /// Fold every measurable adjacent pair in `normalized` using the canonical
    /// change test and gap rule. Allocates only after the byte bound check.
    pub(crate) fn accumulate(
        normalized: &NormalizedSequence<impl Clone + Eq>,
        measurement: MeasurementParameters,
        limits: DifferenceMapLimits,
    ) -> Result<Self>;
}

/// Resolved per-pixel difference-map data, ready for panel rendering.
pub(crate) struct DifferenceMapData {
    accumulators: DifferenceAccumulators,
    range_start: Timestamp,
    range_duration_ns: u64,
    effective_separation_ns: u64,
    frequency_mode: FrequencyMode,
    max_change_count: u32,    // image-wide maxima for Count/Magnitude scaling
    max_magnitude: u64,
}

impl DifferenceMapData {
    pub(crate) fn frequency_value(&self, pixel: usize) -> Option<u32>; // 0..=255 scale basis
    pub(crate) fn is_repeated_change(&self, pixel: usize) -> bool;
    pub(crate) fn timing_offset(&self, pixel: usize) -> Option<u64>;   // weighted avg offset, None if no change
}
```

Accumulation details:

- Allocate six arrays sized to `normalized.dimensions().pixel_count()` only after `pixel_count · 48 ≤ limits.max_accumulator_bytes` and after deriving the output canvas byte bound; otherwise `ResourceLimitExceeded`.
- For each adjacent pair `(i, i+1)`: compute `intersecting_gap_count(...)`; if nonzero the pair is gap-boundary and skipped entirely. Otherwise `later_offset = later.timestamp() − range_start` (checked subtraction). For each in-domain pixel, call `classify_pixel_change`; if `changed`, increment `change_count`, add `weighted_square` (checked `u64`), add `later_offset · weighted_square` (checked `u128`), and update first/last offset. Always increment `comparable_count` for in-domain pixels in a measurable pair.
- `range_duration_ns = last_frame.timestamp() − range_start`; `effective_separation_ns = repeated_change_separation.map(|s| s.as_nanos()).unwrap_or_else(|| max(1, range_duration_ns / 4))`.
- `is_repeated_change(pixel)` = `change_count ≥ 2 && last_offset − first_offset ≥ effective_separation_ns`.
- `timing_offset(pixel)` = `weighted_time_sum / magnitude_sum` (None when `change_count == 0`).
- Image-wide `max_change_count` and `max_magnitude` are computed in a final pass for `Count`/`Magnitude` scaling; the legend reports them.

**Acceptance criteria:**
- [ ] `measure.rs` public outputs and existing tests are unchanged after the helper extraction (behavior-preserving refactor verified by the existing `analysis.rs` suite).
- [ ] `classify_pixel_change` and the existing `measure_pixels` agree on every pixel for the same `MeasurementParameters` (a shared table-driven test covers below-floor, at-floor, one-over, and far-over deltas).
- [ ] Adjacent pairs that cross a declared gap contribute zero to every accumulator; the same pair is reported as `GapBoundary` by `measure_pair`.
- [ ] Masked-out pixels never increment any counter; `comparable_count` is zero for fully excluded pixels.
- [ ] Per-pixel `weighted_time_sum` is range-relative and stays within `u128`; adversarial inputs fail with `ResourceLimitExceeded` instead of wrapping.
- [ ] Accumulator allocation is rejected up front when `pixel_count · 48` exceeds the configured limit.

### Unit 3: Three-panel layout, assembly, and manifest

**File:** `crates/temporal-vision/src/difference_map.rs` (continued)

**Story:** `epic-temporal-vision-toolkit-difference-map-panel-rendering`

```rust
// difference_map.rs (rendering + public entry point)

/// Fixed rectangle geometry for the three-panel composite.
/// Every field is a pure function of the normalized panel dimensions and
/// constants defined in this module.
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
    pub(crate) fn new(panel: PixelDimensions) -> Result<Self>; // checked arithmetic
}

/// The rendered temporal difference map and its reproducible provenance.
pub struct DifferenceMapArtifact<ArtifactId, FrameId, MarkerId, GapId> {
    manifest: ArtifactManifest<ArtifactId, FrameId, MarkerId, GapId>,
    rendered: RenderedArtifact,
}
impl<A, F, M, G> DifferenceMapArtifact<A, F, M, G> {
    pub fn manifest(&self) -> &ArtifactManifest<A, F, M, G>;
    pub fn rendered(&self) -> &RenderedArtifact;
}

/// Render one temporal difference map.
///
/// `sequence` projects authoritative markers, gaps, region, mask, range, and
/// source IDs into the manifest. `normalized` must be the result of normalizing
/// `sequence`; the renderer cross-checks frame count and dimensions and fails
/// with `InvalidParameter` if they disagree.
pub fn render_difference_map<A, F, M, G, P>(
    artifact_id: A,
    sequence: &FrameSequence<F, M, G, P>,
    normalized: &NormalizedSequence<F>,
    parameters: DifferenceMapParameters,
) -> Result<DifferenceMapArtifact<A, F, M, G>>
where
    A: Into<i64> + Clone,  // placeholder bound; see note
    F: Clone + Eq,
    M: Clone + Eq,
    G: Clone + Eq,
    P: AsRef<[u8]>;
```

> Note: `A` (artifact id) carries no arithmetic bound — the `Into<i64>` line above is illustrative only. The real bound is `A: Clone` plus whatever `ArtifactManifest` requires of its first type parameter (currently none beyond what the caller supplies); implement to match `ArtifactManifest`'s actual generic bounds.

Rendering steps inside `render_difference_map`:

1. Validate `parameters.reference_frame_index < normalized.frames().len()` and that `normalized.dimensions()` and frame count are consistent with `sequence`; otherwise `InvalidParameter`.
2. Build `DifferenceMapData` via `DifferenceAccumulators::accumulate`.
3. Compute `DifferenceMapLayout::new(normalized.dimensions())`; verify the resulting canvas RGBA byte length `≤ limits.max_output_bytes`.
4. Allocate `Canvas` filled with `background`.
5. Draw the header band: artifact title `TEMPORAL DIFFERENCE MAP`, range start/end offsets, and a `TIME →` direction indicator.
6. Draw the reference panel from `normalized.frames()[reference_frame_index].linear_rgb16()` via the integer luminance kernel → opaque grayscale RGBA8.
7. Draw the frequency panel from `DifferenceMapData::frequency_value` scaled by the active mode's maximum; render the mode-specific legend with the numeric maximum.
8. Draw the timing panel: for each non-repeated changed pixel, map `timing_offset / range_duration` through the named palette (integer interpolation); for repeated-change pixels, render the fixed indicator color; for unchanged pixels, render `background`. Render the palette legend with numeric start, midpoint, and end offsets.
9. Render the repeated-change indicator swatch plus, when `sequence.gaps()` is nonempty and intersects the range, a visible `GAP` warning band; both appear in the legend area.
10. Encode the canvas via `RenderedArtifact::encode_png`.
11. Assemble `normalization` = `normalized.normalization_steps()` ++ `[parameters.measurement.provenance_step()?]`; assemble `parameters` from `frequency_mode`, `time_palette`, effective separation, `reference_frame_index`, palette stop table, layout constants, and encoding format.
12. Build the manifest via `ArtifactManifest::from_sequence(artifact_id, ArtifactKind::DifferenceMap, EvidenceClass::SourceDerived, AlgorithmDescriptor::new("temporal-difference-map", "v1")?, sequence, vec![reference_frame_id], normalization, parameters, layout.image, rendered.output_hash())`.
13. Return `DifferenceMapArtifact { manifest, rendered }`.

**Acceptance criteria:**
- [ ] The composite image dimensions equal `DifferenceMapLayout`'s computed `image` and the manifest's `output_dimensions`.
- [ ] The reference panel is the grayscale luminance of the chosen reference frame; the frequency and timing panels align pixel-for-pixel with it.
- [ ] Frequency brightness follows the active `FrequencyMode` and its scale maximum is shown in the legend.
- [ ] The timing panel uses the named palette with numeric start/midpoint/end labels; repeated-change pixels use the indicator color and the legend lists the indicator and the effective separation.
- [ ] A visible gap warning appears iff the sequence has at least one declared gap intersecting the range; gap-crossing pairs contribute nothing to any panel.
- [ ] Identical inputs (sequence, normalized, parameters, algorithm version) produce byte-identical PNG output and an identical manifest.
- [ ] The manifest's `selected_frame_ids` is exactly the reference frame and its counts, range, annotations, region, mask, normalization, and hash are internally consistent.

### Unit 4: Public contract and deterministic regression tests

**File:** `crates/temporal-vision/tests/difference_map.rs` (new)

**Story:** `epic-temporal-vision-toolkit-difference-map-public-contract-tests`

Build one small browser-free synthetic sequence (typed local IDs, a declared non-black background, region/mask, a declared gap, and a few frames with known per-pixel changes). Exercise the public `render_difference_map` end-to-end and assert:

- the returned `DifferenceMapArtifact` exposes a manifest with `artifact_kind == DifferenceMap`, `evidence_class == SourceDerived`, the `temporal-difference-map`/`v1` algorithm, the reference frame as the only selected frame, correct counts, and the gap carried through;
- `rendered().encoding() == Png`, `rendered().bytes()` is non-empty and starts with the PNG signature, and `rendered().output_hash()` equals SHA-256 of `rendered().bytes()`;
- a second call with identical inputs produces byte-identical `bytes()` and an identical manifest (determinism);
- the manifest round-trips through JSON.

Add focused colocated unit tests for the load-bearing mechanics only:
- `classify_pixel_change` agrees with the existing measurement kernel at below-floor, at-floor, one-over, and far-over deltas;
- `intersecting_gap_count` matches `measure_pair`'s `GapBoundary` decision on the same sequences;
- a hand-computed 3-frame × 2×2 fixture yields exact per-pixel `change_count`, `magnitude_sum`, weighted-average timing offset, and repeated-change flag;
- `FrequencyMode::{Count, Magnitude, NormalizedFrequency}` produce the expected brightness ordering on a controlled fixture;
- accumulator and canvas byte bounds reject oversized inputs before allocation.

Do not snapshot full rendered images, test trivial accessors, or reproduce constructor coverage already in `contracts.rs`/`analysis.rs`.

**Acceptance criteria:**
- [ ] A browser-free consumer renders a complete difference map and reads its manifest without importing Krometrail, browser, codec, runtime, filesystem, or image-decoder types.
- [ ] Determinism holds across repeated renders; the PNG hash is reproducible and matches an independent SHA-256.
- [ ] The hand-computed accumulation fixture pins exact counts, magnitudes, timing offsets, and the repeated-change rule.
- [ ] Oversized accumulator/canvas inputs fail with `ResourceLimitExceeded` before allocation.
- [ ] `cargo fmt -p temporal-vision -- --check`, locked package check/test/clippy, and locked workspace check/test/clippy pass subject only to concurrently owned files documented by the orchestrator.

## Implementation order

1. `epic-temporal-vision-toolkit-difference-map-rendering-foundation`
2. `epic-temporal-vision-toolkit-difference-map-change-accumulation` (depends on 1; also touches `measure.rs` — see Risks)
3. `epic-temporal-vision-toolkit-difference-map-panel-rendering` (depends on 2)
4. `epic-temporal-vision-toolkit-difference-map-public-contract-tests` (depends on 3)

The feature remains one cohesive implementation and feature-review bundle. Stories are acceptance and verification checkpoints, not separate worker assignments.

## Simplification

- Render the three panels into one fixed-layout image; do not build a composable panel engine, a `Panel` trait, or a layout negotiation layer.
- Reuse the canonical weighted-square change test and the canonical gap rule from `measure.rs`; do not introduce a second change metric or a second gap detector.
- Reuse the integer luminance kernel for the grayscale reference panel; do not add an inverse color LUT, a color-management path, or `f64`.
- Reuse `ArtifactManifest::from_sequence`, `NormalizedSequence`, `BinaryMask`, `PixelRect`, `PixelDimensions`, `TimeRange`, `Timestamp`, `MeasurementParameters`, and `NormalizationStep` verbatim; do not duplicate manifest, marker, gap, region, mask, normalization, or measurement types.
- Share the rendering seam (`render.rs`, `font.rs`) with the sibling artifact features rather than each renderer re-creating an encoder or font.
- No existing tests require removal. The `measure.rs` change is a behavior-preserving extraction verified by the existing `analysis.rs` suite.

## Testing

- **Public seam:** one synthetic integration test renders a complete difference map and proves manifest correctness, PNG encoding, SHA-256 hashing, determinism, JSON round-trip, and crate independence together.
- **Change-model equivalence:** a shared table-driven test pins `classify_pixel_change` to the existing kernel and `intersecting_gap_count` to `measure_pair`'s gap decision, protecting the no-duplicate-change-model invariant.
- **Accumulation regression:** a hand-computed 3-frame × 2×2 fixture protects exact counts, magnitudes, magnitude-weighted timing offsets, and the span-based repeated-change rule — a one-line accumulation or threshold change would alter every panel.
- **Bounds regression:** focused oversized-input cases protect checked allocation without attempting real memory exhaustion.
- **No low-value coverage:** omit trivial accessor tests, full-image snapshot tests, font glyph enumeration, and constructor coverage already in sibling suites.

## Risks

- **Riskiest assumption — `measure.rs` edit window:** Story 2 extracts two `pub(crate)` helpers from `measure.rs`, which is owned by `epic-temporal-vision-toolkit-normalization-and-measurements` (currently at `stage: review`). The extraction is purely additive and behavior-preserving, but the implementer must sequence it after that feature reaches `done`, or the orchestrator must sequence the shared edit so the two features never hold conflicting `measure.rs` state. The existing `analysis.rs` suite is the regression guard.
- **Parallel rendering-seam introduction:** The sibling artifact features (storyboard, region-filmstrip, motion-history) are being designed concurrently and all need an encoding/font seam. This feature establishes `render.rs` and `font.rs` as the canonical shared seam; if another feature lands first with a different seam, the orchestrator reconciles by adopting whichever lands first and updating the other designs. Design-time parallelism is preserved; the shared code lands once.
- **PNG determinism across `png` crate versions:** A `png` crate upgrade could change byte output for identical RGBA. Mitigation: pin the `png` version in `Cargo.toml`, pin the encoder profile in code, and record the encoding format and profile in provenance so any change is a visible algorithm-version bump.
- **Grayscale reference is lossy:** The reference panel loses color, which slightly weakens "identify the relevant page or region." The manifest's `source_frame_ids` always point to the full-color source frames; a true color reference via an integer inverse LUT is a deferred, separately versioned enhancement.
- **Magnitude-weighted timing can mislead under bursts:** A pixel with one large-magnitude change and several small ones weights the average toward the large event. This is documented behavior, not a defect; the repeated-change indicator surfaces multi-burst pixels separately so the timing panel does not silently average them into one false timestamp.
- **Auto-scaled frequency maximum:** Scaling `Count`/`Magnitude` brightness by the image-wide maximum makes the brightest pixel always full-scale, which could over-emphasize a quiet sequence. The legend reports the actual maximum value so the scale is explicit; `NormalizedFrequency` is available when an absolute scale is preferred.
- **Accumulator memory headroom:** `weighted_time_sum` is `u128` and near-full-range timestamps plus near-maximum deltas across many frames approach the type's capacity. Checked arithmetic fails explicitly on adversarial overflow rather than wrapping, and realistic capture ranges (sub-second to minutes) are many orders of magnitude below the limit.
