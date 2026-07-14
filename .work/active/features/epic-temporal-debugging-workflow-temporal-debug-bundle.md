---
id: epic-temporal-debugging-workflow-temporal-debug-bundle
kind: feature
stage: review
tags: [visual, browser, storage, agent-ux]
parent: epic-temporal-debugging-workflow
depends_on:
  - epic-temporal-debugging-workflow-resolved-temporal-queries
  - epic-temporal-debugging-workflow-artifact-generation-and-cache
  - epic-temporal-debugging-workflow-capture-and-browser-event-context
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Temporal Debug Bundle

## Brief

Deliver the primary single-range investigation capability. Given one already validated temporal query, compose a compact bundle containing the exact `ResolvedRange`, a concise non-diagnostic header, before/during/after orientation, change-aware storyboard, temporal difference map, source-frame and artifact references, complete provenance, and explicit capture-quality, gap, and retention warnings.

Combine visual measurements with timeline context deterministically: preserve interaction and navigation markers and select a bounded set of errors, failed requests, navigation, and browser events nearest the bundle's major visual-change moments. The bundle reports measurements and correlation distance as evidence, never causality or automatic diagnosis, and keeps full event sets and source images behind progressive references.

This feature owns bundle composition and default evidence policy. It does not duplicate artifact algorithms, include motion history in the default bundle before evaluation earns it, compare interactions or sessions, replay actions, track logical elements, or decide MCP wire/resource presentation.

## Epic context

- Parent epic: `epic-temporal-debugging-workflow`
- Position in epic: primary investigation capability — joins resolved queries, generated artifacts, and recorded context; consumed by the MCP surface

## Simplification opportunity

- Compose the existing artifact results and one context query into a single bundle service. Do not add a second storyboard selector, difference metric, gap model, event store, provenance schema, or per-artifact bundle response family.

## Foundation references

- `docs/VISION.md` — Product Thesis, Core Experience, and Visual Evidence
- `docs/SPEC.md` — Temporal Queries and Artifact Provenance
- `docs/ARCHITECTURE.md` — Artifact Generation and MCP Boundary
- `docs/VISUAL-EVIDENCE.md` — Before/During/After Composite, Temporal Debug Bundle, and Progressive Detail

## Grounding and dispatch

- **Driver:** active autopilot `--all`; the caller required no questions, highest capability, standard implementation review, and no nested agents, peeragent, or UI work.
- **Dispatch:** direct-read only. Grounding covered `AGENTS.md`, `.agents/rules/*.md`, `.work/CONVENTIONS.md`, the principles skill and code-design mechanics, all five foundation documents, the parent epic, the complete designs and implementation records for resolved queries, artifact generation/cache, capture/browser-event context, and progressive evidence, commits `ea82451` through `52f225d` and `9ed7958` through `5479a2f`, and the current core/store/temporal-vision/root source and focused tests.
- **Stable authorities found:** `TemporalQueryRequest`/`TemporalQuery` and `ResolvedRange` own natural-anchor resolution; `ArtifactGenerationRequest`/`ArtifactGenerationResult` and exact `temporal_vision::ArtifactManifest` own visual work/cache results; `TemporalContextRequest`/`TemporalContext` own capture quality and compact event correlation; `RecordingStore` is the shared mutation, retention, deletion, event, frame, and artifact authority; progressive handles already prove that no path, base64, payload bytes, or MCP URI belongs in an application result.
- **Manifest sufficiency check:** the storyboard manifest currently carries selected IDs and an untyped parameter map containing timestamps/reasons, but it does not carry a typed, validated visual-measurement summary. The in-memory `StoryboardSelection` is discarded on cache publication. Parsing free-form parameters would create a second fragile selector and cached hits could not expose exact measurement decisions. The design therefore adds one required typed storyboard trace to the existing temporal-vision manifest and no second measurement pass; this prepublic project carries no trace-less compatibility path.
- **UI:** this is an application/API evidence composition with no human screen or journey. Mockups are intentionally skipped.
- **Review weight:** standard at feature implementation review. Design-time advisory review was skipped because the caller explicitly prohibited nested agents and peeragent.
- **Parallel-work safety:** only this feature and its new prefixed stories are changed by this design. The unrelated modified `.work/bin/work-view` is preserved and excluded from the commit.

## Design decisions

### One request, one resolution, one application service

- **Primary boundary:** Add only `TemporalDebugBundles::bundle(TemporalDebugBundleRequest, TemporalDebugBundleContext)`. The request owns the existing validated `TemporalQueryRequest`; the service calls `TemporalQuery::resolve_range` exactly once. There is no sibling API that accepts `ResolvedRange`, no bundle builder exposed to MCP, and no natural-anchor handling in artifact or context code.
- **Requested, resolved, effective truth:** The result returns the exact requested `TemporalQueryRequest`, the exact existing `ResolvedRange`, and `EffectiveBundlePolicy`, which contains the policy version, effective artifact anchor, exact existing generator requests/failure policy, exact existing context filter/selection, and final focus times. It does not paraphrase range options or copy artifact/context result types.
- **Resolved anchor completion:** Extend `ResolvedRange` with one `ResolvedAnchor` produced inside `TemporalRangeResolver`. It records a typed `ResolvedAnchorReference`, `requested_time`, and `effective_time`; `effective_time` is the requested semantic anchor clamped to the retained `resolved_range`. This is the smallest way for every downstream consumer—especially `latest_interaction`—to reuse the resolver's exact decision without another latest lookup or anchor interpretation.
- **Anchor rules:** explicit session-time and wall-clock intervals use the overflow-safe midpoint of the requested interval; source-frame ranges use the midpoint between the two endpoint frame times; interaction and latest-interaction anchors use authoritative dispatch time and retain the exact resolved `InteractionId`; navigation and marker anchors use their exact generic timeline observation time and typed ID. If partial edge retention excludes that time, the effective artifact anchor clamps to the nearest retained endpoint and the bundle emits `AnchorAdjustedForRetention` with both values.
- **Caller policy surface:** The bundle request exposes only `OrientationPolicy::{Include,Omit}` in addition to the query and caller markers. Orientation defaults to `Include`. Tile/noise/normalization/map/output tuning remains the focused artifact operation's responsibility; the primary workflow deliberately has one versioned evidence policy rather than a second general artifact-options API.

```rust
// crates/krometrail-core/src/timeline/range.rs
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum ResolvedAnchorReference {
    Interval,
    Interaction { interaction_id: InteractionId },
    Navigation { navigation_id: NavigationId },
    Marker { marker_id: MarkerId },
    SourceFrames { start_frame_id: FrameId, end_frame_id: FrameId },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResolvedAnchor {
    pub reference: ResolvedAnchorReference,
    pub requested_time: SessionTime,
    pub effective_time: SessionTime,
}

// Existing fields remain; this is added and validated against anchor_kind/ranges.
pub struct ResolvedRange {
    // ...existing exact fields...
    pub resolved_anchor: ResolvedAnchor,
}
```

### Default artifact policy

- **Version:** `temporal-debug-bundle-v1`. The exact effective requests are returned, so future policy changes are observable and versioned rather than silently changing an agent's evidence.
- **Outputs:** For every visual epoch, request exactly one `Storyboard` generator with optional existing `BeforeDuringAfter` output and one `DifferenceMap` generator. Do not request region filmstrip or motion history. The artifact service already emits per-epoch slots and never stretches incompatible epochs together.
- **Storyboard:** tile limit `8`; noise floor `MeasurementParameters::DEFAULT_NOISE_FLOOR` (`512`); no crop; declared black RGB background `(0,0,0)`; `AnalysisScale::FitLimits`; title `TEMPORAL STORYBOARD`; source label `KROMETRAIL RETAINED SOURCE FRAMES`; output ceiling `1920 × 2048`, `16 MiB`; orientation follows `OrientationPolicy` and defaults on.
- **Difference map:** epoch-local `FrameSelector::First`; `FrequencyMode::NormalizedFrequency`; default spectral palette through the existing generator; no explicit repeated-change separation (`None`, so the generator's range-quarter rule is effective and manifested); noise floor `512`; the same no-crop/black/`FitLimits` normalization; black canvas background; output ceiling `8192 × 8192`, `64 MiB`. This accommodates a full 1080p three-panel map as a retained artifact while later MCP presentation remains free to return only a context-sized image/reference.
- **Normalization:** `FitLimits` may choose only the artifact adapter's existing exact integer scales `1,2,4,8`; it records the effective scale in the request, cache transcript, normalization steps, and manifest. A dimension/output combination that cannot fit fails that epoch/output explicitly. There is no stretch, content-aware resize, registration, or generated-pixel fallback.
- **Failure mode:** Artifact generation uses `ArtifactFailurePolicy::AllowPartial`. The exact ordered `ArtifactOutcome` values, cache dispositions, handles, and manifests are retained. Cache hits are first-class successful outcomes; the bundle neither copies bytes nor regenerates a hit for summary purposes.
- **Limits stay layered:** Bundle limits may be stricter but never raise the artifact service's existing 120-frame, 256-marker, 16-output, memory, CPU, and 15-second ceilings.

### Typed manifest trace and major-change focus

- **No second visual computation:** Evolve the existing `StoryboardSelection<FrameId>` to retain a typed `StoryboardVisualSummary<FrameId>` built from measurements the selector already computes: first thresholded change, peak baseline change, and greatest adjacent changed-pixel proportion. Each `VisualChangeMoment` carries exact source IDs/indexes, later timestamp, elapsed time, and the existing `ComparisonOutcome`/`MeasurementVector`. Gap-boundary comparisons never become visual-change moments.
- **Existing manifest, not a new provenance schema:** Add `storyboard_selection: Option<StoryboardSelection<FrameId>>` to `ArtifactManifest`. Storyboard and orientation manifests require it; other artifact kinds require `None`. The field is always serialized, and missing trace fields are rejected rather than preserved for unpublished compatibility. Validation proves trace frame indexes/IDs, selected subsequence, orientation roles, timestamps/range, and measurement indexes agree with manifest source identities.
- **Version/cache consequence:** Bump the shared storyboard/orientation descriptor from `temporal-storyboard/1.0.0` to `1.1.0`. PNG pixels need not change, but manifests and evidence decisions do, so the existing cache transcript produces misses and new artifacts. Difference-map and other generator versions remain unchanged. No SQLite migration or compatibility branch is required; trace-less unpublished cache rows invalidate and regenerate from retained sources.
- **Focus candidates:** Read only available `Storyboard` manifests, never orientation duplicates and never difference-map pixels. Candidate order is: first-change summary, peak-baseline summary, peak-adjacent-area summary, then selected frames carrying `FirstChange`, `PeakBaselineChange`, `LocalChangePeak`, `ChangedRegionTransition`, `ChangeTrend`, or `InformationGain`. Deduplicate by exact `SessionTime`, keeping the earliest policy rank; ties use `(epoch_index, frame_index, FrameId)`; keep at most `MAX_FOCUS_TIMES` (`16`); finally sort by session time before constructing the one compact context request.
- **Unchanged/gapped evidence:** A storyboard with no measured changed pixels has an empty visual summary and no focus times; the header says no thresholded change was measured in retained comparable frames, not that the page was stable. Gaps remain in the manifest/context and prevent comparisons across missing evidence. If all storyboard outcomes are unavailable, context still runs once with an empty focus list and its normal compact priorities.

```rust
// crates/temporal-vision/src/select.rs
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VisualChangeMoment<FrameId> {
    frame_id: FrameId,
    frame_index: usize,
    timestamp: Timestamp,
    comparison: FrameComparison,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StoryboardVisualSummary<FrameId> {
    first_change: Option<VisualChangeMoment<FrameId>>,
    peak_baseline_change: Option<VisualChangeMoment<FrameId>>,
    peak_adjacent_changed_area: Option<VisualChangeMoment<FrameId>>,
}

// Existing selected frames, omitted anchors, orientation indexes, and segment count remain.
pub struct StoryboardSelection<FrameId> {
    // ...existing fields...
    visual_summary: StoryboardVisualSummary<FrameId>,
}

// crates/temporal-vision/src/provenance.rs
pub struct ArtifactManifest<ArtifactId, FrameId, MarkerId, GapId> {
    // ...existing authoritative fields...
    storyboard_selection: Option<StoryboardSelection<FrameId>>,
}
```

### Marker policy and privacy

- **Sources:** Load only `InteractionBoundary`, `Navigation`, and `Marker` observations from the generic timeline through a new bounded, kind-filtered `TimelineRangeQuery`; browser events remain owned by `TemporalContextQuery`. Reuse `InteractionRecordSource` for action records and `InteractionAnchorSource` for anchor-only page operations. Reuse `ArtifactMarker`/`ArtifactMarkerId` for the exact markers passed to artifacts and returned by the bundle. Add no marker table, copied marker DTO, or bundle event store.
- **Generic bounded timeline read:** Extend `TimelineStore` with `selected_range(TimelineRangeQuery) -> TimelineRangeSlice`. The query validates a unique non-empty kind set and `1..=4096` rows; the store obtains an exact filtered count and returns at most the limit in existing deterministic timeline order, setting `truncated` from `matched_count > observations.len()`. The bundle requests the three marker kinds with limit `1024`, excluding high-volume browser-event rows at SQL selection time.
- **Caller markers:** Accept at most `64`; require exact unique `ArtifactMarkerId`, time inside the resolved range, kind at most 64 UTF-8 bytes, and label at most 160 UTF-8 bytes. Caller values are never truncated or rewritten: invalid/oversized values fail the boundary so “exact” remains true.
- **Interaction markers:** One marker per interaction at authoritative dispatch time, `ArtifactMarkerId::Interaction(id)`, kind `interaction`, label `Interaction <uuid>: <operation stable_name>`. Optional records are used only to preserve/validate the existing exact action identity; labels never include sanitized parameters, locator values, page text, keys, filenames, URLs, or parent-batch contents. Anchor-only page operations fall back to their exact `InteractionAnchor`.
- **Navigation markers:** Preserve exact `NavigationId` and observation time with kind `navigation` and label `Navigation <uuid>`. Browser navigation events remain separate correlation evidence and never mint an explicit navigation ID.
- **Persisted and caller markers:** A supplied `ArtifactMarkerId::Marker(id)` matching a generic timeline marker provides its exact caller kind/label. A retained generic marker without supplied presentation metadata remains present with kind `marker`, label `Marker <uuid>`, and `MarkerLabelUnavailable`; the bundle never invents user text. `ArtifactMarkerId::Caller` remains a request-local exact marker.
- **Selection/caps:** Caller markers and the exact resolved natural-anchor marker are mandatory. Timeline candidates rank by absolute session-time distance to the effective anchor, then class (`interaction`, `navigation`, `marker`), session time, and stable ID bytes. Fill to the existing artifact cap of `256`, then present/pass markers in `(session_time, class, stable ID)` order. A 1024-row source truncation or 256-marker result truncation emits exact matched/returned/limit warnings; no omitted marker is silently implied absent.

```rust
// crates/krometrail-core/src/ports/timeline.rs
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimelineRangeQuery {
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub range: SessionRange,
    pub kinds: Vec<ObservationKind>,
    pub limit: NonZeroU16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimelineRangeSlice {
    pub matched_count: u64,
    pub observations: Vec<TimelineObservation>,
    pub truncated: bool,
}

pub trait TimelineStore: Send + Sync {
    // existing append/range stay available
    fn selected_range(
        &self,
        query: TimelineRangeQuery,
    ) -> PortFuture<'_, Result<TimelineRangeSlice>>;
}
```

### Bundle result, header, references, and evidence posture

- **Reuse exact contracts:** `BundleArtifactEvidence::Available` contains the existing `ArtifactGenerationResult`; `BundleContextEvidence::Available` contains the existing `TemporalContext`; markers are existing `ArtifactMarker` values. Per-artifact manifests, selected IDs, cache disposition, output dimensions/hash, gaps, normalization, and parameters stay authoritative in those nested contracts.
- **Source references without a copy:** `ResolvedRange.frame_ids` is the complete ordered source-frame reference set. Each artifact manifest retains exact ordered source and selected frame IDs. The bundle adds no `SourceFrameHandle` copy, byte array, hash scan, URI, base64, segment address, or filesystem path; agents use the progressive service later when they need metadata or bytes.
- **Header:** `TemporalDebugHeader` contains a bounded summary, `EvidencePosture::ObservedChangeAndTemporalProximityOnly`, and exact per-epoch `StoryboardVisualSummary` values extracted from available manifests. Summary language is limited to “observed,” “measured,” “selected,” “co-occurred,” and “nearest by session-time distance.” It never says caused, triggered, diagnosed, fixed, smooth, flicker, reversal, or stable.
- **Warnings remain singular:** Capture gaps, retention, frame warnings, event collection gaps/unavailability, and context truncation stay in `ResolvedRange`, manifests, and `TemporalContext`. `BundleWarning` covers only composition facts: anchor adjustment, timeline/marker truncation, synthesized marker label, missing major-change focus, and component unavailability. It does not restate nested gap models.
- **Degradations:** Exact component errors are retained in `BundleDegradation`. Per-epoch artifact failures remain exact `ArtifactOutcome::Unavailable` values; a request-wide artifact failure or context failure is represented once without manufacturing empty success data.

```rust
// crates/krometrail-core/src/debug_bundle.rs
pub const TEMPORAL_DEBUG_BUNDLE_POLICY_VERSION: &str = "temporal-debug-bundle-v1";
pub const MAX_BUNDLE_CALLER_MARKERS: usize = 64;
pub const MAX_BUNDLE_ARTIFACT_MARKERS: usize = 256;
pub const MAX_BUNDLE_TIMELINE_ROWS: u16 = 1_024;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrientationPolicy { Include, Omit }

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TemporalDebugBundleRequest {
    query: TemporalQueryRequest,
    caller_markers: Vec<ArtifactMarker>,
    orientation: OrientationPolicy,
}

impl TemporalDebugBundleRequest {
    pub fn new(
        query: TemporalQueryRequest,
        caller_markers: Vec<ArtifactMarker>,
        orientation: OrientationPolicy,
    ) -> Result<Self>;
    pub fn default_policy(query: TemporalQueryRequest) -> Result<Self>;
}

#[derive(Clone, Default)]
pub struct TemporalDebugBundleContext {
    pub deadline: Option<Instant>,
    pub cancellation: Option<Arc<dyn CancellationSignal>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EffectiveBundlePolicy {
    pub version: NonEmptyText,
    pub artifact_anchor: SessionTime,
    pub artifact_generators: Vec<ArtifactGeneratorRequest>,
    pub artifact_failure_policy: ArtifactFailurePolicy,
    pub event_filter: BrowserEventFilter,
    pub event_selection: BrowserEventSelection,
    pub focus_times: Vec<SessionTime>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum BundleArtifactEvidence {
    Available(ArtifactGenerationResult),
    Unavailable { error: KrometrailError },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum BundleContextEvidence {
    Available(TemporalContext),
    Unavailable { error: KrometrailError },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidencePosture {
    ObservedChangeAndTemporalProximityOnly,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BundleEpochVisualSummary {
    pub epoch_index: u32,
    pub summary: temporal_vision::StoryboardVisualSummary<FrameId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TemporalDebugHeader {
    pub summary: NonEmptyText, // constructor limits this to 512 UTF-8 bytes
    pub posture: EvidencePosture,
    pub visual_summaries: Vec<BundleEpochVisualSummary>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "warning", rename_all = "snake_case")]
pub enum BundleWarning {
    AnchorAdjustedForRetention {
        requested: SessionTime,
        effective: SessionTime,
    },
    TimelineMarkerEvidenceTruncated {
        matched_count: u64,
        returned_count: u64,
        limit: u16,
    },
    MarkersTruncated {
        matched_count: u64,
        returned_count: u64,
        limit: u16,
    },
    MarkerLabelUnavailable { marker_id: MarkerId },
    NoMajorVisualChangeFocus,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "component", rename_all = "snake_case")]
pub enum BundleDegradation {
    MarkerContextUnavailable { error: KrometrailError },
    ArtifactRequestUnavailable,
    ArtifactOutcomesUnavailable { unavailable: u16, total: u16 },
    ContextUnavailable,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TemporalDebugBundle {
    pub requested_query: TemporalQueryRequest,
    pub range: ResolvedRange,
    pub effective: EffectiveBundlePolicy,
    pub header: TemporalDebugHeader,
    pub markers: Vec<ArtifactMarker>,
    pub artifacts: BundleArtifactEvidence,
    pub context: BundleContextEvidence,
    pub warnings: Vec<BundleWarning>,
    pub degradations: Vec<BundleDegradation>,
}

pub trait TemporalDebugBundles: Send + Sync {
    fn bundle(
        &self,
        request: TemporalDebugBundleRequest,
        context: TemporalDebugBundleContext,
    ) -> PortFuture<'_, Result<TemporalDebugBundle>>;
}
```

### Exact orchestration, failure semantics, and bounded concurrency

The service order is fixed:

1. Validate request/context, compute the earlier of caller deadline and the 20-second bundle wall limit, and acquire one of two global active-bundle permits.
2. Call `TemporalQuery::resolve_range` exactly once with the owned request. Range failure is whole-request failure.
3. Load the bounded marker timeline and selected interaction records/anchors needed for exact artifact labels; release every store future/guard before visual work.
4. Materialize the exact two-generator request and call `ArtifactGeneration::generate` at most once with the same absolute deadline/cancellation.
5. Extract visual summaries and at most 16 focus times only from available typed storyboard manifests.
6. Construct `TemporalContextRequest::new` with the exact same `ResolvedRange`, no clip, default all-class/debug filter, compact limit `24`, and those focus times; call `TemporalContextQuery::context` exactly once.
7. Compose deterministic epoch/outcome/marker/event order and the non-diagnostic header. No additional store, frame, event, measurement, selection, generation, or context call occurs.

- **Global cancellation/deadline:** Every await is wrapped by the same absolute deadline and cancellation future. The artifact service receives that same context rather than a fresh timeout. Cancellation/deadline always fail the whole request and suppress the bundle result. Already-published cache artifacts remain valid reusable derived evidence, matching existing artifact semantics.
- **Fatal evidence lifetime:** `NotFound` from generation or context after successful resolution means source/session evidence was evicted or deleted; discard partial composition and fail the whole request with re-resolve guidance. Session deletion races therefore never return a bundle whose range is known stale.
- **Usable degraded bundle:** Partial edge retention and included gaps are usable only when the caller's existing `TemporalQueryRequest` policy allowed them and remain explicit everywhere. Per-epoch/output artifact failures, request-wide non-lifetime artifact failures, marker-context failure/truncation, event-context failure/truncation, and browser-event unavailability may return a degraded bundle when another component remains useful.
- **Whole-request failure after resolution:** Fail if both context is unavailable and no artifact outcome is available. `Cancelled`, deadline, and `NotFound` always fail regardless of partial work. A `PersistenceFailed` context may coexist with exact validated artifacts and is reported as unavailable context; a source/artifact `NotFound` may not.
- **Cache and concurrency:** The bundle adds no cache or single flight. Existing artifact cache/single flight coalesces identical generator work; context remains one fresh metadata query. Bundle request order does not change artifact result slots. The two-request semaphore bounds marker/context orchestration independently of the artifact service's own two-request, CPU, memory, and per-generator permits.
- **No store gate across artifact work:** Root composes ports over one `RecordingStore`, but range/marker reads complete before artifact generation and context begins only after generation/focus extraction. Tests use barriers to prove frame/event appends can acquire the store mutation gate during visual work.

## Architectural choice

### Option A — make MCP handlers assemble a bundle

The future tool handler could resolve a range, call artifact/context/progressive operations, choose markers, and format a response. This would create a protocol-owned default policy, duplicate orchestration across tools/resources, tempt URI/path/byte concerns into the application result, and make exactly-once resolution hard to prove. Rejected.

### Option B — accept `ResolvedRange` and compose inside `RecordingStore`

A store method could keep range, timeline, context, and artifacts near persistence. It would create a second public bundle entry beside the product query workflow, pull temporal-vision/root computation into infrastructure, and risk holding the store mutation gate through decode/render. Rejected.

### Option C — one root application service over existing inward ports (chosen)

Core owns the validated request/result/port and the small generic timeline-query extension. `src/debug_bundle/` owns policy, marker/focus extraction, orchestration, deadline, and degradation. One concrete `RecordingStore` is projected as temporal query, generic timeline/interaction evidence, and temporal context; one existing artifact service performs all image/cache work. Root wires one `Arc<dyn TemporalDebugBundles>` for future MCP use without changing MCP now. This has one product API, one resolution, one artifact call, one context call, and no copied evidence authority.

A parallel artifact/context fan-out was considered and rejected: context focus times intentionally depend on authoritative artifact decisions, so parallelism would either omit visual correlation or require a second context call. The fixed sequential dependency is honest and still uses bounded parallelism inside artifact generation.

## Trickiest unit first: cache-stable visual decision trace

The highest-risk unit is not the struct assembly; it is making cached storyboard decisions sufficiently typed for deterministic focus correlation without recomputing measurements or teaching bundle code to parse generator-specific parameter maps.

The implementation sequence inside temporal-vision is:

```text
normalize once
  → measure adjacent comparisons once
  → select storyboard frames and existing baseline comparisons
  → retain first / peak-baseline / peak-adjacent summaries in StoryboardSelection
  → render unchanged pixels/labels
  → attach the exact validated StoryboardSelection to both storyboard and orientation manifests
  → persist through existing ArtifactManifest/cache authority
```

The bundle sees the same trace for generated and cache-hit artifacts. Descriptor `1.1.0` prevents an old trace-less cache row from masquerading as the new policy. If the trace extension proves infeasible, the safe fallback is a degraded bundle with empty focus times and normal compact event priority; the service must never perform a second image measurement/selection to recover convenience.

## Implementation units

### Unit 1: resolved anchor, bundle contracts, and typed storyboard trace

**Story:** `epic-temporal-debugging-workflow-temporal-debug-bundle-contracts-and-manifest-trace`

**Files:**

- `crates/krometrail-core/src/debug_bundle.rs` (new)
- `crates/krometrail-core/src/{lib.rs,error.rs}`
- `crates/krometrail-core/src/timeline/{range.rs,query.rs,mod.rs}`
- `crates/temporal-vision/src/{select.rs,render.rs,provenance.rs,lib.rs}`
- `crates/temporal-vision/tests/{storyboard.rs,contracts.rs}`

Implement the exact types/signatures above. `TemporalRangeResolver` constructs one `ResolvedAnchor` in each of its seven seed branches and clamps only in final retention classification. `ResolvedRange::new` validates reference/kind compatibility, requested/effective containment, and exact typed IDs. `StoryboardSelection` gains validated Deserialize and the three-moment visual summary; `ArtifactManifest` requires a trace for storyboard/orientation kinds and rejects trace-less unpublished shapes; storyboard/orientation use descriptor `1.1.0`.

**Acceptance criteria:**

- [ ] All seven anchor forms return one exact typed requested/effective anchor; latest interaction retains the exact resolver-selected ID without a second lookup, and partial-edge clamping is explicit.
- [ ] Bundle request/result/custom Serde re-run nested query/marker limits and contain no alternate `ResolvedRange` API.
- [ ] Storyboard first/peak/adjacent-area summaries reuse existing comparisons, skip gaps, and preserve deterministic ties/source IDs/timestamps/reasons.
- [ ] Generated and cache-deserialized storyboard/orientation manifests expose the same validated trace; other kinds cannot carry it.
- [ ] Storyboard/orientation cache identity changes through descriptor `1.1.0`, difference-map identity does not, trace-less storyboard/orientation manifests are rejected, and existing PNG golden bytes remain unchanged.

### Unit 2: versioned default policy, bounded marker assembly, and focus extraction

**Story:** `epic-temporal-debugging-workflow-temporal-debug-bundle-default-policy-markers-and-focus`

**Files:**

- `crates/krometrail-core/src/ports/{timeline.rs,range.rs,mod.rs}`
- `crates/krometrail-core/src/debug_bundle.rs`
- `crates/krometrail-store/src/index/{timeline.rs,interactions.rs}`
- `crates/krometrail-store/src/recording.rs`
- `src/debug_bundle/{mod.rs,policy.rs,markers.rs,focus.rs}` (new)
- focused tests beside these modules and `crates/krometrail-store/tests/sqlite_timeline.rs`

Add the bounded kind-filtered generic timeline query and delegate `InteractionAnchorSource`/`InteractionRecordSource` through `RecordingStore` so root can project one store authority. Materialize exactly the v1 generator requests, labels, caps, marker privacy policy, focus ranks, and effective-policy value described above.

**Acceptance criteria:**

- [ ] Effective generators are byte-for-byte stable for default and orientation-omitted requests and contain only storyboard/orientation/difference-map policy.
- [ ] One bounded SQL timeline query excludes browser-event rows, retains generic ordering, reports limit+1 truncation, and does not add a table/schema/migration.
- [ ] Interaction/navigation/caller/generic marker IDs/times/kinds/labels follow exact source and privacy rules; equal-time ordering, mandatory anchor inclusion, 64/256/1024 caps, and truncation warnings are deterministic.
- [ ] Marker labels contain only typed IDs and operation stable names; redaction sentinels from interaction records never appear.
- [ ] Focus extraction reads only typed available storyboard traces, handles multi-epoch ties/dedup/caps/gaps/unchanged input, and never invokes temporal-vision measurement or selection APIs.

### Unit 3: bounded bundle composition and degraded operation

**Story:** `epic-temporal-debugging-workflow-temporal-debug-bundle-bounded-composition-and-degradation`

**Files:**

- `src/debug_bundle/{service.rs,header.rs,error.rs,tests.rs}` (new)
- `src/debug_bundle/{policy.rs,markers.rs,focus.rs}`
- `src/main.rs`
- `crates/krometrail-core/src/debug_bundle.rs` only for integration corrections

```rust
pub(crate) struct BundleWorkLimits {
    pub max_active_requests: NonZeroUsize, // default 2
    pub max_wall_time: Duration,           // default 20 seconds
}

pub(crate) struct TemporalDebugBundleService {
    queries: Arc<dyn TemporalQuery>,
    timeline: Arc<dyn TemporalDebugEvidenceStore>,
    artifacts: Arc<dyn ArtifactGeneration>,
    context: Arc<dyn TemporalContextQuery>,
    permits: Arc<Semaphore>,
    limits: BundleWorkLimits,
}
```

`TemporalDebugEvidenceStore` is a zero-method intersection of existing `TimelineStore + InteractionAnchorSource + InteractionRecordSource`; it introduces no facade methods. Implement the exact seven-step sequence, one absolute deadline/cancellation, fatal lifetime rules, usable degradation, component availability, deterministic header, and no-byte/reference behavior.

**Acceptance criteria:**

- [ ] Spies prove one range resolution, at most one artifact generation, exactly one context query after focus extraction, and no duplicate frame/event/store/selection call.
- [ ] The same resolved value reaches artifact/context results unchanged; source IDs exist only in the range/manifests and no bytes/base64/path/URI type enters the bundle.
- [ ] Single/multi-epoch available/unavailable outcomes retain order, handles, cache disposition, manifests, errors, and usable context.
- [ ] Gaps/partial retention/context truncation remain nested authoritative evidence; composition warnings do not rewrite them.
- [ ] Cancellation/deadline/source loss/session deletion fail the whole request; independent artifact/context/marker failures degrade only when useful evidence remains.
- [ ] Header/event language asserts observation and exact session-time proximity only, never diagnosis or causality.

### Unit 4: root composition over one runtime authority

**Story:** `epic-temporal-debugging-workflow-temporal-debug-bundle-root-composition`

**Files:**

- `src/app.rs`
- `src/debug_bundle/mod.rs`
- existing root composition tests in `src/app.rs`

Root constructs one `TemporalDebugBundleService` after the one artifact service, using projections of the same concrete `Arc<RecordingStore>` for temporal query, timeline/interaction evidence, and temporal context. `RuntimeDependencies` gains `temporal_debug_bundles: Arc<dyn TemporalDebugBundles>` for the later MCP feature. `build_service`, MCP registry/tools/resources/schemas, and progressive service remain unchanged.

**Acceptance criteria:**

- [ ] Pointer-identity tests prove one concrete store under query/timeline/interaction/context and the existing frame/artifact authorities.
- [ ] One shared artifact service/cache/scheduler is used by bundle and progressive evidence; no second scheduler/cache/decoder exists.
- [ ] Artifact work runs after marker reads without a store mutation guard, and controlled persistence progresses while visual work is blocked.
- [ ] Runtime owns one future-MCP bundle dependency, while MCP registration, resources, wire schemas, URIs, and response projection are unchanged.

### Unit 5: integrated bundle qualification and focused goldens

**Story:** `epic-temporal-debugging-workflow-temporal-debug-bundle-qualification`

**Files:**

- `src/debug_bundle/tests.rs`
- `crates/temporal-vision/tests/{storyboard.rs,contracts.rs}`
- `crates/krometrail-store/tests/{range_context.rs,temporal_queries.rs,sqlite_timeline.rs}`
- `src/artifacts/{qualification_tests.rs,service_tests.rs}` only where cache-version integration requires it
- `src/app.rs` tests
- small deterministic fixtures/goldens under existing temporal-vision/artifact test conventions only

Build one real schema-v5 store/artifact/context fixture with all seven anchor requests, tied times, caller and durable markers, unchanged and changing frames, one/two visual epochs, gaps, edge retention, browser-event collection gaps/unavailability, cache hits, per-output failures, and controlled cancellation/deletion barriers. Goldens cover the compact typed selection trace/effective policy/header posture rather than snapshotting entire SQL rows or large bundles.

**Acceptance criteria:**

- [ ] All anchor forms resolve once and report exact requested/resolved/effective anchor/options; no dual resolved-input path exists.
- [ ] Default parameters and cache keys are stable with orientation on/off; exact repeats hit and policy/algorithm-version changes miss.
- [ ] Single/multi epochs, unchanged/change, gaps, partial retention, and per-epoch artifact unavailability produce the designed usable/fatal results without stretching.
- [ ] Manifest focus extraction covers summary/selection priorities, equal-time/frame-ID ties, dedup, 16-cap, missing trace, and no second measurement.
- [ ] Marker tests cover exact caller labels, interaction/navigation/generic IDs/times, equal-time order, mandatory anchor, source/result caps, truncation, and privacy sentinels.
- [ ] Artifact/context partial failures, event proximity distances/reasons, and non-causal language remain explicit; verbose/full events are not pulled into the bundle.
- [ ] Cache reuse, cancellation/deadline, source eviction, and session-deletion races return no stale result or late bundle publication.
- [ ] Serialized application results contain no encoded image bytes, base64, data URLs, filesystem paths, segment addresses, or MCP URIs.
- [ ] One-call spies and root integration prove one store/query/artifact/context/bundle authority and no store gate across artifact work.
- [ ] Rust 1.85 locked format/check/test/Clippy gates pass. No tests target trivial wrappers/getters, each enum branch, SQL text, or MCP behavior outside this feature.

## Implementation order

1. `epic-temporal-debugging-workflow-temporal-debug-bundle-contracts-and-manifest-trace`
2. `epic-temporal-debugging-workflow-temporal-debug-bundle-default-policy-markers-and-focus` — depends on resolved bundle/trace contracts
3. `epic-temporal-debugging-workflow-temporal-debug-bundle-bounded-composition-and-degradation` — depends on exact policy, marker, and focus inputs
4. `epic-temporal-debugging-workflow-temporal-debug-bundle-root-composition` — depends on the complete service
5. `epic-temporal-debugging-workflow-temporal-debug-bundle-qualification` — depends on root composition

One feature owner should carry these checkpoints as a cohesive implementation and feature-review bundle. The stories preserve contract/policy/orchestration/composition/qualification order; they are not five worker assignments or parallelism signals.

## Simplification and elimination

- Keep one `TemporalQueryRequest` entry and one exact `ResolvedRange`; add only the resolver-produced anchor evidence needed to avoid re-resolution.
- Carry exact `ArtifactGenerationResult`, `TemporalContext`, `ArtifactMarker`, and `ArtifactManifest` values. Add no bundle artifact family, context DTO, source handle, event list, gap model, capture counter, or provenance schema.
- Put typed storyboard decisions into the existing manifest/cache authority; remove the need for untyped parameter parsing or a second measurement/selection pass.
- Reuse generic timeline and interaction record/anchor sources with one bounded filtered read. Add no marker/event table or store.
- Reuse the artifact service's epoch splitting, normalization, limits, scheduler, single flight, publication, cache validation, retention, and deletion fences. Bundle code never decodes or renders.
- Reuse the context service's compact priority, exact focus distance, warnings, and browser-event privacy vocabulary. Bundle code never queries events directly.
- Keep motion history, region artifacts, focused variants, source bytes, pins, verbose events, MCP presentation/resources/URIs, comparison, replay, tracking, diagnosis, and causality outside this feature.

## Testing strategy

- **Stable boundary:** validated request/result and one-call spies protect the primary product workflow and prevent a later resolved-input shortcut.
- **Complex visual seam:** typed selection-trace validation and cache-version tests protect generated/hit equivalence without re-testing image algorithms.
- **Policy unit:** exact generator requests, anchor rules, marker caps/order/privacy, focus ranks/ties, header posture, and degradation classification receive small table-driven tests because these are the feature's novel logic.
- **Real integration:** one schema-v5 store plus production artifact/context services protects source lifetime, cache reuse, timeline labels, event correlation, gaps/retention, and root pointer identity.
- **Concurrency regression:** deterministic barriers protect cancellation/deletion and prove no store gate spans visual work; no stopwatch assertions or sleeps.
- **Goldens:** retain small stable trace/effective-policy/header fixtures and existing PNG hashes; do not snapshot whole manifests, SQLite schemas, or large images.
- **No wrapper tests:** do not test simple trait delegation, getters, every warning variant, or future MCP projection.

## Risks and rollback

- **Manifest evolution can invalidate cache unexpectedly.** The explicit storyboard `1.1.0` bump makes invalidation intentional; old manifests deserialize without the trace and remain progressively readable. If trace validation is faulty, disable bundle focus extraction and regenerate only storyboard `1.1.0` artifacts after correction; never parse parameters or recompute in the bundle.
- **A 20-second sequential bundle may be slower than parallel fan-out.** Context depends on visual focus by contract, so one artifact call then one context call is the honest path. Artifact cache hits keep repeated queries fast. If measured latency is poor, optimize the existing artifact/context services or add a precomputed trace behind their ports; do not add a second context query.
- **Timeline marker volume can be high.** Kind-filtered SQL, 1024-row source cap, 256 artifact cap, and explicit truncation bound it. If evaluation needs broader marker coverage, raise validated limits or add cursor-based progressive marker retrieval later rather than silently dropping caps.
- **Synthetic generic-marker labels are less informative.** They are explicitly flagged and contain only typed IDs. Exact caller labels remain available when supplied. A future owned marker-payload contract may enrich the generic timeline, but this feature does not invent a store/schema.
- **Partial evidence can be over-trusted.** Usability never removes nested gaps, retention, event-unavailability, or component errors; the fixed header posture avoids negative claims and causality. If agents still over-trust degraded bundles, future MCP presentation can prioritize warnings without changing this domain result.
- **ResolvedRange gains a field across many tests/callers.** The extension is additive and constructor-validated. Rollback to an out-of-band latest lookup would reintroduce a race and is not acceptable; if migration cost is larger than expected, add a constructor helper/default only for internal test fixtures, not a second production anchor authority.
- **Root overlap with later MCP work:** this feature adds only a runtime dependency. If MCP work lands concurrently, rebase the one dependency field/wiring change; do not let protocol types enter `debug_bundle`.

## Pre-mortem

The most damaging failure is a cache-hit storyboard whose visible panels are correct but whose bundle correlates browser events to moments reconstructed by different logic. That would make proximity appear precise while using a second, drifting visual interpretation. The typed manifest trace, shared algorithm version, generated/hit equivalence tests, and strict no-remeasurement rule attack that directly. The fallback is empty-focus compact context, not a hidden recomputation.

The next failure is a degraded bundle that looks complete after retention, event loss, or an epoch-specific artifact failure. Exact nested warnings/outcomes remain authoritative, the header makes no negative/stability claim, and the result distinguishes unavailable components from empty evidence. Source/session `NotFound` after resolution is fatal rather than returning stale handles.

The least certain area is marker density and the usefulness of generic labels. Caps, mandatory anchor/caller preservation, deterministic proximity ranking, and explicit truncation make the trade observable. The design can later consume a richer generic marker payload without changing artifact/context/bundle authorities.

## Blockers

None. Resolved queries are done; artifact generation/cache is reviewed and done; capture/browser-event context and progressive evidence have stable review-stage implementation contracts. The current manifests require the scoped typed trace extension described above, but no external research, schema migration, MCP change, or new visual algorithm is needed.

## Integrated verification (all five child stories done)

- All five child stories completed and advanced directly to `done` with green Rust 1.85 locked gates.
- Unit 1 (contracts/trace): seven anchor forms, bundle request/result validation, storyboard visual summary, manifest trace validation, descriptor `1.1.0` isolation, PNG golden stability.
- Unit 2 (policy/markers/focus): byte-stable v1 generators, bounded privacy-safe marker assembly with 64/256/1024 caps, deterministic major-change focus extraction with 16-cap and dedup, kind-filtered SQL timeline read.
- Unit 3 (service): controlled spies prove one resolve, at-most-one generate, exactly-one post-focus context; fatal lifetime/cancellation/deadline paths; usable degraded bundles; `controlled` wraps every await.
- Unit 4 (root): pointer-identity one-store proof, shared artifact service, blocked-generation-permits-frame-persistence barrier test, MCP unchanged.
- Unit 5 (qualification): real schema-v5 store + production artifact service end-to-end, cache reuse, interaction anchor, orientation on/off, session deletion fatal, golden effective policy/header, thorough no-leak serialization.
- Integration correction: `validate_resolved_range` in `context.rs` fixed to use `range.validate()` instead of `ResolvedRange::new`, enabling all seven anchor kinds through `TemporalContextRequest`.
- Workspace gates: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace --all-targets --locked`, `cargo check --workspace --all-targets --locked` — all pass.
- Feature advances to `review` for standard independent review.
