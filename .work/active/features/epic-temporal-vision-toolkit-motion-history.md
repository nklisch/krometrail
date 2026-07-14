---
id: epic-temporal-vision-toolkit-motion-history
kind: feature
stage: review
tags: [visual]
parent: epic-temporal-vision-toolkit
depends_on: [epic-temporal-vision-toolkit-normalization-and-measurements]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Motion-History Image

## Brief

This feature renders a motion-history image that accumulates recently changed pixels over one spatial reference, with explicit source-vs-inference boundaries.

The default rendering combines a subdued source-frame reference, a motion-history layer showing recent change stronger than older change, and changed-region outlines. A visible decay legend maps intensity to relative time. Start and end labels are included. The artifact links to the storyboard and region filmstrip for disambiguation when overlapping states make text unreadable.

The crate does not add direction arrows, velocity vectors, object trajectories, or other inferred motion analysis to this source-derived artifact. Any future inferred overlay must be labeled as a separate artifact with its own method, version, and confidence.

This feature depends on the same thresholded change measurements used by storyboard and difference-map rendering. It does not produce storyboards or difference maps.

## Epic context

- Parent epic: `epic-temporal-vision-toolkit`
- Position in epic: independent artifact feature — bounded experiment in source-derived motion visualization

## Simplification opportunity

- Implement one deterministic decay model and expose it through parameters rather than multiple rendering modes.
- Produce a single combined image; do not split the reference, history, and outline layers unless evaluation shows agents need them separately.
- Explicitly forbid inferred overlays in this feature; inferred analysis is a separate future extension with its own provenance contract.

## Foundation references

- `docs/VISUAL-EVIDENCE.md` — Motion-History Image, Inferred Analysis, Visual-Change Measurements
- `docs/EVALUATION.md` — Motion-history evaluation criteria

## Design decisions

- **One deterministic decay model, parameterized.** Ship `motion-history` algorithm version `1.0.0` with exactly one accumulation model: per-pixel saturating accumulation of integer exponential-decay weights, where a change event at recency rank `r` (0 = most recent measured pair in its continuity segment) contributes `peak_intensity >> (r / half_life_ranks)`, zero once the shift reaches the u16 bit width. `MotionDecay` exposes `peak_intensity` (default `u16::MAX`) and `half_life_ranks` (default `1`) only. There is no decay-mode registry, no pluggable decay strategy, and no second accumulation model — this honors the brief's "avoid multiple modes" constraint.
- **Canonical change metric.** A pixel "changes" in a measured pair iff it passes the exact thresholded weighted-linear-RGB test from `measure.rs` (`weighted_square = 13_933·dr² + 46_871·dg² + 4_732·db²`, changed iff `weighted_square > noise_floor² · 65_536`). Motion-history consumes the `pub(crate) classify_pixel_change` helper extracted verbatim from `measure_pixels` by the sibling difference-map feature, so storyboard selection, difference-map accumulation, and motion-history accumulation share one source of truth for "changed." There is no second change metric in this feature.
- **Canonical gap rule.** Adjacent-pair measurability and gap segmentation come from the public `measure_adjacent` API, which already classifies each adjacent pair as `Measured` or `GapBoundary` using `gap.start ≤ later.timestamp && gap.end ≥ earlier.timestamp`. Motion-history groups consecutive `Measured` pairs into continuity segments; `GapBoundary` pairs start a new segment and contribute nothing to any accumulator. No cross-gap decay is computed and no unseen time is treated as observed stability.
- **Per-segment accumulation, max-composite across segments.** Within a continuity segment, each output pixel accumulates the saturating `u16` sum of decay weights for every pair in which it changed (repeated traversal accumulates, satisfying the repeated-traversal evaluation criterion). Across segments, the composite motion intensity is the per-pixel **maximum** over per-segment accumulations, so gap-separated activity in an earlier segment is not hidden by a later segment and is not conflated by summing across a gap. The decay reference is each segment's own newest pair, so "recency" is honest within each continuous chunk.
- **Subdued reference = integer linear luminance.** The spatial reference is the caller-chosen reference frame (default first frame) rendered as a dim grayscale backdrop using the existing integer Rec.709-weighted `linear_luminance` kernel from `measure.rs` (the same one the difference-map feature consumes), scaled by `reference_strength` (default 64). This keeps the crate integer-only, avoids a 64 KB inverse sRGB LUT, and matches the sibling accumulation artifact's reference posture; full-color reference and a true linear→sRGB inverse are deferred, separately versioned enhancements. The manifest's `source_frame_ids` always identify the full-color source frames for closer inspection.
- **Motion layer = single-hue accent, brightness-encoded.** The motion intensity at each pixel is composited as a straight-alpha blend of a fixed `accent_color` (default amber `Rgb8::new(255, 176, 0)`) over the subdued reference, with alpha equal to the accumulated `u16` intensity. Brightness (intensity) is the primary encoding of recency, so color is never the only indication of time or change — the brightness ramp, the decay legend, start/end timestamps, and changed-region outlines all carry redundant meaning.
- **Changed-region outlines = 4-connectivity boundary of `ever_changed`.** A separate `ever_changed` mask records every pixel that changed in any measured pair in any segment, independent of the decay window. Its 4-connectivity boundary is drawn in a fixed high-contrast `outline_color` (default white) over the composited image. The outline communicates the full spatial extent of motion ("where change happened at all"), complementing the decayed intensity layer ("how recent"). Outline is a shape encoding, not a color encoding.
- **`selected_frame_ids` = reference frame.** Following the difference-map convention, the manifest's selected frame is the single reference frame shown as the subdued backdrop; every source frame contributes to the accumulation but only the reference is selected for display, so `omitted_frame_count = source_frame_count − 1`. The full source-frame set remains in `source_frame_ids`.
- **Fixed single-image layout, no layer framework.** The renderer hardcodes one composition (header band, main motion-history area, footer band with decay legend, start/end labels, gap warning, and the source-derived disclaimer) computed from the normalized output dimensions plus fixed integer constants. There is no `Layer` trait, no composable layer graph, no separate reference/history/outline output, and no layout negotiation. The reference, motion, and outline layers are internal composition stages, not selectable outputs — this honors the brief's "do not split the layers" constraint.
- **Bounded experiment, removable from the default bundle.** This feature exposes a standalone opt-in generator (`generate_motion_history`). The crate makes no claim that motion-history is part of any "default bundle"; the debug bundle is assembled outside this crate (in `epic-temporal-debugging-workflow`) and may omit motion-history entirely if `EVALUATION.md`'s motion-history criteria (path visibility, repeated-traversal visibility, legend comprehension, text-smearing/overlap failure, false-direction inference) show it harms interpretation. The generator remains technically correct independent of its bundle inclusion.
- **Explicit prohibition on inference.** This feature produces **no** direction arrows, velocity vectors, object trajectories, optical-flow fields, logical-element tracking, or any inferred motion analysis. The visible output carries a `MOTION HISTORY — source-derived; no direction inferred` disclaimer. The manifest's `evidence_class` is `SourceDerived`, never `Inferred`. Any future inferred motion overlay is a separate artifact with its own method, version, and confidence contract — it does not enter this feature.
- **Fixed numeric and memory bounds.** Allocation is bounded before any pixel work: the accumulation composite (`u16` per output pixel), the per-segment accumulator (`u16` per output pixel, reused), and the `ever_changed`/`outline` bit masks (`ceil(pixels/8)` bytes each) are checked against `RenderLimits::max_canvas_bytes` before allocation; output dimensions, canvas bytes, and encoded PNG bytes are each checked against `RenderLimits`. Overflow in any checked accumulator is `ResourceLimitExceeded`, never wrapping. Memory is independent of source-frame text length (escaped/ellipsized into fixed annotation rows) and independent of total session duration.
- **Deterministic PNG and hash.** PNG output is RGB8 via the shared `encode.rs` seam (pinned `png` encoder settings: fixed filter and compression, no timestamp/text chunks). `OutputHash` is SHA-256 of the exact encoded bytes. The format and encoder profile are part of the `motion-history 1.0.0` algorithm version; a settings change that alters bytes requires a new algorithm version.
- **New crate dependencies are rendering-only.** `sha2` (already a workspace dep) and `png` (workspace dep added by the sibling render seam) are normal dependencies of `temporal-vision`, used only by the shared render/encode modules. The contract/normalize/measure surfaces retain only `serde` + `thiserror`. motion-history itself adds no new dependency.
- **No UI surface.** This is a generated evidence image in a browser-agnostic Rust crate, not an application screen or interactive flow; mockups do not apply.
- **Dispatch rationale.** Direct reading covered the full implemented crate (`error`, `frame`, `geometry`, `sequence`, `provenance`, `normalize`, `measure`), the parent epic, the three already-designed sibling artifact features (storyboard, difference-map, region-filmstrip) whose shared render seam and `pub(crate) classify_pixel_change`/`linear_luminance` helpers this feature reuses, and both foundation docs. No exploratory agent, advisory review, peeragent, or push is used under the autopilot caller constraint.

## Architectural choice

### Chosen: one renderer module reusing the shared render seam and canonical measurement helpers

Add one feature module and reuse two shared seams established by sibling artifact features:

- `motion_history.rs` — the feature surface: `MotionDecay`, `MotionHistoryParameters`, the gap-aware per-pixel decay accumulation kernel, `MotionHistoryPlan`, the fixed composition layout, manifest construction, and the public `build_motion_history_plan` and `generate_motion_history` entry points.
- `render.rs` (+ `render/font.rs`, `encode.rs`) and the encoded-artifact result type (`EncodedImage` / `GeneratedArtifact`) — the **shared rendering seam** every artifact renderer consumes, specified canonically by the storyboard design and reused by difference-map and region-filmstrip. This feature defers to that seam rather than introducing a parallel one: Unit 2 reuses the already-landed modules if present, or lands them in that canonical layout if motion-history is the first renderer to execute. The illustrative signatures below assume the seam exists and adapt to the canvas channel format (RGB8) the shared seam settles on.
- `measure.rs` — consumes two `pub(crate)` helpers extracted verbatim from existing private code by the sibling accumulation features: `classify_pixel_change` (per-pixel thresholded change decision, from `measure_pixels`) and `linear_luminance` (integer Rec.709-weighted luminance, already present and exposed `pub(crate)`). The extraction is behavior-preserving and additive; the existing measurement outputs and tests do not change. If motion-history lands before its siblings, it makes these additive exposures itself in the same canonical form.

`FrameSequence`, `NormalizedSequence`, `measure_adjacent`, `MeasurementParameters`, `ArtifactManifest::from_sequence`, `ArtifactKind::MotionHistory`, `EvidenceClass::SourceDerived`, `Rgb8`, `BinaryMask`, `PixelDimensions`, and `TimeRange` are reused unchanged.

This keeps the change metric, gap semantics, and luminance reference canonical (one source of truth across storyboard, difference-map, and motion-history), isolates all encoding/font/drawing infrastructure behind a reusable seam, and leaves the contract and measurement surfaces untouched. motion-history's only edit to a shared file is the additive `mod motion_history;` plus `pub use` lines in `lib.rs` — every other shared module is read-only reuse.

### Rejected: a composable layer framework

A `Layer` trait with pluggable reference/history/outline layers would generalize the composition, but the motion-history image has exactly one fixed three-stage composition (subdued reference, then accent-over-reference by intensity, then outline overlay). A framework would add abstraction, indirection, and test surface for no current consumer, and would directly contradict the brief's "do not split the reference, history, and outline layers" and "avoid multiple modes/layer framework" constraints.

### Rejected: a separate decay-mode registry

A `DecayMode::{Exponential, Linear, Windowed}` registry would let callers pick a decay shape, but the brief requires one deterministic decay model exposed through parameters. `MotionDecay { peak_intensity, half_life_ranks }` parameterizes the single exponential model without multiplying provenance and compatibility contracts. An evaluated semantic change requires a new algorithm version, not a new mode.

### Rejected: a separate change metric for motion history

Computing per-pixel change with a different threshold or distance would create a second source of truth that could drift from the measurement kernel, storyboard selection, and difference-map accumulation. Reusing `classify_pixel_change` keeps all three artifacts on one versioned metric.

### Rejected: direction, velocity, or trajectory overlays

Optical-flow vectors, velocity fields, or object trajectories would require inferred motion analysis with its own method, version, and confidence contract. They are explicitly forbidden in this source-derived artifact; the visible disclaimer and `EvidenceClass::SourceDerived` enforce the boundary. Inferred overlays are a separate future extension.

## Tricky unit first: bounded gap-aware per-pixel decay accumulation

The load-bearing unit is the per-pixel accumulator that folds every measurable adjacent comparison into a decayed intensity and an `ever_changed` union without floating point, cross-gap contamination, unbounded memory, or numeric wrap. It must:

1. **Validate alignment.** Confirm source and normalized frame counts, IDs, timestamps, and normalized dimensions agree (the same check storyboard and difference-map perform); reject a mismatched pair rather than accumulating from one and rendering another. Confirm `reference_frame_index < frame_count`.
2. **Segment via the canonical gap rule.** Call `measure_adjacent(normalized, measurement)` once. Group consecutive `Measured` pairs into continuity segments; each `GapBoundary` ends the current segment and starts a new one. A segment with zero measured pairs contributes nothing.
3. **Bound memory before allocation.** Compute `output_pixels = normalized.dimensions().pixel_count()`. Require `4 * output_pixels + ceil(output_pixels / 4)` (the composite `u16` accumulator, the reused per-segment `u16` accumulator, and the two bit masks) plus the planned canvas bytes to fit within `RenderLimits::max_canvas_bytes`; fail `ResourceLimitExceeded` before the first allocation.
4. **Accumulate per segment, per pixel.** For each segment, in source-pair order, compute each pair's recency rank `r = (segment_pair_count − 1) − k` and its weight `decay.weight_at(r)`. Skip weights of zero (older than the live window). For each in-domain output pixel (analysis mask respected), apply `classify_pixel_change(measurement, before, after)`; on change, `segment_accum[p] = segment_accum[p].saturating_add(weight)` and set `ever_changed[p] = true`.
5. **Composite by per-pixel maximum.** After each segment, `composite[p] = max(composite[p], segment_accum[p])`, then clear `segment_accum` for reuse. No cross-gap summation occurs.
6. **Derive the outline.** A pixel is an outline pixel iff `ever_changed[p]` and at least one 4-neighbor (in-bounds) is not in `ever_changed`.

All arithmetic is checked `u16`/`u32`/`u128` saturating; overflow is `ResourceLimitExceeded`, never wrap. The kernel visits each measured pair exactly once and each output pixel at most once per measured pair, so runtime is `O(measured_pair_count · output_pixels)` with no hidden full-sequence rescans. If this kernel is correct and bounded, the composition, encoding, and manifest assembly above it are mechanical.

## Implementation units

### Unit 1: Decay model, gap-aware accumulation kernel, and motion-history plan

**Files:**
- `crates/temporal-vision/src/motion_history.rs` (new)
- `crates/temporal-vision/src/lib.rs` (add `mod motion_history;` and explicit exports; additive)

**Story:** `epic-temporal-vision-toolkit-motion-history-decay-and-plan`

```rust
// motion_history.rs
use std::num::NonZeroU8;
use serde::{Deserialize, Serialize};

use crate::{
    BinaryMask, ErrorCode, FrameSequence, NormalizedSequence, PixelDimensions, PixelRect,
    MeasurementParameters, Rgb8, Result, TimeRange, Timestamp, VisionError,
    measure::measure_adjacent,
};

/// One deterministic exponential decay curve over recency rank.
/// A change at recency rank `r` (0 = most recent measured pair in its segment)
/// contributes `peak_intensity >> (r / half_life_ranks)`, zero once the shift
/// reaches the `u16` bit width. There is no decay-mode registry.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MotionDecay {
    peak_intensity: u16,
    half_life_ranks: NonZeroU8,
}

impl MotionDecay {
    pub const DEFAULT_PEAK_INTENSITY: u16 = u16::MAX;
    pub const DEFAULT_HALF_LIFE_RANKS: u8 = 1;
    pub const DEFAULT: Self = Self {
        peak_intensity: Self::DEFAULT_PEAK_INTENSITY,
        half_life_ranks: match NonZeroU8::new(1) {
            Some(value) => value,
            None => loop {},
        },
    };

    pub const fn new(peak_intensity: u16, half_life_ranks: NonZeroU8) -> Self {
        Self { peak_intensity, half_life_ranks }
    }

    pub const fn peak_intensity(self) -> u16 {
        self.peak_intensity
    }

    pub const fn half_life_ranks(self) -> NonZeroU8 {
        self.half_life_ranks
    }

    /// Weight contributed by a change at `rank_from_newest` (0 = most recent).
    /// Halves every `half_life_ranks` ranks; zero once the shift reaches 16 bits.
    pub const fn weight_at(self, rank_from_newest: u32) -> u16 {
        let shift = rank_from_newest / (self.half_life_ranks.get() as u32);
        if shift >= 16 {
            0
        } else {
            self.peak_intensity >> shift
        }
    }

    /// Number of recency ranks (from 0) that can contribute nonzero weight.
    pub const fn live_window(self) -> u32 {
        16 * (self.half_life_ranks.get() as u32)
    }
}

impl Default for MotionDecay {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Fixed motion-history rendering choices for one common-geometry sequence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MotionHistoryParameters {
    reference_frame_index: usize,
    measurement: MeasurementParameters,
    decay: MotionDecay,
    reference_strength: u8,   // 0..=255 subdued backdrop scale
    accent_color: Rgb8,       // motion layer tint
    outline_color: Rgb8,      // changed-region outline
    labels: crate::render::ArtifactLabels,
    limits: crate::render::RenderLimits,
}

impl MotionHistoryParameters {
    pub fn new(
        reference_frame_index: usize,
        measurement: MeasurementParameters,
        decay: MotionDecay,
        reference_strength: u8,
        accent_color: Rgb8,
        outline_color: Rgb8,
        labels: crate::render::ArtifactLabels,
        limits: crate::render::RenderLimits,
    ) -> Self {
        Self {
            reference_frame_index,
            measurement,
            decay,
            reference_strength,
            accent_color,
            outline_color,
            labels,
            limits,
        }
    }
    // read-only accessors for every field
}

/// The deterministic, render-independent result of motion-history accumulation.
/// Renderable without rescanning pixels; inspectable without decoding PNG.
#[derive(Clone, Debug, PartialEq)]
pub struct MotionHistoryPlan<FrameId> {
    accumulation: Box<[u16]>,           // composite (max across segments) intensity per output pixel
    ever_changed: BinaryMask,           // union of all changes (outline source)
    outline: BinaryMask,                // 4-connectivity boundary of ever_changed
    dimensions: PixelDimensions,        // normalized output dimensions
    reference_frame_index: usize,
    reference_frame_id: FrameId,
    continuity_segment_count: usize,
    live_window: u32,
    measured_pair_count: usize,
    gap_pair_count: usize,
    changed_pixel_count: u64,           // pixels set in ever_changed
    max_segment_rank: u32,              // highest rank actually observed (for legend)
    range: TimeRange,
}

impl<F> MotionHistoryPlan<F> {
    pub fn accumulation(&self) -> &[u16];
    pub fn ever_changed(&self) -> &BinaryMask;
    pub fn outline(&self) -> &BinaryMask;
    pub const fn dimensions(&self) -> PixelDimensions;
    pub const fn reference_frame_index(&self) -> usize;
    pub fn reference_frame_id(&self) -> &F;
    pub const fn continuity_segment_count(&self) -> usize;
    pub const fn live_window(&self) -> u32;
    pub const fn measured_pair_count(&self) -> usize;
    pub const fn gap_pair_count(&self) -> usize;
    pub const fn changed_pixel_count(&self) -> u64;
    pub const fn max_segment_rank(&self) -> u32;
    pub const fn range(&self) -> TimeRange;
}

/// Compute the deterministic motion-history plan without rendering.
/// Independently testable; does not touch the shared render seam.
pub fn build_motion_history_plan<F, M, G, P>(
    source: &FrameSequence<F, M, G, P>,
    normalized: &NormalizedSequence<F>,
    parameters: &MotionHistoryParameters,
) -> Result<MotionHistoryPlan<F>>
where
    F: Clone + Eq,
    M: Eq,
    G: Eq,
    P: AsRef<[u8]>;
```

`build_motion_history_plan` validates source/normalized alignment and the reference-frame index, segments via `measure_adjacent`, bounds memory before allocation, accumulates per segment with the saturating kernel and `classify_pixel_change`, composites by per-pixel maximum, and derives the 4-connectivity outline. The `analysis_mask` from `NormalizedSequence` restricts the domain exactly as it does in `measure_pixels`. The plan exposes everything the renderer and the legend need without committing to image bytes.

**Acceptance criteria:**
- [ ] Source/normalized mismatch (frame count, ID order, timestamp order, or dimensions) and an out-of-range `reference_frame_index` fail with a stable error before any allocation.
- [ ] A single change event at recency rank 0 contributes `peak_intensity`; rank `half_life_ranks` contributes half; ranks at or beyond `live_window` contribute zero; weights are exact for hand-checked `peak_intensity` / `half_life_ranks` pairs.
- [ ] Repeated change at one pixel within a segment accumulates saturatingly up to `u16::MAX`; across segments the composite is the per-pixel maximum, never a cross-gap sum.
- [ ] A `GapBoundary` pair starts a new segment, contributes no weight, and does not reset `ever_changed`; the visible gap count equals `source.gaps().len()`.
- [ ] The analysis mask excludes pixels from accumulation and from `ever_changed`; `changed_pixel_count` matches the set bits of `ever_changed`.
- [ ] The outline is exactly the 4-connectivity boundary of `ever_changed`; an isolated changed pixel is its own outline; an unchanged pixel is never an outline pixel.
- [ ] Accumulator memory and canvas-byte budget are checked before allocation; tiny `RenderLimits` fail explicitly without partial success, and no checked arithmetic wraps.
- [ ] Identical input produces an identical, serializable plan on repeated runs and supported platforms; the plan contains no `f32`/`f64`, no diagnostic label, and no inferred-motion claim.

### Unit 2: Bounded composition, deterministic PNG, and provenance

**Files:**
- `crates/temporal-vision/src/motion_history.rs` (continue)
- `crates/temporal-vision/src/lib.rs` (additive exports only)
- reuse: `crates/temporal-vision/src/render.rs`, `render/font.rs`, `encode.rs`, `EncodedImage` / `GeneratedArtifact` (shared seam), `measure::{classify_pixel_change, linear_luminance}` (`pub(crate)` helpers)

**Story:** `epic-temporal-vision-toolkit-motion-history-rendering`

```rust
// motion_history.rs (continued)
use crate::{
    ArtifactKind, ArtifactManifest, AlgorithmDescriptor, EvidenceClass, NormalizationStep,
    OutputHash, ParameterValue, Parameters, Timestamp,
    render::{ArtifactLabels, EncodedImage, GeneratedArtifact, RenderLimits},
};

/// Render and encode a motion-history image with full provenance.
pub fn generate_motion_history<A, F, M, G, P>(
    artifact_id: A,
    source: &FrameSequence<F, M, G, P>,
    normalized: &NormalizedSequence<F>,
    parameters: MotionHistoryParameters,
) -> Result<GeneratedArtifact<A, F, M, G>>
where
    A: Clone,
    F: Clone + Eq + std::fmt::Display,
    M: Clone + Eq,
    G: Clone + Eq,
    P: AsRef<[u8]>;
```

The renderer first calls `build_motion_history_plan`, then composes one checked RGB8 canvas through the shared seam:

1. **Layout.** Compute the output dimensions from `plan.dimensions()` plus fixed integer header/footer annotation heights; reject if the resulting width/height or canvas bytes exceed `parameters.limits`. The main area preserves the normalized aspect ratio with no decorative border around source-derived pixels.
2. **Header band.** Draw the nonempty caller `labels.title` and `labels.source` (shared `ArtifactLabels`), escaped and ellipsized into fixed rows via the shared bitmap font.
3. **Main area composition.** For each output pixel `(x, y)`: read the reference frame's linear RGB16 triple at the same coordinate; compute `lum = linear_luminance([r, g, b])` (the integer Rec.709-weighted `u16`); compute the subdued backdrop `gray = (u32::from(lum) * u32::from(reference_strength) + 32_767) / 65_535` (round-half-up `u8`); read `alpha = plan.accumulation()[p]` (`u16`); straight-alpha composite each channel as `out_c = (u32::from(gray) * (65_535 − alpha) + u32::from(accent_c) * alpha + 32_767) / 65_535`. Where `alpha` is zero the pixel is the subdued backdrop; where `alpha` is `u16::MAX` the pixel is the accent color.
4. **Outline overlay.** For each pixel set in `plan.outline()`, overwrite with `outline_color` (drawn last, high contrast).
5. **Footer band.** Draw the decay legend: a brightness ramp of the accent color from rank 0 (`NEWEST`) to `min(live_window − 1, max_segment_rank)` (`OLDEST RETAINED`), with the global range start/end timestamps (session-relative) and total span; a `GAP — N declared; unseen behavior may have occurred` warning when `source.gaps()` is nonempty; a `MOTION HISTORY — source-derived; no direction inferred` disclaimer; and an explicit `TIME →` indicator. All text is escaped/ellipsized into fixed rows; exact caller values remain in the manifest.
6. **Encode and hash.** Encode the canvas through the shared `encode.rs` seam as RGB8 PNG with pinned filter/compression and no ancillary chunks; cap encoded bytes at `parameters.limits.max_encoded_bytes`. Compute `OutputHash` as SHA-256 of the exact returned bytes.
7. **Manifest.** Build via `ArtifactManifest::from_sequence` with `ArtifactKind::MotionHistory`, `EvidenceClass::SourceDerived`, `AlgorithmDescriptor::new("motion-history", "1.0.0")?`, `selected_frame_ids = vec![reference_frame_id.clone()]`, `normalization = normalized.normalization_steps() + [measurement.provenance_step()?]`, `parameters` recording `reference_frame_index`, `reference_frame_id`, `decay.peak_intensity`, `decay.half_life_ranks`, `reference_strength`, `accent_color`, `outline_color`, `continuity_segment_count`, `measured_pair_count`, `gap_pair_count`, `changed_pixel_count`, `max_segment_rank`, `live_window`, the fixed layout constants, and the PNG encoder profile. `output_dimensions` and `output_hash` come from the encode step. Visible labels derive from those same parameter values.

**Acceptance criteria:**
- [ ] The image composites the subdued luminance reference, the accent-tinted motion intensity, and the white outline in one combined output; no separate reference/history/outline outputs are produced and no `Layer` abstraction exists.
- [ ] Title, source context, start/end timestamps and span, decay legend (`NEWEST`/`OLDEST RETAINED`), `GAP` warning when gaps exist, `MOTION HISTORY — source-derived; no direction inferred`, and `TIME →` are all visible, derive from manifest values, and never alter source pixels with decorative borders.
- [ ] Identical input produces identical plan, RGB canvas, PNG bytes, SHA-256, parameters, and manifest on repeated runs and supported platforms; decoded PNG dimensions and selected anchor pixel colors match the source-derived canvas.
- [ ] Checked layout rejects excessive width/height/canvas bytes/encoded bytes without partial artifact success; memory stays bounded independently of input text length and total session duration.
- [ ] The manifest's selected frame is exactly the reference frame, `omitted_frame_count = source_frame_count − 1`, normalization records the canonical steps plus the threshold step, and every visible label is reproducible from manifest parameters while source frames remain available.
- [ ] The output contains no direction arrow, velocity vector, object trajectory, optical-flow field, or inferred-motion claim; `evidence_class` is `SourceDerived`.
- [ ] The shared bitmap font and escaped text require no host font, locale, shaping engine, UI toolkit, filesystem, browser, or GPU.

### Unit 3: Public deterministic motion-history contract and useful render tests

**Files:**
- `crates/temporal-vision/tests/motion_history.rs` (new)
- focused colocated tests in `src/motion_history.rs` only for private accumulation/outline/decay mechanics

**Story:** `epic-temporal-vision-toolkit-motion-history-public-contract-tests`

Build a browser-free typed-ID sequence whose design exercises: a moving block (a bright square translated across frames, producing a decaying trail), a repeated-traversal pixel (changed in two separated segments), one stable interval, a declared gap that partitions accumulation, an analysis-mask exclusion, and tied timestamps. Generate the plan and the rendered artifact from normalized pixels. Assert exact accumulation values at hand-picked pixels, exact `ever_changed`/`outline` masks, exact `changed_pixel_count`, segment/gap counts, deterministic PNG signature/hash, manifest round trip, and the `selected_frame_ids == [reference]` invariant. Keep images tiny and hand-reviewable; use one committed hash for a tiny fixed raster, not large binary golden fixtures.

Add focused colocated tests for the decay curve at boundary ranks, saturating accumulation, max-across-segments compositing, the gap-segmentation reset, the 4-connectivity outline of a corner/edge/interior region, mask exclusion, and checked layout arithmetic. Do not test accessors/derives, duplicate `FrameSequence` validation, every glyph, every decay parameter combination, PNG internals owned by the codec crate, or visual diagnosis correctness.

**Acceptance criteria:**
- [ ] A browser-free consumer with arbitrary typed IDs produces a `MotionHistory` source-derived artifact through the public API and can trace the rendered intensity, outline, and reference backdrop to source frame IDs and manifest parameters.
- [ ] Exact fixtures prove: single-change weight at rank 0; halving at `half_life_ranks`; zero at `live_window`; saturating repeated traversal; max-composite across a gap; `ever_changed` survives decay fade-out; outline is the 4-connectivity boundary; analysis-mask exclusion removes pixels from accumulation and outline.
- [ ] A repeated tiny render has identical bytes/hash/manifest; decoded PNG dimensions and selected pixel colors match the source-derived canvas; the visible disclaimer, legend labels, start/end timestamps, and gap warning are verified by private glyph-layout evidence without OCR or fragile full-image snapshots.
- [ ] Tiny width/height/canvas/encoded limits fail explicitly; tests never allocate near production maxima.
- [ ] Normal dependencies remain browser/Krometrail/runtime/UI/font/filesystem/GPU-free and add only the shared bounded PNG encoding plus SHA-256 already established by the sibling render seam; motion-history introduces no new dependency.
- [ ] `cargo fmt -p temporal-vision -- --check`, `cargo check -p temporal-vision --all-targets --locked`, `cargo test -p temporal-vision --locked`, and `cargo clippy -p temporal-vision --all-targets --locked -- -D warnings` pass, with any concurrent unowned-file interference reported rather than edited.

## Implementation order

1. `epic-temporal-vision-toolkit-motion-history-decay-and-plan`
2. `epic-temporal-vision-toolkit-motion-history-rendering` (depends on 1; reuses the shared render seam established by sibling artifact features)
3. `epic-temporal-vision-toolkit-motion-history-public-contract-tests` (depends on 2)

The feature remains one cohesive owner and feature-review bundle. Stories are durable accumulation-kernel, rendering/provenance, and public-evidence checkpoints, not parallel worker assignments. Unit 1 is independently implementable and testable without the shared render seam; Unit 2 coordinates the seam (reusing it where landed, landing it canonically if motion-history is first).

## Simplification

- Add one feature module (`motion_history.rs`) and reuse the shared render seam, the canonical `classify_pixel_change` and `linear_luminance` helpers, and `measure_adjacent` for gap segmentation. Do not add a layer framework, decay-mode registry, second change metric, codec registry, host font, scene graph, async/streaming API, cache, storage sink, filesystem behavior, GPU path, or inferred-motion surface.
- Reuse `NormalizedSequence`, `MeasurementParameters`, `ArtifactManifest::from_sequence`, `ArtifactKind::MotionHistory`, `EvidenceClass::SourceDerived`, `Rgb8`, `BinaryMask`, `PixelDimensions`, `TimeRange`, `ArtifactLabels`, `RenderLimits`, `EncodedImage`, and `GeneratedArtifact` unchanged. Do not duplicate pixel metrics, gap semantics, provenance schemas, or encoding plumbing.
- Keep all annotations outside source-derived pixels and all exact text in the manifest; deterministic escaping/ellipsizing bounds the raster without pretending truncated display text is complete.
- No existing code or tests are obsolete. `ArtifactKind::MotionHistory` already exists in the registry and needs no second entry.

## Testing

- **Accumulation interface:** exact synthetic pixel values protect the feature's highest-risk contract — the decay curve, saturating within-segment accumulation, max-across-segments compositing, gap segmentation, `ever_changed`/`outline` derivation, and mask exclusion — without involving PNG bytes.
- **Render/provenance interface:** one tiny generated artifact protects visible labels (title, source, start/end, decay legend, gap warning, source-derived disclaimer), reference/intensity/outline composition, deterministic encoding/hash, and manifest agreement. This is valuable because a mismatch would make evidence untrustworthy.
- **Boundary regressions:** tiny render limits, a one-frame sequence (no pairs), an all-stable sequence (no changes → empty `ever_changed`, no outline), tied timestamps, a gap at the sequence endpoints, and an out-of-range reference index protect honest degraded behavior.
- **Private algorithm tests:** only the decay curve at boundary ranks, saturating accumulation, max-composite, the 4-connectivity outline of corner/edge/interior regions, mask exclusion, and checked layout merit colocated tests.
- **No low-value coverage:** no getter/derive matrix, giant golden image, exhaustive glyph test, exhaustive decay-parameter sweep, duplicate `FrameSequence` validation, codec-library conformance suite, browser fixture, visual-diagnosis assertion, inferred-direction assertion, or benchmark-success claim.

## Risks

- **Motion-history value is not yet empirically proven.** `EVALUATION.md` warns that an artifact which consistently harms interpretation is removed from the default bundle even if technically correct. The generator is opt-in and standalone; the crate claims no bundle inclusion. Evaluation of path visibility, repeated-traversal visibility, legend comprehension, text-smearing/overlap failure, and false-direction inference decides whether the debug bundle omits it. The implementation must not claim effectiveness before that evidence.
- **Decay window vs. sequence length.** With the default `half_life_ranks = 1`, only the 16 most recent measured pairs per segment contribute; older motion fades to zero and survives only in the `ever_changed` outline. Callers can raise `half_life_ranks` to lengthen the live window at the cost of slower decay. This is an honest bounded-history tradeoff, not a defect; the legend communicates the retained rank range.
- **Accumulation saturation can blur the legend's intensity→recency mapping.** A pixel changed many times within the live window saturates to `u16::MAX` (full accent), which then overstates its single-change recency. The legend describes the single-change curve, the outline and `ever_changed` carry the repeated-activity signal, and the manifest records `changed_pixel_count` and `max_segment_rank` so a reader can distinguish saturation from a single recent change. Evaluation can motivate a separately versioned normalization (e.g., log compression) only through a new algorithm version.
- **Max-across-segments compositing loses per-segment timing detail.** A pixel active in two gap-separated segments shows the stronger of the two per-segment accumulations, not their sum or their temporal order. The gap warning, segment count in the manifest, and the explicit "no cross-gap accumulation" rule keep this honest; the legend's ranks are per-segment, not global.
- **Grayscale reference loses color context.** A dim luminance backdrop preserves spatial layout but not color, which can hide color-only state changes behind the motion layer. This matches the sibling difference-map posture; the manifest's `source_frame_ids` always point to the full-color source frames for closer inspection, and a true linear→sRGB inverse is a deferred, separately versioned enhancement.
- **Text smearing and overlap failure.** When many states overlap, interface text in the reference backdrop can become unreadable. The artifact points (via the visible disclaimer and the manifest) to the storyboard and region-filmstrip artifacts for disambiguation, exactly as `VISUAL-EVIDENCE.md` requires. The crate does not invent a readability fix here.
- **PNG byte stability depends on a pinned encoder version/settings.** Lock the dependency and set every relevant encoder option through the shared seam. A codec/settings change that alters bytes requires a new algorithm version or explicit compatibility evidence even if decoded pixels match.
- **Riskiest assumption — the single exponential decay model is useful.** The halving-per-`half_life_ranks` curve is deterministic and simple but may not match how agents interpret recency. Versioning and provenance make evaluation comparable; an evaluated semantic change requires a new algorithm version, not a mode flag.

## Blockers

None. `epic-temporal-vision-toolkit-normalization-and-measurements` has completed verified implementation and is at feature review, so its `NormalizedSequence`, exact thresholded change metric, gap-aware adjacent comparison, and provenance contracts satisfy this feature dependency. The shared render seam and `pub(crate) classify_pixel_change` / `linear_luminance` helpers are established by the sibling artifact features (storyboard, difference-map, region-filmstrip); if motion-history lands first, it makes the same additive exposures in the canonical form.

## Implementation notes

- Execution capability: raised/high, selected by the autopilot caller because deterministic image evidence, bounded integer processing, gap semantics, and public provenance are contract-sensitive.
- Review weight: standard (autopilot caller). This implementation stops at `stage: review` and does not self-approve.
- Dispatch: one cohesive feature owner carried decay-and-plan → rendering/provenance → public contract evidence. Child stories were sequential design checkpoints, not separate ownership units.
- Files changed: `crates/temporal-vision/src/motion_history.rs`, additive exports in `crates/temporal-vision/src/lib.rs`, and `crates/temporal-vision/tests/motion_history.rs`.
- Public surface: `MotionDecay`, `MotionHistoryParameters`, serializable `MotionHistoryPlan`, `MotionHistoryArtifact`, `build_motion_history_plan`, and `generate_motion_history`.
- Implementation: canonical per-pixel classification and adjacent-gap outcomes feed one integer exponential decay; repeated changes saturate within each continuity segment, gap-separated segments max-compose, and an independent changed union produces a 4-connected outline. Rendering combines a subdued integer-luminance reference, amber history, and white outline into one bounded RGB8 image.
- Evidence posture: visible and machine-readable output states source-derived evidence, chronological time, declared gaps, no direction inference, and storyboard/region-filmstrip disambiguation. The manifest makes no default-debug-bundle inclusion or interpretation-success claim.
- Tests added: focused private decay/accumulation/outline/bounds/annotation checks and one browser-free public typed-ID fixture covering tied timestamps, translated motion, stable evidence, repeated traversal, one gap, mask exclusion, deterministic RGB/PNG/SHA-256, manifest round trip, selected/omitted provenance, and tiny resource limits.
- Verification: package format/check/test/Clippy passed (47 tests across 8 suites); workspace format/check/test/Clippy passed (318 tests across 33 suites, warnings denied).
- Commits: `80a6744` (decay and plan), `5f3eeb0` (rendering and provenance), `9f1fa1b` (public contract tests).
- Simplification: reused the established canvas/font/PNG/hash and measurement/luminance seams; added no layer framework, decay registry, second metric, codec abstraction, runtime adapter, browser dependency, inferred overlay, filesystem behavior, or large golden fixture.
- Discrepancies from design: (1) image-edge neighbors count as outside the changed set so isolated edge and 1×1 changes are outlined, resolving conflicting wording in favor of the explicit acceptance criterion; (2) the manifest includes an explicit RGB8 display-conversion step after normalization and threshold provenance, matching the established difference-map convention; (3) the hand-checkable public fixture uses a translated 3×3 block to preserve an interior non-outline motion pixel.
- Stats: three child checkpoints advanced directly to `done`; one new feature module and one public integration-test module; 47 temporal-vision tests and 318 workspace tests green.
- Adjacent issues parked: none.
- Blockers: none.
