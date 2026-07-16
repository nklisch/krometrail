---
id: epic-temporal-vision-toolkit-normalization-and-measurements
kind: feature
stage: done
tags: [visual]
parent: epic-temporal-vision-toolkit
depends_on: [epic-temporal-vision-toolkit-frame-sequence-contracts]
release_binding: 1.0.0
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Normalization and Visual-Change Measurements

## Brief

This feature turns raw source frames into a common pixel representation and computes the direct visual-change measurements used by every artifact in the crate.

Normalization supports the source-derived transformations allowed by `docs/VISUAL-EVIDENCE.md`: color-space conversion to a common working space, alpha compositing against a declared background, integer scaling with recorded parameters, fixed cropping, and light denoising or thresholding with recorded parameters. Each normalization step records its parameters in provenance.

Measurements are descriptive, not diagnostic. The crate computes: absolute pixel difference, changed-pixel proportion, changed-region bounds, luminance difference, color difference, perceptual frame distance, and elapsed time since the preceding captured frame. Noise thresholds are configurable and appear in provenance. Default thresholds reduce encoding and anti-aliasing noise without claiming to remove all irrelevant change.

This feature does not select frames or render final artifacts. It exposes normalized pixel buffers and a measurement vector that storyboard, difference-map, region-filmstrip, and motion-history features consume.

## Epic context

- Parent epic: `epic-temporal-vision-toolkit`
- Position in epic: second foundation feature — provides the prepared pixels and metrics every artifact depends on

## Simplification opportunity

- Start with a single working pixel representation (e.g., linear RGBA8 or a small set of supported formats) rather than a pluggable color-management pipeline.
- Keep perceptual distance simple and deterministic; avoid heavyweight perceptual models until evaluation shows they improve selection or artifact interpretation.
- Record every transformation parameter in provenance rather than hiding defaults, so reproducibility is explicit.

## Foundation references

- `docs/VISUAL-EVIDENCE.md` — Normalization, Visual-Change Measurements, Determinism
- `docs/ARCHITECTURE.md` — Temporal Visual Crate

## Design decisions

- **Working pixels:** Normalize every accepted straight-alpha sRGB RGBA8 frame to tightly packed, opaque linear-light RGB16 (`u16` R/G/B triples). A checked-in 256-entry sRGB-to-linear lookup table and integer arithmetic make conversion byte-for-byte stable without a runtime floating-point or color-management dependency.
- **Alpha background:** Require the caller to declare an sRGB `Rgb8` background for every normalization request. Composite straight alpha in linear light with round-half-up integer division; do not hide white, black, or transparency semantics behind a default.
- **Geometric order:** Apply one source-coordinate fixed crop, then one whole-number scale. Upscaling uses nearest-neighbor replication; downscaling uses exact non-overlapping box averages and requires both cropped dimensions to be divisible by the factor. No interpolation, padding, fractional scaling, registration, or silent stretching is allowed.
- **Geometry epochs:** Accept only a validated `FrameSequence`, which already enforces one common source geometry and pixel format. A dimension/device-scale change must arrive as another sequence/epoch; normalization never reconciles incompatible epochs implicitly.
- **Region and mask semantics:** The sequence region and mask define the analysis domain, not an automatic crop. Intersect them with the explicit crop. Upscaled pixels inherit source membership; a downscaled pixel is measurable only when every source pixel in its box is included, preventing masked-out pixels from entering measurements through averaging.
- **Noise handling:** Start with one configurable per-pixel perceptual noise floor and no spatial denoiser. Thresholding is sufficient for the first deterministic pipeline and avoids inventing pixels or erasing short-lived local changes before evaluation can measure that tradeoff.
- **Threshold semantics:** Compute a luminance-weighted squared linear-RGB delta. A pixel changes only when its delta is strictly greater than the configured floor; pixels at or below the floor contribute zero to every visual-change aggregate. The default floor is 512 on the 0–65535 working scale and is recorded as a thresholding provenance step.
- **Stable numeric output:** Publish integer sums, exact changed/compared counts, a rational proportion, rounded integer means, and integer RMS distance. Do not store or serialize derived `f32`/`f64` metrics; consumers can format the exact rational proportion for display.
- **Gap boundaries:** Produce one adjacent comparison for every frame after the first, including elapsed nanoseconds. If any declared closed gap range intersects the closed timestamp interval between the pair, return an explicit gap-boundary outcome and compute no pixel measurements across it.
- **Resource bounds:** Check frame count, output pixels per frame, and retained working bytes before allocation. Defaults cap 4,096 frames, 16,777,216 output pixels per frame, and 512 MiB of retained RGB16 buffers plus a transformed analysis mask; callers can choose lower nonzero limits, not unbounded sentinel values.
- **Provenance integration:** `NormalizedSequence` owns the exact ordered `NormalizationStep`s for conversion/compositing/crop/scale. `MeasurementParameters::provenance_step` returns the threshold step; artifact features concatenate these values when constructing `ArtifactManifest` rather than re-authoring algorithm parameters.
- **No UI surface:** This is a browser-agnostic Rust computation contract with no screen or flow, so no mockups apply.
- **Dispatch rationale:** Direct reading covered the complete implemented crate and public contract. No exploratory agent was needed; this feature remains one cohesive implementation owner with three sequential checkpoints.

## Architectural choice

### Chosen: immutable normalized sequence plus one integer measurement kernel

Add `normalize.rs` for validated normalization parameters and owned linear RGB16 sequences, and one `measure.rs` for gap-aware pair/adjacent comparisons. Normalization accepts the existing immutable `FrameSequence`; it returns a `NormalizedSequence` whose frames, transformed analysis mask, gap ranges, and ordered provenance steps are internally consistent. Measurement functions consume only that normalized aggregate and a small `MeasurementParameters` value. Artifact modules can reuse the same crate-private pixel kernel while the public API exposes exact frame-level measurements.

This keeps the computation browser-free, batch-oriented, deterministic, and small. It gives all four artifact features one prepared-pixel and metric authority without a plugin registry, image abstraction, or stateful processor.

### Rejected: keep sRGB RGBA8 as the working buffer

This would minimize memory, but alpha composition and luminance/color comparison would remain gamma-encoded and background-dependent. Each artifact would be tempted to reinterpret alpha and color separately. Converting once to opaque linear RGB16 costs six bytes per output pixel but gives one explicit comparison space and bounded integer arithmetic.

### Rejected: floating Lab/Delta-E or learned perceptual metrics

Lab and Delta-E are familiar perceptual measures, but they add floating-point/color-reference choices and substantially more code before evaluation establishes value. Learned or structural-similarity metrics add dependencies and model-like behavior. The selected Rec.709-weighted linear-RGB RMS is deterministic, monotonic, source-derived, and deliberately described as a simple perceptual distance rather than a diagnosis.

### Rejected: configurable normalization pipeline/trait graph

An ordered list of polymorphic transformations would make invalid orders and unsupported combinations part of the public contract. The first version has one fixed semantic order and explicit options. New operations should be added only when evaluation proves a need and provenance can version them.

## Tricky unit first: deterministic normalization and transformed analysis domain

The normalization pass is load-bearing because every later metric and renderer must observe identical pixels and coordinates. For each source sample, look up R/G/B in `SRGB8_TO_LINEAR16`, convert the declared sRGB background through the same table, and composite each channel as `(source * alpha + background * (255 - alpha) + 127) / 255`. Crop uses the caller's half-open source `PixelRect`. Upscaling replicates a composited source pixel into an `factor × factor` block. Downscaling composites each pixel in an exact source block, sums each `u16` channel in `u64`, and divides by `factor²` with round-half-up. The implementation may fuse these loops, but the result must equal this declared order.

The effective source analysis domain is `crop ∩ sequence.region ∩ sequence.mask`, with omitted region/mask meaning unrestricted. A transformed one-bit mask is allocated only when the domain is restricted. Upscaling replicates membership. Downscaling includes an output pixel only if every source sample in its box is included. An empty transformed domain fails before frames are retained. This prevents a crop, mask, or downscale boundary from silently admitting excluded pixels.

`FrameSequence` construction is the geometry-epoch gate. Normalization checks the crop and scale once against the sequence's common dimensions and has no API that accepts mixed frames individually. Source geometry changes therefore cannot be normalized into apparent continuity.

## Implementation units

### Unit 1: Normalization configuration, limits, and opaque linear working frames

**Files:**
- `crates/temporal-vision/src/normalize.rs` (new)
- `crates/temporal-vision/src/error.rs` (extend the one error registry)
- `crates/temporal-vision/src/lib.rs` (module and explicit exports)

**Story:** `epic-temporal-vision-toolkit-normalization-and-measurements-normalized-sequence`

```rust
// normalize.rs
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Rgb8 { /* private r, g, b */ }
impl Rgb8 {
    pub const fn new(red: u8, green: u8, blue: u8) -> Self;
    pub const fn channels(self) -> [u8; 3];
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IntegerScale { /* private direction and factor; factor 1 is identity */ }
impl IntegerScale {
    pub const IDENTITY: Self;
    pub fn up(factor: std::num::NonZeroU8) -> Result<Self>;   // 1..=8
    pub fn down(factor: std::num::NonZeroU8) -> Result<Self>; // 1..=8
    pub const fn factor(self) -> u8;
    pub const fn is_identity(self) -> bool;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessingLimits { /* nonzero private limits */ }
impl ProcessingLimits {
    pub fn new(
        max_frames: std::num::NonZeroUsize,
        max_pixels_per_frame: std::num::NonZeroUsize,
        max_retained_bytes: std::num::NonZeroUsize,
    ) -> Self;
}
impl Default for ProcessingLimits; // 4096 frames, 16_777_216 pixels/frame, 512 MiB

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NormalizationParameters {
    /* background, optional source crop, integer scale, processing limits */
}
impl NormalizationParameters {
    pub fn new(
        background: Rgb8,
        crop: Option<PixelRect>,
        scale: IntegerScale,
        limits: ProcessingLimits,
    ) -> Self;
    // read-only accessors
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedFrame<FrameId> {
    /* source id, timestamp, common output dimensions, packed linear RGB16 */
}
impl<F> NormalizedFrame<F> {
    pub fn id(&self) -> &F;
    pub const fn timestamp(&self) -> Timestamp;
    pub const fn dimensions(&self) -> PixelDimensions;
    pub fn linear_rgb16(&self) -> &[u16]; // exactly width * height * 3 values
}

#[derive(Clone, Debug, PartialEq)]
pub struct NormalizedSequence<FrameId> {
    /* source dimensions/crop, normalized frames, optional output mask,
       declared gap ranges, ordered normalization steps */
}
impl<F> NormalizedSequence<F> {
    pub fn frames(&self) -> &[NormalizedFrame<F>];
    pub const fn dimensions(&self) -> PixelDimensions;
    pub const fn source_dimensions(&self) -> PixelDimensions;
    pub const fn source_crop(&self) -> PixelRect;
    pub fn analysis_mask(&self) -> Option<&BinaryMask>;
    pub fn normalization_steps(&self) -> &[NormalizationStep];
    pub fn gap_ranges(&self) -> &[TimeRange];
    pub fn analysis_pixel_count(&self) -> u64;
}

pub fn normalize_sequence<F: Clone + Eq, M: Eq, G: Eq, P: AsRef<[u8]>>(
    sequence: &FrameSequence<F, M, G, P>,
    parameters: NormalizationParameters,
) -> Result<NormalizedSequence<F>>;
```

Add `InvalidScale`, `EmptyAnalysisDomain`, and `ResourceLimitExceeded` to the existing `ErrorCode` registry. Scale construction rejects factors above eight; factor one canonicalizes to identity. Downscale rejects non-divisible crop dimensions. Output dimension multiplication, `width * height * 3 * size_of::<u16>()`, transformed-mask bytes, and frame-count multiplication use checked arithmetic. The retained-byte limit covers every returned RGB16 buffer plus the optional output mask; caller-owned input bytes are not double-counted. Validate all limits before allocating the first output frame.

Store `SRGB8_TO_LINEAR16: [u16; 256]` in this module. Its entries are the IEC 61966-2-1 sRGB transfer function mapped to 0–65535 with round-half-up, generated once and checked in; runtime code performs lookup only. The ordered provenance uses stable versions and exact parameters:

1. `ColorSpaceConversion`, `srgb8-to-linear16-v1`, input/output names;
2. `AlphaCompositing`, `straight-alpha-linear-v1`, declared background RGB;
3. `FixedCrop`, `source-pixel-crop-v1`, only when explicitly requested;
4. `IntegerScaling`, `integer-scale-v1`, only when non-identity, including direction, factor, kernel, output dimensions, and the `all_source_pixels` mask-reduction rule.

**Acceptance criteria:**
- [ ] Identical RGBA8 bytes and parameters produce identical packed opaque RGB16 values and ordered provenance on repeated runs.
- [ ] Transparent, opaque, and partial-alpha fixtures match exact LUT/compositing integer results for non-black backgrounds.
- [ ] Crop precedes scale; upscaling is nearest-neighbor; exact-divisor downscaling is box average; unsupported factors, non-divisible downscales, overflow, and out-of-bounds crop fail explicitly.
- [ ] Sequence region/mask/crop intersection transforms deterministically, and no output pixel is measurable from a partly excluded downscale block.
- [ ] Mixed source geometry remains rejected by `FrameSequence`; this module adds no path that stretches or combines epochs.
- [ ] Configured/default resource limits reject work before allocation and errors do not leak IDs or pixels.

### Unit 2: Exact direct visual-change kernel and gap-aware comparisons

**Files:**
- `crates/temporal-vision/src/measure.rs` (new)
- `crates/temporal-vision/src/lib.rs` (explicit exports)

**Story:** `epic-temporal-vision-toolkit-normalization-and-measurements-direct-measurements`

```rust
// measure.rs
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MeasurementParameters { noise_floor: u16 }
impl MeasurementParameters {
    pub const DEFAULT_NOISE_FLOOR: u16 = 512;
    pub const fn new(noise_floor: u16) -> Self;
    pub const fn noise_floor(self) -> u16;
    pub fn provenance_step(self) -> Result<NormalizationStep>;
}
impl Default for MeasurementParameters;

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChangedPixelProportion { changed: u64, compared: u64 }
impl ChangedPixelProportion {
    pub const fn changed(self) -> u64;
    pub const fn compared(self) -> u64;
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MeasurementVector {
    /* exact thresholded aggregates */
}
impl MeasurementVector {
    pub const fn absolute_pixel_difference(&self) -> u64;
    pub const fn changed_pixel_proportion(&self) -> ChangedPixelProportion;
    pub const fn changed_region_bounds(&self) -> Option<PixelRect>;
    pub const fn mean_luminance_difference(&self) -> u16;
    pub const fn mean_color_difference(&self) -> u16;
    pub const fn perceptual_frame_distance(&self) -> u16;
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ComparisonOutcome {
    Measured(MeasurementVector),
    GapBoundary { declared_gap_count: std::num::NonZeroUsize },
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FrameComparison {
    /* earlier/later indices, elapsed nanoseconds, outcome */
}
impl FrameComparison {
    pub const fn earlier_frame_index(&self) -> usize;
    pub const fn later_frame_index(&self) -> usize;
    pub const fn elapsed_nanos(&self) -> u64;
    pub const fn outcome(&self) -> &ComparisonOutcome;
}

pub fn measure_pair<F>(
    sequence: &NormalizedSequence<F>,
    earlier_frame_index: usize,
    later_frame_index: usize,
    parameters: MeasurementParameters,
) -> Result<FrameComparison>;

pub fn measure_adjacent<F>(
    sequence: &NormalizedSequence<F>,
    parameters: MeasurementParameters,
) -> Result<Box<[FrameComparison]>>;
```

`measure_pair` requires two valid indices with `earlier < later`; dimensions are guaranteed by `NormalizedSequence`. Elapsed nanoseconds use checked subtraction. A gap interrupts the pair when `gap.start <= later.timestamp && gap.end >= earlier.timestamp`; count intersecting declared gaps and return `GapBoundary` without touching pixels. `measure_adjacent` calls the same path for `(0,1)`, `(1,2)`, and so on, so each captured frame after the first has either direct metrics or an explicit boundary.

For each included output pixel, let `dr`, `dg`, and `db` be absolute linear RGB16 channel differences. Compute:

```text
weighted_square = 13_933*dr² + 46_871*dg² + 4_732*db²  # weights sum to 65_536
changed iff weighted_square > noise_floor² * 65_536
luma = (13_933*R + 46_871*G + 4_732*B + 32_768) / 65_536
```

For pixels below or equal to the floor, all aggregate contributions are zero. For retained pixels:

- `absolute_pixel_difference` is checked `Σ(dr + dg + db)`;
- changed proportion stores exact `changed_pixel_count / compared_pixel_count` without floating serialization;
- bounds are the minimal half-open output-coordinate rectangle containing changed included pixels, or `None`;
- mean luminance difference is `Σ|luma_after-luma_before| / compared_count`, round-half-up;
- mean color difference is `Σ(dr+dg+db) / (3*compared_count)`, round-half-up;
- perceptual frame distance is `floor_sqrt(Σweighted_square / (65_536*compared_count))` using an integer `u128` square-root implementation.

Use `u128` checked accumulators and checked conversion to the public bounded integer fields. The normalization limits make valid output fit, but measurement code still fails rather than wraps if its contract changes later. The transformed analysis mask determines `compared_count`; normalization guarantees it is nonzero. The threshold step is `NormalizationKind::Thresholding`, version `weighted-linear-rgb-v1`, with the exact floor, comparison rule, weights, and below-floor-zeroing policy in deterministic `Parameters`.

**Acceptance criteria:**
- [ ] Identity frames produce zero sums/means/distance, zero changed count, exact nonzero compared count, and no changed bounds.
- [ ] A hand-computed synthetic pair produces exact absolute, rational proportion, bounds, luminance, color, and integer perceptual values.
- [ ] Pixels at the floor are unchanged; pixels one unit over are retained; excluded mask pixels affect no count, bound, or aggregate.
- [ ] Adjacent equal timestamps yield elapsed zero without reordering; invalid/reversed indices fail explicitly.
- [ ] Any intersecting declared gap yields `GapBoundary`, preserves elapsed time and indices, and never reports unseen time as measured stability.
- [ ] Metrics contain no NaN, infinity, negative zero, platform-dependent float rounding, diagnostic label, or inferred motion claim.

### Unit 3: Public analysis contract and deterministic regression fixtures

**File:** `crates/temporal-vision/tests/analysis.rs` (new)

**Story:** `epic-temporal-vision-toolkit-normalization-and-measurements-public-contract-tests`

Build one small browser-free sequence with caller-owned typed IDs, straight-alpha sRGB RGBA8 pixels, a declared non-black background, explicit crop and scale variants, region/mask restrictions, and a declared gap. Exercise the public normalization and measurement APIs, ordered provenance assembly, exact repeated outputs, arbitrary baseline comparison, and adjacent comparisons. Keep exact LUT/compositing and metric vectors small enough to review by hand.

Add focused colocated tests only where private mechanics need direct coverage: the checked-in LUT sentinel/checksum values, integer square root boundary cases, checked byte arithmetic, and effective-mask downscale policy. Do not snapshot large pixel arrays, test getters/derives, add encoded-image fixtures, or reproduce every constructor test from the contract feature.

**Acceptance criteria:**
- [ ] A consumer can normalize borrowed decoded frames and receive owned linear RGB16 buffers without importing Krometrail, browser, codec, runtime, filesystem, or image types.
- [ ] Repeated normalization, provenance serialization, and measurements are byte/value-identical for the same pixels and parameters.
- [ ] Exact fixtures cover transparent/partial-alpha composition, crop-before-upscale, box downscale, transformed region/mask behavior, threshold equality/one-over behavior, changed bounds, and a gap boundary.
- [ ] Default and deliberately tiny limits prove bounded rejection before large allocation; arithmetic-overflow paths remain explicit.
- [ ] `cargo tree -p temporal-vision --edges normal` adds no codec, GPU, async, Krometrail, CDP, MCP, plugin, or floating color dependency.
- [ ] `cargo fmt --all -- --check`, locked package check/test/clippy, and locked workspace check/test/clippy pass subject only to concurrently owned files documented by the orchestrator.

## Implementation order

1. `epic-temporal-vision-toolkit-normalization-and-measurements-normalized-sequence`
2. `epic-temporal-vision-toolkit-normalization-and-measurements-direct-measurements` (depends on 1)
3. `epic-temporal-vision-toolkit-normalization-and-measurements-public-contract-tests` (depends on 2)

The feature remains one cohesive implementation and feature-review bundle. Stories are contract and verification checkpoints, not separate worker assignments.

## Simplification

- Add two focused modules, not a generic image type hierarchy or `measure/` directory before the code earns it.
- Use one opaque linear RGB16 representation and one metric kernel. Artifact modules can inspect normalized pixels and reuse crate-private helpers instead of inventing alternate color/threshold semantics.
- Do not add codecs, format negotiation, dynamic transforms, builders, trait objects, async/streaming APIs, sinks, plugin hooks, GPU paths, SIMD variants, inferred motion, region tracking, or diagnostic labels.
- Use thresholding instead of a spatial denoiser in the initial pipeline. The existing `NormalizationKind::Denoising` remains a provenance vocabulary option for future evaluated algorithms, not an obligation to implement one now.
- Retain the existing sequence geometry gate and provenance types. Do not duplicate marker/gap identities or create another manifest builder.
- No existing tests or abstractions require removal; the crate contract is newly established and already minimal.

## Testing

- **Public seam:** one synthetic integration fixture protects the downstream contract—borrowed RGBA8 input, declared normalization, owned RGB16 output, transformed analysis domain, ordered provenance, direct measurements, and gap behavior together.
- **Algorithm regression:** hand-computed tiny images protect LUT/composition order, integer scale kernels, threshold boundary, changed bounds, exact rational counts, and fixed-point means/RMS. These are valuable because a one-line rounding or order change would alter every artifact.
- **Boundary regression:** focused tiny-limit and invalid geometry/scale cases protect checked allocation and epoch behavior without attempting an actual memory-exhaustion test.
- **No low-value coverage:** omit trivial accessor, derive, enum-shape, and large snapshot tests. Do not test every RGB value beyond LUT sentinels plus a stable checksum.
- **Dependency evidence:** inspect the normal dependency tree; the implementation should need only the crate's existing Serde/thiserror dependencies.

## Risks

- **Riskiest assumption — simple metric quality:** Rec.709-weighted linear RGB RMS may not select the most useful frames for text anti-aliasing, scrolling, or small transient defects. It is deterministic and versioned; evaluation can replace or supplement it without changing normalized pixels. Until then, callers must describe it as a perceptual distance, not validated perceptual equivalence.
- **Linear RGB16 memory:** Six bytes per pixel can be substantial for long 1080p ranges. Explicit default/caller limits fail safely, and artifact generation already runs on bounded workers. If profiling later proves this representation too costly, a performance feature can change storage/iteration while preserving the exact algorithm and public measurements.
- **Conservative downscaled masks:** Requiring every source sample in a box may remove narrow masked regions. This avoids contamination and is recorded in scale provenance; region-filmstrip evaluation can motivate another explicitly versioned policy.
- **Threshold default:** A floor of 512 is a reasoned starting point, not an empirical claim that all anti-aliasing or encoding noise lies below it. `EVALUATION.md` must calibrate defaults against stable controls and defect recall; the manifest keeps exact values reproducible.
- **Static transfer table maintenance:** An accidental table edit would change every artifact. Sentinel/checksum tests and the versioned conversion name make such changes visible; an intentional table change requires a new algorithm version.
- **Gap intersection conservatism:** A gap touching either retained frame timestamp suppresses the comparison. This can omit a measurable pair, but it cannot falsely claim continuity across declared missing evidence.

## Blockers

None. The frame-sequence feature is implemented and review-ready, and its validated RGBA8, geometry, gap, and provenance contracts provide the required foundation.

## Implementation notes

- Execution capability: raised/high (autopilot caller), because all downstream artifact algorithms consume these deterministic pixels and exact metrics.
- Review weight: standard (caller); implementation stops at `stage: review` for independent feature review.
- Dispatch: one cohesive owner implemented the three ordered child checkpoints; story boundaries remained acceptance and commit checkpoints rather than separate workers.
- Files changed: `crates/temporal-vision/src/normalize.rs`, `src/measure.rs`, `src/error.rs`, `src/lib.rs`, and `tests/analysis.rs`.
- Tests added: 11 focused unit/integration tests protecting LUT identity, linear-light alpha composition, crop/integer scaling, conservative transformed domains, checked limits/overflow, exact thresholded metrics, gap boundaries, deterministic provenance, and the browser-free public seam. The package now passes 22 tests across four suites.
- Simplification: one immutable RGB16 sequence and one checked integer measurement kernel; no codec, image framework, async/streaming, plugin, GPU, registration, inference, diagnostic label, or floating metric was introduced.
- Discrepancies from design: none.
- Foundation alignment: existing `VISUAL-EVIDENCE.md`, `ARCHITECTURE.md`, and `EVALUATION.md` assertions remain current; no rolling-foundation edit was required.
- Adjacent issues parked: none.

## Integrated verification

- `cargo fmt -p temporal-vision -- --check` — passed. The initial workspace format check passed; a final rerun was externally blocked by concurrently authored, unformatted `krometrail-store/tests/segment_writer_smoke.rs`.
- `cargo check -p temporal-vision --all-targets --locked` — passed.
- `cargo test -p temporal-vision --locked` — passed (22 tests, including doc tests with zero examples).
- `cargo clippy -p temporal-vision --all-targets --locked -- -D warnings` — passed.
- `cargo tree -p temporal-vision --edges normal --locked` — only existing Serde/thiserror dependencies and their derive machinery.
- `cargo check --workspace --all-targets --locked` — passed.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` — passed.
- `cargo test --workspace --all-targets --locked` — external concurrent interference: one `krometrail-cdp` page-observation test failed while that feature was actively bounced and its screenshot/snapshot/test/fixture files were dirty. The temporal-vision package remained fully green and no unowned file was edited.

## Review (2026-07-13)

**Verdict**: Approve with comments

**Blockers**: none
**Important**: none
**Nits**: Stronger full-table LUT recomputation, measurement serde round-trip, checked multiplication
style, and avoiding a redundant full-domain mask are optional hardening. Rounded luminance and
threshold-coupled aggregate naming are documented semantics for downstream renderers.
**Rejected**: none

**Notes**: Standard-weight cross-model review by GLM 5.2. The reviewer independently recomputed the
complete IEC transfer table, alpha composite, strict threshold boundary, integer square root, and
hand-derived measurement vector; inspected every normalization/measurement line; and reran 22
package tests, formatting, Clippy, and dependency independence. All acceptance contracts passed.
Workspace-wide rerun was deferred during concurrent lockfile work, with no package-local failure.
