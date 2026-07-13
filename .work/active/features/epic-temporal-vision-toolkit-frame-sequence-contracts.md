---
id: epic-temporal-vision-toolkit-frame-sequence-contracts
kind: feature
stage: review
tags: [visual]
parent: epic-temporal-vision-toolkit
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-13
updated: 2026-07-13
---

# Frame, Sequence, and Provenance Contracts

## Brief

This feature delivers the crate's input and provenance contracts: a generic `Frame`, an ordered `FrameSequence`, caller-supplied markers, declared capture gaps, regions, masks, and the `ArtifactManifest` that every generated artifact returns.

The contracts are browser-agnostic and infrastructure-free. They do not depend on Chrome, CDP, Krometrail storage, MCP, DOM, or framework types. A frame carries only an identifier, timestamp, dimensions, pixel format, and pixel payload. A sequence carries frames in deterministic order plus optional markers, region, mask, and gap annotations. Provenance records the artifact kind, evidence class, algorithm version, source and selected frame identifiers, omitted-frame count, range, markers, gaps, region, normalization, parameters, output dimensions, and output hash.

This feature does not decode pixels, measure change, or render images. It supplies the typed vocabulary that normalization, measurement, and rendering features share.

## Epic context

- Parent epic: `epic-temporal-vision-toolkit`
- Position in epic: foundation feature — every other feature depends on its contracts

## Simplification opportunity

- Keep the initial pixel format small and require callers to decode into one common RGBA8 representation. Avoid building an image-codec pipeline inside the crate; the crate operates on decoded pixels.
- Defer streaming or incremental sequence APIs. The first shape is an immutable batch sequence; callers with streaming needs can build that themselves until workloads prove otherwise.
- Treat inferred-analysis provenance as a distinct evidence-class label rather than a separate crate module.

## Foundation references

- `docs/VISION.md` — Reusable Temporal Vision
- `docs/ARCHITECTURE.md` — Temporal Visual Crate
- `docs/VISUAL-EVIDENCE.md` — Input Sequence, Shared Artifact Contract, Provenance, Determinism

## Design decisions

- **Identifier ownership:** Parameterize frame, marker, gap, and artifact identifiers independently instead of importing or cloning Krometrail UUID types. This keeps compile-time identity distinctions while allowing any caller-owned ID representation.
- **Time and tie ordering:** Use a crate-owned unsigned nanosecond `Timestamp`. `FrameSequence::new` requires nondecreasing frame timestamps and preserves caller order when timestamps tie; it never sorts or uses identifier ordering as hidden chronology. Marker and gap vectors follow the same caller-declared ordering rule.
- **Decoded input:** Support exactly tightly packed, row-major, straight-alpha sRGB RGBA8. `OwnedFrame` owns `Box<[u8]>`; `BorrowedFrame` borrows `&[u8]`. Both use the same validated generic `Frame<Id, Pixels>` and no codec abstraction.
- **Coordinate system:** Regions and masks use source-frame pixel coordinates: origin at the upper-left, `x` increases right, `y` increases down, and rectangles are half-open. Callers must map viewport, device-scale, DOM, or other coordinate systems before constructing a sequence.
- **Mask representation:** Store a deterministic row-major, MSB-first one-bit-per-pixel mask with zeroed padding bits. The complete mask is retained in provenance rather than only a digest, so the manifest remains reproducible without an external mask store.
- **Sequence invariants:** Require a nonempty sequence, unique frame IDs, common dimensions/format, exact payload lengths, in-range ordered markers, unique marker/gap IDs, ordered non-overlapping gaps within the frame range, an in-bounds region, and a full-frame mask. A region and mask combine by intersection.
- **Provenance construction:** Build manifests from a validated `FrameSequence`; derive source IDs, range, annotations, region, mask, source count, and omitted count instead of accepting duplicate caller-authored copies that can disagree.
- **Deterministic parameters:** Use a tagged recursive `ParameterValue` and `BTreeMap`-backed `Parameters`. Floating values pass through a finite-number newtype that canonicalizes negative zero; maps serialize in key order and cannot contain empty keys.
- **Output hash:** `OutputHash` is exactly a SHA-256 digest of the returned encoded artifact bytes, serialized as 64 lowercase hexadecimal characters. Hash computation belongs to renderers; this feature validates and carries the digest.
- **No UI surface:** This is a Rust library contract with no screen or flow, so no mockups are applicable.
- **Dispatch rationale:** Direct repository reading was sufficient: `temporal-vision` is currently a one-line boundary crate and the relevant implemented patterns are the core typed-ID registry, validating constructors, validated deserialization, and stable enum registries. No exploratory delegation was needed.

## Architectural choice

### Chosen: validated generic aggregates with one shared registry macro

Split the vocabulary into five cohesive modules: errors, decoded frames, pixel geometry, ordered sequences, and provenance. Public structs keep fields private and expose validating constructors plus read-only accessors. `Frame<Id, Pixels>` and `FrameSequence<FrameId, MarkerId, GapId, Pixels>` carry caller-selected ID and pixel-storage types without a trait object or adapter. A crate-local stable-registry macro generates every growing enum's serialized name, `ALL`, and `as_str` from one declaration. `ArtifactManifest::from_sequence` projects authoritative sequence data into provenance and accepts only artifact-specific choices.

This optimizes for a small, deterministic, infrastructure-free public surface. It reuses the foundation's established constructor/deserializer pattern without depending on `krometrail-core`.

### Rejected: one erased identifier and byte-source trait

A common string/UUID identifier plus `dyn PixelSource` would shorten type signatures, but it would lose marker/frame/artifact type distinctions, introduce dispatch and lifetime complexity, and invite filesystem or streaming behavior into what should remain an immutable in-memory batch.

### Rejected: generic image formats and normalization pipeline in `Frame`

A format enum covering RGB, grayscale, encoded PNG/JPEG, strides, and color profiles would move decoding and normalization policy into the foundation contract before evaluation justifies it. The selected RGBA8 shape is intentionally narrow; later formats can be added through the pixel-format registry only when a measured need outweighs the compatibility cost.

## Tricky unit first: deterministic sequence authority

`FrameSequence` is the load-bearing unit because every later measurement and artifact assumes that one ordering, coordinate space, and annotation set is authoritative. The constructor does not repair ambiguous inputs. It validates caller order, preserves timestamp ties in insertion order, rejects duplicate frame IDs, and requires every frame to have identical dimensions and pixel format. Markers are nondecreasing and contained in the inclusive first/last frame range. Gaps are nondecreasing, non-overlapping, and contained in that range; touching gap boundaries are allowed, but reversed or overlapping intervals are not. Region bounds use checked arithmetic, and a mask must cover the complete frame dimensions. This makes later parallel processing free to enumerate by sequence index without producing a different result.

## Implementation units

### Unit 1: Stable validation error boundary

**Files:**
- `crates/temporal-vision/src/error.rs` (new)
- `crates/temporal-vision/src/lib.rs` (replace boundary stub; add registry macro, modules, and explicit exports)

**Story:** `epic-temporal-vision-toolkit-frame-sequence-contracts-frame-and-geometry`

```rust
// error.rs
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum ErrorCode {
    InvalidDimensions,
    PixelLengthMismatch,
    EmptySequence,
    DuplicateIdentifier,
    OutOfOrder,
    IncompatibleFrame,
    AnnotationOutOfRange,
    InvalidRegion,
    InvalidMask,
    InvalidParameter,
    InvalidManifest,
    InvalidOutputHash,
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error, serde::Serialize, serde::Deserialize)]
#[error("{code}: {message}")]
pub struct VisionError {
    pub code: ErrorCode,
    pub message: Box<str>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<usize>,
}

pub type Result<T, E = VisionError> = std::result::Result<T, E>;
```

`ErrorCode` is declared through the crate-local stable registry with snake-case wire names. `VisionError` never formats generic IDs or pixel content into messages. Internal helpers create nonempty static/programmer-authored messages and optionally identify the invalid collection index. Deserialization of invariant-bearing public types must call the same constructors used by Rust callers.

**Acceptance criteria:**
- [ ] The crate exports an infrastructure-neutral `Result`, `VisionError`, and stable `ErrorCode` registry with exhaustive name/serde tests.
- [ ] Invalid data returns a specific stable code without leaking caller IDs, pixels, paths, or implementation sources.
- [ ] Public validated types cannot be deserialized around their constructors.

### Unit 2: Decoded RGBA8 frame and geometry contracts

**Files:**
- `crates/temporal-vision/src/frame.rs` (new)
- `crates/temporal-vision/src/geometry.rs` (new)

**Story:** `epic-temporal-vision-toolkit-frame-sequence-contracts-frame-and-geometry`

```rust
// frame.rs
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct Timestamp(u64); // nanoseconds in one caller-declared sequence clock

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
pub struct PixelDimensions { /* non-zero width and height */ }
impl PixelDimensions {
    pub fn new(width: u32, height: u32) -> Result<Self>;
    pub const fn width(self) -> u32;
    pub const fn height(self) -> u32;
    pub fn pixel_count(self) -> Result<usize>;
    pub fn rgba8_byte_len(self) -> Result<usize>;
}

// Stable registry; initial wire name is `rgba8_srgb_straight`.
pub enum PixelFormat { Rgba8SrgbStraight }

pub struct Frame<Id, Pixels> {
    id: Id,
    timestamp: Timestamp,
    dimensions: PixelDimensions,
    pixel_format: PixelFormat,
    pixels: Pixels,
}

pub type OwnedFrame<Id> = Frame<Id, Box<[u8]>>;
pub type BorrowedFrame<'a, Id> = Frame<Id, &'a [u8]>;

impl<Id, Pixels: AsRef<[u8]>> Frame<Id, Pixels> {
    pub fn new(
        id: Id,
        timestamp: Timestamp,
        dimensions: PixelDimensions,
        pixel_format: PixelFormat,
        pixels: Pixels,
    ) -> Result<Self>;
    pub fn id(&self) -> &Id;
    pub const fn timestamp(&self) -> Timestamp;
    pub const fn dimensions(&self) -> PixelDimensions;
    pub const fn pixel_format(&self) -> PixelFormat;
    pub fn pixels(&self) -> &[u8];
    pub fn as_borrowed(&self) -> BorrowedFrame<'_, &Id>;
    pub fn into_parts(self) -> (Id, Timestamp, PixelDimensions, PixelFormat, Pixels);
}

impl<Id: Clone, Pixels: AsRef<[u8]>> Frame<Id, Pixels> {
    pub fn to_owned(&self) -> OwnedFrame<Id>;
}

// geometry.rs
pub struct PixelRect { /* x/y plus non-zero width/height */ }
impl PixelRect {
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Result<Self>;
    pub fn right_exclusive(self) -> Result<u32>;
    pub fn bottom_exclusive(self) -> Result<u32>;
    pub fn fits_within(self, dimensions: PixelDimensions) -> bool;
}

pub struct FrameRegion { rect: PixelRect }
impl FrameRegion {
    pub fn new(rect: PixelRect, frame_dimensions: PixelDimensions) -> Result<Self>;
    pub const fn rect(self) -> PixelRect;
}

pub struct BinaryMask {
    dimensions: PixelDimensions,
    bits: Box<[u8]>,
}
impl BinaryMask {
    pub fn new(dimensions: PixelDimensions, bits: impl Into<Box<[u8]>>) -> Result<Self>;
    pub const fn dimensions(&self) -> PixelDimensions;
    pub fn bits(&self) -> &[u8];
    pub fn includes(&self, x: u32, y: u32) -> Option<bool>;
}
```

`Timestamp` supplies ordering only; it does not claim wall-clock or Krometrail session-clock identity. RGBA bytes are tightly packed in rows with no stride: byte order is R, G, B, A for each pixel, the color channels are sRGB encoded, and alpha is straight. Length uses checked `width * height * 4` arithmetic. `PixelRect` is half-open. `BinaryMask` stores `ceil(width * height / 8)` bytes in row-major pixel order, with the first pixel in the most-significant bit of byte zero; every unused low bit in the final byte must be zero.

**Acceptance criteria:**
- [ ] Owned and borrowed payloads use one `Frame` implementation and expose identical metadata/pixels without a codec, trait object, async API, filesystem, or Krometrail type.
- [ ] Zero/overflowing dimensions and payload lengths other than exact RGBA8 size fail deterministically.
- [ ] Rectangles use checked half-open bounds; masks validate byte count and trailing padding and have deterministic pixel lookup.
- [ ] Pixel format and its byte semantics have one registry-backed serialized name.

### Unit 3: Ordered sequences, markers, and declared gaps

**File:** `crates/temporal-vision/src/sequence.rs` (new)

**Story:** `epic-temporal-vision-toolkit-frame-sequence-contracts-sequence-and-annotations`

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
pub struct TimeRange { start: Timestamp, end: Timestamp } // inclusive
impl TimeRange {
    pub fn new(start: Timestamp, end: Timestamp) -> Result<Self>;
    pub const fn start(self) -> Timestamp;
    pub const fn end(self) -> Timestamp;
    pub const fn contains(self, timestamp: Timestamp) -> bool;
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct Marker<Id> {
    id: Id,
    timestamp: Timestamp,
    kind: NonEmptyText,
    label: NonEmptyText,
}
impl<Id> Marker<Id> {
    pub fn new(id: Id, timestamp: Timestamp, kind: impl Into<String>, label: impl Into<String>) -> Result<Self>;
    // read-only accessors
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct DeclaredGap<Id> {
    id: Id,
    range: TimeRange,
    reason: NonEmptyText,
    estimated_missing_frames: Option<std::num::NonZeroU64>,
}
impl<Id> DeclaredGap<Id> {
    pub fn new(
        id: Id,
        range: TimeRange,
        reason: impl Into<String>,
        estimated_missing_frames: Option<std::num::NonZeroU64>,
    ) -> Result<Self>;
    // read-only accessors
}

pub struct FrameSequence<FrameId, MarkerId, GapId, Pixels> {
    frames: Box<[Frame<FrameId, Pixels>]>,
    markers: Box<[Marker<MarkerId>]>,
    gaps: Box<[DeclaredGap<GapId>]>,
    region: Option<FrameRegion>,
    mask: Option<BinaryMask>,
}

pub type OwnedFrameSequence<F, M, G> = FrameSequence<F, M, G, Box<[u8]>>;
pub type BorrowedFrameSequence<'a, F, M, G> = FrameSequence<F, M, G, &'a [u8]>;

impl<F: Eq, M: Eq, G: Eq, P: AsRef<[u8]>> FrameSequence<F, M, G, P> {
    pub fn new(
        frames: Vec<Frame<F, P>>,
        markers: Vec<Marker<M>>,
        gaps: Vec<DeclaredGap<G>>,
        region: Option<FrameRegion>,
        mask: Option<BinaryMask>,
    ) -> Result<Self>;
    pub fn frames(&self) -> &[Frame<F, P>];
    pub fn markers(&self) -> &[Marker<M>];
    pub fn gaps(&self) -> &[DeclaredGap<G>];
    pub const fn region(&self) -> Option<FrameRegion>;
    pub fn mask(&self) -> Option<&BinaryMask>;
    pub fn range(&self) -> TimeRange;
    pub fn dimensions(&self) -> PixelDimensions;
    pub fn frame_by_id(&self, id: &F) -> Option<&Frame<F, P>>;
}
```

Uniqueness checks are equality-based linear scans because temporal batches are bounded and this avoids requiring caller IDs to implement `Hash`, `Ord`, `Debug`, or serialization merely to analyze pixels. Frame and annotation order is accepted only when nondecreasing. Tied entries retain their exact vector order. Gaps may touch at a boundary but may not overlap (`previous.end() > next.start()`); calculations in later features split at every gap and do not interpret missing time as stability. Marker kinds remain validated caller labels and never become browser-specific enums.

**Acceptance criteria:**
- [ ] Empty, out-of-order, duplicate-ID, mixed-dimension, mixed-format, out-of-range annotation, overlapping-gap, invalid-region, and mismatched-mask sequences are rejected with stable errors and index context where useful.
- [ ] Timestamp ties preserve input order through iteration and serialization; neither IDs nor parallel execution can reorder frames.
- [ ] A valid borrowed sequence performs no pixel copy; conversion to owned is explicit.
- [ ] Declared gaps remain data, not inferred from timestamps or identifier/ordinal arithmetic.

### Unit 4: Stable provenance registries and deterministic parameters

**File:** `crates/temporal-vision/src/provenance.rs` (new)

**Story:** `epic-temporal-vision-toolkit-frame-sequence-contracts-provenance-manifest`

```rust
// Each enum is one stable registry with `ALL`, `as_str`, Display, and serde names.
pub enum ArtifactKind {
    BeforeDuringAfter,
    Storyboard,
    DifferenceMap,
    RegionFilmstrip,
    MotionHistory,
}
pub enum EvidenceClass { SourceFrame, SourceDerived, Inferred }
pub enum NormalizationKind {
    ColorSpaceConversion,
    AlphaCompositing,
    IntegerScaling,
    FixedCrop,
    Denoising,
    Thresholding,
}

pub struct AlgorithmDescriptor {
    name: NonEmptyText,
    version: NonEmptyText,
}
impl AlgorithmDescriptor {
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Result<Self>;
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
#[serde(transparent)]
pub struct FiniteNumber(f64);
impl FiniteNumber {
    pub fn new(value: f64) -> Result<Self>; // reject non-finite; normalize -0.0 to 0.0
    pub const fn get(self) -> f64;
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum ParameterValue {
    Bool(bool),
    Signed(i64),
    Unsigned(u64),
    Number(FiniteNumber),
    Text(Box<str>),
    List(Vec<ParameterValue>),
    Object(std::collections::BTreeMap<Box<str>, ParameterValue>),
}

pub struct Parameters(std::collections::BTreeMap<Box<str>, ParameterValue>);
impl Parameters {
    pub fn new(values: std::collections::BTreeMap<Box<str>, ParameterValue>) -> Result<Self>;
    pub fn empty() -> Self;
    pub fn get(&self, name: &str) -> Option<&ParameterValue>;
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, &ParameterValue)>;
}

pub struct NormalizationStep {
    kind: NormalizationKind,
    algorithm_version: NonEmptyText,
    parameters: Parameters,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct OutputHash([u8; 32]); // SHA-256 of exact returned encoded artifact bytes
impl OutputHash {
    pub const fn from_bytes(bytes: [u8; 32]) -> Self;
    pub const fn as_bytes(&self) -> &[u8; 32];
}
impl std::fmt::Display for OutputHash { /* lowercase hex */ }
impl std::str::FromStr for OutputHash { type Err = VisionError; /* exactly 64 lowercase hex */ }
```

Wire names are: artifacts `before_during_after`, `storyboard`, `difference_map`, `region_filmstrip`, `motion_history`; evidence `source_frame`, `source_derived`, `inferred`; normalization `color_space_conversion`, `alpha_compositing`, `integer_scaling`, `fixed_crop`, `denoising`, `thresholding`. Custom deserialization validates nonempty text, finite numbers, nonempty parameter keys at every nested object, and output-hash form. Lists and normalization steps preserve declared order; maps serialize lexicographically.

**Acceptance criteria:**
- [ ] Every registry derives exhaustive stable-name, display, and serde coverage from one declaration.
- [ ] Parameters cannot contain NaN/infinity, negative zero ambiguity, or empty object keys, and nested maps serialize in deterministic key order.
- [ ] Output hashes round-trip as exactly lowercase SHA-256 hex and reject wrong length, uppercase, or non-hex text.
- [ ] No registry duplicates artifact/evidence/normalization variants in validation or display code.

### Unit 5: Manifest projection from authoritative sequence

**File:** `crates/temporal-vision/src/provenance.rs`

**Story:** `epic-temporal-vision-toolkit-frame-sequence-contracts-provenance-manifest`

```rust
pub struct ArtifactManifest<ArtifactId, FrameId, MarkerId, GapId> {
    artifact_id: ArtifactId,
    artifact_kind: ArtifactKind,
    evidence_class: EvidenceClass,
    algorithm: AlgorithmDescriptor,
    source_frame_ids: Box<[FrameId]>,
    selected_frame_ids: Box<[FrameId]>,
    source_frame_count: u64,
    omitted_frame_count: u64,
    range: TimeRange,
    markers: Box<[Marker<MarkerId>]>,
    gaps: Box<[DeclaredGap<GapId>]>,
    region: Option<FrameRegion>,
    mask: Option<BinaryMask>,
    normalization: Box<[NormalizationStep]>,
    parameters: Parameters,
    output_dimensions: PixelDimensions,
    output_hash: OutputHash,
}

impl<A, F: Clone + Eq, M: Clone, G: Clone> ArtifactManifest<A, F, M, G> {
    #[allow(clippy::too_many_arguments)]
    pub fn from_sequence<P: AsRef<[u8]>>(
        artifact_id: A,
        artifact_kind: ArtifactKind,
        evidence_class: EvidenceClass,
        algorithm: AlgorithmDescriptor,
        sequence: &FrameSequence<F, M, G, P>,
        selected_frame_ids: Vec<F>,
        normalization: Vec<NormalizationStep>,
        parameters: Parameters,
        output_dimensions: PixelDimensions,
        output_hash: OutputHash,
    ) -> Result<Self>;
    // read-only accessors for every field
}
```

`from_sequence` clones the source IDs and all annotations in authoritative order, clones region/mask, verifies that selected IDs form a unique ordered subsequence of source IDs, checks count conversions, and computes `source_frame_count` and `omitted_frame_count`. It never sorts selected IDs. A private `validate` backs custom generic deserialization so persisted manifests cannot claim contradictory counts, duplicate/reordered selected IDs, invalid annotation order/range, or invalid geometry. The entire manifest is serializable when caller IDs are serializable; frame pixels are never embedded. Visible render labels are later derived from this manifest rather than separately authored.

**Acceptance criteria:**
- [ ] A manifest cannot disagree with its sequence about source order, range, annotations, gaps, region, mask, or counts at construction.
- [ ] Selected IDs must be a unique ordered subsequence; omitted count is computed with checked conversion rather than caller supplied.
- [ ] The manifest round-trips through JSON for string and UUID-like caller IDs, and malformed count/order/hash/geometry data fails deserialization.
- [ ] The mask needed for reproduction is present in the machine-readable manifest, while source pixel payloads are represented only by ordered IDs.

### Unit 6: Public contract and independence tests

**Files:**
- `crates/temporal-vision/tests/contracts.rs` (new)
- `crates/temporal-vision/Cargo.toml` (add `serde_json` only as a dev-dependency if needed)

**Story:** `epic-temporal-vision-toolkit-frame-sequence-contracts-public-contract-tests`

Build one 2x2 synthetic RGBA sequence using distinct local newtype IDs, tied timestamps, two markers, one declared gap, a half-frame region, and a binary mask. Exercise borrowed construction, explicit owned conversion, sequence iteration, manifest creation, deterministic JSON bytes, and JSON round-trip. The test must compile without importing any Krometrail crate. Colocated tests cover malformed constructors and each registry once; the integration test protects the stable consumer seam.

**Acceptance criteria:**
- [ ] A browser-free synthetic consumer can construct borrowed and owned sequences and a complete manifest using its own strongly typed IDs.
- [ ] Repeated serialization of identical inputs produces identical bytes and preserves tied frame/marker order.
- [ ] Focused malformed cases cover wrong RGBA length, duplicate/out-of-order frames, annotation range/order, gap overlap, region overflow, mask padding, non-finite parameters, selected-ID reordering, and malformed persisted manifests.
- [ ] `cargo tree -p temporal-vision --edges normal` contains no Krometrail crate, CDP, MCP, Tokio, filesystem adapter, or image codec.
- [ ] `cargo fmt --all -- --check`, `cargo check -p temporal-vision --all-targets --locked`, `cargo test -p temporal-vision --all-targets --locked`, and `cargo clippy -p temporal-vision --all-targets --locked -- -D warnings` pass.

## Implementation order

1. `epic-temporal-vision-toolkit-frame-sequence-contracts-frame-and-geometry`
2. `epic-temporal-vision-toolkit-frame-sequence-contracts-sequence-and-annotations` (depends on 1)
3. `epic-temporal-vision-toolkit-frame-sequence-contracts-provenance-manifest` (depends on 2)
4. `epic-temporal-vision-toolkit-frame-sequence-contracts-public-contract-tests` (depends on 3)

The feature remains one cohesive implementation and review bundle. Stories are durable contract checkpoints, not separate worker assignments.

## Simplification

- Retain the crate's current single library boundary; add only modules that own a distinct public concept. Do not add ports, builders, codec traits, streaming traits, async wrappers, sinks, or feature flags.
- Use one generic frame for owned and borrowed pixels instead of parallel structs and conversion code.
- Use one registry macro for every growing stable enum and one validating constructor/deserializer path per invariant-bearing type.
- Build provenance from the sequence instead of maintaining a second caller-populated copy of sequence metadata.
- Do not copy `krometrail-core` IDs, clocks, errors, or capture-gap reasons. The visual crate's caller IDs, sequence clock, and free-text marker/gap labels are intentionally independent.
- No existing tests or abstractions need removal; `temporal-vision` currently contains only its boundary comment.

## Testing

- **Public seam:** one synthetic integration test proves generic typed IDs, owned/borrowed payloads, tied ordering, annotations, geometry, mask, manifest projection, deterministic serialization, and crate independence together. This is the highest-value test because every later feature consumes this seam.
- **Invariant regression:** table-driven constructor/deserializer cases protect exact byte sizing, checked geometry, ordering, uniqueness, range containment, non-overlapping gaps, finite deterministic parameters, selected-subsequence validation, and output hash shape.
- **Registry contract:** one generated loop per stable registry protects names and serde round trips; do not write a test per enum variant.
- **No low-value coverage:** omit tests for trivial getters, derives, and type aliases. Do not snapshot debug output or private struct layout.
- **Dependency evidence:** inspect normal dependencies with `cargo tree`; compilation of a consumer using only local ID newtypes proves no Krometrail identity dependency.

## Risks

- **Generic serde complexity:** Validated generic deserialization can accumulate difficult bounds. Mitigation: use private wire structs with explicit serde bounds and one `validate` path; if this becomes disproportionate, keep runtime frame payloads non-deserializable while preserving validated manifest deserialization, which is the durable machine-readable boundary.
- **RGBA8 narrowness:** High-bit-depth, grayscale, premultiplied-alpha, or color-managed callers must convert before entry. This is deliberate and reversible through the pixel-format registry after evaluation; accepting ambiguous bytes now would be harder to correct.
- **Timestamp semantics:** A bare nanosecond value cannot prove shared clock origin. The type documentation and constructor require one caller-declared sequence clock; cross-clock normalization remains outside this crate.
- **Large masks in manifests:** Full bit masks increase manifest size, but they retain reproducibility and use one bit per pixel. If measured manifests become problematic, a future content-addressed mask reference can be added without silently dropping the mask today.
- **Inclusive gaps at retained-frame boundaries:** A declared gap may share a boundary timestamp with a retained frame. Later calculations must split at the gap record rather than infer continuity from timestamp adjacency; tests preserve this rule.
- **Riskiest assumption:** One common frame geometry is sufficient for the first artifact pipeline. Foundation docs require epochs to split on incompatible dimensions unless explicit normalization is requested, so rejecting mixed geometry is safer than silently stretching; the next normalization feature can define an explicit pre-sequence adaptation only if needed.

## Blockers

None. The implemented crate remains an immutable browser-agnostic batch boundary, and all child dependencies completed in order.

## Implementation summary

- Execution capability: highest/raised (caller-selected) because this crate-wide public contract is the dependency foundation for every temporal artifact implementation.
- Review weight: standard (caller/autopilot); the feature is intentionally left at `stage: review` for the parent orchestrator's fresh review lane.
- Dispatch: one cohesive owner implemented all four sequential child checkpoints to preserve generic type and serde context across the public boundary.
- Files changed: `crates/temporal-vision/Cargo.toml`, `crates/temporal-vision/src/{lib,error,frame,geometry,sequence,provenance}.rs`, `crates/temporal-vision/tests/contracts.rs`, and `Cargo.lock`.
- Public contracts delivered: caller-owned generic identifiers; owned/borrowed validated RGBA8 frames; checked dimensions, regions, and complete bit masks; immutable ordered frame sequences with markers and declared gaps; stable registries; deterministic parameters; canonical output hashes; and sequence-projected manifests with validated persistence.
- Tests added: 12 focused unit/integration tests across two suites, including a browser-free typed-ID consumer, no-copy borrowing, explicit ownership, tied-order stability, deterministic JSON, registry round trips, constructor invariants, and malformed persisted manifests.
- Simplification: no codec, image, browser, Krometrail, filesystem, async, streaming, or trait-object system was added. Normal dependencies remain only `serde` and `thiserror`.
- Design deviations: `Timestamp` includes the necessary public `ZERO`, `from_nanos`, and `as_nanos` API; manifest marker/gap IDs require `Eq` to revalidate persisted uniqueness; `FrameRegion` revalidates retained rectangle invariants on deserialize and defers source-frame containment to sequence construction because it intentionally stores no duplicate dimensions.
- Focused verification: `cargo fmt -p temporal-vision -- --check`, locked package check/test/clippy, and dependency-tree inspection all passed; 12 tests passed.
- Workspace verification: locked workspace check passed and 213 workspace tests passed. Workspace formatting and clippy were temporarily blocked only by concurrent unowned browser-operation work (`crates/krometrail-cdp/src/control/**` formatting; a `krometrail-core/src/browser/operation.rs` large-enum lint). No failing result originated in `temporal-vision`, and no unowned file was changed or staged for this feature.
- Adjacent issues parked: none.
