---
id: epic-temporal-vision-toolkit-region-filmstrip
kind: feature
stage: done
tags: [visual]
parent: epic-temporal-vision-toolkit
depends_on: [epic-temporal-vision-toolkit-normalization-and-measurements]
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Region Filmstrip

## Brief

This feature renders a region filmstrip that presents one visual region across time at a readable, consistent scale.

A region can be fixed in viewport coordinates, fixed in source-image coordinates, or supplied independently for each frame by a declared tracking method. A fixed region does not follow a logical element; tracked regions are inferred and must state their tracking method and confidence. The artifact includes a locator image showing the region within a full source frame.

Filmstrip crops use a consistent output scale. Padding is explicit when a region extends beyond a frame. Each crop is labeled with its session-relative timestamp and offset from the query anchor. The output records the region definition, scale, and any per-frame tracking method in provenance.

This feature does not track logical elements unless the caller supplies a tracking method and per-frame region. It focuses on cropping, scaling, and arranging region crops into a deterministic strip.

## Epic context

- Parent epic: `epic-temporal-vision-toolkit`
- Position in epic: independent artifact feature — useful for localized defects and progressive detail

## Simplification opportunity

- Support fixed viewport and source-image regions first; defer caller-supplied tracking to a follow-up that can prove the contract with evaluation data.
- Render the locator and strip into one image rather than producing separate outputs.
- Reuse the normalization feature for pixel access and scale rather than adding a second decode path.

## Foundation references

- `docs/VISUAL-EVIDENCE.md` — Region Filmstrip, Normalization, Provenance
- `docs/EVALUATION.md` — Region-filmstrip evaluation criteria

## Design decisions

- **Region scope for v1:** Implement fixed source-image regions and fixed viewport regions only. Do not expose per-frame tracked regions in this feature. A future caller-provided per-frame geometry contract can be source-derived when the caller supplies every frame rectangle, but this feature must not imply that the crate tracks logical elements or inferred motion.
- **Coordinate honesty:** Source-image coordinates are source-frame pixel coordinates. Viewport coordinates are caller-declared viewport units mapped to source pixels by explicit rational X/Y scales; the crate records both the original viewport rectangle and the rounded source-pixel rectangle. No DOM, node reference, CSS layout, scrolling, or logical element identity enters the crate.
- **Out-of-bounds regions:** The declared region may extend beyond the source image. The visible intersection is cropped from each source frame and missing edges are filled with a declared padding color. Fully out-of-bounds tiles are valid all-padding evidence with a visible warning, not a silent failure.
- **Normalization use:** The generator consumes a validated `FrameSequence` and internally calls the existing normalization pipeline with a full-frame identity crop/scale to obtain opaque linear RGB16 source pixels. Region crop, padding, and display scale are filmstrip transformations recorded after normalization. No image decoder, filesystem path, browser type, or second color pipeline is introduced.
- **Frame selection:** Render all source frames when they fit the tile limit. If the sequence exceeds the limit, preserve first and final frames and choose the remaining tiles by deterministic source-order temporal coverage. This is a compact display choice, not a change-aware or diagnostic selection algorithm; omitted count is visible and recorded.
- **Locator frame:** The locator image uses a caller-selected source frame index, defaulting to the first selected frame at or after the anchor and then the first selected frame. It displays the full source frame, an outline for the declared region clipped to the image, and directional out-of-bounds chevrons when the rectangle extends past an edge.
- **Consistent crop scale:** Every filmstrip tile has identical declared-region logical dimensions and identical display scale. Cropped source pixels and padding are scaled by the same integer kernel. The layout never stretches individual frames to hide viewport drift or resizing.
- **Gap posture:** Declared gaps are copied into the manifest and shown as text-plus-pattern separators whenever a gap intersects the closed interval between adjacent rendered tiles. The artifact never interpolates or claims stability across missing time.
- **Rendering/encoding seam:** Keep filmstrip-specific planning and layout in `filmstrip.rs`. Reuse the shared `EncodedImage` / `GeneratedArtifact` / deterministic PNG-and-SHA256 seam from the storyboard feature when present; if this feature lands first, introduce the same shared seam without storyboard-specific behavior. Do not add a UI engine, scene graph, host fonts, runtime, GPU path, cache, storage sink, or codec registry.
- **Provenance location:** Existing `ArtifactManifest::region()` represents an in-bounds sequence analysis region and cannot fully express viewport or padded out-of-bounds rectangles. The filmstrip stores the complete region definition, coordinate mapping, padding, selection, locator, scale, layout, and encoding parameters in deterministic manifest `parameters`; it sets `region` only when the declared fixed source-image rectangle fits the source frame exactly.
- **No UI surface:** This is a generated evidence image in a browser-agnostic Rust crate, not an application screen or interactive flow; mockups do not apply.
- **Dispatch rationale:** Direct reading covered the parent epic, foundation visual-evidence/evaluation contracts, and implemented temporal-vision frame/normalization/provenance APIs. No exploratory agent, advisory review, peeragent, or push is used under the autopilot caller constraint.

## Architectural choice

### Chosen: source sequence → fixed-region plan → bounded raster/PNG

Add one public `filmstrip.rs` module that resolves coordinate semantics, selects rendered source frames, computes per-frame crop/padding plans, and renders the locator plus chronological strip from normalized full-frame pixels. The module returns the shared encoded-artifact type and a provenance manifest whose parameters reproduce the visual output while source frames remain available.

This keeps the crate browser-agnostic and source-derived. It also keeps the hard problem — coordinate and padding honesty — in one place instead of scattering region math across render helpers and future MCP adapters.

### Rejected: inferred tracking in the first filmstrip

Per-frame tracking would require a method, confidence, failure modes, and evaluation evidence. Implementing it here would blur the source-derived boundary and risk telling agents that a logical element was followed when only a fixed visual rectangle was shown. The selected design leaves a future `caller_provided_per_frame_regions` extension possible, but v1 accepts no tracking method and makes fixed-region semantics visible.

### Rejected: pre-normalized-only input

Accepting only `NormalizedSequence` would avoid one normalization call, but the current normalized contract does not expose enough source-to-output mapping for viewport/source-image rectangles and a full-frame locator. Starting from `FrameSequence` lets the filmstrip use existing validation, gaps, markers, and pixel bytes while still reusing the normalization pipeline for color/alpha handling. A future performance pass can add a pre-normalized fast path once the mapping API earns it.

### Rejected: separate locator and strip artifacts

Separate images would simplify layout, but the contract requires enough context to locate the region. A single combined image avoids mismatched provenance and makes the primary evidence self-contained. Source frames remain available for progressive detail.

## Tricky unit first: fixed-region coordinate resolution and padding plan

The highest-risk unit is the region plan, because a filmstrip that silently follows an element, clips a region, or changes scale would create false evidence.

Define `SignedPixelRect` as a non-empty half-open rectangle with signed `x/y` and nonzero `width/height`. A source-image region is already in source pixels. A viewport region carries a `ViewportMapping` with declared viewport dimensions and rational source-pixels-per-viewport-unit scales for X and Y. Convert viewport bounds to source pixels by outward rounding: floor left/top and ceil right/bottom. This guarantees the rendered source region contains the declared viewport rectangle rather than accidentally dropping edge pixels. Reject zero dimensions, overflowing bounds, non-finite or implicit scales, and viewport mappings whose rounded dimensions contradict the frame source dimensions.

For each selected frame, intersect the resolved source rectangle with `[0, frame.width) × [0, frame.height)`. The tile's logical crop size remains the declared source rectangle size; missing left/top/right/bottom edges become `PaddingInsets`. Scaling is applied after padding, so every rendered tile has identical output dimensions. Fully out-of-bounds regions produce `source_rect: None` plus padding for the whole tile and a visible `OUTSIDE SOURCE` label. Locator rendering uses the same resolved source rectangle, clipping the outline to the image and adding edge chevrons for out-of-bounds portions.

The plan stores selected frame IDs/indices, timestamps, signed anchor offsets, gap separators, locator index, original coordinate definition, resolved source rectangle, crop/padding per frame, tile dimensions, and output layout. Rendering consumes this plan without re-resolving coordinates.

## Implementation units

### Unit 1: Fixed region contract, frame selection, locator/crop plan

**Files:**
- `crates/temporal-vision/src/filmstrip.rs` (new)
- `crates/temporal-vision/src/lib.rs` (module and explicit exports)

**Story:** `epic-temporal-vision-toolkit-region-filmstrip-region-plan`

```rust
// filmstrip.rs
stable_registry! {
    pub enum RegionCoordinateSpace {
        SourceImage => "source_image",
        Viewport => "viewport",
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SignedPixelRect { /* private signed x/y and nonzero width/height */ }
impl SignedPixelRect {
    pub fn new(x: i64, y: i64, width: std::num::NonZeroU32, height: std::num::NonZeroU32) -> Result<Self>;
    pub const fn x(self) -> i64;
    pub const fn y(self) -> i64;
    pub const fn width(self) -> u32;
    pub const fn height(self) -> u32;
    pub fn right_exclusive(self) -> Result<i64>;
    pub fn bottom_exclusive(self) -> Result<i64>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RationalScale { /* positive numerator / denominator */ }
impl RationalScale {
    pub fn new(numerator: std::num::NonZeroU32, denominator: std::num::NonZeroU32) -> Self;
    pub const fn numerator(self) -> u32;
    pub const fn denominator(self) -> u32;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ViewportMapping {
    /* viewport dimensions and source-pixels-per-viewport-unit rational scales */
}
impl ViewportMapping {
    pub fn new(
        viewport_dimensions: PixelDimensions,
        scale_x: RationalScale,
        scale_y: RationalScale,
    ) -> Self;
    pub const fn viewport_dimensions(self) -> PixelDimensions;
    pub const fn scale_x(self) -> RationalScale;
    pub const fn scale_y(self) -> RationalScale;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum RegionDefinition {
    FixedSourceImage { rect: SignedPixelRect },
    FixedViewport { rect: SignedPixelRect, mapping: ViewportMapping },
}
impl RegionDefinition {
    pub const fn coordinate_space(self) -> RegionCoordinateSpace;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PaddingInsets { /* left, top, right, bottom in unscaled source pixels */ }
impl PaddingInsets {
    pub const fn left(self) -> u32;
    pub const fn top(self) -> u32;
    pub const fn right(self) -> u32;
    pub const fn bottom(self) -> u32;
    pub const fn is_empty(self) -> bool;
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct FilmstripTilePlan<FrameId> {
    /* cloned frame id, source index, timestamp, source intersection, padding */
}
impl<F> FilmstripTilePlan<F> {
    pub fn frame_id(&self) -> &F;
    pub const fn frame_index(&self) -> usize;
    pub const fn timestamp(&self) -> Timestamp;
    pub const fn source_rect(&self) -> Option<PixelRect>;
    pub const fn padding(&self) -> PaddingInsets;
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct RegionFilmstripPlan<FrameId> {
    /* selected tile plans, locator frame index, resolved source rect, dimensions, omitted count */
}
impl<F> RegionFilmstripPlan<F> {
    pub fn tiles(&self) -> &[FilmstripTilePlan<F>];
    pub const fn locator_frame_index(&self) -> usize;
    pub const fn coordinate_space(&self) -> RegionCoordinateSpace;
    pub const fn declared_region(&self) -> SignedPixelRect;
    pub const fn resolved_source_region(&self) -> SignedPixelRect;
    pub const fn tile_source_dimensions(&self) -> PixelDimensions;
    pub const fn omitted_frame_count(&self) -> u64;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FilmstripTileLimit(/* private NonZeroU8 */);
impl FilmstripTileLimit {
    pub const DEFAULT: Self; // 12
    pub fn new(value: u8) -> Result<Self>; // accepts 1..=24
    pub const fn get(self) -> u8;
}
impl Default for FilmstripTileLimit;

pub fn plan_region_filmstrip<F: Clone + Eq, M: Eq, G: Eq, P: AsRef<[u8]>>(
    source: &FrameSequence<F, M, G, P>,
    region: RegionDefinition,
    anchor: Timestamp,
    tile_limit: FilmstripTileLimit,
    locator_frame_index: Option<usize>,
) -> Result<RegionFilmstripPlan<F>>;
```

`plan_region_filmstrip` validates the anchor is inside `source.range()`, the optional locator index exists, viewport mappings match the sequence's source dimensions after outward scale rounding, and the resolved region's width/height fit `PixelDimensions`. It selects tiles in source declaration order. When `source.frames().len() <= tile_limit`, all frames are selected. Otherwise, always select index `0` and the final index, then fill remaining slots by deterministic temporal/source-order coverage using integer ratios over source indices; ties choose earlier source index. Selected IDs are an ordered unique subsequence.

Use existing `ErrorCode::InvalidRegion`, `InvalidScale`, `InvalidParameter`, and `ResourceLimitExceeded`; do not add a new error variant unless implementation proves an existing code cannot describe a boundary failure.

**Acceptance criteria:**
- [ ] Fixed source-image and fixed viewport region definitions serialize deterministically and preserve their declared coordinate space.
- [ ] Viewport-to-source conversion uses explicit rational scales, outward rounding, and rejects contradictory mappings instead of guessing at device scale.
- [ ] Negative and beyond-edge regions produce exact padding insets; fully outside regions produce all-padding tile plans without pretending source pixels exist.
- [ ] Tile selection is chronological, deterministic, bounded by `FilmstripTileLimit`, preserves first/final when thinning, and records omitted count.
- [ ] Locator defaults to the first selected frame at or after the anchor and validates explicit caller indices.
- [ ] The plan contains every value rendering needs; rendering does not recalculate coordinate semantics or follow logical elements.

### Unit 2: Deterministic filmstrip rendering, encoding, and manifest parameters

**Files:**
- `crates/temporal-vision/src/filmstrip.rs` (rendering entry and filmstrip-specific layout)
- `crates/temporal-vision/src/artifact.rs` (reuse or introduce shared `EncodedImage` / `GeneratedArtifact` contract)
- `crates/temporal-vision/src/encode.rs` (reuse or introduce deterministic bounded PNG + SHA-256 seam)
- `crates/temporal-vision/src/render/canvas.rs` and `src/render/font.rs` (reuse shared checked RGB8 canvas and embedded bitmap font if already present; otherwise keep equivalent helpers crate-private and minimal)
- `crates/temporal-vision/src/lib.rs` and crate/workspace manifests for explicit exports/dependencies

**Story:** `epic-temporal-vision-toolkit-region-filmstrip-rendering`

```rust
// filmstrip.rs
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegionFilmstripLabels { /* validated title and source/context */ }
impl RegionFilmstripLabels {
    pub fn new(title: impl Into<String>, source: impl Into<String>) -> Result<Self>;
    pub fn title(&self) -> &str;
    pub fn source(&self) -> &str;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegionFilmstripRenderLimits {
    /* nonzero max width, height, canvas bytes, encoded bytes, source frames */
}
impl RegionFilmstripRenderLimits {
    pub const fn new(
        max_width: std::num::NonZeroU32,
        max_height: std::num::NonZeroU32,
        max_canvas_bytes: std::num::NonZeroUsize,
        max_encoded_bytes: std::num::NonZeroUsize,
    ) -> Self;
}
impl Default for RegionFilmstripRenderLimits; // 4096×4096, 64 MiB canvas/PNG

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegionFilmstripParameters {
    /* region, anchor, tile limit, locator, background, padding color, display scale, labels, limits */
}
impl RegionFilmstripParameters {
    pub fn new(
        region: RegionDefinition,
        anchor: Timestamp,
        tile_limit: FilmstripTileLimit,
        background: Rgb8,
        padding_color: Rgb8,
        display_scale: IntegerScale,
        labels: RegionFilmstripLabels,
        limits: RegionFilmstripRenderLimits,
    ) -> Self;
    pub fn with_locator_frame_index(self, index: usize) -> Self;
    // read-only accessors
}

#[derive(Clone, Debug, PartialEq)]
pub struct RegionFilmstripArtifact<A, F, M, G> {
    /* encoded image, manifest, reusable plan */
}
impl<A, F, M, G> RegionFilmstripArtifact<A, F, M, G> {
    pub const fn image(&self) -> &EncodedImage;
    pub const fn manifest(&self) -> &ArtifactManifest<A, F, M, G>;
    pub const fn plan(&self) -> &RegionFilmstripPlan<F>;
}

pub fn generate_region_filmstrip<A, F, M, G, P>(
    artifact_id: A,
    source: &FrameSequence<F, M, G, P>,
    parameters: RegionFilmstripParameters,
) -> Result<RegionFilmstripArtifact<A, F, M, G>>
where
    F: Clone + Eq + std::fmt::Display,
    M: Clone + Eq,
    G: Clone + Eq,
    P: AsRef<[u8]>;
```

`generate_region_filmstrip` normalizes the full source sequence with `NormalizationParameters::new(background, None, IntegerScale::IDENTITY, limits.into_processing_limits())`, then renders from the normalized RGB16 frames. The filmstrip display scale is a separate integer transform applied to each padded crop. Upscaling uses nearest-neighbor replication; downscaling requires the declared region width and height to divide exactly by the factor. Padding is filled with `padding_color` after the same linear-to-sRGB conversion used for source pixels, and padding boundaries receive a high-contrast hatch/label so generated pixels are visibly not source evidence.

The combined image layout is: title/source/gap warning header; one locator panel on the left; chronological crop tiles to the right with wrap to additional rows only when needed to respect max width; a bottom time-direction band. Each crop tile shows session-relative timestamp, signed anchor offset, source-frame ID, and `OUTSIDE SOURCE` when applicable. Gap separators between rendered neighboring tiles show `GAP` plus hatch marks when any declared gap intersects their closed timestamp interval. Color is never the only signal.

Use `ArtifactKind::RegionFilmstrip`, `EvidenceClass::SourceDerived`, and `AlgorithmDescriptor::new("region-filmstrip", "1.0.0")`. Build the manifest after PNG encoding so `output_hash` covers the exact returned bytes. Concatenate normalization steps with display-conversion, region-crop/padding, locator, layout, text, and PNG parameter records. `parameters` must include: original region definition, coordinate space, viewport mapping when present, resolved source rectangle, selected frame indices/reasons, omitted count, locator index, padding color, display scale, tile dimensions, gap warning count, label truncation policy, output layout, PNG settings, and the fact that no tracking method was applied. Set manifest `region` only for an in-bounds fixed source-image rectangle; otherwise leave it `None` and rely on the richer deterministic parameters.

**Acceptance criteria:**
- [ ] The output image includes a full-frame locator and a chronological strip in one PNG, with source content separate from annotation bands.
- [ ] Every tile uses identical declared crop dimensions, identical display scale, explicit padding, and timestamp/anchor-offset/source-ID labels.
- [ ] Fixed viewport regions are rendered from their recorded source-pixel mapping; no DOM, scroll, node reference, or logical element tracking is claimed.
- [ ] Declared gaps appear visibly in the header and between affected neighboring tiles; no hidden interpolation or stability claim crosses a gap.
- [ ] Identical input produces identical plan, canvas, PNG bytes, SHA-256, parameters, and manifest on repeated supported runs.
- [ ] Render limits reject excessive dimensions/canvas/encoded bytes before returning an artifact; no partial manifest/image pair is emitted.
- [ ] The implementation adds no UI engine, host font, browser dependency, decode path, filesystem sink, async runtime, GPU path, cache, or inferred-analysis type.

### Unit 3: Public source-derived contract and edge-case tests

**Files:**
- `crates/temporal-vision/tests/filmstrip.rs` (new public integration tests)
- focused colocated tests in `src/filmstrip.rs` and shared render/encode helpers only where private arithmetic needs direct coverage

**Story:** `epic-temporal-vision-toolkit-region-filmstrip-contract-tests`

Build browser-free typed-ID RGBA8 sequences with small hand-checkable frames, markers, and declared gaps. Cover: in-bounds source-image region, negative/out-of-bounds source-image region, viewport region with rational 2× mapping, fully outside region, tile thinning, locator choice, padding color, downscale divisibility rejection, and manifest determinism. Use tiny images and one stable PNG hash; do not add large binary goldens or OCR tests.

Tests should prove visible metadata through deterministic canvas/glyph layout or decoded pixel checks rather than image snapshots. Dependency evidence should confirm the package remains free of Krometrail, CDP, MCP, browser, UI, font-discovery, filesystem, runtime, GPU, and image-decoder dependencies beyond the shared deterministic PNG encoder.

**Acceptance criteria:**
- [ ] A public caller can generate a region filmstrip from a `FrameSequence` with arbitrary typed IDs and trace every rendered crop to retained source-frame IDs.
- [ ] Source-image and viewport coordinate cases record distinct coordinate semantics and reproduce the expected crop/padding plan.
- [ ] Out-of-bounds and fully outside regions visibly use padding and never claim missing pixels as source observations.
- [ ] Gap warnings, timestamp labels, signed anchor offsets, locator outline/edge chevrons, selected IDs, omitted count, and no-tracking parameter are present and manifest-aligned.
- [ ] Repeated generation yields identical PNG bytes/hash/manifest for the same source pixels and parameters.
- [ ] Tiny render/processing limits and invalid scale/mapping inputs fail explicitly without large allocations.
- [ ] Package and workspace format/check/test/clippy gates pass, with any concurrent unowned-file interference reported rather than edited.

## Implementation order

1. `epic-temporal-vision-toolkit-region-filmstrip-region-plan`
2. `epic-temporal-vision-toolkit-region-filmstrip-rendering` (depends on 1)
3. `epic-temporal-vision-toolkit-region-filmstrip-contract-tests` (depends on 2)

The feature remains one cohesive implementation and feature-review bundle. Stories are durable design checkpoints and acceptance slices, not separate parallel worker assignments.

## Simplification

- One `filmstrip.rs` module owns coordinate semantics, crop/padding plans, and filmstrip layout; do not add a strategy registry or tracking subsystem.
- Reuse `FrameSequence`, `DeclaredGap`, `Marker`, `normalize_sequence`, `IntegerScale`, `Rgb8`, `ArtifactManifest`, stable registries, and the shared PNG/hash seam. Do not duplicate frame validation, gap models, provenance schema, or image decoding.
- Start with fixed source-image and viewport regions only. Caller-provided per-frame geometry and inferred tracking are deferred until evaluation and provenance can distinguish caller-supplied rectangles from crate inference.
- Select all frames when bounded, and use simple deterministic temporal thinning only when needed. Do not import storyboard's change-aware selection or invent diagnostic labels.
- Keep locator and strip together in one image. Do not create a panel engine, UI toolkit, CSS-like layout, host-font dependency, scene graph, filesystem sink, or cache.
- No existing code or tests are obsolete. If shared artifact/encoding modules already exist from storyboard implementation, extend by reuse rather than rewriting them.

## Testing

- **Coordinate interface:** small exact source/viewport cases protect the most important public contract: what region was shown and what it does not claim to follow.
- **Padding and locator regression:** negative, beyond-edge, and fully outside rectangles protect explicit missing-pixel rendering and locator context.
- **Render/provenance seam:** one tiny generated PNG protects deterministic labels, gap warnings, output hash, and manifest agreement without a large golden image.
- **Boundary tests:** invalid viewport mapping, impossible scale, source-frame limit, canvas/encoded limits, and all-padding tiles protect fail-fast behavior and memory bounds.
- **No low-value coverage:** skip accessor/derive matrices, duplicate `FrameSequence` constructor tests, exhaustive glyph tests, browser fixtures, OCR, visual diagnosis assertions, and benchmark-success claims.

## Risks

- **Viewport mapping precision:** Browser adapters must supply an exact rational mapping from viewport units to source pixels. If they only have approximate device scale values, they should convert to source-image coordinates before calling this API rather than smuggling uncertainty into the crate.
- **Full-frame normalization cost:** The v1 generator normalizes full frames so the locator and region coordinates share one pixel authority. This can be heavier than crop-only normalization for large ranges; explicit processing/render limits fail safely, and a future pre-normalized mapping API can optimize without changing evidence semantics.
- **Temporal thinning can omit a transient:** When the source sequence exceeds the tile limit, uniform source-order thinning may miss a brief local change. The omitted count is visible, and callers can narrow the range or request explicit source frames. This feature does not claim change-aware selection.
- **Manifest region field limitation:** The current `ArtifactManifest::region()` cannot represent viewport or padded out-of-bounds rectangles. Complete reproducibility therefore relies on deterministic `parameters`; implementers must not store only the clipped `FrameRegion` and lose the declared region.
- **Shared render seam timing:** Storyboard implementation may introduce shared `artifact.rs`, `encode.rs`, and render helpers concurrently. The filmstrip owner should reuse compatible helpers and avoid rewriting shared APIs; merge conflicts in those files are coordination issues, not product blockers.
- **Padding as generated pixels:** Padding is necessary to keep scale consistent but is not source evidence. The image and manifest must make padding visually and machine-readably explicit.

## Blockers

None. `epic-temporal-vision-toolkit-normalization-and-measurements` has completed verified implementation and is at feature review, which is sufficient for dependency-ordered implementation. The design intentionally avoids inferred tracking and any browser/UI dependency.

## Implementation notes

- Execution capability: raised/high, selected by the autopilot caller because fixed coordinate conversion, generated-padding honesty, deterministic rendering, and provenance alignment form a high-consequence visual-evidence boundary.
- Review weight: standard (caller). This implementation stops at `stage: review`; it does not self-approve.
- Dispatch: one cohesive owner used direct repository reads for region plan → rendering → contract tests; no exploratory agent, subagent, peeragent, push, or overlapping write ownership.
- Files changed: `crates/temporal-vision/src/filmstrip.rs`, `src/lib.rs`, `src/normalize.rs`, `src/provenance.rs`, `tests/filmstrip.rs`, and the three owned child-story records.
- Reused seams: existing `Canvas`, embedded bitmap font, deterministic PNG/SHA-256 encoder, `EncodedImage`, `GeneratedArtifact`, normalization contracts, provenance parameters, gap model, and stable registry.
- Public contract: fixed source-image and rationally mapped viewport coordinates; signed out-of-bounds rectangles; deterministic source-order thinning; explicit locator source; exact crop/padding plan; bounded wrapped rendering; visible timestamp/anchor/source/gap/padding/no-tracking labels; source-derived manifest and exact output hash.
- Traceability correction: an explicit locator frame outside the crop-strip selection is added to the manifest's ordered source subsequence. Shared artifact omission and strip-tile omission are recorded separately so visible and machine-readable claims do not disagree.
- Provenance correction: the internal manifest constructor accepts an artifact-specific region/domain override. Filmstrip sets `region` only for an in-bounds fixed source-image rectangle and does not claim that an unrelated sequence analysis mask was applied to the fixed crop.
- Tests added: 3 public integration tests plus 2 focused planning tests cover typed IDs, source/viewport semantics, rational 2× mapping, partial/full padding, thinning, locator choice, visible warnings, deterministic PNG/hash/manifest, explicit locator traceability, invalid mapping/downscale/tile deserialization, and processing/render ceilings. One tiny PNG hash is pinned; no binary golden or OCR coverage was added.
- Simplification: v1 remains one `filmstrip.rs` module with no tracking strategy, decoder, UI/layout framework, host font, browser/CDP/MCP type, filesystem sink, runtime, GPU path, cache, or inferred-analysis type.
- Design reconciliations: a one-tile thinning request deterministically keeps the first source frame because one tile cannot preserve two distinct endpoints; locator-only source frames count as artifact sources but not crop-strip tiles; `with_max_source_frames` extends the four-argument limits constructor without changing its designed signature; layout/text/PNG facts are manifest parameters rather than falsely classified normalization steps; downscaling uses a recorded non-overlapping sRGB8 box average over the complete padded crop.
- Verification: owned Rust files pass `rustfmt --check`; `cargo check -p temporal-vision --all-targets --locked`; `cargo test -p temporal-vision --locked` (41 passed across 7 suites); `cargo clippy -p temporal-vision --all-targets --locked -- -D warnings`.
- Workspace gate interference: `cargo fmt --all -- --check` and `cargo check --workspace --all-targets --locked` were attempted, but concurrent unowned browser-control/store edits were respectively unformatted and temporarily uncompilable (`krometrail-cdp` navigation/session call-site work and `krometrail-store` writer work). Those files were preserved and not edited; the parent orchestrator must rerun workspace format/check/test/clippy after their owners settle.
- Child commits: `4d6f680` (region plan), `0f477bb` (rendering), `82f3d80` (contract tests and final traceability hardening).
- Discrepancies from design: only the explicit reconciliations above; no acceptance behavior was removed.
- Adjacent issues parked: none.

## Review (2026-07-14)

**Verdict**: Approve with comments

**Blockers**: none
**Important**: none
**Nits**: Input-normalization and canvas budgets are coupled; `with_max_source_frames` extends the
literal draft signature; rational viewport mapping intentionally permits exact fractional scales;
and one provenance helper clones a small map. None affects correctness.
**Rejected**: none

**Notes**: Standard-weight fresh-context GLM 5.2 review verified coordinate conversion, signed
padding, crop/scaling, thinning, locator/gap selection, chevrons, labels, no-tracking honesty,
limits, manifest traceability/hash, deterministic output, 41 package tests, Clippy, and formatting.
The review's workspace handoff was subsequently satisfied by a clean locked workspace run with 318
tests and warnings denied during browser-lifecycle remediation. No material issue remains.
