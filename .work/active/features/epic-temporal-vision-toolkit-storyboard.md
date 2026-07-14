---
id: epic-temporal-vision-toolkit-storyboard
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

# Temporal Storyboard and Orientation Composite

## Brief

This feature delivers deterministic representative-frame selection and the primary temporal storyboard artifact, plus the simple before/during/after orientation composite.

The storyboard preserves required anchors when available: the last source frame before the range anchor, the first frame after the anchor, the first measurable visual change, the frame with the greatest difference from the pre-action baseline, and the final retained frame. Remaining tiles are selected from captured source frames using deterministic visual-change measurements, favoring local change peaks, trend changes, appearance/disappearance of changed regions, information gain relative to selected neighbors, and temporal coverage of long intervals.

Each tile displays its session-relative timestamp, offset from the query anchor, source-frame identifier, intersecting marker labels, and gap indication. The layout preserves source aspect ratio, avoids decorative borders that can be mistaken for page content, and supports a caller-configurable tile count within the 3–12 range. The default bundle uses no more than eight tiles.

The before/during/after composite selects three frames by the same deterministic rules and lays them out as a low-complexity orientation entry point. "During" is selected by a declared rule and identifies its source frame; it is not generated or averaged.

This feature does not render difference maps, region filmstrips, or motion-history images. It produces a single storyboard image and optionally the orientation composite.

## Epic context

- Parent epic: `epic-temporal-vision-toolkit`
- Position in epic: primary artifact feature — consumes normalized pixels and measurements

## Simplification opportunity

- Implement one deterministic selection algorithm and version it in provenance rather than exposing multiple strategies.
- Render labels and time-direction indicators with simple, high-contrast drawing rather than pulling in a full UI layout engine.
- Fold before/during/after into the same feature because it reuses the same frame-selection logic and rendering primitives.

## Foundation references

- `docs/VISUAL-EVIDENCE.md` — Temporal Storyboard, Before/During/After Composite, Shared Artifact Contract, Determinism
- `docs/EVALUATION.md` — Storyboard evaluation criteria

## Design decisions

- **One selection algorithm:** Ship `temporal-storyboard` version `1.0.0` for storyboard and orientation outputs. The artifact kind distinguishes the render; selection, tie-breaking, time formatting, rasterization, font, and PNG settings are all part of that one versioned algorithm.
- **Core-anchor budget conflict:** A requested tile limit is a hard maximum. Distinct core anchors are admitted in `PreAnchor`, `PeakBaselineChange`, `FinalFrame`, `FirstChange`, `PostAnchor` priority, merging roles that name one frame. A limit of three therefore yields the useful before/during/after spine. Any available lower-priority role that cannot fit is recorded as an omitted anchor in provenance rather than silently exceeding the limit or failing a valid 3–12 request.
- **Boundary anchors:** Frames immediately associated with caller markers and on each retained side of a declared gap are supplementary anchors after core anchors and before scored fill. They use source declaration order for ties and may be omitted only when the hard tile budget is already full; omission is recorded.
- **Gap posture:** Declared gaps partition the sequence into continuity segments. No baseline, trend, local-peak, or information-gain metric crosses a gap. Selection first represents an unrepresented segment when budget remains, and rendering always shows a text-plus-hatch `GAP` break and a visible global warning.
- **Deterministic score:** Remaining candidates are chosen iteratively by the lexicographic tuple `(unrepresented continuity segment, supplementary boundary role count, cumulative-change information gain, local adjacent-change peak, adjacent-change trend delta, changed-region appearance/disappearance, temporal coverage, earlier source index)`. Cumulative change is the checked sum of adjacent perceptual distances inside one segment, avoiding repeated full-pixel comparisons.
- **Baseline and change semantics:** The baseline is the last frame strictly before the anchor, falling back to the first source frame. `FirstChange` is the first measured adjacent comparison whose later frame is at or after the anchor and has a nonzero changed count, then the first such comparison anywhere if no post-anchor change exists. `PeakBaselineChange` is the greatest measured baseline-to-frame perceptual distance in the baseline segment; ties compare changed count/proportion, absolute difference, then earlier source index.
- **Orientation rule:** `BEFORE` is the pre-anchor baseline, `DURING` is peak baseline change (falling back to first frame strictly after the anchor, then baseline), and `AFTER` is the final retained frame. Panels can repeat a source when fewer than three distinct states exist; the manifest's selected IDs remain the unique chronological subsequence and role-to-frame mappings remain explicit parameters.
- **Raster source:** Select over `NormalizedSequence` and render its opaque linear RGB16 pixels after one deterministic inverse sRGB lookup. This keeps crop, integer scaling, mask geometry, alpha background, and visual measurements aligned. The conversion is recorded as `linear16-to-srgb8-v1`; source-frame references remain authoritative.
- **Rendering/encoding boundary:** Raster layout and drawing produce a private checked RGB8 canvas. A small `encode.rs` adapter owns deterministic PNG encoding and bounded SHA-256 hashing. No public renderer trait, UI engine, filesystem sink, or codec registry is introduced.
- **Font strategy:** Check in one tiny 6×10 printable-ASCII bitmap atlas under `src/render/font.rs`. Convert arbitrary UTF-8 labels with Rust's deterministic escaped form before drawing; visibly middle-ellipsize bounded title, source, frame-ID, reason, and marker fields with `… see manifest` semantics. This avoids host fonts, font discovery, floating glyph rasterization, and platform-dependent layout while the manifest retains exact caller text.
- **Layout and bounds:** Use one left-to-right strip, source-aspect-preserving contain-fit tiles, separate dark annotation bands, and no border around source pixels. Default preferred/minimum tile widths are 240/160 px. Default limits are 4096×4096 output, 64 MiB RGB canvas, and 64 MiB encoded PNG; checked layout and a bounded PNG writer reject before or during allocation. No frame-count-dependent retained raster cache is added beyond the selected tiles and one output canvas.
- **Visible semantics:** Every output shows nonempty caller title and source/context label, session-relative time, signed anchor offset, source-frame label, explicit `TIME →`, selected reasons, marker labels assigned to the first selected tile at or after their timestamp, and gap warnings. Text, arrows, ordering, and hatch patterns make color nonessential.
- **No diagnosis or inference:** Labels are descriptive selection reasons (`first change`, `peak baseline change`, `trend change`, `changed region transition`), never `flicker`, `reversal`, `defect`, causality, smoothness, velocity, or logical-element tracking claims.
- **No UI surface:** These are generated evidence images in a browser-agnostic Rust crate, not an application screen or interactive flow; mockups do not apply.
- **Dispatch rationale:** Direct reading covered the complete temporal-vision public contracts, parent design, and evaluation rules. The feature stays one cohesive implementation owner with three sequential checkpoints; no exploratory or advisory agent is used per the autopilot caller constraint.

## Architectural choice

### Chosen: pure selection plan → bounded raster → deterministic PNG

Add one `select.rs` that turns the source/normalized sequence and adjacent/baseline measurements into a reusable `StoryboardSelection`; one small render module that consumes that plan into a checked canvas; and one PNG adapter that returns encoded bytes plus the hash used by the existing `ArtifactManifest`. The public generator optionally emits both artifacts from one analysis pass. Selection is testable without image bytes, rendering cannot reinterpret selection, and machine-readable provenance is assembled from the exact values that drew the image.

### Rejected: render while greedily selecting

A single pass would be shorter initially, but it would couple scoring, marker/gap assignment, layout, and encoding. It would make tie-breaking hard to inspect and would allow visible labels to drift from manifest selection reasons. The explicit plan is a small earned boundary, not a plugin abstraction.

### Rejected: generic layout/codec/font traits

Traits for renderers, codecs, canvases, or fonts would advertise unsupported variation and weaken byte determinism. This feature needs one evidence layout and one PNG output. Future artifacts can reuse crate-private canvas helpers without turning them into a public UI framework.

### Rejected: uniform or multiple selectable strategies

Uniform sampling is an evaluation baseline, not the product algorithm. Exposing strategy variants before evaluation would multiply provenance and compatibility contracts. Version `1.0.0` remains the sole strategy; an evaluated semantic change requires a new algorithm version.

## Tricky unit first: bounded required-anchor and change-aware selection

Compute adjacent comparisons once with the caller's `MeasurementParameters`. Each `GapBoundary` increments a continuity-segment index and contributes no change value. Within each segment, build checked cumulative perceptual distance, local peaks (`current >= previous && current > next`, with missing neighbors treated as zero), absolute adjacent-distance trend deltas, and changed-region `None ↔ Some` transitions. Compare the baseline with each later frame only while `measure_pair` remains measured; a gap ends baseline candidates.

Core roles are resolved independently, merged by source index, then admitted by the declared priority. Supplementary marker candidates use the first source frame at or after each marker; gap candidates use the last frame before the gap start and first frame after the gap end when present. The iterative fill recomputes each unselected candidate's tuple against selected indices. Information gain is the minimum cumulative-change distance to the selected predecessor/successor in the same segment (a missing side uses the available side; no selected peer is zero). Temporal coverage is the analogous minimum nanosecond distance. All sums are checked `u64/u128`; overflow is `ResourceLimitExceeded`. Equal tuples choose the earlier source declaration index, which remains authoritative when timestamps tie.

A three-tile request does not pretend every distinct five-role anchor fits. The plan exposes both selected role mappings and `OmittedAnchor { frame_index, reason }`; rendering adds a concise `anchors omitted: N; see manifest` warning and provenance stores every omission. This makes the hard maximum, lossiness, and tie behavior reproducible.

## Implementation units

### Unit 1: Versioned selection plan and exact tie-breaking

**Files:**
- `crates/temporal-vision/src/select.rs` (new)
- `crates/temporal-vision/src/error.rs` (reuse registry; no new code unless a distinct checked-layout error proves necessary)
- `crates/temporal-vision/src/lib.rs` (module and explicit exports)

**Story:** `epic-temporal-vision-toolkit-storyboard-selection`

```rust
// select.rs
stable_registry! {
    pub enum SelectionReason {
        PreAnchor => "pre_anchor",
        PostAnchor => "post_anchor",
        FirstChange => "first_change",
        PeakBaselineChange => "peak_baseline_change",
        FinalFrame => "final_frame",
        MarkerBoundary => "marker_boundary",
        GapBoundary => "gap_boundary",
        LocalChangePeak => "local_change_peak",
        ChangeTrend => "change_trend",
        ChangedRegionTransition => "changed_region_transition",
        InformationGain => "information_gain",
        TemporalCoverage => "temporal_coverage",
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StoryboardTileLimit(/* private NonZeroU8 */);
impl StoryboardTileLimit {
    pub const DEFAULT: Self; // 8
    pub fn new(value: u8) -> Result<Self>; // accepts 3..=12 only
    pub const fn get(self) -> u8;
}
impl Default for StoryboardTileLimit;

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct SelectedFrame<FrameId> {
    /* cloned source id, source index, timestamp, ordered unique reasons */
}
impl<F> SelectedFrame<F> {
    pub fn frame_id(&self) -> &F;
    pub const fn frame_index(&self) -> usize;
    pub const fn timestamp(&self) -> Timestamp;
    pub fn reasons(&self) -> &[SelectionReason];
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct OmittedAnchor {
    /* source index and omitted required/supplementary role */
}
impl OmittedAnchor {
    pub const fn frame_index(&self) -> usize;
    pub const fn reason(&self) -> SelectionReason;
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct StoryboardSelection<FrameId> {
    /* chronological selected frames, omitted anchors, role indices, segment count */
}
impl<F> StoryboardSelection<F> {
    pub fn selected_frames(&self) -> &[SelectedFrame<F>];
    pub fn omitted_anchors(&self) -> &[OmittedAnchor];
    pub const fn before_index(&self) -> usize;
    pub const fn during_index(&self) -> usize;
    pub const fn after_index(&self) -> usize;
    pub const fn continuity_segment_count(&self) -> usize;
}

pub fn select_storyboard_frames<F: Clone + Eq, M: Eq, G: Eq, P: AsRef<[u8]>>(
    source: &FrameSequence<F, M, G, P>,
    normalized: &NormalizedSequence<F>,
    anchor: Timestamp,
    tile_limit: StoryboardTileLimit,
    measurement: MeasurementParameters,
) -> Result<StoryboardSelection<F>>;
```

Validate that source and normalized frame counts, IDs, timestamps, and normalized dimensions align; reject a mismatched pair rather than selecting from one and rendering the other. Require the anchor inside `source.range()`. `SelectionReason` is the sole reason registry and controls stable wire text plus visible labels. Keep all selection computation integer-only and preserve source order in the final selected subsequence.

**Acceptance criteria:**
- [ ] Limits 3 and 12 succeed, default is 8, and 2/13 fail with `InvalidParameter`; actual selections never exceed the limit.
- [ ] Distinct core anchors merge roles or follow the documented priority; omitted available roles are explicit and deterministic.
- [ ] Baseline, first-change, peak-baseline, final, marker, and gap candidates resolve exactly at strict/equal timestamp boundaries, including tied timestamps.
- [ ] No comparison or cumulative score crosses a declared gap; remaining budget represents unrepresented continuity segments first.
- [ ] Local peaks, trend deltas, region transitions, information gain, and temporal coverage influence the declared tuple, with exact earlier-index final tie-breaking.
- [ ] Orientation indices implement before/peak/final fallbacks without averaging, interpolation, diagnosis, or inference.
- [ ] Repeated selection and Serde output are byte/value-identical for identical input and algorithm version.

### Unit 2: Bounded artifact raster, embedded text, PNG, and provenance

**Files:**
- `crates/temporal-vision/src/artifact.rs` (new public encoded-result contract)
- `crates/temporal-vision/src/render.rs` (new layout/composition entry)
- `crates/temporal-vision/src/render/canvas.rs` (new checked RGB8 primitives and nearest-neighbor contain-fit)
- `crates/temporal-vision/src/render/font.rs` (new embedded 6×10 ASCII bitmap and escaping/ellipsizing)
- `crates/temporal-vision/src/encode.rs` (new deterministic bounded PNG adapter and SHA-256)
- `crates/temporal-vision/src/normalize.rs` (crate-private deterministic linear16→sRGB8 inverse lookup helper)
- `crates/temporal-vision/src/lib.rs` (explicit exports)
- `Cargo.toml` and `crates/temporal-vision/Cargo.toml` (workspace `png` plus existing workspace `sha2`)

**Story:** `epic-temporal-vision-toolkit-storyboard-rendering`

```rust
// artifact.rs
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedImage {
    /* fixed image/png media type, dimensions, exact bytes */
}
impl EncodedImage {
    pub const fn media_type(&self) -> &'static str; // "image/png"
    pub const fn dimensions(&self) -> PixelDimensions;
    pub fn bytes(&self) -> &[u8];
}

#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedArtifact<A, F, M, G> {
    /* encoded image and its matching manifest */
}
impl<A, F, M, G> GeneratedArtifact<A, F, M, G> {
    pub const fn image(&self) -> &EncodedImage;
    pub const fn manifest(&self) -> &ArtifactManifest<A, F, M, G>;
}

// render.rs
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactLabels { /* validated nonempty title and source/context */ }
impl ArtifactLabels {
    pub fn new(title: impl Into<String>, source: impl Into<String>) -> Result<Self>;
    pub fn title(&self) -> &str;
    pub fn source(&self) -> &str;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RenderLimits {
    /* nonzero max width, height, canvas bytes, encoded bytes */
}
impl RenderLimits {
    pub const fn new(
        max_width: std::num::NonZeroU32,
        max_height: std::num::NonZeroU32,
        max_canvas_bytes: std::num::NonZeroUsize,
        max_encoded_bytes: std::num::NonZeroUsize,
    ) -> Self;
}
impl Default for RenderLimits; // 4096×4096, 64 MiB canvas/PNG

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoryboardParameters {
    /* anchor, tile limit, measurement, labels, preferred/min tile width, limits */
}
impl StoryboardParameters {
    pub fn new(
        anchor: Timestamp,
        tile_limit: StoryboardTileLimit,
        measurement: MeasurementParameters,
        labels: ArtifactLabels,
        limits: RenderLimits,
    ) -> Self;
    // read-only accessors; preferred/min tile width fixed by algorithm v1
}

#[derive(Clone, Debug, PartialEq)]
pub struct StoryboardArtifacts<A, F, M, G> {
    /* one storyboard, optional orientation, and reusable selection */
}
impl<A, F, M, G> StoryboardArtifacts<A, F, M, G> {
    pub const fn storyboard(&self) -> &GeneratedArtifact<A, F, M, G>;
    pub fn orientation(&self) -> Option<&GeneratedArtifact<A, F, M, G>>;
    pub const fn selection(&self) -> &StoryboardSelection<F>;
}

pub fn generate_storyboard<A, F, M, G, P>(
    storyboard_artifact_id: A,
    orientation_artifact_id: Option<A>,
    source: &FrameSequence<F, M, G, P>,
    normalized: &NormalizedSequence<F>,
    parameters: StoryboardParameters,
) -> Result<StoryboardArtifacts<A, F, M, G>>
where
    F: Clone + Eq + std::fmt::Display,
    M: Clone + Eq,
    G: Clone + Eq,
    P: AsRef<[u8]>;
```

Render the storyboard chronologically in one strip. Calculate the largest equal tile width no greater than 240 that fits the selected count and limits; reject if it falls below 160. Derive height from normalized aspect ratio with checked round-half-up contain-fit. Source pixels receive no decorative frame or overlay. Dark header/annotation/timeline areas remain geometrically separate from source pixels. Orientation uses three equal panels labeled `BEFORE`, `DURING — PEAK BASELINE CHANGE` (or the explicit fallback reason), and `AFTER`.

Assign each marker to the first selected tile with `tile.timestamp >= marker.timestamp`, falling back to the final tile. Preserve marker declaration order. Escape text to printable ASCII, wrap within fixed annotation rows, and visibly ellipsize overflow while retaining exact values in the manifest. Draw gap breaks between tiles whenever a declared gap intersects their closed interval; include `GAP — unseen behavior may have occurred`, hatching, and a top warning. A gap at a selected frame timestamp remains visible.

Convert each linear channel to the nearest sRGB8 table entry, ties lower, and record `linear16-to-srgb8-v1`. Nearest-neighbor tile scaling uses integer center mapping and records source/output dimensions and kernel. PNG output is RGB8, has no timestamp/text chunks, uses fixed filter/compression settings, and writes through a byte cap. Hash exact returned bytes with SHA-256. Build both manifests only after encoding, with `EvidenceClass::SourceDerived`, the same `AlgorithmDescriptor("temporal-storyboard", "1.0.0")`, ordered unique selected IDs, concatenated normalization + threshold + display-conversion steps, exact layout/encoding/text/selection parameters, role mappings, omissions, and output dimensions/hash. Visible labels are generated from those same parameter values.

**Acceptance criteria:**
- [ ] Storyboard and optional orientation render from one selection pass, use source aspect ratio, and identify every rendered source panel without altering source pixels with annotations or borders.
- [ ] Title, source context, times, signed offsets, IDs, reasons, assigned markers, `TIME →`, and textual/patterned gap warnings are visible and derive from manifest values.
- [ ] Orientation uses exact source frames for before/during/after, labels its fallback rule, and never generates or averages a panel.
- [ ] Identical input produces identical selection, RGB canvas, PNG bytes, SHA-256, parameters, and manifest on repeated runs and supported platforms.
- [ ] Checked layout rejects sub-160 px tiles, excessive dimensions/canvas bytes, and encoded-output overflow without partial artifact success.
- [ ] The checked-in bitmap font and escaped text require no host font, locale, shaping engine, UI toolkit, filesystem, browser, or GPU.
- [ ] Manifest source/selected counts, gaps, markers, normalization, algorithm/version, tile limit, anchor, reasons/omissions, output dimensions, and hash reproduce the output while source frames remain available.

### Unit 3: Public deterministic storyboard contract and useful render tests

**Files:**
- `crates/temporal-vision/tests/storyboard.rs` (new)
- focused colocated tests in `src/select.rs`, `src/render.rs`, `src/render/font.rs`, and `src/encode.rs` only for private mechanics

**Story:** `epic-temporal-vision-toolkit-storyboard-public-contract-tests`

Build a browser-free typed-ID sequence whose anchors are intentionally distinct, includes tied timestamps, one stable interval, local change peaks, a changed-region appearance/disappearance, multiple markers, and a declared gap separating two continuity segments. Generate a three-tile and default-eight storyboard plus orientation from normalized pixels. Assert exact selected IDs/reasons/omissions, visible metadata regions, deterministic PNG signature/hash, and manifest round trip. Keep images tiny and hand-reviewable; use one committed hash for a tiny fixed raster, not large binary golden fixtures.

Add focused unit tests for score/tie ordering, bounded-writer failure, inverse lookup endpoints/ties, font escaping, marker assignment, gap intersection, and checked layout arithmetic. Do not test accessors/derives, duplicate constructor coverage, every glyph, every tile count, or PNG internals owned by the codec crate.

**Acceptance criteria:**
- [ ] A browser-free consumer with arbitrary typed IDs produces both source-derived artifact kinds through the public API and can trace every panel to source IDs.
- [ ] Exact fixtures prove over-budget required-anchor disposition, equal-timestamp/index ties, marker buckets, segment-first gap behavior, peak/trend/region/information/coverage scoring, and orientation fallbacks.
- [ ] A repeated tiny render has identical bytes/hash/manifest; decoded PNG dimensions and selected tile colors match the source-derived canvas.
- [ ] Visible `GAP`, `TIME →`, before/during/after, timestamps/offsets, frame labels, and marker text are verified by pixel or private glyph-layout evidence without OCR or fragile full-image snapshots.
- [ ] Tiny width/height/canvas/encoded limits fail explicitly; tests never allocate near production maxima.
- [ ] Normal dependencies remain browser/Krometrail/runtime/UI/font/filesystem/GPU-free and add only bounded PNG encoding plus SHA-256.
- [ ] Formatting and locked package/workspace check, test, and clippy gates pass, with concurrent unowned-file interference reported rather than edited.

## Implementation order

1. `epic-temporal-vision-toolkit-storyboard-selection`
2. `epic-temporal-vision-toolkit-storyboard-rendering` (depends on 1)
3. `epic-temporal-vision-toolkit-storyboard-public-contract-tests` (depends on 2)

The feature remains one cohesive owner and feature-review bundle. Stories are durable algorithm, rendering, and public-evidence checkpoints, not parallel worker assignments.

## Simplification

- Add one selection plan, one tiny raster helper, and one fixed PNG adapter; do not add strategy traits, renderer plugins, a scene graph, widgets, CSS, layout engine, host fonts, codec registry, async APIs, cache, storage sink, or filesystem behavior.
- Reuse `measure_pair`, `measure_adjacent`, `NormalizedSequence`, the existing normalization/threshold provenance, `ArtifactManifest`, stable registries, and source marker/gap contracts. Do not duplicate pixel metrics or provenance schemas.
- Use cumulative adjacent change for neighbor information gain instead of retaining an additional frame fingerprint or repeatedly rescanning all pixels for every greedy candidate.
- Keep all annotations outside source pixels and all exact text in the manifest; deterministic escaping/ellipsizing bounds the raster without pretending truncated display text is complete.
- No existing code or tests are obsolete. `ArtifactKind::{Storyboard, BeforeDuringAfter}` already exist and need no second registry.

## Testing

- **Selection interface:** exact synthetic IDs/reasons/omissions protect the feature's highest-risk contract—anchors, hard limits, tie order, gap partitioning, and change-aware fill—without involving PNG bytes.
- **Render/provenance interface:** one tiny generated artifact protects visible labels, source mapping, deterministic encoding/hash, and manifest agreement. This is valuable because a mismatch would make evidence untrustworthy.
- **Boundary regressions:** tiny render limits, marker/text overflow, timestamp ties, one-frame/unchanged sequences, and gaps at endpoints protect honest degraded behavior.
- **Private algorithm tests:** only inverse LUT tie behavior, font escaping, score tuple ordering, checked layout, and bounded writer merit colocated tests.
- **No low-value coverage:** no getter/derive matrix, giant golden image, exhaustive glyph test, duplicate `FrameSequence` validation, codec-library conformance suite, browser fixture, visual diagnosis assertion, or benchmark-success claim.

## Risks

- **Selection quality is not yet empirically proven:** The deterministic score may under-select small text changes or over-select legitimate broad motion. Versioning and provenance make evaluation comparable; `EVALUATION.md` decides whether v1 beats uniform sampling. The implementation must not claim effectiveness before that evidence.
- **Five core roles can exceed three tiles:** The explicit priority and omission manifest preserve honesty and the orientation spine, but a low tile limit can miss first-change/post-anchor frames. Callers can request more tiles or retrieve source frames; no hidden overflow is allowed.
- **Linear-to-sRGB and nearest-neighbor display can reduce fine detail:** Exact source IDs remain available, tiles never shrink below 160 px, and the artifact points to source evidence rather than inventing detail. Evaluation can justify a separately versioned scaling kernel later.
- **Long caller labels and many markers:** Fixed annotation rows require visible deterministic ellipsizing. Exact labels remain in the manifest, and the image explicitly directs readers there; layout memory remains bounded independently of input text length.
- **PNG byte stability depends on a pinned encoder version/settings:** Lock the dependency and set every relevant encoder option. A codec/settings change that alters bytes requires a new algorithm version or explicit compatibility evidence even if decoded pixels match.
- **Portrait or extreme aspect ratios:** A 12-tile horizontal strip can exceed height or make content too small. Checked limits fail with a recovery-oriented error so callers can reduce tile count or request a region; the renderer never silently distort-stretches.
- **Cumulative-change information gain is path-based:** Repeated changes can score highly even if a frame resembles a selected endpoint. This is a descriptive candidate heuristic, not a reversal or defect diagnosis, and evaluation can replace it only through a new algorithm version.

## Blockers

None. `epic-temporal-vision-toolkit-normalization-and-measurements` has completed verified implementation and is at feature review, so its `NormalizedSequence`, exact measurement, threshold, gap, and provenance contracts satisfy this feature dependency.

## Implementation notes

- Execution capability: raised/high (autopilot caller), because deterministic evidence selection, bounded image generation, and machine/visible provenance agreement form a public crate contract consumed by every temporal bundle.
- Review weight: standard (caller/autopilot); implementation stops at `stage: review` without self-approval.
- Dispatch: one cohesive owner implemented all three ordered checkpoints—selection, rendering, and public contract evidence—to keep scoring, visible labels, and manifests on one authority.
- Files changed: workspace and crate manifests/lock; `crates/temporal-vision/src/{artifact,encode,lib,normalize,render,select}.rs`; `src/render/{canvas,font}.rs`; and `tests/storyboard.rs`.
- Public contracts delivered: deterministic 3–12/default-8 representative selection with exact role/omission/tie/gap rules; shared storyboard/orientation generation; bounded RGB8 raster and PNG; embedded host-independent text; visible time/marker/gap semantics; and source-projected manifests with exact hashes.
- Tests added: 11 focused selection/render/encoding/font/layout tests plus four browser-free public tests covering anchor pressure, tied timestamps, both continuity segments, every score family, marker buckets, orientation fallback, decoded source colors, semantic raster regions, manifest round trip, fixed PNG hash, and tiny resource ceilings. The package now passes 33 tests across four suites.
- Simplification: one algorithm/version, one selection plan, one private canvas, one embedded font, and one pinned codec adapter; no strategy/renderer/font/codec registry, UI engine, filesystem, browser, Krometrail, async runtime, GPU, diagnosis, inference, or retained raster cache was added.
- Discrepancies from design: none. The 5×7 glyph marks occupy fixed 6×10 cells, matching the designed tiny deterministic 6×10 text raster while leaving stable spacing.
- Foundation alignment: existing visual-evidence, architecture, and evaluation assertions remain current; no foundation edit was required.
- Adjacent issues parked: none.

## Integrated verification

- `cargo fmt -p temporal-vision -- --check` — passed (through the package formatting stride).
- `cargo check -p temporal-vision --all-targets --locked` — passed.
- `cargo test -p temporal-vision --all-targets --locked` — passed, 33 tests.
- `cargo clippy -p temporal-vision --all-targets --locked -- -D warnings` — passed.
- `cargo tree -p temporal-vision --edges normal --locked` — only Serde/thiserror plus pinned `png = 0.17.16`, SHA-256, and their computation-only transitive dependencies; no Krometrail/browser/runtime/UI/font/filesystem/GPU dependency.
- Workspace format/check/test/clippy were each attempted. They were externally blocked by concurrently owned, unformatted and API-incomplete browser lifecycle/control changes in `krometrail-core`/`krometrail-cdp` (including page request field and `BrowserSessionPort` migration errors) plus one concurrent Clippy finding. No unowned file was edited; the locked temporal-vision package remained fully green.

## Review (2026-07-13)

**Verdict**: Approve with comments

**Blockers**: none
**Important**: none
**Nits**: ASCII arrow/dash fallbacks, broad `TemporalCoverage` labels, and a harmless long end-label
margin calculation are cosmetic. Full LUT monotonicity could gain a future invariant test.
**Rejected**: none

**Notes**: Standard-weight fresh-context review by GLM 5.2, grounded in source and fixture
verification. It reproduced 33 package tests, formatting, dependency independence, selection under
anchor pressure, gap partitioning, tie order, orientation fallback, bounded rendering, PNG/hash,
and manifest contracts. The reviewer disclosed that GLM also authored some temporal work, so this
is fresh-context rather than cross-model evidence. No material issue survived adjudication.
