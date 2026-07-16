---
id: epic-temporal-debugging-workflow-artifact-generation-and-cache
kind: feature
stage: done
tags: [visual, storage]
parent: epic-temporal-debugging-workflow
depends_on: [epic-temporal-debugging-workflow-resolved-temporal-queries]
release_binding: 1.0.0
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Bounded Artifact Generation and Cache

## Brief

Deliver the Krometrail adapter from a `ResolvedRange` and retained encoded frames to the browser-agnostic `temporal-vision` crate. The adapter decodes source images, preserves ordered frame identities and session-relative timing, maps declared gaps and caller markers, splits incompatible visual epochs rather than silently stretching them, and invokes the existing storyboard, orientation, difference-map, region-filmstrip, and other supported source-derived generators.

Run decoding and visual work under independent concurrency, memory, source-frame, and output bounds so an investigation cannot block capture ingestion or grow with session duration. Persist exact encoded artifacts and their existing provenance manifests through one artifact-store/cache authority; cache identity derives from ordered source frames, artifact kind, transformation parameters, and algorithm version, and retained hits return the same traceable evidence without regeneration.

This feature owns adaptation, bounded generation, persistence, lookup, and cache invalidation with source retention. It does not resolve natural anchors, compose the agent-facing debug bundle, interpret visual change as a diagnosis, or add a Krometrail-specific manifest parallel to `temporal-vision::ArtifactManifest`.

## Epic context

- Parent epic: `epic-temporal-debugging-workflow`
- Position in epic: generation foundation — consumes resolved ranges and is shared by the primary bundle and progressive region/artifact retrieval

## Simplification opportunity

- Turn the store's existing artifact schema and retention hooks into one production artifact port, replacing test-only direct rows and the root's no-op temporal-vision import. Reuse `FrameSource`, temporal-vision artifact/provenance types, and the authoritative usage ledger instead of adding another frame reader, manifest, cache index, or image pipeline.

## Foundation references

- `docs/ARCHITECTURE.md` — Artifact Generation, Temporal Visual Crate, Recording Store, and Failure Isolation
- `docs/SPEC.md` — Temporal Queries and Artifact Provenance
- `docs/VISUAL-EVIDENCE.md` — Shared Artifact Contract, Input Sequence, Determinism, and Provenance
- `docs/EVALUATION.md` — Performance Evaluation and Storage and Retention Evaluation

## Grounding and dispatch

- **Driver:** active autopilot `--all`; design decisions were resolved from current contracts and completed implementation without questions.
- **Dispatch:** direct-read only. The caller prohibited nested agents and peer review. Grounding covered project rules/conventions, all five foundation documents, the parent epic, archived temporal-vision and durable-memory epics through their `git_ref` history, resolved-query commits `27475fa`, `a8e3edd`, `6198b71`, `ef9bf21`, and `76f63e6`, the full public temporal-vision generators/contracts/tests, current schema v3, frame segments and reads, retention/removal/recovery/usage/session deletion, core frame/range ports, root composition, and CDP capture configuration/header parsing.
- **Current seams:** `ResolvedRange.frame_ids` is already ordered by capture ordinal; `FrameSource::frames_by_id` is the one encoded-frame reader; `RecordingStore` is the retention/usage/deletion mutation authority; schema v1 already reserved `artifacts` and `artifact_frames` but only tests write them; temporal-vision already owns all manifests and deterministic PNG generation; root currently imports temporal-vision only as `_`.
- **Actual capture formats:** `CaptureConfig` requests JPEG by default (`quality=80`) and permits PNG; `Page.startScreencast` is sent exactly `format: "jpeg" | "png"`, and `EncodedFrame.metadata().format()` persists that declared format. The adapter supports exactly those two formats and never sniffs an unrelated codec.
- **UI:** no human screen or journey exists. Mockups are intentionally skipped.
- **Review weight:** standard at implementation review. Design-time advisory review is skipped by the caller boundary.

## Design decisions

- **Application boundary:** Add one `ArtifactGeneration` port whose request owns one already-resolved `ResolvedRange`, caller-supplied typed markers, one or more generator specifications, and an all-required versus degraded policy. It never accepts a natural anchor and never resolves a range again.
- **Supported generators:** The request registry has four generator variants: `Storyboard`, `DifferenceMap`, `RegionFilmstrip`, and `MotionHistory`. A storyboard may request its existing before/during/after orientation output. `BeforeDuringAfter` remains the existing temporal-vision artifact kind produced by `generate_storyboard`; it is not a fifth independently dispatched generator or an invented Krometrail kind.
- **Artifact authority:** `krometrail-core` depends only on the pure browser-agnostic `temporal-vision` crate and aliases its generic manifest with Krometrail IDs. The exact `temporal_vision::ArtifactManifest` is carried through the application/store boundary. There is no parallel manifest struct, JSON projection, or copied artifact-kind registry.
- **Adapter placement:** Core owns request/result/store ports and infrastructure-free policy values. `krometrail-store` owns files, SQLite, usage, retention invalidation, and crash recovery. `src/artifacts/` owns decoding, visual-epoch adaptation, cache-key construction, scheduling, and temporal-vision invocation. `src/app.rs` composes those ports. Temporal-vision remains independent of every Krometrail crate.
- **Frame authority:** Generation reads `ResolvedRange.frame_ids` through the existing `FrameSource::frames_by_id`; no second frame reader, decoded-frame cache, or source ledger is introduced. Reads and final publication revalidate exact frame identity, session, target, order, metadata, and retention.
- **Epoch policy:** A visual epoch is a maximal contiguous run with identical image dimensions, viewport dimensions, and exact `DeviceScaleFactor::get().to_bits()`. A format change alone does not split an epoch because the decoder produces the same declared RGBA8 representation. Any geometry/scale change starts a new epoch; outputs are generated per epoch and never silently stretched together.
- **Decode semantics:** Force the decoder selected by stored `ImageFormat`; require decoded dimensions to equal persisted image dimensions; accept only 8-bit gray/gray-alpha/RGB/RGBA results; expand to straight RGBA8; preserve PNG straight alpha; inject alpha `255` for JPEG; apply no EXIF orientation, profile transform, premultiplication, geometric registration, or hidden resize. The adapter treats current Chrome screencast channels as sRGB, records that assumption and decoder profile, and rejects higher-bit-depth/unsupported outputs rather than silently reducing them.
- **Decoder choice:** Add exact `image = "=0.25.9"` with `default-features = false, features = ["jpeg", "png"]`. `cargo info image@0.25.9` on 2026-07-14 reports `rust-version: 1.85.0`; current `0.25.10` reports `rust-version: 1.88.0`, so 0.25.9 is the newest verified release compatible with this workspace's Rust 1.85 contract. Keep exact `png 0.17.16` for temporal-vision's deterministic output encoder; two versions may coexist because input decoding and locked output bytes have different compatibility duties.
- **Markers and gaps:** The request supplies complete marker identity/time/kind/label values because `ResolvedRange.marker_ids` intentionally contains identities, not presentation labels. Gaps come only from `ResolvedRange.gaps`. Both are sorted deterministically and clipped to each epoch's inclusive first/last frame time; no gap is inferred from ordinals. Equal-time marker declaration order is retained. A marker may appear in two epochs only when tied frame timestamps make both inclusive epoch ranges contain it; that duplication is explicit in each manifest.
- **Normalization:** Storyboard, difference map, and motion history share one decoded sequence and one normalized sequence per epoch. `FitLimits` chooses the smallest exact integer downscale in `1,2,4,8` that divides both dimensions and fits configured pixels/bytes; explicit identity/downscale never silently changes. Region filmstrip keeps its existing fixed-region and display-scale semantics. Every effective choice appears in the authoritative manifest and cache parameters.
- **Independent scheduling:** A root-owned scheduler has separate global active-request, global blocking-CPU, global memory, and per-request generator permits. Decode/render/encode run through `spawn_blocking`; no capture task and no `RecordingStore` mutation gate performs image work. CPU workers default to `min(4, max(1, available_parallelism - 1))`, leaving a logical processor for I/O where possible.
- **Default ceilings:** At most 2 active requests globally, 2 generator jobs per request, 120 source frames, 512 MiB encoded source bytes, 8,192 on either source dimension, 16,777,216 pixels per frame, 512 MiB decoded RGBA, 512 MiB normalized retained bytes, 1 GiB combined request memory, 16 outputs, 64 MiB per output, 256 MiB total returned/persisted output, 256 markers, and 15 seconds service wall time. Configuration may lower or raise these only through validated root startup values; temporal-vision receives the effective lower limits.
- **Cancellation/deadline:** Caller cancellation and the earlier of its `Instant` deadline or the 15-second service deadline stop awaiting, suppress later publication, and remove the waiter from single-flight work. Decode checks between frames and generation checks between outputs/phases. Rust cannot safely preempt a currently executing decoder/renderer thread; that bounded unit may finish, but its result is discarded unless another waiter remains. Frame/pixel/memory/output ceilings are the hard backstop.
- **Deterministic ordering:** Epochs order by first capture ordinal; generator slots retain request order; storyboard output orders `Storyboard` before optional `BeforeDuringAfter`; all parallel results are placed into preassigned slots. Scheduling cannot change result, manifest, or publication order.
- **Degraded semantics:** `RequireAll` returns one error if any requested epoch/output fails, while already-created cache entries remain harmless reusable derived data. `AllowPartial` returns an ordered `Unavailable` outcome for per-epoch decode/generation/resource/publication failures and continues independent slots. Cancellation, deadline, session deletion, or source-retention loss aborts the whole request because continuing would publish against an invalid caller contract.
- **Cache identity:** Each output has one SHA-256 cache key over a versioned length-prefixed binary transcript: ordered source entries `(FrameId UUID bytes, capture ordinal, session time, encoded format tag, image/viewport dimensions, device-scale bits, SHA-256 of exact encoded bytes)`, output `ArtifactKind`, canonical effective generator/normalization/marker/gap parameters, visual-epoch hash, temporal-vision generator name/version, adapter/decoder profile version, and cache-key schema version. Session and target IDs are included as scope. Reordering frames or changing any content, timing, format, epoch, parameter, or version changes the key.
- **Canonical parameters:** Constructors materialize every default before hashing; maps use sorted keys; numbers are integers or canonical finite values; strings are length-prefixed UTF-8; request-only resource ceilings are included when temporal-vision records them in provenance. Gaps and markers are part of the canonical parameter transcript because they affect pixels and manifests.
- **Algorithm registry:** Move the four private generator name/version constants behind one public temporal-vision descriptor registry used both by generators and cache-key construction. `BeforeDuringAfter` maps to the storyboard descriptor. Any output-affecting temporal algorithm change must bump that descriptor; any decode/epoch/canonicalization change bumps `krometrail-artifact-adapter-v1`.
- **Single flight:** A process-wide map keys work by the ordered set of missing output cache keys. One leader performs bounded decode/generation; followers await the same result. Each waiter has independent cancellation/deadline. When the last waiter leaves, the shared cooperative token suppresses publication. Cache lookup is repeated inside the leader and publication is unique by cache key, so races converge on one ready row/artifact.
- **Hit validation:** A hit is returned only after the store verifies a `ready` row, exact cache/source fingerprints, ordered source links still present, exact stored manifest bytes deserialize to the authoritative manifest type, manifest IDs/kind/range/source IDs agree with indexed columns, artifact bytes exist, byte length agrees, and SHA-256 agrees with both row and manifest. Missing/corrupt entries are invalidated through the existing deletion/usage authority and become deterministic regeneration misses.
- **Atomic publication:** Publication is atomic per artifact, not per multi-output request. One `staging` artifact row, ordered source links, cache metadata, exact serialized manifest, and conservative artifact usage reservation commit first under the store mutation gate. File writing occurs outside that gate on the bounded artifact worker: write temp, `sync_all`, rename to final UUID filename, then fsync the artifact directory. A final source/session/cache revalidation and one SQLite transaction change the row to `ready`. Cache readers see only `ready`.
- **Crash recovery:** Startup resumes deletion journals first, then reconciles artifacts before capture starts. It finalizes a valid durable `staging` publication or removes it, removes orphan temp/final files with managed UUID names, invalidates ready rows with missing/corrupt bytes/manifests/source links, and reconciles artifact usage. A crash can leave an explicit staging row or orphan file, never a visible ready row whose bytes were not fsynced.
- **Retention and pinning:** Existing retention remains the sole authority. Every staging/ready artifact links all manifest source frames in order. Before any source segment/frame is evicted, all linked artifacts are included in the same deletion batch. Artifacts are regenerable and may be evicted independently; user pins protect source segments, not derived artifacts. Publication reservations are store mutation state, not user pins or a second retention policy.
- **Session deletion/race fence:** `RecordingStore` tracks bounded active artifact publications by session. Deletion marks the session deleted, cancels new/future publication, drains active file work without holding the mutation gate, then reacquires the gate and runs the existing journaled session removal. A deletion success therefore cannot be followed by a late final artifact rename. In-flight CPU work may finish but cannot publish.
- **Schema ownership:** This feature exclusively owns additive artifact migration **v4** in `schema_v4.rs` and the corresponding `migrations.rs` entry. It replaces the previously test-only artifact rows with the ready/staging cache contract and purges legacy derived artifact rows/usage that cannot satisfy it; source frames are untouched and orphan artifact files are removed by recovery. The sibling browser-event feature must chain its migration after this checkpoint as **v5 or later** and must depend on `...-artifact-schema-and-publication`; it must not claim v4. This resolves the parent epic's migration-collision risk without inventing a shared migration runner or a false semantic dependency between artifact generation and event capture.

## Architectural choice

### Option A — generate and cache inside `krometrail-store`

The store already owns artifact tables and files, so putting decoding/rendering there would appear direct. It would make persistence depend on image processing and temporal-vision, tempt callers to hold the mutation gate across CPU work, and blur the source-of-truth versus derived-cache boundary. Rejected.

### Option B — root-only files and an in-memory cache

The root could decode and write UUID files without extending core/store ports. This is small initially but bypasses the global budget, source-link invalidation, session deletion, crash recovery, and future MCP/resource abstraction. It creates the exact second cache/usage/retention authority this feature must eliminate. Rejected.

### Option C — core application/store ports, root computation adapter, existing store authority (chosen)

Core defines the typed request/result and focused artifact-store port while directly aliasing temporal-vision's manifest. Root reads through `FrameSource`, adapts and computes on bounded blocking workers, and calls one store publication operation. `RecordingStore` reuses its mutation gate, deletion journal, usage table, and retention queries. This is the shortest architecture that keeps capture independent, provenance singular, and persistence crash-safe.

A separate artifact microservice/subprocess was considered for hard CPU preemption. It would make wall-time termination stronger, but introduces IPC, another lifecycle, duplicate byte transfer, and a new crash surface before measured decoder/renderer behavior justifies it. Cooperative cancellation plus strict work bounds is the reversible first implementation.

## Trickiest unit first: source-valid atomic publication

The hard part is not calling `generate_storyboard`; it is preventing a cache row, source-eviction race, crash, or session deletion from leaving a reproducibility claim that no longer has exact bytes and retained sources.

The chosen state machine is:

```text
loaded source frames + generated artifact in memory
        │
        ▼
cache hit recheck + source revalidation under RecordingStore mutation gate
        │ miss
        ▼
INSERT artifacts(state='staging') + artifact_frames + usage reservation (one tx)
        │ release mutation gate
        ▼
write bounded temp file → sync_all → rename final → fsync artifacts directory
        │
        ▼
reacquire mutation gate → source/session/cache revalidate
        │
        ├── source deleted / competing ready winner → delete loser through journal, return error/winner
        ▼
UPDATE artifact state='ready' (one tx; visibility point)
```

Retention sees both `staging` and `ready` source links, but cache reads select only `ready`. The store's publication method owns all intermediate paths and states; neither core nor root sees a filesystem path. Recovery treats the state machine as a durable journal and converges every crash point.

## Implementation units

### Unit 1: application contracts, authoritative manifest alias, and cache identity

**Story:** `epic-temporal-debugging-workflow-artifact-generation-and-cache-artifact-contracts-and-cache-identity`

**Files:**

- `crates/krometrail-core/Cargo.toml`
- `crates/krometrail-core/src/artifacts.rs` (new)
- `crates/krometrail-core/src/ports/artifacts.rs` (new)
- `crates/krometrail-core/src/ports/mod.rs`
- `crates/krometrail-core/src/{lib.rs,error.rs}`
- `crates/temporal-vision/src/{provenance.rs,lib.rs,render.rs,difference_map.rs,filmstrip.rs,motion_history.rs}`
- `src/artifacts/cache.rs` (new)

Core aliases, rather than copies, the manifest:

```rust
pub type ArtifactManifest = temporal_vision::ArtifactManifest<
    ArtifactId,
    FrameId,
    ArtifactMarkerId,
    GapId,
>;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "source", content = "id", rename_all = "snake_case")]
pub enum ArtifactMarkerId {
    Interaction(InteractionId),
    Navigation(NavigationId),
    Marker(MarkerId),
    Caller(NonEmptyText),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ArtifactMarker {
    pub id: ArtifactMarkerId,
    pub session_time: SessionTime,
    pub kind: NonEmptyText,
    pub label: NonEmptyText,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ArtifactFailurePolicy { RequireAll, AllowPartial }

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AnalysisScale { Identity, Down(u8), FitLimits }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "generator", rename_all = "snake_case")]
pub enum ArtifactGeneratorRequest {
    Storyboard(StoryboardRequest),
    DifferenceMap(DifferenceMapRequest),
    RegionFilmstrip(RegionFilmstripRequest),
    MotionHistory(MotionHistoryRequest),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StoryboardRequest {
    pub anchor: SessionTime,
    pub tile_limit: u8,                 // temporal-vision validates 3..=12
    pub noise_floor: u16,
    pub normalization: NormalizationRequest,
    pub labels: ArtifactLabelsRequest,
    pub include_orientation: bool,
    pub output: OutputLimitsRequest,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DifferenceMapRequest {
    pub reference: FrameSelector,
    pub frequency_mode: temporal_vision::FrequencyMode,
    pub repeated_change_separation_nanos: Option<u64>,
    pub noise_floor: u16,
    pub normalization: NormalizationRequest,
    pub canvas_background: temporal_vision::Rgb8,
    pub output: OutputLimitsRequest,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RegionFilmstripRequest {
    pub region: temporal_vision::RegionDefinition,
    pub anchor: SessionTime,
    pub tile_limit: u8,                 // temporal-vision validates 1..=24
    pub locator: Option<FrameId>,
    pub background: temporal_vision::Rgb8,
    pub padding: temporal_vision::Rgb8,
    pub display_scale: AnalysisScale,   // FitLimits is rejected here; explicit only
    pub labels: ArtifactLabelsRequest,
    pub output: OutputLimitsRequest,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MotionHistoryRequest {
    pub reference: FrameSelector,
    pub noise_floor: u16,
    pub normalization: NormalizationRequest,
    pub decay_peak: u16,
    pub decay_half_life_ranks: u8,
    pub reference_strength: u8,
    pub accent: temporal_vision::Rgb8,
    pub outline: temporal_vision::Rgb8,
    pub labels: ArtifactLabelsRequest,
    pub output: OutputLimitsRequest,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArtifactGenerationRequest {
    pub range: ResolvedRange,
    pub markers: Vec<ArtifactMarker>,
    pub generators: Vec<ArtifactGeneratorRequest>,
    pub failure_policy: ArtifactFailurePolicy,
}

impl ArtifactGenerationRequest {
    pub fn new(
        range: ResolvedRange,
        markers: Vec<ArtifactMarker>,
        generators: Vec<ArtifactGeneratorRequest>,
        failure_policy: ArtifactFailurePolicy,
    ) -> Result<Self>;
}
```

`NormalizationRequest` contains optional source-pixel crop `(x,y,width,height)`, declared background, and `AnalysisScale`. `OutputLimitsRequest` contains non-zero maximum width, height, and encoded bytes; the service rejects values above runtime caps rather than clamping them. `FrameSelector` is `First | Last | Frame(FrameId)` and resolves only within one epoch.

The result and service boundary are:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ArtifactCacheDisposition {
    Hit,
    Generated,
    RegeneratedAfterInvalidation,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VisualEpoch {
    pub index: u32,
    pub frame_ids: Vec<FrameId>,
    pub image: PixelDimensions,
    pub viewport: PixelDimensions,
    pub device_scale_factor: DeviceScaleFactor,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArtifactHandle {
    pub artifact_id: ArtifactId,
    pub cache: ArtifactCacheDisposition,
    pub media_type: NonEmptyText,
    pub encoded_byte_len: u64,
    pub manifest: ArtifactManifest,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ArtifactOutcome {
    Available {
        epoch_index: u32,
        generator_index: u32,
        artifact: ArtifactHandle,
    },
    Unavailable {
        epoch_index: u32,
        generator_index: u32,
        artifact_kind: temporal_vision::ArtifactKind,
        error: KrometrailError,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArtifactGenerationResult {
    pub range: ResolvedRange,
    pub epochs: Vec<VisualEpoch>,
    pub outcomes: Vec<ArtifactOutcome>,
}

#[derive(Clone, Default)]
pub struct ArtifactGenerationContext {
    pub deadline: Option<std::time::Instant>,
    pub cancellation: Option<Arc<dyn CancellationSignal>>,
}

pub trait ArtifactGeneration: Send + Sync {
    fn generate(
        &self,
        request: ArtifactGenerationRequest,
        context: ArtifactGenerationContext,
    ) -> PortFuture<'_, Result<ArtifactGenerationResult>>;
}
```

Add `ErrorCode::ArtifactGenerationFailed` and `ErrorCode::ResourceLimitExceeded`; use existing `Cancelled`, `NotFound`, `PersistenceFailed`, and `BudgetExhausted` for their established meanings.

The temporal-vision registry is the only algorithm-name/version source:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GeneratorDescriptor {
    pub name: &'static str,
    pub version: &'static str,
}

pub const fn generator_descriptor(kind: ArtifactKind) -> GeneratorDescriptor;
// Storyboard + BeforeDuringAfter => temporal-storyboard / 1.0.0
// DifferenceMap => temporal-difference-map / v1
// RegionFilmstrip => region-filmstrip / 1.0.0
// MotionHistory => motion-history / 1.0.0
```

Every generator constructs its manifest descriptor from this function; the cache imports the same function. `src/artifacts/cache.rs` owns `ArtifactCacheKey([u8; 32])`, `SourceFingerprint`, and the versioned framed hasher. Tests change one field at a time to prove key sensitivity and canonical default equivalence.

**Acceptance criteria:**

- [ ] One validated request consumes exactly one `ResolvedRange`; empty generators, duplicate marker IDs, out-of-range markers, invalid scales/tile/output limits, and unknown fields fail before frame I/O.
- [ ] The four generator variants are the application registry; orientation is only an optional storyboard output with the existing `BeforeDuringAfter` kind.
- [ ] Core/store results carry the exact temporal-vision manifest type with no copied manifest/kind fields.
- [ ] Cache key tests prove sensitivity to ordered identity, exact encoded content, format, timestamp, dimensions/scale epoch, marker/gap/normalization parameters, artifact kind, and both algorithm versions.
- [ ] Temporal generators and cache keys read one descriptor registry; existing output hashes remain unchanged by the registry refactor.

### Unit 2: schema v4, focused artifact store, durable publication, and recovery

**Story:** `epic-temporal-debugging-workflow-artifact-generation-and-cache-artifact-schema-and-publication`

**Files:**

- `crates/krometrail-store/src/index/schema_v4.rs` (new; exclusive migration ownership)
- `crates/krometrail-store/src/index/{migrations.rs,artifacts.rs,mod.rs,retention.rs,deletion.rs,maintenance.rs}`
- `crates/krometrail-store/src/artifacts/{mod.rs,files.rs,recovery.rs}` (new)
- `crates/krometrail-store/src/{recording.rs,recovery.rs,lib.rs}`
- `crates/krometrail-store/tests/{artifact_store.rs,artifact_recovery.rs}` (new)

The focused core port has no paths or SQLite types:

```rust
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ArtifactCacheKey([u8; 32]);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactSourceFingerprint {
    pub frame_id: FrameId,
    pub encoded_sha256: [u8; 32],
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArtifactCacheMetadata {
    pub cache_key: ArtifactCacheKey,
    pub source_fingerprint: [u8; 32],
    pub parameter_hash: [u8; 32],
    pub visual_epoch_hash: [u8; 32],
    pub cache_schema_version: u32,
    pub adapter_version: NonEmptyText,
    pub generator_name: NonEmptyText,
    pub generator_version: NonEmptyText,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArtifactPublication {
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub sources: Vec<ArtifactSourceFingerprint>,
    pub cache: ArtifactCacheMetadata,
    pub manifest: ArtifactManifest,
    pub media_type: NonEmptyText,
    pub encoded_bytes: Arc<[u8]>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StoredArtifact {
    pub cache: ArtifactCacheMetadata,
    pub manifest: ArtifactManifest,
    pub media_type: NonEmptyText,
    pub encoded_bytes: Arc<[u8]>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ArtifactLookup { Miss, Hit(StoredArtifact), Invalidated }
#[derive(Clone, Debug, PartialEq)]
pub enum ArtifactPublish { Published(StoredArtifact), Existing(StoredArtifact) }

pub trait ArtifactStore: Send + Sync {
    fn lookup_artifact(
        &self,
        key: ArtifactCacheKey,
        expected_sources: Vec<ArtifactSourceFingerprint>,
    ) -> PortFuture<'_, Result<ArtifactLookup>>;

    fn publish_artifact(
        &self,
        publication: ArtifactPublication,
    ) -> PortFuture<'_, Result<ArtifactPublish>>;

    fn artifact(
        &self,
        artifact_id: ArtifactId,
    ) -> PortFuture<'_, Result<Option<StoredArtifact>>>;
}
```

`ArtifactPublication::new` verifies non-empty exact PNG bytes, SHA-256 against `manifest.output_hash`, manifest artifact/session-independent invariants, ordered source IDs, media type, and cache metadata before the store sees it.

Schema v4 rebuilds the test-only artifact tables with strict ready/staging and cache metadata:

```sql
-- Existing artifact rows/usage are derived test-era cache data and are purged in v4.
-- Source frames, segments, pins, and timeline rows are untouched.
CREATE TABLE artifacts_v4 (
    artifact_id BLOB PRIMARY KEY CHECK(length(artifact_id)=16),
    session_id BLOB NOT NULL CHECK(length(session_id)=16),
    target_id BLOB NOT NULL CHECK(length(target_id)=16),
    state TEXT NOT NULL CHECK(state IN ('staging','ready')),
    kind TEXT NOT NULL,
    start_time_be BLOB NOT NULL CHECK(length(start_time_be)=8),
    end_time_be BLOB NOT NULL CHECK(length(end_time_be)=8),
    manifest_json TEXT NOT NULL,
    manifest_hash BLOB NOT NULL CHECK(length(manifest_hash)=32),
    media_type TEXT NOT NULL CHECK(media_type='image/png'),
    output_hash BLOB NOT NULL CHECK(length(output_hash)=32),
    relative_path TEXT NOT NULL UNIQUE,
    byte_len_be BLOB NOT NULL CHECK(length(byte_len_be)=8),
    cache_key BLOB NOT NULL UNIQUE CHECK(length(cache_key)=32),
    source_fingerprint BLOB NOT NULL CHECK(length(source_fingerprint)=32),
    parameter_hash BLOB NOT NULL CHECK(length(parameter_hash)=32),
    visual_epoch_hash BLOB NOT NULL CHECK(length(visual_epoch_hash)=32),
    cache_schema_version INTEGER NOT NULL CHECK(cache_schema_version>0),
    adapter_version TEXT NOT NULL CHECK(length(adapter_version)>0),
    generator_name TEXT NOT NULL CHECK(length(generator_name)>0),
    generator_version TEXT NOT NULL CHECK(length(generator_version)>0),
    FOREIGN KEY(session_id,target_id) REFERENCES targets(session_id,target_id)
) STRICT;

CREATE TABLE artifact_frames_v4 (
    artifact_id BLOB NOT NULL CHECK(length(artifact_id)=16),
    source_position INTEGER NOT NULL CHECK(source_position>=0),
    frame_id BLOB NOT NULL CHECK(length(frame_id)=16),
    encoded_hash BLOB NOT NULL CHECK(length(encoded_hash)=32),
    PRIMARY KEY(artifact_id,source_position),
    UNIQUE(artifact_id,frame_id),
    FOREIGN KEY(artifact_id) REFERENCES artifacts_v4(artifact_id) ON DELETE CASCADE,
    FOREIGN KEY(frame_id) REFERENCES frames(frame_id)
) STRICT;
```

The migration performs the SQLite-safe rename/recreate sequence, recreates range/cache/source indexes, removes legacy artifact usage, and sets `user_version=4` through the existing contiguous migration runner. No artifact cache table or second usage table is added.

`RecordingStore::publish_artifact` owns the staging protocol and uses one bounded blocking artifact-file worker. It reserves exact usage in the staging transaction, writes no file while holding `mutations`, and finalizes only after a second source/session/cache check. All paths are UUID-derived and crate-private. `lookup_artifact` snapshots metadata under the gate, reads/hashes bytes outside it, then reacquires the gate for unchanged-row/source final validation. Corruption goes through the deletion journal before `Invalidated` is returned.

Startup order becomes: migrate v4 → segment recovery → construct removal worker and resume deletion batches → artifact recovery/reconciliation → expose `RecordingStore` to capture. Recovery reports staging rows finalized/removed, ready rows invalidated, orphan files removed, and artifact usage reconciled; a second pass is a no-op.

**Migration coordination contract:** this story exclusively edits `schema_v4.rs` and the v4 `migrations.rs` entry. The browser-event sibling's schema story must declare this story in `depends_on`, add `schema_v5.rs`, and update only the v5/latest registry. If concurrent work has already claimed v4, stop and rebase that sibling to v5 rather than merging two definitions.

**Acceptance criteria:**

- [ ] Fresh and v3 databases migrate transactionally to strict v4; future versions refuse; migration failure rolls back; legacy derived rows/usage are purged without touching source evidence.
- [ ] Equal cache-key publications converge on one ready artifact; no caller sees `staging`.
- [ ] Exact manifest JSON/hash, output bytes/hash, ordered source links/hashes, cache metadata, and usage commit/recover together.
- [ ] Injected crashes after staging transaction, temp sync, rename, directory sync, and ready commit converge on reopen without a visible dangling claim or unaccounted file.
- [ ] No SQLite transaction or recording mutation gate spans output file writing/hashing.
- [ ] Store ports never expose a relative/absolute path and reuse the current deletion journal, usage ledger, and retention candidates.

### Unit 3: encoded-frame decoder and visual-epoch adapter

**Story:** `epic-temporal-debugging-workflow-artifact-generation-and-cache-frame-adaptation-and-decoding`

**Files:**

- root `Cargo.toml` and `Cargo.lock`
- `src/artifacts/{mod.rs,decode.rs,epoch.rs}` (new)
- `tests/fixtures/artifacts/{chrome-rgb.jpg,chrome-rgba.png,malformed.jpg,bomb-header.png}` (new bounded fixtures)
- focused unit tests beside the adapter

The adapter boundary is:

```rust
pub(crate) const ADAPTER_VERSION: &str = "krometrail-artifact-adapter-v1";
pub(crate) const DECODER_PROFILE: &str =
    "image-0.25.9-forced-jpeg-png-rgba8-straight-no-orientation-v1";

pub(crate) struct EpochInput {
    pub descriptor: VisualEpoch,
    pub source_fingerprints: Vec<ArtifactSourceFingerprint>,
    pub sequence: temporal_vision::OwnedFrameSequence<
        FrameId,
        ArtifactMarkerId,
        GapId,
    >,
}

pub(crate) fn validate_and_partition(
    range: &ResolvedRange,
    frames: Vec<EncodedFrame>,
    markers: &[ArtifactMarker],
    limits: &ArtifactWorkLimits,
    cancellation: &WorkCancellation,
) -> Result<Vec<EpochInput>>;

fn decode_frame(
    frame: &EncodedFrame,
    limits: DecodeLimits,
) -> Result<temporal_vision::OwnedFrame<FrameId>>;
```

Before decoder allocation, check source-frame count, encoded-byte sum, persisted dimensions, `width * height`, and exact `width * height * 4` against request/global budgets. Configure `image::Limits` to the effective dimensions/allocation limit through `ImageReader::limits` and force `image::ImageFormat::{Jpeg,Png}` from `EncodedFrame` metadata. Compare decoded dimensions again. Match only 8-bit dynamic variants and explicitly expand their channels; do not call a conversion that silently truncates 16-bit/float input. The crate documents width/height as strict and `max_alloc` as potentially decoder-dependent, so Krometrail's checked metadata/pixel/byte reservations remain the hard allocation boundary.

Frame timestamps are `Timestamp::from_nanos(metadata.session_time().as_nanos())`; tied timestamps preserve `ResolvedRange.frame_ids`/capture-ordinal order. Source identity validation requires exactly one returned frame per requested ID, no extras/duplicates, exact session/target, strictly increasing capture ordinal, nondecreasing session time, and time within `resolved_range`.

Epoch construction clips gaps and markers to the epoch time range. Gap reason is the stable `CaptureGapReason::as_str()`; estimated missing count is preserved; gap detail is not promoted into provenance. Markers sort by `(session_time, request_position)`. The adapter passes `region=None, mask=None`; region-focused behavior comes only from the explicit generator request.

**Acceptance criteria:**

- [ ] Real default JPEG and configured PNG screencast-shaped fixtures decode to exact RGBA8 pixels/dimensions; JPEG is opaque and PNG alpha remains straight and unchanged.
- [ ] Wrong declared format, malformed/truncated images, header/decode dimension mismatch, unsupported bit depth, oversized dimensions, overflow, and decompression-bomb headers fail before unbounded allocation.
- [ ] Mixed JPEG/PNG frames with common geometry remain one epoch; image/viewport/device-scale changes split maximal contiguous epochs without resizing.
- [ ] Equal-time frame order, clipped gaps, estimated loss, marker ties, and per-epoch IDs round-trip through `OwnedFrameSequence` exactly.
- [ ] Initial source disappearance/corruption is explicit and never converts into an empty or repaired sequence.

### Unit 4: bounded generation service, single flight, and root composition

**Story:** `epic-temporal-debugging-workflow-artifact-generation-and-cache-bounded-generation-service`

**Files:**

- `src/artifacts/{service.rs,scheduler.rs,generators.rs,single_flight.rs,tests.rs}` (new)
- `src/{main.rs,app.rs}`
- root `Cargo.toml`
- temporal-vision generator calls through existing public APIs only

```rust
pub(crate) struct ArtifactWorkLimits {
    pub max_active_requests: NonZeroUsize,
    pub max_blocking_jobs: NonZeroUsize,
    pub max_parallel_generators_per_request: NonZeroUsize,
    pub max_source_frames: NonZeroUsize,
    pub max_encoded_source_bytes: NonZeroUsize,
    pub max_dimension: NonZeroU32,
    pub max_pixels_per_frame: NonZeroUsize,
    pub max_decoded_bytes: NonZeroUsize,
    pub max_normalized_bytes: NonZeroUsize,
    pub max_combined_request_bytes: NonZeroUsize,
    pub max_outputs: NonZeroUsize,
    pub max_output_bytes_each: NonZeroUsize,
    pub max_output_bytes_total: NonZeroUsize,
    pub max_markers: NonZeroUsize,
    pub max_wall_time: Duration,
}

pub(crate) struct TemporalVisionArtifactService {
    frames: Arc<dyn FrameSource>,
    artifacts: Arc<dyn ArtifactStore>,
    ids: Arc<dyn IdSource>,
    scheduler: Arc<ArtifactScheduler>,
    flights: Arc<SingleFlight>,
}

impl ArtifactGeneration for TemporalVisionArtifactService {
    fn generate(
        &self,
        request: ArtifactGenerationRequest,
        context: ArtifactGenerationContext,
    ) -> PortFuture<'_, Result<ArtifactGenerationResult>>;
}
```

The service sequence is exact:

1. validate request and effective deadline; acquire a global request permit;
2. load `range.frame_ids` once through `FrameSource` and compute exact source fingerprints;
3. partition metadata into epochs and resolve effective normalization/output parameters before keying;
4. calculate potential output count and reject above the cap;
5. lookup every output key; validated hits occupy deterministic result slots;
6. join/create a work single-flight for missing slots;
7. acquire weighted memory plus bounded CPU permits and decode/normalize in `spawn_blocking`;
8. invoke existing generators, using preassigned artifact IDs; storyboard optionally yields both existing output kinds;
9. enforce exact per-output/total bytes and publish each exact generated artifact through `ArtifactStore`;
10. collect slots in epoch/request/kind order and apply `RequireAll` or `AllowPartial` semantics.

Storyboard, difference map, and motion history reuse one `Arc<OwnedFrameSequence>` and `Arc<NormalizedSequence>` per epoch. Region filmstrip consumes the same decoded sequence but retains its own existing region normalization/render API. Reference selectors resolve to epoch-local indexes; a requested frame outside an epoch yields that epoch's deterministic unavailable/error outcome.

Root removes `use temporal_vision as _`, constructs one scheduler/service after `RecordingStore`, and retains `Arc<dyn ArtifactGeneration>` in `RuntimeDependencies` for later bundle/MCP features. No MCP handler, resource, debug bundle, event correlation, diagnosis, replay, or comparison is added.

**Acceptance criteria:**

- [ ] Global request/CPU/memory and per-request generator permits independently cap work; output order and hashes do not vary across permit counts.
- [ ] `FitLimits` chooses a deterministic exact divisor and the effective normalization is present in key and manifest; impossible geometry fails rather than stretches.
- [ ] Storyboard-only, storyboard+orientation, difference-map, fixed-region filmstrip, and motion-history requests call their existing public generators with exact typed parameters.
- [ ] Identical concurrent requests invoke decode/generation once; leader failure wakes all waiters; one cancelled waiter does not cancel remaining waiters; the last waiter suppresses publication.
- [ ] Deadline/cancellation before decode, between frames, between outputs, while awaiting a flight, and before final publication are covered; no cancelled artifact becomes ready without another live waiter.
- [ ] Saturating artifact workers does not hold the recording mutation gate or prevent a bounded `append_frame`/gap persistence operation from completing.
- [ ] Root shares one `FrameSource`, `RecordingStore` artifact port, ID source, and service; capture ingestion remains ignorant of decode/generation.

### Unit 5: retention, corruption, deletion, and integrated qualification

**Story:** `epic-temporal-debugging-workflow-artifact-generation-and-cache-retention-recovery-and-qualification`

**Files:**

- `crates/krometrail-store/src/{recording.rs,recovery.rs}` and artifact/index modules only as qualification fixes require
- `crates/krometrail-store/tests/{artifact_store.rs,artifact_recovery.rs,retention_small_budget.rs}`
- `src/artifacts/tests.rs`
- `tests/fixtures/artifacts/` golden inputs

Build a real v4 store fixture with JPEG and PNG source frames, tied times, two visual epochs, markers, a gap, tiny budgets, pins, and deterministic IDs. Exercise the root service against real `FrameSource`/`ArtifactStore` adapters, then reopen the same directory at each publication/deletion crash point.

**Acceptance criteria:**

- [ ] Fixed IDs/inputs/parameters produce stable cache key, exact manifest JSON round-trip, temporal output hash, and PNG bytes across repeated generation; focused image goldens protect decode/color/alpha and visible artifact semantics without snapshotting SQL.
- [ ] Frame order/content/format/time, markers/gaps, epoch, parameters, adapter version, or generator version changes miss; exact repeats hit and return the original artifact ID, bytes, and manifest.
- [ ] Corrupt/missing artifact bytes, manifest JSON/hash, source links, and output hash invalidate then regenerate; invalid data is never returned as a hit.
- [ ] Evicting any source segment invalidates every mixed-source staging/ready artifact before frame removal; an unrelated artifact survives with all of its sources.
- [ ] Pins preserve source frames but not artifacts; independent artifact eviction followed by regeneration leaves the pin unchanged.
- [ ] Session deletion cancels/drains active publication and leaves no source/artifact/temp/final/cache/source-link/usage row; a late CPU result cannot recreate it.
- [ ] Publication and deletion failpoints converge after reopen, usage counts exact artifact bytes once, and recovery is idempotent.
- [ ] Source count, encoded/decoded/normalized/combined memory, dimensions/pixels, marker/output counts, per-output/total bytes, deadline, and cancellation fail at exact boundaries.
- [ ] `RequireAll` versus `AllowPartial` behavior is deterministic across mixed epochs, decode failures, unsupported references, and generator failures.
- [ ] A manual ignored two-second 1080p workload reports uncached/cached latency, peak decoded/normalized/output bytes, CPU jobs, and concurrent ingestion latency against `docs/EVALUATION.md`; CI uses deterministic bounds/no-starvation assertions rather than timing-sensitive pass/fail thresholds.
- [ ] Rust 1.85 locked format/check/test/Clippy gates pass. No test is added for trivial getters/wrappers, each SQL statement, or full schema snapshots.

## Implementation order

1. `epic-temporal-debugging-workflow-artifact-generation-and-cache-artifact-contracts-and-cache-identity`
2. `epic-temporal-debugging-workflow-artifact-generation-and-cache-artifact-schema-and-publication` — depends on contracts/cache identity
3. `epic-temporal-debugging-workflow-artifact-generation-and-cache-frame-adaptation-and-decoding` — depends on the authoritative store/cache contract
4. `epic-temporal-debugging-workflow-artifact-generation-and-cache-bounded-generation-service` — depends on store publication and decoded epoch input
5. `epic-temporal-debugging-workflow-artifact-generation-and-cache-retention-recovery-and-qualification` — depends on the composed service

These are sequential checkpoints for one future feature owner. They preserve cross-resource ordering and acceptance evidence; they are not five worker assignments. The artifact-schema checkpoint is also the explicit migration dependency that the sibling browser-event feature must consume before adding schema v5.

## Simplification and elimination

- Replace the root's no-op temporal-vision import with one real adapter service.
- Turn the reserved/test-only artifact tables into the only cache/store authority; do not add a cache table, filesystem index, sidecar manifest, or directory-size usage ledger.
- Reuse `FrameSource::frames_by_id`; do not create an artifact-specific frame reader or persistent decoded-frame cache.
- Reuse `ArtifactManifest`, `ArtifactKind`, normalization records, output hash, and generator APIs directly from temporal-vision; no Krometrail manifest DTO exists.
- Centralize temporal generator versions so private constants cannot drift from cache identity.
- Reuse the store mutation gate, deletion journal, artifact usage class, source links, session deletion, and retention candidate queries. Staging is an artifact state, not another recovery system.
- Keep orientation coupled to storyboard selection rather than adding a redundant generator path.
- Keep motion history explicit/opt-in pending evaluation; this feature supports it but does not put it in a default bundle.
- Keep MCP, resources, event context, bundle assembly, natural anchors, diagnosis, replay, and comparison unchanged.

## Testing strategy

- **Stable application seam:** request validation, generator registry, orientation semantics, result ordering, and exact manifest alias protect future bundle/MCP callers.
- **Complex pure unit:** cache transcript and epoch partitioning get exhaustive one-field-change examples because a collision would return false provenance.
- **Decoder regression:** small real JPEG/PNG fixtures protect forced format, RGBA/color/alpha semantics, dimensions, and malformed/bomb handling.
- **Store integration:** publication failpoints, exact hit validation, source-link invalidation, usage, and recovery protect the cross-filesystem/SQLite invariant.
- **Concurrency regression:** controlled decode/generator barriers protect single-flight, cancellation, memory/CPU permits, and capture non-starvation without wall-clock sleeps.
- **Retention integration:** tiny budgets, mixed-source artifacts, pins, corruption, and session deletion protect the one existing authority.
- **Golden evidence:** retain focused decoded pixels and existing temporal output SHA-256 goldens; add a full image golden only where visible adapter labels/alpha cannot be protected by semantic pixel regions.
- **Manual benchmark:** report EVALUATION metrics without making ordinary CI depend on host speed.
- Do not snapshot migration SQL, test trait delegation, assert every row getter, or duplicate temporal-vision's already-complete generator algorithm tests.

## Risks and rollback

- **Artifact publication crosses SQLite and files.** The staging/ready state and file-before-ready ordering are the mitigation. If startup finalization proves unreliable, recovery may conservatively discard every staging row/file and regenerate; ready semantics and the public/store ports do not change.
- **Decoder color behavior can overclaim.** The adapter accepts only current 8-bit screencast outputs, performs no implicit orientation/profile transform, records the exact profile, and rejects unsupported precision. If real Chrome evidence contradicts the sRGB assumption, bump the adapter version and add explicit color management rather than silently changing cached outputs.
- **Memory can still be large while bounded.** Full decoded and normalized sequences coexist because current temporal-vision APIs require both. `FitLimits`, weighted reservations, and low CPU concurrency bound the cost. If measured 1080p workloads remain too costly, evolve temporal-vision toward metadata/pixel separation or epoch streaming behind the same service port; do not add a second decoded cache.
- **Blocking work cannot be forcibly killed safely.** Cancellation prevents publication and bounds each unit. If a verified codec can exceed the hard wall materially under bounded dimensions/bytes, the rollback/fallback is to disable uncached generation explicitly while capture and retained hits remain operational, then isolate decoding in a subprocess in a separately designed feature.
- **Retention can invalidate work mid-generation.** This wastes bounded CPU but never publishes stale provenance because cache return and publication revalidate. Callers can pin source ranges through the existing authority when they need stronger lifetime.
- **Migration collision with browser events.** Artifact v4 ownership and the sibling v5 dependency are explicit. If concurrent changes land first, rebase version numbers and preserve contiguous one-run migration; never merge two schema-v4 definitions.
- **Staging rows reserve budget before files exist.** This is intentionally conservative. Recovery removes abandoned reservations; status may temporarily overcount, never undercount. If reservation churn affects capture, lower artifact concurrency rather than bypassing the ledger.
- **Legacy artifact rows are purged by v4.** They were never production-written and cannot satisfy exact cache/source/hash contracts. They are derived, regenerable data. The migration transaction rolls back as a unit on failure and never removes source evidence.

## Pre-mortem

The likeliest severe failure is a cache hit whose PNG and manifest look valid but whose source frame was evicted or whose cache key omitted one output-affecting detail. The agent would receive confident, unreproducible evidence. The design attacks both halves: source links participate in retention deletion, every hit revalidates source rows plus exact bytes/manifest, and the cache transcript includes encoded content, source timing/geometry/scale, markers/gaps, effective parameters, output kind, and both algorithm versions.

The next failure is capture starvation caused by a large decode while holding the store gate or consuming every CPU. The design reads source bytes and performs all decoding/rendering outside the mutation gate, caps CPU jobs independently, and includes a controlled append-during-generation regression. The least certain area is real Chrome color metadata across platforms; the initial contract is deliberately narrow and versioned, with reject/bump rather than silent conversion as the fallback.

## Implementation record

- Completed sequential checkpoints: contracts/cache identity (`5a73b16`), schema v4/publication (`e524bc0`), frame adaptation/decoding (`bf0e74f`), bounded service (`13e6464`), and retention/recovery/qualification (`72daea2`). Each child records its exact files, verification, decisions, and discrepancies and is now `done`.
- Core exposes validated artifact request/result/service/store contracts while carrying the exact generic `temporal_vision::ArtifactManifest` alias. One descriptor registry and one length-framed SHA-256 transcript bind complete source, epoch, marker/gap, effective parameter, output-kind, adapter, and algorithm identity.
- Store schema v4 is the exclusive artifact migration and sole durable authority: staging/ready rows, ordered source hashes, exact manifest and artifact hashes, conservative usage, atomic file publication, hit validation, corruption invalidation, deletion-journal reuse, retention linkage, startup convergence, and session deletion fencing.
- Root now forces declared JPEG/PNG decode through exact `image 0.25.9`, preserves the narrow RGBA8/epoch contract, invokes all four temporal generator families, materializes `FitLimits`, and composes one shared service. CPU work runs through bounded blocking workers under independent request/CPU/memory/generator/frame/pixel/output/deadline/cancellation ceilings; deterministic result slots and process-wide single flight preserve ordering and prevent late publication.
- Integrated real-store qualification covers mixed formats, tied epoch boundaries, two epochs, markers/gaps, exact cache hits and deterministic bytes, corruption regeneration, limit boundaries, permit independence, last-waiter cancellation, source eviction, pin behavior, recovery, session-deletion races, and ingestion non-starvation. The ignored synthetic 24-frame 1080p workload passed when explicitly run and reports workload shape without a speed threshold or live-Chrome claim.
- Final verification: Rust 1.85 locked workspace all-target format/check/test/Clippy `-D warnings` passed in an isolated copy that excluded another feature owner's concurrent uncommitted CDP event-transport files; focused root/core/store qualification passed again after the final retention case. No MCP, presentation, browser-event, natural-anchor, diagnosis, replay, comparison, UI, documentation, or foundation surface was added.
- Review posture: standard feature-level review was performed independently after implementation.

## Review (2026-07-14)

**Verdict**: Approve after fix

**Blockers**: none

**Important finding fixed**:
- Three Tokio `Notify` state-check loops could lose `notify_waiters()` broadcasts before their `Notified` futures registered, delaying single-flight/cancellation waiters and theoretically hanging session-deletion publication drain. Focused story `bug-prevent-artifact-notify-lost-wakeups` pinned and enabled notifications before state checks at all three sites, added multi-threaded regression guards, passed the full Rust 1.85 workspace gate, and was reviewed/archived in commits `4ba4214`, `dae0739`, and `ace0b39`.

**Nits adjudicated**:
- Artifact errors remain source-safe but often lack scope context; this valid lower-risk improvement is parked as `idea-artifact-error-context` for the first bundle/MCP consumer.
- Schema v4's use of the stable `artifacts`/`artifact_frames` names matches the design's rebuild intent; illustrative `_v4` names required no change.

**Evidence**: Independent cross-model standard review verified all eleven material lenses: one resolved-range boundary, authoritative manifests/generator registry, complete deterministic cache identity, exact Rust-1.85 JPEG/PNG decode, epoch/gap/marker fidelity, bounded scheduling and single-flight, transactional v4 migration, source-valid atomic publication, recovery/usage/retention/session-deletion convergence, failure-policy semantics, root composition, and qualification. Focused review passed 495 tests and Clippy; the accepted concurrency fix then passed current locked Rust 1.85 format, full workspace tests, and Clippy with warnings denied. Standard weight requires no re-review.
