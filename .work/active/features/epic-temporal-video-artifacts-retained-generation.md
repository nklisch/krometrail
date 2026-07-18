---
id: epic-temporal-video-artifacts-retained-generation
kind: feature
stage: review
tags: [visual, storage, security]
parent: epic-temporal-video-artifacts
depends_on: [epic-temporal-video-artifacts-clip-contracts]
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Retained temporal video generation

## Brief

Build the bounded application service that reads an exact resolved range, partitions compatible visual epochs, creates the canonical presentation plan, adapts source frames and explicit gap slates, invokes an injected encoder, and publishes the resulting MP4 plus typed manifest. It must include encoder identity in cache validation, preserve cancellation/deletion races, reject partial or contradictory output, and make the retained clip and provenance readable through the existing evidence authority.

Generalize the image-only artifact persistence boundary additively so existing PNG artifacts and retained database rows remain readable. This feature owns no FFmpeg discovery, concrete process execution, MCP tool registration, host upload, or agent-facing setup prose.

## Epic context

- Parent epic: `epic-temporal-video-artifacts`
- Position in epic: application/storage consumer of `epic-temporal-video-artifacts-clip-contracts`; uses a fake encoder port and can proceed in parallel with the production FFmpeg runtime

## Simplification opportunity

- Extend the current artifact publication, cache, SQLite index, retention, recovery, deletion, and resource-read authority for another validated media/manifest variant instead of creating a video database, storage root, URI grammar, or cleanup subsystem.

## Foundation references

- `docs/SPEC.md` — Disk Budget and Retention, Temporal Ranges, Artifact Provenance, and Local Data
- `docs/ARCHITECTURE.md` — Recording Store, Temporal Range Resolution, Artifact Generation, and Failure Isolation
- `docs/VISUAL-EVIDENCE.md` — Input Sequence, Temporal Video Clip, Capture Gaps, and Provenance
- `docs/EVALUATION.md` — Storage and Retention Evaluation and Temporal video evaluation

## Parent decisions inherited

- Video consumes authoritative retained source frames and never becomes a second capture path.
- One canonical plan supplies both encoder input and manifest timing, including visible gap slates and held-frame disclosure.
- Existing image artifact compatibility is a stable 1.x boundary; the storage change is additive.
- Encoded bytes are reusable only under the exact adapter/build/encoder identity.

## Readiness and dispatch

- `epic-temporal-video-artifacts-clip-contracts` has all three child stories done with workspace fmt/check/test/clippy evidence recorded. Under autopilot dependency semantics that is completed verified implementation, so this feature is ready even if its feature-level stage is temporarily reopened for receiver adjudication.
- `.work/bin/work-view` is not usable from the checked-in host tool in this run. The parent/feature/story frontmatter graph was inspected directly. The new child nodes only depend on their parent feature's already-ordered children and introduce no path back to themselves, so the dependency graph remains acyclic.
- Direct source reading was used because the relevant artifact service, store adapter, migration registry, recovery, and tests are bounded and already expose the required patterns. The active autopilot delegation prohibits nested agents and peeragent.
- Worker capability remains the highest available capability selected by autopilot because retained-data migration, cache identity, and cancellation/deletion publication races are high-consequence. Review weight remains `standard` from the autopilot default: one later independent feature pass, then receiver adjudication, fixes, verification, and finish without re-review.

## Design decisions

- **Additive typed projections over one private storage engine**: keep the existing image `ArtifactPublication`, `StoredArtifact`, `ArtifactEvidenceHandle`, `ArtifactRead`, and their serialized provenance unchanged. Add video-specific publication/read/result types and methods to the same `ArtifactStore` port, then make `RecordingStore` route both through one private staging, file, cache-lock, validation, recovery, retention, and deletion implementation. A tagged replacement for the image types was rejected because it would change stable 1.x image JSON; a `VideoStore` or second table was rejected because it would duplicate lifetime authority.
- **Schema v6 widens instead of purging**: rebuild the v5 `artifacts` and `artifact_frames` tables transactionally with `temporal_video`/`video/mp4` added to the closed kind/media constraint, copy every existing row and source link unchanged, recreate the same indexes, and advance the ordered migration registry to v6. Editing schema v4 or deleting legacy image artifacts is forbidden.
- **One file directory and deletion journal**: image rows continue to name `<artifact-id>.png`; video rows name `<artifact-id>.mp4`; both stage through `<artifact-id>.tmp` in the existing private `artifacts/` directory and are removed by the existing `DeletionObjectKind::Artifact` journal. File paths remain store-derived and never enter the core request or manifest.
- **Separate video resource handle preserves image compatibility**: add `VideoArtifactEvidenceHandle`/`VideoArtifactRead` rather than widening `ArtifactEvidenceHandle.provenance` into a tagged enum. The later MCP surface can publish the new handle without changing any existing still resource response or URI grammar.
- **Resolved range is already the request boundary**: `TemporalVideoGenerationRequest` accepts one validated `ResolvedRange`, policy, and the existing `OutputLimitsRequest`. It rejects more than 30 seconds, more than 120 total source frames, dimensions beyond 1920x1080, dimensions too small to form an even H.264 canvas, and encoded-byte limits beyond 64 MiB before source I/O.
- **One returned clip per visual epoch, all-or-error response**: incompatible geometry/device-scale epochs produce ordered retained clips, each with its own plan, manifest, and cache disposition. The service returns a result only when every epoch succeeds. A valid clip published before a later epoch fails may remain as an unexposed reusable cache entry; it is source-linked, budget-accounted, and normally evicted/deleted. The service never returns an ambiguous partial-success result or attempts an unsafe compensating delete that could race another cache consumer.
- **Meaningful states reuse the still selector**: real-time plans carry no meaningful-frame selection. Model-optimized plans reuse `temporal_vision::select_storyboard_frames` with a closed `temporal-video-meaningful-selection-v1` profile, twelve-tile ceiling, epoch-local clamped resolved anchor, declared gaps, deterministic thumbnail normalization, and fixed noise parameters. The manifest records a bounded selector identity and its parameter hash; the plan records exact selected frame IDs. No provider sampling heuristic or caller-supplied importance list is added.
- **Visibility is decided by the planner**: for model-optimized clips, first build the real-time plan with no meaningful IDs and retain only selector IDs that appear in source-frame presentation segments. This avoids claiming a hold for a frame fully replaced by a declared gap while leaving the pure planner as the single authority for gap overlap.
- **Canonical plan precedes cache lookup**: source fingerprints, exact plan, selector identity, output profile, and `TemporalVideoEncoder::identity()` produce the cache key before encoding. This means a model-optimized cache hit still performs bounded thumbnail selection; accepting that cost keeps the cache contract exactly reproducible and avoids a second preliminary-key/index scheme.
- **Per-key encoding lock, not a second cache**: one weak per-key async lock in the service serializes the lookup/encode/publish window and performs a second lookup after acquiring it. It prevents duplicate external encodes while the persistent cache remains exclusively owned by `ArtifactStore`.
- **Original source bytes feed source segments**: source-frame segments reuse the retained JPEG/PNG `Arc<[u8]>` and declared source dimensions. The encoder adapter owns the plan-declared scale, one-pixel trailing pad, H.264 conversion, and MP4 validation. Gap segments receive deterministic PNG slates at the exact canvas dimensions. The service does not decode/re-encode ordinary frames merely to feed FFmpeg.
- **Gap slates are deterministic and visibly non-source**: `src/video/slate.rs` renders a high-contrast patterned PNG labeled `CAPTURE GAP` plus the source-time interval from only the typed gap segment. The slate contains no page content, filesystem path, error text, or provider data. Its pixels are encoder input only; the manifest's gap IDs/range remain authoritative.
- **Selector decoding is streaming and bounded**: model-optimized selection decodes one source image at a time through the existing strict JPEG/PNG decoder, scales it to a small fixed analysis canvas, and drops the full-size pixels before retaining thumbnails. Real-time generation performs no visual decode. Request, blocking-work, analysis-memory, input-byte, and wall-time limits remain independent of capture ingestion.
- **Structured cancellation owns the encoder future**: the service derives one absolute deadline, passes an internal `WorkCancellation` into `VideoEncodingContext`, and explicitly checks caller cancellation/deadline before source work, after planning, after encoding, and before publication. On cancellation/deadline it signals the encoder token and waits for the port future's bounded cleanup contract; the FFmpeg adapter remains responsible for killing/reaping its process before returning. A fake encoder proves the application behavior without launching FFmpeg.
- **Deletion is a publication fence, not a new generation registry**: session deletion may complete while bounded selection or encoding is still running. The existing store deletion marker causes the later video publication guard to reject the deleted session, so late work cannot recreate files, rows, or usage. Adding a second session-work lease/drain registry was rejected: it would make deletion latency depend on an optional external encode, while the deadline plus no-republication fence supplies the required bounded safety.
- **Manifest selection provenance is additive pre-release video work**: extend `TemporalVideoManifest` and `canonical_video_cache_parameters` with `Option<VideoSelectionIdentity>`. Real-time requires `None` and no meaningful IDs; model-optimized requires `Some` and selected IDs. Temporal video has not shipped yet, so this completes its schema-v1 contract rather than migrating retained video data; existing image manifests are untouched.
- **Production validation remains at the adapter boundary**: the retained store verifies typed manifest reconstruction, media kind, length, source links/hashes, output hash, and exact encoder/profile/cache identity. It does not implement a second MP4/H.264 parser. The qualified production adapter proves the media contract; deterministic service/store tests use a fake encoder.
- **No UI surface**: this is a core/application/storage capability with no screen, page, modal, or flow. The parent epic explicitly records no UI work, so feature-tier mockups do not apply.

## Architectural choice

Three approaches were considered:

1. **Replace image artifact types with one tagged public enum.** One generic publication/read API would minimize method count, but it would change persisted and MCP-facing image provenance shapes and force every still consumer to handle an impossible video branch. That is disproportionate for a stable 1.x surface.
2. **Create a video store and video index.** Isolation would make initial implementation locally simple, but cache locks, usage accounting, recovery, corruption invalidation, source eviction, session deletion, and resource reads would acquire competing authorities. This directly violates the feature and foundation boundary.
3. **Add video-specific core projections over one generalized private artifact engine.** Image contracts stay byte-for-byte stable; video receives strong manifest/read types; SQLite rows, files, locks, recovery, retention, and deletion remain singular. The private implementation absorbs the small variant dispatch.

Choose approach 3. It pays a bounded type/API cost to preserve the concrete stable consumer while keeping every retained-data lifetime rule in one place.

The trickiest unit is the store generalization, not the encoder call. A migration or variant-dispatch mistake can invalidate already-retained images or let a video survive source/session deletion. It is designed and verified first; the application service then consumes that exact port.

## Implementation units

### Unit 1: Additive video provenance and retained-resource contracts

**Files**: `crates/krometrail-core/src/video/manifest.rs`, `crates/krometrail-core/src/video/generation.rs`, `crates/krometrail-core/src/video/mod.rs`, `crates/krometrail-core/src/ports/artifacts.rs`, `crates/krometrail-core/src/ports/mod.rs`, `crates/krometrail-core/src/lib.rs`

**Story**: `epic-temporal-video-artifacts-retained-generation-additive-artifact-persistence`

```rust
pub const VIDEO_MEANINGFUL_SELECTOR_NAME: &str = "temporal-video-meaningful-selection";
pub const VIDEO_MEANINGFUL_SELECTOR_VERSION: &str = "v1";

pub struct VideoSelectionIdentity {
    name: NonEmptyText,
    version: NonEmptyText,
    parameters_sha256: [u8; 32],
}

pub struct TemporalVideoGenerationRequest {
    range: ResolvedRange,
    policy: VideoPresentationPolicy,
    output: OutputLimitsRequest,
}

impl TemporalVideoGenerationRequest {
    pub fn new(
        range: ResolvedRange,
        policy: VideoPresentationPolicy,
        output: OutputLimitsRequest,
    ) -> Result<Self>;
}

pub struct VideoArtifactEvidenceHandle {
    pub artifact_id: ArtifactId,
    pub scope: EvidenceScope,
    pub media_type: NonEmptyText,
    pub content_sha256: Sha256Digest,
    pub encoded_byte_len: u64,
    pub provenance: TemporalVideoManifest,
}

pub struct VideoArtifactRead {
    pub handle: VideoArtifactEvidenceHandle,
    encoded_bytes: Arc<[u8]>,
}

pub struct TemporalVideoGenerationClip {
    pub epoch_index: u32,
    pub cache: ArtifactCacheDisposition,
    pub artifact: VideoArtifactEvidenceHandle,
}

pub struct TemporalVideoGenerationResult {
    pub range: ResolvedRange,
    pub clips: Vec<TemporalVideoGenerationClip>,
}

pub trait TemporalVideoGeneration: Send + Sync {
    fn generate_video(
        &self,
        request: TemporalVideoGenerationRequest,
        context: ArtifactGenerationContext,
    ) -> PortFuture<'_, Result<TemporalVideoGenerationResult>>;
}

pub struct VideoArtifactPublication {
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub sources: Vec<ArtifactSourceFingerprint>,
    pub cache: ArtifactCacheMetadata,
    pub manifest: TemporalVideoManifest,
    pub encoded_bytes: Arc<[u8]>,
    cancellation: Option<Arc<dyn CancellationSignal>>,
}

pub struct StoredVideoArtifact {
    pub cache: ArtifactCacheMetadata,
    pub manifest: TemporalVideoManifest,
    pub encoded_bytes: Arc<[u8]>,
}

pub enum VideoArtifactLookup { Miss, Hit(Box<StoredVideoArtifact>), Invalidated }
pub enum VideoArtifactPublish {
    Published(StoredVideoArtifact),
    Existing(StoredVideoArtifact),
}
pub enum VideoArtifactReadLookup {
    Missing,
    Available(Box<VideoArtifactRead>),
    Invalidated,
}

pub trait ArtifactStore: Send + Sync {
    // Existing image methods remain unchanged.
    fn read_video_artifact(
        &self,
        request: RetrieveArtifactRequest,
    ) -> PortFuture<'_, Result<VideoArtifactReadLookup>>;
    fn lookup_video_artifact(
        &self,
        key: ArtifactCacheKey,
        expected_sources: Vec<ArtifactSourceFingerprint>,
    ) -> PortFuture<'_, Result<VideoArtifactLookup>>;
    fn publish_video_artifact(
        &self,
        publication: VideoArtifactPublication,
    ) -> PortFuture<'_, Result<VideoArtifactPublish>>;
    fn video_artifact(
        &self,
        artifact_id: ArtifactId,
    ) -> PortFuture<'_, Result<Option<StoredVideoArtifact>>>;
}
```

**Implementation notes**:

- `TemporalVideoManifest` adds `selection: Option<VideoSelectionIdentity>` and its constructor/cache transcript accepts the same value. Constructor-backed deserialization enforces policy/meaningful-ID alignment and hashes the complete selection parameters.
- `TemporalVideoGenerationRequest` validates the complete request range, total frame/duration limits, output ceilings, and at least a 2x2 possible canvas. It remains a resolved internal/public-service contract; the later MCP feature owns natural-anchor resolution.
- Video publication verifies scope, source order against `plan.input_frame_ids()`, exact `video/mp4`, encoded length, SHA-256, cache generator identity, encoder identity, and profile before it can reach infrastructure.
- Existing image constructors, fields, Serde/schema, trait methods, and retry behavior do not change. New video trait methods have the same explicit unsupported default used by coherent image reads so existing non-video fakes remain source-compatible until they opt in.

**Acceptance criteria**:

- [x] Existing image handle/publication/read JSON and round-trip tests remain byte-identical.
- [x] Video request, selector identity, handle, publication, stored value, and read bytes reject unknown fields, scope/source/order/hash/media/profile/limit contradictions.
- [x] Real-time manifests require no selector and no meaningful IDs; model-optimized manifests bind one privacy-safe selector identity and exact selected IDs.
- [x] The same `ArtifactStore` trait is the only retained artifact authority; no video storage/index/path port appears.

### Unit 2: Schema-v6 and one variant-aware artifact engine

**Files**: `crates/krometrail-store/src/index/schema_v6.rs`, `crates/krometrail-store/src/index/mod.rs`, `crates/krometrail-store/src/index/migrations.rs`, `crates/krometrail-store/src/index/artifacts.rs`, `crates/krometrail-store/src/artifacts/mod.rs`, `crates/krometrail-store/src/artifacts/files.rs`, `crates/krometrail-store/src/artifacts/recovery.rs`, `crates/krometrail-store/src/recording.rs`, `crates/krometrail-store/tests/artifact_store.rs`, `crates/krometrail-store/tests/video_artifact_store.rs`

**Story**: `epic-temporal-video-artifacts-retained-generation-additive-artifact-persistence`

```rust
pub(crate) enum RetainedArtifactKind {
    Image(temporal_vision::ArtifactKind),
    TemporalVideo,
}

impl RetainedArtifactKind {
    pub(crate) fn as_str(&self) -> &'static str;
    pub(crate) fn media_type(&self) -> &'static str;
    pub(crate) fn extension(&self) -> &'static str;
}

impl SqliteIndex {
    pub(crate) fn stage_image_artifact(
        &self,
        publication: &ArtifactPublication,
    ) -> Result<StageArtifact>;
    pub(crate) fn stage_video_artifact(
        &self,
        publication: &VideoArtifactPublication,
    ) -> Result<StageArtifact>;
}

impl ArtifactFiles {
    pub(crate) async fn publish(
        &self,
        artifact_id: ArtifactId,
        relative_path: String,
        bytes: Arc<[u8]>,
        deletion_cancellation: Arc<AtomicBool>,
        external_cancellation: Option<Arc<dyn CancellationSignal>>,
    ) -> Result<()>;
    pub(crate) fn path(&self, relative_path: &str) -> Result<PathBuf>;
    pub(crate) fn temp_path(&self, artifact_id: ArtifactId) -> PathBuf;
}
```

**Implementation notes**:

- Migration v6 transactionally rebuilds only the two artifact tables to widen their closed constraints, copies all v5 image rows/source links without reserializing manifests or changing paths/cache keys/hashes, recreates indexes, and drops the renamed old tables. Migration tests insert a real valid v5 image row, migrate, and prove all selected bytes/columns/source links remain equal.
- `ArtifactRow.kind` becomes store-private `RetainedArtifactKind`. `decode_artifact` enforces the exact kind/media/extension tuple. It never accepts arbitrary MIME types or suffixes.
- Extract one private staging/finalization/read-snapshot/publication flow parameterized by validated image/video parts. Public image/video methods only project to their exact typed result. Do not replace the public types with a generic serialized envelope.
- `ArtifactFiles` receives the row-derived relative path instead of hard-coding `.png`; it rejects separators and any name not equal to the expected ID plus allowlisted extension before filesystem access. Temporary files remain ID-only.
- Recovery reads each row's exact relative path, dispatches typed manifest validation by row kind, scans `png`, `mp4`, and `tmp` orphans, and hands invalid rows to the existing deletion journal. Usage remains class `artifact`.
- Retention ordering, source-segment invalidation, pin semantics, deletion batches, and session-deletion publication guards remain shared and unchanged except for variant dispatch.

**Acceptance criteria**:

- [x] A v5 database with valid retained PNG rows migrates to v6 with identical image manifest JSON, hashes, cache identity, source links, file name, and readable bytes.
- [x] Image and video publications use one cache lock registry, usage class, artifact table, source-link table, directory, recovery pass, eviction order, and deletion journal.
- [x] Startup finalizes a durable staged MP4, invalidates corrupt/mismatched MP4 metadata or bytes, and removes orphan `.mp4`/`.tmp` files idempotently.
- [x] Source eviction, budget eviction, and session deletion remove linked MP4s, rows, source links, files, and usage exactly as they do PNGs; pins protect source segments rather than derived video.
- [x] Concurrent equal video cache keys have one ready winner, and cancellation at every publication phase leaves no ready row or accounted partial file.

### Unit 3: Deterministic video cache identity

**File**: `src/artifacts/cache.rs`

**Story**: `epic-temporal-video-artifacts-retained-generation-bounded-generation-service`

```rust
pub(crate) struct VideoCacheIdentityInput<'a> {
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub sources: &'a [SourceFingerprint],
    pub canonical_parameters: &'a [u8],
    pub selector: Option<&'a VideoSelectionIdentity>,
}

pub(crate) fn video_cache_metadata(
    input: VideoCacheIdentityInput<'_>,
) -> ArtifactCacheMetadata;
```

**Implementation notes**:

- Extract only the private common framed-hash/source/epoch mechanics from `cache_metadata`; leave the existing image wrapper and cache bytes unchanged.
- Video uses kind/generator `temporal_video`, a versioned retained-generation adapter name, the canonical manifest transcript, exact source metadata/encoded hashes, selector identity, plan, output ceilings, and encoder identity. No artifact ID, output bytes, path, stderr, or unordered map enters the key.

**Acceptance criteria**:

- [x] Existing still cache golden/sensitivity tests are unchanged.
- [x] Video keys are stable for identical input and change for source order/bytes/metadata, gaps, policy, timing, selection, geometry, output limit, adapter, FFmpeg build/encoder, or argument-policy identity.
- [x] The cache metadata generator fields agree with video publication validation and contain no path-bearing or private process data.

### Unit 4: Bounded frame adaptation, meaningful selection, and gap slates

**Files**: `src/video/adapt.rs`, `src/video/slate.rs`, `src/video/plan.rs`, `src/video/mod.rs`

**Story**: `epic-temporal-video-artifacts-retained-generation-bounded-generation-service`

```rust
pub(crate) struct PreparedVideoEpoch {
    pub epoch: EpochPlan,
    pub plan: VideoPresentationPlan,
    pub selection: Option<VideoSelectionIdentity>,
    pub profile: VideoEncodingProfile,
    pub sources: Vec<ArtifactSourceFingerprint>,
    pub cache_sources: Vec<SourceFingerprint>,
}

pub(crate) fn output_geometry(
    source: PixelDimensions,
    output: OutputLimitsRequest,
) -> Result<VideoOutputGeometry>;

pub(crate) fn meaningful_selection(
    epoch: &EpochPlan,
    anchor: SessionTime,
    cancellation: &WorkCancellation,
) -> Result<(Vec<FrameId>, VideoSelectionIdentity)>;

pub(crate) fn encode_inputs(
    prepared: &PreparedVideoEpoch,
    cancellation: &WorkCancellation,
) -> Result<Vec<VideoEncodeFrame>>;

pub(crate) fn render_gap_slate(
    canvas: PixelDimensions,
    source_range: SessionRange,
) -> Result<Arc<[u8]>>;
```

**Implementation notes**:

- Reuse `artifacts::epoch::validate_and_plan`, `EpochPlan`, `SourceFingerprint`, the strict image decoder, and the pure `build_presentation_plan`; make only the narrow visibility/helper exports needed by the video sibling module.
- Geometry chooses the largest exact aspect-preserving non-upscaled integer/rational fit whose zero/one-pixel right/bottom padding produces an even canvas within caller and server limits. It fails rather than crops, stretches, upscales, or exceeds the request.
- For model-optimized selection, decode/thumbnail one source frame at a time, construct one small common-coordinate `FrameSequence`, apply the versioned temporal-vision storyboard selector, filter against source segments in the provisional real-time plan, and preserve selected source order. The canonical parameter hash includes anchor, analysis geometry, scale/filter, noise floor, tile limit, temporal-vision selector version, and gap input.
- `encode_inputs` follows the final plan segment order exactly. Source segments clone retained encoded bytes by frame ID; gap segments reuse deterministic slate bytes by exact `(gap ids, range, canvas)` identity. It never invents a source frame or interpolates a gap.

**Acceptance criteria**:

- [x] Multi-epoch source input produces one plan per exact geometry/device-scale epoch and never stretches frames across epochs.
- [x] Real-time adaptation performs no visual decode; model optimization deterministically selects only visible retained source frames and records the selection identity/IDs.
- [x] Odd/even, portrait/landscape, exact-boundary, no-upscale, and impossible-fit geometry cases produce the expected canvas or stable limit error.
- [x] Gap input is a deterministic valid PNG at the exact canvas size with a visible non-source label, and every encoder frame exactly matches its plan segment index/source/dimensions.

### Unit 5: Cancellation-safe retained generation service

**Files**: `src/video/service.rs`, `src/video/mod.rs`, `src/video/service_tests.rs`, `src/app.rs`

**Stories**: `epic-temporal-video-artifacts-retained-generation-bounded-generation-service`, `epic-temporal-video-artifacts-retained-generation-lifecycle-qualification`

```rust
pub(crate) struct VideoGenerationLimits {
    pub max_active_requests: NonZeroUsize,
    pub max_blocking_jobs: NonZeroUsize,
    pub max_analysis_bytes: NonZeroUsize,
    pub max_wall_time: Duration,
}

#[derive(Clone)]
pub(crate) struct TemporalVideoGenerationService {
    frames: Arc<dyn FrameSource>,
    artifacts: Arc<dyn ArtifactStore>,
    ids: Arc<dyn IdSource>,
    encoder: Arc<dyn TemporalVideoEncoder>,
    limits: VideoGenerationLimits,
    requests: Arc<Semaphore>,
    blocking: Arc<Semaphore>,
    cache_locks: Arc<VideoCacheLocks>,
}

impl TemporalVideoGenerationService {
    pub(crate) fn new(
        frames: Arc<dyn FrameSource>,
        artifacts: Arc<dyn ArtifactStore>,
        ids: Arc<dyn IdSource>,
        encoder: Arc<dyn TemporalVideoEncoder>,
        limits: VideoGenerationLimits,
    ) -> Result<Self>;
}

impl TemporalVideoGeneration for TemporalVideoGenerationService {
    fn generate_video(
        &self,
        request: TemporalVideoGenerationRequest,
        context: ArtifactGenerationContext,
    ) -> PortFuture<'_, Result<TemporalVideoGenerationResult>>;
}
```

**Implementation notes**:

- Resolve the absolute deadline as `min(caller deadline, now + max_wall_time)`, acquire one bounded request permit, fetch exact `range.frame_ids`, validate source metadata/bytes and partition epochs off the async I/O path, and prepare plans/cache keys before any encode.
- Process epochs in stable order. Acquire the per-key lock under the same deadline/cancellation, lookup and fully validate a cache hit, recheck after the lock, then compose one exact `VideoEncodeRequest`.
- Compare the returned clip identity/profile with the requested qualified identity/profile before constructing `TemporalVideoManifest`. Hash/length/profile mismatch, encoder failure, cancellation, or deadline never reaches publication.
- Construct a new artifact ID only for a missing valid encode result. Publish with the internal cancellation token. `Existing` returns that winner's exact manifest/bytes handle; `Published` reports generated or regenerated-after-invalidation disposition.
- Revalidate session/source lifetime through `ArtifactStore` publication. If deletion wins while selection/encoding runs, publication returns the deleted-session failure and the service returns no clip.
- `src/app.rs` gains only the retained service assembly hook/trait object needed by the later agent-surface feature; FFmpeg qualification and conditional MCP wiring remain outside this feature.

**Acceptance criteria**:

- [x] A deterministic fake encoder receives the exact plan/frame/profile request and produces a retained video result whose handle, manifest, bytes, cache, and scoped read agree.
- [x] A repeat request hits cache without a second encode; concurrent equal requests encode once; changing encoder identity, policy, source byte, gap, selector, output limit, or epoch forces the expected miss.
- [x] Cancellation before load, while waiting for permits/lock, during selection, during encode, and before publication returns the stable cancellation code and leaves no new ready artifact.
- [x] Deadline, fake encoder failure, mismatched returned identity/profile/hash, and store failure return no success and publish no partial bytes.
- [x] Session deletion while source work or encode is paused completes, the late task cannot republish, and no video row/file/usage survives; active frame/event ingestion does not wait for encoding.
- [x] Multi-epoch generation returns all clips in epoch order only after every epoch succeeds; a later failure never returns a partial success and any earlier valid cached output remains governed by normal retention.

### Unit 6: Stable storage/lifecycle qualification

**Files**: `src/video/service_tests.rs`, `src/artifacts/qualification_tests.rs`, `crates/krometrail-store/tests/video_artifact_store.rs`, `crates/krometrail-store/src/index/migrations.rs`

**Story**: `epic-temporal-video-artifacts-retained-generation-lifecycle-qualification`

**Implementation notes**:

- Use constructor-valid in-memory MP4-like fake bytes; do not launch FFmpeg, a browser, a network service, or a provider. These tests prove application/store contracts, not codec qualification.
- Reuse real `RecordingStore` fixtures with small budgets and deterministic pause points to exercise source-link validation, cache corruption, file recovery, budget eviction, concurrent deletion, and frame/event ingestion independence.
- Keep one migration preservation test and one end-to-end retained-video lifecycle test as the primary high-value evidence. Focused unit tables cover geometry/selector/slate/cache only where isolated examples protect novel logic.

**Acceptance criteria**:

- [x] Locked CI requires no installed FFmpeg and distinguishes fake application/storage proof from the later opt-in live adapter qualification.
- [x] A v5 retained-image fixture remains readable after v6, while a video survives restart only when row, manifest, source links, bytes, and usage all validate.
- [x] Corruption, source eviction, budget eviction, session deletion, cancellation, and recovery tests prove one shared lifetime authority without weakening existing image tests.
- [x] No test is deleted, skipped, loosened, or made implementation-tautological to obtain green results.

## Implementation order

1. `epic-temporal-video-artifacts-retained-generation-additive-artifact-persistence`
2. `epic-temporal-video-artifacts-retained-generation-bounded-generation-service`
3. `epic-temporal-video-artifacts-retained-generation-lifecycle-qualification`

The stories are durable checkpoints inside one cohesive feature implementation and review boundary. The normal implement-orchestrator owner should carry all three sequentially; they are not three independent worker assignments.

## Simplification

- Preserve the old image public types instead of introducing compatibility shims or a tagged manifest wrapper that every still consumer must unwrap.
- Extract one private variant-aware row/publication validator and keep one store, artifact table, source-link table, cache-lock registry, usage class, directory, recovery pass, and deletion journal.
- Reuse `ResolvedRange`, `VisualEpoch`, `EpochPlan`, `SourceFingerprint`, `ArtifactCacheMetadata`, `OutputLimitsRequest`, `ArtifactGenerationContext`, `WorkCancellation`, `TemporalVideoEncoder`, `VideoPresentationPlan`, and the temporal-vision selector rather than creating parallel time, frame, gap, hash, cancellation, or importance vocabularies.
- Generalize the existing framed cache hasher without changing still cache bytes. Do not add a preliminary video cache index, generic media framework, provider upload layer, audio path, or codec/container registry.
- Keep deletion latency independent of optional encoding. The existing deleted-session publication fence plus hard deadline makes late work safe without a second session-work registry.
- No existing tests are scheduled for removal. Implementation may consolidate duplicated image/video store test fixture construction, but variant-specific contract assertions remain explicit.

## Testing

- **Core interface tests** protect additive wire/manifest/read contracts, exact selector-policy alignment, source/hash/media/profile validation, and unchanged image serialization.
- **Migration/store interface tests** protect v5 image preservation, one shared publication/cache/recovery/retention/deletion authority, exact file extensions, corruption invalidation, and scoped reads.
- **Pure unit tests** protect only novel deterministic geometry fitting, selector parameters/visible-ID filtering, gap-slate pixels, and cache sensitivity.
- **Service seam tests with a fake encoder** protect exact request composition, one encode per cache key, identity/profile mismatch rejection, cancellation/deadline, store failure, multi-epoch ordering, and all-or-error responses.
- **Real-store race tests** protect the demonstrated risks: deletion during source/encode work cannot republish, cancellation during publication leaves no state, and active generation does not block frame/gap/event persistence.
- **No FFmpeg test here**: real MP4/H.264 proof, process cleanup, stderr, and executable disappearance belong to `epic-temporal-video-artifacts-ffmpeg-runtime` and the final opt-in agent-surface qualification.

## Risks

- **The v6 table rebuild could damage stable image rows or foreign keys.** Mitigation: immutable new migration, transaction-only rebuild/copy, exact pre/post row assertions, foreign-key checks, and reopen/read validation. Fallback: stop with the database at v5 because migration rollback is atomic; never purge image artifacts.
- **A typed variant can be decoded under the wrong manifest shape.** Mitigation: the closed row kind/media/extension tuple chooses exactly one constructor-backed manifest decoder and validates row/source/output/cache fields before returning bytes.
- **Model-optimized selection can misstate importance or duration.** Mitigation: reuse the descriptive still selector, expose exact selector identity and selected IDs, filter through provisional-plan visibility, and keep every hold labeled in the canonical plan. It remains presentation, not diagnosis.
- **Cancellation can drop an adapter future while an external child exists.** The port contract requires cancellation/drop-safe process cleanup, and the service signals then awaits bounded adapter cleanup. The fake proves signal/order behavior; the FFmpeg feature must prove real process reaping.
- **Session deletion during encoding cannot synchronously cancel work it does not own.** The encode remains hard-deadline bounded and cannot publish after deletion. This intentionally favors prompt deletion and a single lifetime registry over waiting on an optional process.
- **All-or-error multi-epoch generation is not an atomic multi-file transaction.** Earlier valid clips may remain cached if a later epoch fails, but no partial result escapes; retained clips are source-linked and reclaimed by the same budget/deletion authority. A cross-artifact transaction would add disproportionate failure machinery.
- **Exact plan keys make model-optimized cache hits perform selection work.** This is accepted for v1 because correctness and reproducibility outweigh a second index/key scheme. Performance evaluation can motivate a later additive optimization without changing stored identity.
- **Fake bytes cannot prove MP4/H.264.** This feature deliberately tests only core/application/storage contracts. Production adapter qualification remains the sole codec/container proof and is a dependency of the later agent surface.

## Other agent review

- Invoked because: the design changes stable retained-data migration and source/deletion/cache contracts, which ordinarily warrant independent completeness review.
- Skipped/degraded: the active autopilot delegation explicitly prohibited nested agents and peeragent. This design-time advisory degradation is non-blocking under the principles policy; direct source/test grounding, the pre-mortem above, and the unchanged `standard` one-pass feature/final reviews remain the closure path.

## Implementation summary

- Execution capability: GPT-5.6 Sol at xhigh reasoning, the caller-selected highest-capability worker, coordinated as one sequential cohesive feature owner under autopilot.
- Review weight: `standard`. This implementation is ready for the independent feature review; the implementing worker did not self-close the feature.
- Completed child stories and commits:
  - `epic-temporal-video-artifacts-retained-generation-additive-artifact-persistence` — `a84038f`
  - `epic-temporal-video-artifacts-retained-generation-bounded-generation-service` — `914c860`
  - `epic-temporal-video-artifacts-retained-generation-lifecycle-qualification` — `68a87f2`
- The additive schema-v6 migration preserves retained image rows and bytes while one variant-aware artifact engine now owns image/video staging, files, cache locks, source links, usage, recovery, eviction, deletion, and scoped reads.
- The bounded service partitions exact resolved input by visual epoch, derives deterministic geometry, meaningful-frame selection, and gap slates, composes canonical plans and cache identity before encoding, rejects contradictory encoder results, and publishes only through the store's source/session lifetime fence.
- Real-store qualification covers restart, recovery, corruption, source and budget eviction, cancellation/publication races, session deletion during encoding, and concurrent frame/event ingestion. Locked tests use a deterministic fake encoder and launch no FFmpeg, browser, provider, or network service; the sibling FFmpeg feature owns production codec qualification.
- No adjacent issue was discovered or parked. The earlier interim full-workspace Clippy conflict was in the concurrently implemented sibling FFmpeg tree; after that feature reached review, the authoritative combined serial workspace gate passed without discrepancy.

## Verification evidence

- `cargo fmt --all -- --check` — passed on the clean integrated workspace.
- `cargo check --workspace --all-targets --locked` — passed.
- `cargo test --workspace --all-targets --locked` — passed across all workspace crates and targets; the root suite reported 119 passed and 2 existing explicitly ignored tests.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` — passed.
- The four authoritative integrated commands ran serially after `epic-temporal-video-artifacts-ffmpeg-runtime` reached review, avoiding concurrent browser-profile ownership and validating the combined tree.
- No existing test was removed, weakened, loosened, or newly skipped to obtain these results.
