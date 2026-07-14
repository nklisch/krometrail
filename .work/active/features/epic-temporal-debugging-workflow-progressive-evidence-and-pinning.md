---
id: epic-temporal-debugging-workflow-progressive-evidence-and-pinning
kind: feature
stage: implementing
tags: [visual, storage, agent-ux]
parent: epic-temporal-debugging-workflow
depends_on:
  - epic-temporal-debugging-workflow-resolved-temporal-queries
  - epic-temporal-debugging-workflow-artifact-generation-and-cache
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Progressive Evidence Retrieval and Pinning

## Brief

Deliver the focused investigation operations beneath the primary bundle: retrieve an individual generated artifact, list or fetch selected retained source frames, generate a fixed region filmstrip with locator context, and request supported artifact variants without loading a complete recording. Every result remains tied to the same resolved session, target, source identities, gaps, and provenance used by the bundle.

Support the SPEC region forms by mapping declared viewport/source coordinates, a region selected from a source frame, current structured-reference geometry, or a caller mask into the existing temporal-vision contracts without claiming logical element tracking. Add pin and unpin operations that protect the storage segments intersecting the exact resolved range, and report the actual protected range/segments and retention state so agents know what evidence remains available.

This feature owns progressive domain operations and stable evidence handles for later resource presentation. It does not create tracked regions, infer geometry across time, pin derived artifacts as authoritative evidence, expose remote file access, or maintain a second source-frame read or retention path.

## Epic context

- Parent epic: `epic-temporal-debugging-workflow`
- Position in epic: progressive-detail capability — parallel to bundle composition after range and artifact foundations; consumed by MCP resources and focused tools

## Simplification opportunity

- Reuse `FrameSource`, the artifact cache, structured snapshot geometry, and `RetentionStore` directly behind one progressive-evidence service. Do not duplicate frame decoding, resource payload storage, region math, or pin bookkeeping in MCP handlers.

## Foundation references

- `docs/SPEC.md` — Temporal Queries, Regions of Interest, Disk Budget and Retention, and Artifact Provenance
- `docs/ARCHITECTURE.md` — Retention, Temporal Range Resolution, Artifact Generation, and MCP Boundary
- `docs/VISUAL-EVIDENCE.md` — Region Filmstrip and Progressive Detail

## Grounding and dispatch

- **Driver:** active autopilot `--all`; the caller required no questions, no nested agents/peeragent, highest capability, and standard implementation review.
- **Dispatch:** direct-read only. Grounding covered `AGENTS.md`, `.agents/rules/*.md`, `.work/CONVENTIONS.md`, all five foundation documents, the parent epic, the completed resolved-query design/commits and review, artifact implementation `5a73b16` through `622f9be`, durable retention/pinning, structured page observation/reference lifetime, temporal-vision filmstrip/geometry/mask contracts, schema-v5 event retention, root composition, and the current core/store/CDP/root source and focused tests.
- **Current authorities:** `ResolvedRange` is the only resolved interval; `RecordingStore` owns mutation/deletion/retention order; `FrameSource` is the only encoded-frame read contract; `ArtifactStore` and `ArtifactGeneration` own validated cache hits and generation; `RetentionStore` owns exact pin rows and segment links; `SnapshotRegistry` owns the one live reference generation per target; temporal-vision owns `RegionDefinition`, `ViewportMapping`, `SignedPixelRect`, and `BinaryMask` math.
- **UI:** this is an application/API/resource foundation with no human screen or journey. Mockups are intentionally skipped.
- **Review weight:** standard at feature implementation review. Design-time advisory review is skipped because the caller prohibited nested agents and peeragent.
- **Parallel-work safety:** only this feature and its new prefixed stories are changed by design. The modified `.work/bin/work-view` is preserved and excluded from the commit.

## Design decisions

### One application registry and one store authority

- **Registry:** Add one macro-backed `PROGRESSIVE_EVIDENCE_REGISTRY` in core. It generates operation kind, stable name, typed request/result association, Serde request routing, and exhaustive registry tests for eight operations: `retrieve_artifact`, `list_source_frames`, `fetch_source_frames`, `generate_artifacts`, `generate_region_filmstrip`, `pin_resolved_range`, `unpin_resolved_range`, and `query_pin_state`.
- **Resolved input:** Every frame, generation, region, and pin request consumes one existing `ResolvedRange`. There is no natural anchor, time parser, or re-resolution in this service. Artifact retrieval is the sole exception because the persisted artifact ID already identifies a generated result; it still requires expected session/target scope.
- **Store composition:** Define `ProgressiveEvidenceStore` only as the intersection of the existing `FrameSource + ArtifactStore + RetentionStore` ports. `RecordingStore` implements the existing ports and is root-wired once as this composite authority. The marker adds no read method, cache, lock, pin table, or ledger.
- **Application service:** `ProgressiveEvidenceService` lives in `src/progressive/`. It depends on one `Arc<dyn ProgressiveEvidenceStore>` and the existing `Arc<dyn ArtifactGeneration>`. MCP will invoke this one service later; it will not read `SqliteIndex`, segment files, artifact files, or retention tables.
- **Exact associations:** The registry declaration is the complete operation surface:

```rust
define_progressive_evidence_operations! {
    RetrieveArtifact(RetrieveArtifactRequest) => ArtifactRead {
        stable_name: "retrieve_artifact",
    },
    ListSourceFrames(SourceFramesRequest) => SourceFrameList {
        stable_name: "list_source_frames",
    },
    FetchSourceFrames(SourceFramesRequest) => SourceFrameBatch {
        stable_name: "fetch_source_frames",
    },
    GenerateArtifacts(ArtifactGenerationRequest) => ArtifactGenerationResult {
        stable_name: "generate_artifacts",
    },
    GenerateRegionFilmstrip(RegionFilmstripEvidenceRequest) => RegionFilmstripEvidence {
        stable_name: "generate_region_filmstrip",
    },
    PinResolvedRange(ResolvedRangeEvidenceRequest) => PinChange {
        stable_name: "pin_resolved_range",
    },
    UnpinResolvedRange(ResolvedRangeEvidenceRequest) => PinChange {
        stable_name: "unpin_resolved_range",
    },
    QueryPinState(ResolvedRangeEvidenceRequest) => PinState {
        stable_name: "query_pin_state",
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SourceReadLimitsRequest {
    max_frames: NonZeroU16,
    max_item_bytes: NonZeroU64,
    max_total_bytes: NonZeroU64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RetrieveArtifactRequest {
    pub scope: EvidenceScope,
    pub artifact_id: ArtifactId,
    pub max_encoded_bytes: NonZeroU64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SourceFramesRequest {
    pub range: ResolvedRange,
    pub selection: SourceFrameSelection,
    pub limits: SourceReadLimitsRequest,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RegionFilmstripEvidenceRequest {
    pub range: ResolvedRange,
    pub region: ProgressiveRegion,
    pub markers: Vec<ArtifactMarker>,
    pub anchor: SessionTime,
    pub tile_limit: u8,
    pub background: temporal_vision::Rgb8,
    pub padding: temporal_vision::Rgb8,
    pub display_scale: AnalysisScale,
    pub labels: ArtifactLabelsRequest,
    pub output: OutputLimitsRequest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ResolvedRangeEvidenceRequest {
    pub range: ResolvedRange,
}

pub struct SourceFrameList {
    pub range: ResolvedRange,
    pub frames: Vec<SourceFrameHandle>,
}

pub struct SourceFrameBatch {
    pub range: ResolvedRange,
    pub frames: Vec<SourceFrameRead>,
}

pub struct RegionFilmstripEvidence {
    pub region: ResolvedProgressiveRegion,
    pub generation: ArtifactGenerationResult,
}
```

Constructors and custom deserializers re-run all nested invariants. `SourceReadLimitsRequest` accepts only values at or below runtime caps and requires `max_item_bytes <= max_total_bytes`. Region generation is always `RequireAll` after the service proves one epoch; generic generation preserves the caller's existing failure policy.
- **Current geometry dependency:** Current structured-reference requests receive a narrow `CurrentReferenceGeometry` port through non-serialized execution context. The service is still a single root-wired instance; the active browser-session owner supplies the current-only port at call time. This avoids a root-global session registry and lets non-browser operations run without a live browser.

### Stable handles, bytes, and lifetime

- **Two typed handles:** Reuse and complete `ArtifactHandle` for artifacts, and add `SourceFrameHandle` for source frames. Each carries typed ID, exact `EvidenceScope { session_id, target_id }`, media type, SHA-256, encoded byte length, and provenance. Artifact provenance is the exact temporal-vision manifest; source provenance is the full current `CapturedFrame` metadata including format, image/viewport dimensions, device scale, source/observed/session times, capture ordinal, and warnings.
- **No locations:** Handles contain no absolute/relative path, data URL, base64 payload, SQLite address, segment offset, CDP identifier, or MCP URI. A future MCP resource may encode the typed ID and scope in its own validated resource key, but this feature adds no URI/resource protocol.
- **Request-scoped bytes:** `retrieve_artifact` and `fetch_source_frames` return bounded `Arc<[u8]>` payloads beside their handles because those calls explicitly request bytes. `list_source_frames`, generation, region generation, and pin operations return handles/metadata only. Result byte containers are deliberately not Serde/schema values, matching `EncodedFrame`/`EncodedScreenshot`; the future resource adapter reads again by typed ID/scope rather than retaining a process-wide payload map.
- **No streaming port yet:** The hard per-item/total byte caps make a request-scoped payload the shorter contract than a reader/stream lifecycle. If measured resource reads require streaming, it can be added behind `ProgressiveEvidence` later without changing handle identity. There is no long-lived open file or in-memory payload handle in v1.
- **Weak retained lifetime:** A handle means “this exact content was valid at successful return,” not a lease. Retention may invalidate it immediately afterward. A later typed read revalidates scope, row/source links, exact bytes/hash/length, and session lifetime; eviction or session deletion returns `NotFound`. A corrupt derived artifact is invalidated through the existing cache/deletion authority and returns explicit `EvidenceInvalidated`; source corruption is `PersistenceFailed` because source evidence is not regenerable.
- **Read race:** Encoded reads use optimistic store validation: snapshot exact metadata under the mutation gate, read/hash bytes outside it, then reacquire the gate and prove the session, row, ordered identities, metadata, and hashes are unchanged. Eviction/deletion during the read discards bytes and returns `NotFound`; a successful call never returns content that became invalid during its read. The gate is not held across segment/artifact file I/O, hashing, decode, render, or browser protocol work.

### Source-frame listing and fetch

- **Selection:** `SourceFrameSelection::ResolvedOrder` selects the exact `ResolvedRange.frame_ids` capture order. `SourceFrameSelection::Ids(Vec<FrameId>)` preserves caller request order. IDs must be unique, non-empty, and a subset of the range. Every handle reports both `request_position` and `resolved_position`, so ties and caller reordering remain explicit.
- **Ordering:** Range order remains the resolver's deterministic capture-ordinal order. A selected-ID fetch returns request order exactly. The store must return one exact frame for every requested ID; extras, duplicates, scope disagreement, changed metadata, decreasing ordinals, or missing IDs fail the whole operation rather than returning a misleading partial list.
- **Metadata:** Every list/fetch handle includes MIME (`image/jpeg` or `image/png`), declared `ImageFormat`, image and viewport dimensions, device scale, source/observed/session times, capture ordinal, warnings, exact encoded byte length, and SHA-256 of the exact encoded payload.
- **Hash without a new column:** Listing reads the same bounded encoded payloads to compute length/hash and then discards bytes. It does not add a frame-hash schema column or side ledger. Callers must select a narrower set if the bounded scan is too large.
- **Bounds:** Runtime defaults cap one operation at 64 frames, 32 MiB per source frame, and 256 MiB total encoded bytes. Request limits may lower these values but cannot raise them. Count and persisted length are checked before allocation where available and total actual bytes are checked while reading. No operation returns partial bytes after a limit failure.

### Artifact retrieval and variants

- **Typed retrieval:** `RetrieveArtifactRequest` requires `(session_id, target_id, artifact_id)` and bounded bytes. `StoredArtifact` gains persisted scope so the store validates expected scope instead of relying on globally unique UUID probability. `ArtifactStore::artifact` returns `Missing | Available | Invalidated`, preserving the difference between ordinary lifetime expiry and a corrupt cache entry removed during validation.
- **Authoritative validation:** Artifact-by-ID retrieval uses the same ready-row, source-link, manifest, output-hash, byte-length, and source-retention validation already authoritative for cache hits. It does not implement a looser resource read.
- **Variants:** `GenerateArtifactsRequest` wraps the existing `ArtifactGenerationRequest` unchanged and delegates to `ArtifactGeneration::generate`. The four generator variants and optional storyboard orientation remain owned by `ArtifactGeneratorRequest`; progressive evidence does not copy their registry or implement another cache/single-flight path.
- **Region delegation:** `generate_region_filmstrip` resolves one declared region, then constructs exactly one existing `ArtifactGeneratorRequest::RegionFilmstrip` and calls the same service. Decode, epoch adaptation, deterministic generation, cache keys, single flight, publication, source-link invalidation, cancellation, and deadlines stay in the implemented artifact service.

### Region forms and coordinate contract

- **No tracking:** Every region is fixed after request resolution. `SelectedFromSourceFrame` is a caller declaration, not computer-vision selection. `CurrentReference` samples current geometry once and converts it to a fixed viewport region. No form follows a logical element, re-resolves an anchor per frame, or infers geometry through time.
- **Wire forms:** `ProgressiveRegion` has four variants: `SourcePixels { rect, source_frame_id }`, `ViewportCss { rect, source_frame_id }`, `SelectedFromSourceFrame { source_frame_id, shape }`, and `CurrentReference { session_id, reference, source_frame_id }`. Forms 3 and 5 from the brief intentionally share `SelectedFromSourceFrame`: `CallerRegionShape` is `Rect(SignedPixelRect) | Mask(BinaryMask)`. This is the same honest operation—a caller supplies a rectangle or one-bit mask anchored to a named frame—so separate “selected” and “caller-provided” APIs would imply a nonexistent CV selector.
- **Resolved mapping result:** The service returns the exact fixed domain it passed to artifact generation:

```rust
pub struct ResolvedProgressiveRegion {
    pub declared: ProgressiveRegion,
    pub source_frame: CapturedFrame,
    pub temporal_region: temporal_vision::RegionDefinition,
    pub mask: Option<temporal_vision::BinaryMask>,
    pub viewport_mapping: Option<temporal_vision::ViewportMapping>,
    pub reference_geometry: Option<ResolvedReferenceGeometry>,
}
```

For masks, `temporal_region` is the non-empty mask bounds in source pixels and `mask` retains the full exact shape. For source rectangles it is `FixedSourceImage`; for CSS/current geometry it is `FixedViewport`. This result records selection provenance without placing browser reference identity inside the browser-agnostic artifact manifest.
- **Chosen frame:** `source_frame_id` must occur in the supplied `ResolvedRange`, and it is always the locator frame. It declares the image/viewport/device-scale mapping used for the fixed region; it is not a claim that the region was detected in that frame.
- **Epoch compatibility:** Before region generation, load metadata for every range frame and require one exact visual epoch: identical image dimensions, viewport dimensions, and `DeviceScaleFactor::get().to_bits()`. The chosen frame must match that epoch. A multi-epoch range fails with recovery to narrow/re-resolve the interval; it does not silently apply one mapping across resize/scale changes or rely on artifact partial mode.
- **Source pixels:** A `SignedPixelRect` is a half-open rectangle in the chosen frame's source-image pixels. Negative/beyond-edge coordinates are permitted and use temporal-vision's existing intersection and explicit padding plan. Overflow/zero dimensions fail. The same fixed source rectangle is applied to every frame in the compatible epoch.
- **Viewport/CSS:** A `CssRect` is relative to the current visual viewport in CSS units. Temporal-vision gains `SignedPixelRect::from_outward_f64_bounds`, which floors left/top and ceils right/bottom, including negative coordinates. `ViewportMapping::for_source(viewport, image)` derives canonical exact rational X/Y scales from the chosen frame's persisted viewport and image dimensions; existing temporal-vision code performs the second outward mapping to source pixels and rejects contradictory geometry. Core/root do not copy rational rounding math.
- **Device scale:** Actual `image / viewport` rational scales perform the pixel mapping because capture downscaling can differ from nominal device scale. The persisted device-scale bits remain part of epoch compatibility, resolved-region provenance, and the artifact cache identity. The service never multiplies CSS geometry by a guessed host/browser scale.
- **Current reference:** `CurrentReferenceGeometry` verifies exact session, target, `SnapshotGeneration`, `SnapshotNodeId`, attachment generation, current document fingerprint, backing node, visibility, and finite non-zero geometry through the existing `SnapshotRegistry` resolver. It reads fresh layout viewport geometry, converts the document quad bounds to a viewport-relative `CssRect` without clipping, and returns only core IDs/geometry/timing. A stale generation, wrong target/session, reconnect, navigation, detached/hidden node, or missing active snapshot fails explicitly. The returned geometry is current-only and is never stored as historical node identity; the selected source frame merely supplies the declared fixed mapping.
- **Reference timing:** `ResolvedReferenceGeometry` reports the exact reference, attachment generation, and `resolved_at` session time. The result does not assert pixel/snapshot simultaneity or that the chosen retained frame captured that exact layout. It says only that current geometry was projected through the named frame's contract. The generated artifact manifest records the final fixed viewport/source mapping and source IDs, not a durable element identity.
- **Masks:** `BinaryMask` remains temporal-vision's full-frame row-major, MSB-first, one-bit-per-pixel encoding. The wire is its validated dimensions plus raw byte array—never a data URL or image codec. Progressive requests cap masks at 8,192 on either dimension, 16,777,216 pixels, and 2 MiB encoded bits; dimensions must exactly equal the chosen source image and every compatible epoch frame; unused tail bits remain zero; an all-zero mask is invalid.
- **Masked filmstrip:** Temporal-vision adds `BinaryMask::bounds()` and optional `RegionFilmstripParameters::with_mask`. The crop is the mask's non-empty source-pixel bounds; excluded pixels inside that crop render with the declared padding/mask pattern and legend, the locator shows the fixed mask bounds, and the exact mask enters `ArtifactManifest.mask`, deterministic parameters, and cache identity. The mask is applied at identical source coordinates to every frame. It is not resized, clipped, tracked, inferred, or interpreted as element identity.
- **Clipping/padding:** Rectangles use temporal-vision's existing half-open intersection and `PaddingInsets`; fully outside rectangles remain valid all-padding evidence with visible warning. Masks cannot extend beyond their exact full-frame dimensions. Filmstrip tile dimensions/scale remain constant, and the locator is the declared source frame.

### Pin semantics and reporting

- **Exact resolved command:** `RetentionPinRequest::from_resolved(&ResolvedRange)` carries the existing exact `RetentionRange { session_id, target_id, range: resolved_range }` plus the ordered frame IDs expected to remain retained. The wrapper is a validation command, not a second range type or pin ledger.
- **Atomic revalidation:** `RecordingStore::pin_range` holds its existing mutation gate, rejects deleted sessions, flushes/registers open segments for the session, proves every expected frame still exists in exact order/scope/range, and only then creates the exact pin row and segment links. A stale/partly evicted range is `NotFound`; it never produces an empty or partial pin.
- **Segment granularity:** The exact pin row remains `(session, target, start, end)`. It protects every sealed segment intersecting the inclusive range. Actual segment bounds can extend beyond the requested interval and are returned explicitly. Flushing the session may seal unrelated targets, but only intersecting segments for the exact target are linked.
- **Reporting extension, no migration:** Replace the thin `PinChange` payload with focused `PinState`/`ProtectedSegment` reporting derived from the existing `pins`, `pin_segments`, `segments`, and usage rows. No schema or ledger is added.

```rust
pub enum PinProtectionScope { SourceSegmentsOnly }

pub struct ProtectedSegment {
    pub segment_id: SegmentId,
    pub retained_range: SessionRange,
    pub byte_len: u64,
}

pub enum RangeEvidenceAvailability {
    Complete,
    PartiallyUnavailable {
        retained_frame_ids: Vec<FrameId>,
        missing_frame_ids: Vec<FrameId>,
    },
    Unavailable { missing_frame_ids: Vec<FrameId> },
}

pub struct PinState {
    pub request: RetentionRange,
    pub exact_pin_active: bool,
    pub evidence: RangeEvidenceAvailability,
    pub protection_scope: PinProtectionScope,
    pub protected_segments: Vec<ProtectedSegment>,
    pub coalesced_protected_ranges: Vec<SessionRange>,
    pub pinned_usage_bytes: u64,
    pub retention: RetentionStatus,
}

pub struct PinChange {
    pub changed: bool,
    pub state: PinState,
}
```

- **Meaning after mutation:** `protected_segments` are segments intersecting the request that remain protected by any pin after the operation; coalesced ranges are their true union, not a falsely continuous min/max span. `exact_pin_active` distinguishes this exact request from overlap protection. `changed=false` means repeated pin or repeated unpin was idempotent.
- **Overlap:** Exact overlapping range pins remain independent. Unpin removes only the exact row. A segment remains in post-unpin state while another pin references it. `pinned_usage_bytes` remains global distinct segment usage and is not double-counted by overlap.
- **Unpin and budget recovery:** Unpin accepts the exact `RetentionPinRequest`, removes only its exact range row even if evidence has since become unavailable, immediately enforces the budget under the same gate, wakes paused recording only when the final status is `Available`, and then reports post-enforcement state. Newly unprotected segments may therefore be evicted before return; the availability and protected ranges describe the final truth.
- **Pin and budget:** Pin succeeds as user retention authority, then enforces budget against other removable artifacts/events/unpinned segments. If protected evidence fills the budget, the returned status is `PausedBudget`; pinned source is not deleted. Capture resumes through the existing watch generation after unpin/deletion/increased budget makes the status available.
- **Query:** `query_pin_state` performs the same exact frame availability and overlap queries under the mutation gate without changing a pin. A stale unpinned handle can report partial/unavailable evidence; session deletion is `NotFound` because no pin or evidence identity survives deletion.
- **Artifacts and events:** Pins protect source segments only. Derived artifacts remain independently evictable and regenerable, and browser events remain independently evictable with v5 unavailable-range tombstones. Pin reporting states this scope explicitly; it never reports artifact/event bytes as protected. Session deletion overrides pins and removes source segments, artifacts, events, pin rows, and indexes.

### Validation, errors, cancellation, and privacy

- **Boundary validation:** All operation requests, selections, limits, scope values, region forms, geometry, masks, and retention commands use validated constructors plus `deny_unknown_fields` Serde. The registry's request enum is the future schema source; no MCP DTO copy is designed here.
- **Cancellation/deadline:** `ProgressiveEvidenceContext` carries deadline, cancellation, and optional current-geometry port. It derives the existing `ArtifactGenerationContext`. Source/artifact reads check cancellation before/after I/O and before returning bytes. Pin mutations are cancellation-safe at transaction boundaries: an unpolled future does nothing; once the immediate transaction commits, the operation completes/reporting or returns an explicit persistence failure rather than pretending cancellation rolled it back.
- **Errors:** malformed selection/geometry/mask/limits is `InvalidInput`; limits are `ResourceLimitExceeded`; stale live references are `StaleReference`; absent geometry source is `InvalidLifecycleTransition`; ordinary eviction/deletion/missing IDs are `NotFound`; corrupt derived cache invalidation is `EvidenceInvalidated`; source/store corruption is `PersistenceFailed`; generation errors retain existing codes; all-pinned pressure is visible in `PinState.retention` and later capture may return existing `BudgetExhausted`.
- **Retry/recovery:** `NotFound` is not retryable by the same handle and instructs the caller to re-resolve/re-list; `EvidenceInvalidated` instructs regeneration from the original artifact request if sources remain; stale references instruct a fresh snapshot and retry with its new reference; epoch/mapping failures instruct a narrower single-epoch range/current frame; resource-limit failures instruct fewer IDs/smaller output; transient persistence I/O is safe only where the underlying error says so.
- **Log safety:** Logs contain operation stable name, Krometrail IDs, counts, byte totals, result class, and timing. They never include image/mask bytes, page text, selectors, node names, URLs, artifact manifest JSON, filesystem paths, raw geometry, CDP values, or serialized requests. Stable errors contain typed scope/range and recovery guidance only.

## Architectural choice

### Option A — implement progressive reads and pinning in future MCP handlers

MCP already owns the active browser session, so handlers could read frames/artifacts and call retention directly. This would couple protocol/resources to storage, duplicate bounds and region mapping across tools/resources, and make reference geometry look like historical identity. Rejected.

### Option B — compose independent index, artifact, retention, and browser calls without one store-facing authority

A service over `SqliteIndex` as `FrameSource`, `RecordingStore` as artifact/retention, and browser operations could be locally small. Eviction/session deletion could interleave between metadata and payload reads or between range validation and pin insertion; root would keep exposing the index as the production frame reader; artifact/source resource semantics would diverge. Rejected.

### Option C — one registry/service over a composite existing-store port plus contextual live geometry (chosen)

Core owns registry/contracts; root owns one `ProgressiveEvidenceService`; `RecordingStore` implements the existing ports and optimistic coherent reads; `ArtifactGeneration` remains the only decode/cache path; `RetentionStore` extends reporting over its existing rows; `CurrentReferenceGeometry` is a narrow current-only browser port supplied in execution context. This is the shortest architecture that joins lifetime, region, cache, and pin behavior without a second reader/cache/ledger or direct MCP/CDP/store dependency.

A durable resource-payload table and long-lived read leases were also rejected. Bounded request-scoped bytes plus optimistic final revalidation provide honest reads without another payload store or making ordinary handles retention locks. Pins remain the only user-visible lifetime authority.

## Trickiest unit first: coherent weak handles across files, SQLite, and deletion

The hardest unit is proving a successful read/pin describes one still-valid evidence generation while never holding the recording mutation gate across file I/O, browser protocol work, or visual generation.

For source/artifact reads:

```text
mutation gate: validate session + snapshot exact row/metadata/source links
        │ release
        ▼
read bounded bytes → verify length/hash/manifest/source payloads
        │
        ▼
mutation gate: reject deleted session + re-read exact row/metadata/links
        │
        ├── changed/missing → discard bytes, NotFound/Invalidated
        ▼
return request-scoped bytes + weak stable handle
```

For a pin:

```text
mutation gate
  → reject deleted session
  → flush/register open session segments
  → revalidate exact ordered ResolvedRange frame IDs/scope/range
  → insert-or-observe exact pin + intersecting segment links (one tx)
  → enforce global budget without deleting linked segments
  → query exact/overlap pin state + coalesced actual segment ranges + status
release gate → return PinChange
```

No caller can turn a read into a retention lease. If it needs continued availability, it explicitly pins the exact resolved range and receives segment-granular truth.

## Implementation units

### Unit 1: progressive registry, handles, region/mask, and pin contracts

**Story:** `epic-temporal-debugging-workflow-progressive-evidence-and-pinning-contracts-and-region-semantics`

**Files:**

- `crates/krometrail-core/src/progressive.rs` (new)
- `crates/krometrail-core/src/ports/progressive.rs` (new marker/service ports)
- `crates/krometrail-core/src/ports/{browser.rs,artifacts.rs,retention.rs,mod.rs}`
- `crates/krometrail-core/src/{artifacts.rs,error.rs,lib.rs}`
- `crates/krometrail-core/src/recording/retention.rs`
- `crates/temporal-vision/src/{geometry.rs,filmstrip.rs,lib.rs}`
- `crates/temporal-vision/tests/filmstrip.rs`

Representative application boundary:

```rust
pub struct EvidenceScope {
    pub session_id: SessionId,
    pub target_id: TargetId,
}

pub struct SourceFrameHandle {
    pub scope: EvidenceScope,
    pub frame_id: FrameId,
    pub request_position: u32,
    pub resolved_position: u32,
    pub media_type: NonEmptyText,
    pub content_sha256: Sha256Digest,
    pub encoded_byte_len: u64,
    pub metadata: CapturedFrame,
}

pub struct ArtifactRead {
    pub handle: ArtifactHandle,
    encoded_bytes: Arc<[u8]>,
}

pub struct SourceFrameRead {
    pub handle: SourceFrameHandle,
    encoded_bytes: Arc<[u8]>,
}

pub enum SourceFrameSelection {
    ResolvedOrder,
    Ids(Vec<FrameId>),
}

pub enum CallerRegionShape {
    Rect(temporal_vision::SignedPixelRect),
    Mask(temporal_vision::BinaryMask),
}

pub enum ProgressiveRegion {
    SourcePixels { rect: SignedPixelRect, source_frame_id: FrameId },
    ViewportCss { rect: CssRect, source_frame_id: FrameId },
    SelectedFromSourceFrame { source_frame_id: FrameId, shape: CallerRegionShape },
    CurrentReference {
        session_id: SessionId,
        reference: NodeReference,
        source_frame_id: FrameId,
    },
}

pub struct ProgressiveEvidenceContext {
    pub deadline: Option<Instant>,
    pub cancellation: Option<Arc<dyn CancellationSignal>>,
    pub current_reference_geometry: Option<Arc<dyn CurrentReferenceGeometry>>,
}

pub trait ProgressiveEvidence: Send + Sync {
    fn execute(
        &self,
        request: ProgressiveEvidenceRequest,
        context: ProgressiveEvidenceContext,
    ) -> PortFuture<'_, Result<ProgressiveEvidenceResult>>;
}

pub trait ProgressiveEvidenceStore: FrameSource + ArtifactStore + RetentionStore {}
impl<T: FrameSource + ArtifactStore + RetentionStore + ?Sized> ProgressiveEvidenceStore for T {}
```

The registry macro associates all eight request/result pairs. `ArtifactHandle`/`StoredArtifact` gain exact scope and content hash; `ArtifactStore::artifact` gains expected scope and `Missing | Available | Invalidated`. `RegionFilmstripRequest` gains optional `BinaryMask`. The temporal filmstrip applies/labels the fixed mask and records it in manifest/cache parameters. `PinState`/`PinChange` replace only reporting values; SQL remains unchanged.

**Acceptance criteria:**

- [ ] One exhaustive registry generates eight stable operations and request/result association; all request wire types reject unknown fields and invalid limits/scope/selections before I/O.
- [ ] Handles carry typed ID, exact scope, MIME, SHA-256, length, and source/derived provenance without paths, base64, data URLs, MCP identifiers, or serialized payload bytes.
- [ ] Source/caller/current region forms map to one fixed-region vocabulary; source-frame selection is explicitly caller-declared rather than CV/tracking.
- [ ] CSS outward rounding, canonical viewport mapping, source padding, full-frame MSB-first masks, non-empty mask bounds, limits, and manifest mask semantics remain in temporal-vision rather than duplicated in core/root.
- [ ] Pin reports exact activation, actual protected segment bounds/bytes, true coalesced unions, overlap state, evidence availability, global pinned usage/status, and source-segments-only scope without a migration.

### Unit 2: coherent store reads and exact pin reporting

**Story:** `epic-temporal-debugging-workflow-progressive-evidence-and-pinning-coherent-store-reads-and-pin-reporting`

**Files:**

- `crates/krometrail-store/src/recording.rs`
- `crates/krometrail-store/src/index/{frames.rs,artifacts.rs,retention.rs}`
- `crates/krometrail-store/src/artifacts/mod.rs`
- `crates/krometrail-store/tests/{artifact_store.rs,retention_small_budget.rs}`
- `crates/krometrail-store/tests/progressive_evidence_store.rs` (new)

Implement `FrameSource` for `RecordingStore` using the optimistic two-gate protocol and root no longer treats `SqliteIndex` as the production progressive reader. Metadata-only methods run under the gate; encoded methods snapshot/read/revalidate. Artifact-by-ID returns scoped availability and distinguishes invalidation. Pin index helpers return insertion/removal change, segment bounds/bytes ordered by range/retention sequence/ID, any-pin overlap state, coalesced ranges, expected-frame availability, and current usage from existing rows.

`RetentionStore` pin/unpin/query implementations consume `RetentionPinRequest`, flush before pin, validate exact ordered frames under the mutation gate, run budget enforcement, and report final state. Unpin remains exact and idempotent; session deletion remains authoritative.

**Acceptance criteria:**

- [ ] Source list/fetch and artifact reads that race segment/artifact eviction or session deletion either return one fully revalidated bounded payload set or explicit `NotFound`/`EvidenceInvalidated`; they never return stale partial bytes.
- [ ] No mutation gate spans segment/artifact file read, hashing, or an injected delayed reader; concurrent bounded append/event persistence remains able to acquire the gate between read phases.
- [ ] Scope/order/metadata/hash/length/source-link disagreements fail source-safely; corrupt source is never downgraded to a derived-cache miss.
- [ ] Pin flushes open data, refuses stale/partial expected frames, reports actual segment overreach, handles overlap/idempotence/exact unpin, and serializes with concurrent eviction/deletion.
- [ ] Post-unpin budget enforcement can evict released source while overlap protection survives; final availability/status and paused/available wake behavior are truthful.
- [ ] Pinned source survives while linked artifacts and v5 browser events remain independently evictable; no artifact/event row enters `pin_segments`.

### Unit 3: narrow current-reference geometry port

**Story:** `epic-temporal-debugging-workflow-progressive-evidence-and-pinning-current-reference-geometry`

**Files:**

- `crates/krometrail-core/src/ports/browser.rs`
- `crates/krometrail-core/src/progressive.rs`
- `crates/krometrail-cdp/src/control/{snapshot.rs,mod.rs}`
- `crates/krometrail-cdp/src/session/{mod.rs,operations.rs}`
- `crates/krometrail-cdp/tests/temporal_evidence.rs`
- focused current-reference tests in existing control/session test modules

```rust
pub struct CurrentReferenceGeometryRequest {
    pub session_id: SessionId,
    pub reference: NodeReference,
}

pub struct ResolvedReferenceGeometry {
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub reference: NodeReference,
    pub attachment_generation: u64,
    pub resolved_at: SessionTime,
    pub viewport_css_rect: CssRect,
}

pub trait CurrentReferenceGeometry: Send + Sync {
    fn current_reference_geometry(
        &self,
        request: CurrentReferenceGeometryRequest,
    ) -> PortFuture<'_, Result<ResolvedReferenceGeometry>>;
}

pub trait BrowserSessionPort: CurrentReferenceGeometry + Send + Sync { /* existing methods */ }
```

`ProductionSession` routes this focused read through the existing serialized session actor. `PageControl` calls the existing `SnapshotRegistry::resolve(..., VisibleGeometry)`, reads fresh layout viewport origin, converts the resolved document quad bounds to viewport CSS, and returns only core values. It does not register a browser operation, mint a snapshot/reference, expose backend/transport IDs, take a screenshot, or persist geometry. The MCP session owner can later implement/delegate the same narrow port without core depending on MCP.

**Acceptance criteria:**

- [ ] Exact current snapshot/reference/session/target/attachment/document/backing-node checks are reused; no name/role/selector fallback or natural-anchor re-resolution exists.
- [ ] Fresh finite non-zero viewport-relative CSS geometry is returned without clipping; visible disabled/inert nodes remain geometry-readable while hidden/detached/stale nodes fail as existing contracts require.
- [ ] Wrong session/target, newer generation, navigation, reconnect, target close, absent active snapshot, and malformed layout/quad fail with stable source-safe errors and refresh guidance.
- [ ] The port performs protocol reads before/without any recording mutation gate and contains no CDP/MCP type in core.
- [ ] The result is explicitly current-only; tests prove the same `NodeReference` cannot be treated as a historical source-frame identity.

### Unit 4: progressive service and root composition

**Story:** `epic-temporal-debugging-workflow-progressive-evidence-and-pinning-progressive-service-and-composition`

**Files:**

- `src/progressive/{mod.rs,service.rs,region.rs}` (new)
- `src/artifacts/generators.rs` (mask delegation only)
- `src/{main.rs,app.rs}`
- focused root/service tests

Implement one service dispatch:

1. validate context/deadline/cancellation and operation bounds;
2. validate selected IDs against one `ResolvedRange`;
3. use the one composite store for bounded reads or checked retention mutations;
4. for current references, resolve live geometry before any store gate/file read;
5. for region requests, load exact frame metadata, require one compatible epoch, map through temporal-vision helpers, and build one `RegionFilmstripRequest` with the selected locator/mask;
6. delegate all generation to existing `ArtifactGeneration` with derived context;
7. return registry-associated results in deterministic order with no payload persistence.

Root keeps the concrete `Arc<RecordingStore>`, passes it to artifact generation as both existing frame/artifact ports, constructs one progressive service over the same store/generator, and stores `Arc<dyn ProgressiveEvidence>` in `RuntimeDependencies`. `build_service` remains unchanged in this feature, so no MCP tool/resource/schema/URI lands early.

**Acceptance criteria:**

- [ ] Artifact retrieval, frame list/fetch, generic variant generation, fixed region generation, and pin/unpin/query all dispatch through one typed service registry.
- [ ] Lists use capture order, explicit selected fetches use request order, and count/per-item/total byte limits are exact with no partial result or base64/path conversion.
- [ ] All four existing generator families and artifact cache/single-flight remain reachable through the unchanged `ArtifactGeneration` registry; region filmstrips hit the same cache and never decode/render independently.
- [ ] Every region form returns its resolved fixed mapping/locator semantics; multi-epoch, wrong-scope/frame, stale reference, contradictory viewport, all-zero/wrong-size mask, and limit failures are explicit.
- [ ] Root shares one store as source/artifact/retention/progressive authority and one artifact service; no index frame reader, cache, payload map, or pin coordinator is duplicated.
- [ ] MCP registration, resources, raw filesystem access, debug-bundle composition, browser-event context, tracked regions, and URI handling remain absent.

### Unit 5: integrated progressive-evidence qualification

**Story:** `epic-temporal-debugging-workflow-progressive-evidence-and-pinning-qualification`

**Files:**

- `src/progressive/tests.rs` (new)
- `crates/krometrail-store/tests/progressive_evidence_store.rs`
- `crates/krometrail-cdp/tests/temporal_evidence.rs`
- `crates/temporal-vision/tests/filmstrip.rs`
- `src/app.rs` tests

Build one real schema-v5 recording fixture with two targets, tied frame times, multiple sealed/open segments, exact and overlapping pins, JPEG/PNG source frames, two visual epochs, a retained/corrupt artifact, and browser events. Use controlled barriers around segment/artifact reads, artifact generation, current geometry, eviction, and session deletion; avoid timing sleeps.

**Acceptance criteria:**

- [ ] Source handles prove MIME/format/dimensions/viewport/scale/source-observed-session times/ordinal/hash/length and deterministic all/requested ordering; duplicate/out-of-range IDs and every count/byte boundary fail.
- [ ] Source and artifact eviction during delayed reads, corrupt artifact invalidation, source corruption, and session deletion produce the designed lifetime errors without stale or partial bytes.
- [ ] Source-pixel in/out-of-bounds, fractional/negative viewport CSS outward rounding, selected-frame rect, current reference, caller mask, padding, locator, wrong target/session/generation, stale reference, epoch mismatch, all-zero/tail-bit/wrong-size/oversized mask, and manifest mask/cache identity are covered.
- [ ] Equivalent region requests reuse the existing artifact cache/single flight; concurrent requests do not duplicate decode/generation, and a cancelled/expired request cannot publish late.
- [ ] Open-segment pin, actual segment overreach, overlap, repeated pin/unpin, exact unpin, stale/evicted state, all-pinned pause, budget recovery, and concurrent eviction/deletion match final `PinState`.
- [ ] Pinned source survives independent artifact and browser-event eviction; source eviction removes linked artifacts before frames; session deletion removes all source/artifact/event/pin state.
- [ ] Controlled barriers prove no recording mutation gate is held across source/artifact file reads, current browser geometry, hashing, decode, render, or artifact generation; append/event persistence can progress between validation phases.
- [ ] Root integration proves one concrete store/generator/progressive service and leaves MCP unchanged. Rust 1.85 locked format/check/test/Clippy gates pass. No wrappers/getters/SQL-line tests or unbounded image goldens are added.

## Implementation order

1. `epic-temporal-debugging-workflow-progressive-evidence-and-pinning-contracts-and-region-semantics`
2. `epic-temporal-debugging-workflow-progressive-evidence-and-pinning-coherent-store-reads-and-pin-reporting` — depends on contracts/region semantics
3. `epic-temporal-debugging-workflow-progressive-evidence-and-pinning-current-reference-geometry` — depends on the public current-geometry/region contracts
4. `epic-temporal-debugging-workflow-progressive-evidence-and-pinning-progressive-service-and-composition` — depends on coherent store reads/pins and current reference geometry
5. `epic-temporal-debugging-workflow-progressive-evidence-and-pinning-qualification` — depends on the composed service

One future feature owner should carry these checkpoints as one cohesive implementation and feature-review bundle. The stories preserve contract/durability/current-browser/composition/qualification order; they are not worker assignments or parallelism signals.

## Simplification and elimination

- Keep `ResolvedRange`, `FrameSource`, `ArtifactStore`, `ArtifactGeneration`, `RetentionRange`, `RetentionStore`, `SnapshotRegistry`, `RegionDefinition`, `ViewportMapping`, and `BinaryMask` as authorities. Add no parser, range copy, direct index reader, artifact cache, decoded cache, pin ledger, payload table, geometry history, or resource map.
- Root stops exposing `SqliteIndex` as the production progressive/artifact frame reader and uses `RecordingStore`'s coherent implementation instead; metadata SQL remains in the existing index helper.
- The composite `ProgressiveEvidenceStore` is a zero-method port intersection, not a wrapper facade with forwarding methods.
- Region form 3 (“selected from source frame”) and caller rect/mask are one declaration because no selector/tracker exists. This removes a misleading duplicate API.
- Actual image/viewport rational mapping and mask bounds/application stay in temporal-vision. Browser-specific current reference lifetime stays in the CDP adapter. Core carries only neutral geometry and typed identity.
- Generic artifact variants and region filmstrips both use the implemented service/cache/single-flight. No second decode/generation function is permitted in `src/progressive/`.
- Pin reporting extends queries over existing rows without migration; artifacts/events remain explicitly outside the protection scope.
- Keep MCP, resources/URIs, debug-bundle composition, verbose browser events, natural anchors, tracked regions, per-frame geometry, inference, and raw filesystem access for their owning features.

## Testing strategy

- **Registry/boundary:** exhaustive operation association and validated Serde protect the future generated schema surface.
- **Complex geometry:** exact outward rounding, rational mapping, mask bounds/application, padding, locator, and epoch failures protect the highest-risk visual semantics; reuse existing temporal tests rather than duplicating render algorithms.
- **Store lifetime:** deterministic barriers around read/evict/delete/invalidate protect weak-handle honesty and no-gate-across-I/O.
- **Source interface:** one real store fixture protects exact order/scope/hash/length/byte limits; no test per metadata getter.
- **Pin interface:** tiny budgets and real segments protect open flush, exact expected frames, overlap/idempotence, coalesced actual bounds, pause/recovery, and source-only scope.
- **Browser current reference:** scripted actor tests protect exact generation/document/attachment and viewport conversion; an existing real-browser fixture may add one opt-in geometry check without creating a new browser app.
- **Generation seam:** cache hit, single-flight, cancellation, and locator/mask manifest checks prove delegation rather than retesting storyboard/difference/motion algorithms.
- **Root seam:** one composition test proves all casts share the same concrete store and MCP remains unmodified.
- Retire/update old pin tests that assert only ID vectors once richer reporting replaces that contract. Do not snapshot SQL, whole schemas, full manifests, or large image files.

## Risks and rollback

- **Optimistic read validation can waste I/O under aggressive eviction.** It preserves correctness and avoids long-held gates. If measured retries become material, add short-lived internal read leases behind `RecordingStore` without changing weak public handles; do not turn reads into user pins or expose paths.
- **Current geometry is not historical geometry.** Mapping it through a retained frame can be semantically surprising if the page moved after capture. The contract reports current-only timing and chosen frame explicitly and makes a fixed region, never historical identity. If evaluation finds this too confusing, disable only `CurrentReference` while source/viewport/frame/mask forms remain operational.
- **Capture viewport metadata may be incompatible with CSS geometry on some renderer/version.** Exact persisted mapping and explicit rejection are safer than guessing device scale. Compatibility evidence can later refine the adapter contract without changing temporal-vision's rational mapping.
- **Mask rendering expands the temporal filmstrip contract.** If visible masking or manifest alignment cannot be implemented without destabilizing existing deterministic output, rollback is to reject mask filmstrip generation explicitly while retaining mask request validation for a separately versioned algorithm; never silently reduce a mask to an unmasked bounding box.
- **Pin reporting queries become more expensive.** Segment counts per exact range are bounded by rotation and only queried on explicit pin operations. If measured cost grows, add an index—not a ledger—without changing `PinState`.
- **Artifact feature is at review rather than done.** Its implementation and review-eligible contracts are present. Any standard-review correction to handle/store/generation semantics must be reconciled before Unit 1 rather than forked.
- **Root/app overlap with bundle work:** defer `src/app.rs` edits to Unit 4 and confine logic to new `src/progressive/`; if bundle composition lands first, rebase the one dependency field/wiring change rather than sharing service files.

## Pre-mortem

The most dangerous failure is a resource-like handle that says evidence exists after its bytes or source links were deleted, followed by a region artifact that appears to track a current element through historical frames. Either would make the agent trust false evidence. The design attacks the first with one store authority, two-phase optimistic validation, scoped hashes/lengths, explicit weak lifetime, and checked pin insertion. It attacks the second by resolving current geometry exactly once, reporting its current-only timing, requiring one declared source-frame mapping and one visual epoch, and then passing only a fixed temporal-vision region/mask.

The next likely failure is a pin that appears successful but missed an open segment or returned a requested range rather than actual segment-granular protection. Pinning therefore flushes first under the store gate, revalidates ordered frame identities, and reports actual segment bounds/bytes plus coalesced unions and final budget state.

The least certain area is CSS-to-captured-viewport compatibility across Chrome scale/downsampling behavior. The fallback is explicit incompatibility with source-pixel/mask forms still available, not inferred scale. Rollback can disable current-reference or viewport-CSS forms independently while preserving source reads, artifact retrieval/variants, source-pixel filmstrips, and exact pin controls.

## Blockers

None. Both declared dependencies have completed implementation; resolved queries are done and artifact generation is dependency-ready at feature review. The modified `.work/bin/work-view` is unrelated and remains untouched.
