---
id: feature-temporal-scale-compact-responses
kind: feature
stage: review
tags: [agent-ux, visual, storage]
parent: null
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-21
updated: 2026-07-22
---

# Compact temporal responses at every-frame scale

## Brief

GitHub issue #14 finding #7: a roughly 90-second streamed interaction under
every-frame capture retained 5,029 frames. The expanded temporal response
enumerated thousands of identifiers and exceeded a practical tool-output
budget, and difference-map analysis degraded to a 59-frame uniform sample with
`resource_limit_exceeded`. Long windows need a compact response mode that
summarizes epochs, gaps, keyframes, and selected differences without
enumerating every retained frame — preserving drill-down authority through
paging and canonical resources rather than inline enumeration
(canonical-result-projection).

- Interaction: `bbbc0f3d-6b9b-41dd-851c-c74b5d66cacb`.
- Range: `2dfc582d-aafd-41aa-97c8-6dc96999f4d7`.

Also from finding #10: a bundle requested immediately after an action marked
the tail partial because the requested post-action interval had not yet
elapsed. Distinguish "future interval not elapsed" from actual evidence loss in
partial-tail reporting (bounded-loss-accounting: every omission states exactly
why).

## Related backlog

The `perf-scout-*` backlog items target raw artifact-pipeline performance
(decode, accumulators, fanout, caching). This feature is about response
*shape* — summarization and truthful degradation — not pipeline throughput.
Cross-reference during design; do not merge. Pipeline perf may independently
raise the point at which sampling degradation kicks in.

## Simplification opportunity

Uniform-sample degradation under `resource_limit_exceeded` may become
unnecessary for summary-level questions once a keyframe/change-aware compact
summary exists; design should check whether the degraded path can be retired
or narrowed rather than kept alongside the new mode.

## References

- GitHub issue #14, findings 7 and 10 (partial-tail clarity).

## Design decisions

- **Bound identifier enumeration at the projection layer, not the domain layer**:
  `ResolvedRange.frame_ids` (and sibling id vectors) stay exact in
  `krometrail-core`. They are the canonical acquisition authority — artifact
  generation validates "the exact resolved frame set"
  (`src/artifacts/epoch.rs::validate_and_plan`), pinning protects exact frames,
  video planning and the process-local handle table all consume them. Bounding
  the domain vectors would force re-queries into every consumer and break
  canonical-result-projection ("canonical acquisition unchanged, presentation
  bounded"). Memory is not the pressure (5,029 UUIDs ≈ 80 KB); serialized tool
  output is. All bounding therefore happens in
  `crates/krometrail-mcp/src/response.rs`.
- **Every detail tier is bounded, including `full`**: today `expanded`/`full`
  serialize the complete `ResolvedRange` (and, for `generate_artifacts`, complete
  inline `ArtifactManifest` id vectors), which is what enumerated thousands of
  identifiers in finding #7. `full` remains the widest tier but caps id
  enumeration with exact omitted counts; the complete sets stay reachable through
  the range handle, paginated `list_source_frames`, `fetch_source_frames`, and
  canonical manifest resources. This is a deliberate behavior reduction at
  `full` recorded here: after this change a caller can no longer reconstruct a
  complete `ResolvedRange` from a projected response when the range exceeds the
  cap — the supported drill-down authorities are the handle and re-resolution
  (deterministic). Per Current Contract Discipline there is no third-party
  consumer of the unbounded shape; SPEC "Temporal Queries" is rolled forward in
  the same stride.
- **Compact summarization is metadata-only (no decode at resolution time)**:
  epochs, gaps, cadence, and counts come from already-loaded `CapturedFrame`
  metadata inside `CaptureQuality`. Change-aware keyframe selection stays where
  it already exists — the storyboard (VISUAL-EVIDENCE "Change-aware selection").
  The compact response points at the storyboard's selected frames as the
  keyframe set instead of inventing a second decoded-pixel selection pass at
  resolution time.
- **Partial-tail state (finding #10b) is a new `RetentionWarning` variant emitted
  by the resolver**, not a projection-layer guess: `RequestedEndNotYetElapsed
  { requested, newest_retained, session_now }` is added *alongside* the existing
  `RequestedEndAfterNewestRetained` when the session is live in this process and
  the requested end exceeds the current session time. The resolver is the only
  place that emits retention warnings and the only layer with the invariants to
  keep them truthful (bounded-loss-accounting: every omission states exactly
  why). The signal is a refinement; when the current session time is unknown or
  unsound (ended session, cross-boot origin, normalize failure), the refinement
  is silently omitted and existing warnings stand — the refinement never blocks
  or fails resolution.
- **Session-ended tails stay as-is**: a requested end beyond an ended session's
  capture is permanent evidence absence and keeps today's warnings. A distinct
  `SessionEndedBeforeRequestedEnd` variant was considered and rejected as scope
  creep beyond issue #14; the ask is only to stop *conflating future intervals
  with loss*.
- **Uniform-sample degradation is narrowed, not retired**: change-aware selection
  cannot replace uniform sampling for difference-map/motion-history statistics —
  selecting change-heavy frames would bias the frequency and timing panels,
  which must be unbiased estimates. `ArtifactSampling::UniformBounded`
  acquisition is unchanged and `Exhaustive` keeps its hard
  `resource_limit_exceeded` failure. What is retired is the *misreport*: the
  success-path degradation warning (`analysis_sampling_warning` /
  `add_analysis_sampling_warnings` in `response.rs`) that stamped a by-design
  bounded analysis with `resource_limit_exceeded`. Sampled-ness becomes
  first-class structured accounting (`sampling_mode`, `analyzed_frame_count`,
  `source_frame_count`) in artifact projections at every tier, and remains in
  the manifest. This removes a warning from successful responses — recorded
  here as an intentional behavior reduction with the above rationale.
- **One visual-epoch authority**: the epoch predicate (`image`, `viewport`,
  `device_scale_factor` equality) currently lives privately in
  `src/artifacts/epoch.rs::same_epoch`. It moves to
  `CapturedFrame::same_visual_epoch` in core so capture-quality summaries and
  artifact planning cannot drift apart; `epoch.rs` delegates.

## Related backlog cross-reference (names only, per design brief)

All `perf-scout-*` items target artifact-pipeline throughput, not response
shape; none is folded in. Most adjacent: `perf-scout-bounded-parallel-decode`
and `perf-scout-lazy-difference-accumulators` (may later raise
`analysis_effective_max_frames`, moving the sampling threshold this feature
reports on), and `perf-scout-profile-artifact-stages` (informs those limits).
Also adjacent but done/active elsewhere: `resilient-compact-temporal-bundles`
(bundle manifests already compact — this feature extends the same discipline to
`generate_artifacts`/`resolve_temporal_range`/full-tier projections) and
`feature-source-frame-listing-economy`.

## Architectural choice

Three options considered:

1. **Domain bounding** — cap `ResolvedRange` id vectors at resolution time.
   Rejected: breaks canonical acquisition (artifacts, pinning, video, handles
   consume exact frame sets) and inverts canonical-result-projection.
2. **A separate compact summary tool** — new MCP tool returning a summary
   object for long ranges. Rejected: the response projector already owns one
   agent-facing detail progression (ARCHITECTURE "MCP Boundary"); a parallel
   surface duplicates it and violates registry-declared-surfaces.
3. **Projection-layer bounding + metadata-only domain summary** (chosen) —
   `ResolvedRange` stays canonical; `CaptureQuality` grows an exact epoch
   summary computed from frame metadata; the MCP projector bounds identifier
   enumeration at every tier with exact omission accounting and named
   drill-down (range handle, paginated listings, manifest resources); the
   resolver gains the truthful not-yet-elapsed tail state.

Choice 3 optimizes for the binding patterns (bounded-loss-accounting,
canonical-result-projection), reuses the existing `CaptureQuality` summary
infrastructure, and leaves acquisition, caching, and retention untouched.

## Implementation Units

### Unit 1: Not-yet-elapsed tail state (trickiest unit — finding #10b)
**File**: `crates/krometrail-core/src/timeline/range.rs`, `crates/krometrail-store/src/recording.rs`, `src/app.rs`, `src/debug_bundle/header.rs`
**Story**: `feature-temporal-scale-compact-responses-not-yet-elapsed-tail`

```rust
// crates/krometrail-core/src/timeline/range.rs
pub enum RetentionWarning {
    // ... existing variants unchanged ...
    /// The requested end lies beyond the current session time of a live
    /// session: the interval has not yet elapsed. Distinct from evidence loss.
    RequestedEndNotYetElapsed {
        requested: SessionTime,
        newest_retained: SessionTime,
        session_now: SessionTime,
    },
}

pub struct TemporalRangeResolver<C, F, G, T, I> {
    // existing ports unchanged, plus:
    clock: Arc<dyn crate::MonotonicClock>,
}

impl<C, F, G, T, I> TemporalRangeResolver<C, F, G, T, I> { /* new() gains clock */ }
```

**Implementation Notes**:
- In `finalize`, fetch the session record once (today `validate_catalog_scope`
  fetches and discards it) and compute
  `session_now = SessionOrigin::new(session.origin()).normalize(clock.now())`
  only when `session.ended_at().is_none()` and the lifecycle is an active
  state. Guard against cross-boot `ObservedTime` skew (Time Model forbids
  cross-clock arithmetic): additionally require `session_now >= resolved.end()`
  and `session_now >=` the newest retained frame time; otherwise drop the
  refinement. On any guard failure emit nothing new — existing warnings stand.
- Emit the new variant in `classify_retention`'s caller (pass
  `session_now: Option<SessionTime>` down) whenever
  `resolved.end() < requested.end() && requested.end() > session_now`. It is
  additive; `RequestedEndAfterNewestRetained` and `PartiallyCaptured` still
  describe the retained truth. `validate()` needs no new invariant beyond the
  existing "partial requires warnings".
- `RecordingStore` gains an injected `Arc<dyn MonotonicClock>` at construction
  (composition root `src/app.rs` passes the process clock; store tests inject
  fixed clocks) and passes it when building `TemporalRangeResolver` in
  `TemporalQuery::resolve_range`.
- `src/debug_bundle/header.rs::compose_header`: when the warning is present,
  the bounded summary names the tail as "not yet elapsed" instead of generic
  partial phrasing (non-diagnostic language rules apply).
- Optional cheap refinement (same signal): the exact-failure message for an
  explicit range whose end exceeds captured bounds may state the end is in the
  future when `session_now` is known.
- Wire schema: `RetentionWarning` is a wire enum — regenerate and verify with
  `bash scripts/check-wire-enum-schemas.sh`.
- Docs (same stride): SPEC "Temporal Ranges" gains the not-yet-elapsed
  sentence; SPEC "Errors and Degraded Operation" degrade list gains "a
  requested post-action interval that has not yet elapsed is reported as
  not-yet-elapsed tail evidence, distinct from evidence loss".

**Acceptance Criteria**:
- [ ] Live session, interaction anchor, after-window beyond newest frame and
      beyond injected `now`: warnings contain both
      `RequestedEndAfterNewestRetained` and `RequestedEndNotYetElapsed` with
      the exact injected `session_now`.
- [ ] Ended session with the same shape: no `RequestedEndNotYetElapsed`.
- [ ] Guard failure (e.g. `session_now` < newest retained): no new variant,
      resolution succeeds unchanged.
- [ ] `check-wire-enum-schemas.sh` passes with the regenerated schema.

---

### Unit 2: Epoch summary in capture quality
**File**: `crates/krometrail-core/src/recording/frame.rs`, `crates/krometrail-core/src/timeline/context.rs`, `src/artifacts/epoch.rs`
**Story**: `feature-temporal-scale-compact-responses-epoch-capture-summary`

```rust
// crates/krometrail-core/src/recording/frame.rs
impl CapturedFrame {
    /// Single visual-epoch authority: geometry identity for epoch partitioning.
    pub fn same_visual_epoch(&self, other: &Self) -> bool {
        self.image() == other.image()
            && self.viewport() == other.viewport()
            && self.device_scale_factor().get().to_bits()
                == other.device_scale_factor().get().to_bits()
    }
}

// crates/krometrail-core/src/timeline/context.rs
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EpochSummary {
    pub epoch_index: u32,
    pub range: SessionRange,
    pub frame_count: u64,
    pub first_frame: FramePoint,
    pub last_frame: FramePoint,
    pub image: PixelDimensions,
    pub viewport: PixelDimensions,
    pub device_scale_factor: DeviceScaleFactor,
}

pub struct CaptureQuality {
    // ... existing fields unchanged, plus:
    pub epochs: Vec<EpochSummary>,
}
```

**Implementation Notes**:
- Computed in `context.rs::capture_quality` from the `metadata: &[CapturedFrame]`
  it already receives — one O(n) pass splitting on `same_visual_epoch`, no
  decoding, no new queries. The domain vec is exact (adversarial per-frame
  geometry churn makes epochs == frames; that is truthful and cheap — bounding
  is the projector's job).
- `src/artifacts/epoch.rs::same_epoch` delegates to
  `CapturedFrame::same_visual_epoch` (consolidation; existing epoch-partition
  tests pin equivalence).
- This lands the "epochs" leg of the compact summary: `resolve_temporal_range`
  and the bundle context both carry `capture_quality`, so a long range is
  describable as epochs + gaps + cadence + counts without any id enumeration.

**Acceptance Criteria**:
- [ ] Frames with one geometry change mid-range produce exactly two
      `EpochSummary` rows with exact per-epoch counts, ranges, and endpoints.
- [ ] Uniform-geometry range produces one epoch covering all frames.
- [ ] Artifact epoch partitioning behavior is unchanged (existing
      `src/artifacts` tests stay green with the delegated predicate).

---

### Unit 3: Bounded resolved-range and manifest projection
**File**: `crates/krometrail-mcp/src/response.rs` (plus SPEC/ARCHITECTURE doc roll-forward)
**Story**: `feature-temporal-scale-compact-responses-bounded-projection`

```rust
// crates/krometrail-mcp/src/response.rs
const MAX_EXPANDED_RANGE_EVENT_IDS: usize = 32;  // per kind: interaction/navigation/marker
const MAX_FULL_RANGE_EVENT_IDS: usize = 128;
const MAX_FULL_RANGE_FRAME_IDS: usize = 256;
const MAX_PROJECTED_EPOCHS: usize = 8;           // concise; 32 at expanded/full

#[derive(Serialize)]
struct BoundedIds<T: Serialize> {
    ids: Vec<T>,
    omitted_count: u64, // exact
}

/// One projection for every tier; replaces all direct
/// `serde_json::to_value(&range)` sites.
fn bounded_resolved_range(
    range: &krometrail_core::ResolvedRange,
    detail: ResponseDetail,
) -> Result<Value, ResponseInvariantError>;

/// Inline manifest presentation with capped id vectors and the canonical
/// manifest resource URI; the persisted manifest resource stays complete.
fn bounded_manifest_value(
    scope: krometrail_core::EvidenceScope,
    manifest: &temporal_vision::ArtifactManifest,
) -> Result<Value, ResponseInvariantError>;
```

**Implementation Notes**:
- Tier semantics: **concise** keeps `CompactResolvedRange` (counts only).
  **expanded** = compact fields + first/last frame id + bounded per-kind
  interaction/navigation/marker id lists + exact `omitted_*_count`s + a
  drill-down block naming `list_source_frames` (offset paging) and the range
  handle. **full** = expanded + leading `frame_ids` slice up to
  `MAX_FULL_RANGE_FRAME_IDS` with exact `omitted_frame_id_count` and the
  `next_offset` that continues in `list_source_frames`.
- Replacement sites: `map_temporal_range_resolution_result`
  (expanded/full arm), `ListSourceFrames` non-concise range,
  `GenerateArtifacts`/`GenerateRegionFilmstrip` non-concise arms (project
  `generation.range` and use compact artifact handles at expanded, bounded
  inline manifests at full), `compact_bundle_value` (`value["range"]`), and the
  full-bundle arm (`bundle.range` and `bundle.context.range`).
- Ordering hazard: `compact_temporal_context_value` deserializes the serialized
  range back into `ResolvedRange` — apply bounding only at final presentation,
  never before context compaction.
- `BundleArtifactHandle` gains `sampling_mode` (from manifest
  `analysis_sampling`); `analyzed_frame_count`/`source_frame_count` already
  present. Retire `analysis_sampling_warning` and
  `add_analysis_sampling_warnings` (see design decision).
- Epoch presentation: the capture-quality projection in
  `compact_temporal_context` gains `epochs` bounded to `MAX_PROJECTED_EPOCHS`
  (32 at expanded/full) with exact `omitted_epoch_count`.
- Docs (same stride): SPEC "Temporal Queries" — replace "Full responses retain
  complete generator, frame, and provenance structures" with bounded-enumeration
  wording (widest tier, capped id enumeration, exact omitted counts, complete
  sets via handle/paging/manifest resources); ARCHITECTURE "MCP Boundary"
  temporal-bundle paragraph updated to match; VISUAL-EVIDENCE "Progressive
  Detail" gains one line that compact levels never inline the complete retained
  id enumeration.

**Acceptance Criteria**:
- [ ] A synthetic 1,000-frame `ResolvedRange` projected at expanded contains no
      frame-id array beyond first/last and reports exact omitted counts.
- [ ] The same range at full contains at most `MAX_FULL_RANGE_FRAME_IDS` frame
      ids plus an exact `omitted_frame_id_count` and drill-down offset.
- [ ] Expanded bundle `range` is bounded (regression for finding #7's expanded
      enumeration).
- [ ] `generate_artifacts` at full inlines bounded manifests while the manifest
      resource returns complete provenance (existing resource test extended).
- [ ] A UniformBounded difference-map success response carries structured
      sampling accounting at every tier and no `resource_limit_exceeded`
      degradation warning; `Exhaustive` over-limit still fails with
      `resource_limit_exceeded`.

---

## Implementation Order

1. `feature-temporal-scale-compact-responses-not-yet-elapsed-tail` (Unit 1 —
   independent; touches wire schemas early)
2. `feature-temporal-scale-compact-responses-epoch-capture-summary` (Unit 2 —
   independent; provides the epoch data Unit 3 presents)
3. `feature-temporal-scale-compact-responses-bounded-projection` (Unit 3 —
   consumes both: projects the epoch summary and finalizes the doc roll-forward)

## Simplification

- One visual-epoch predicate authority (`CapturedFrame::same_visual_epoch`);
  `src/artifacts/epoch.rs` delegates instead of owning a private copy.
- `analysis_sampling_warning` + `add_analysis_sampling_warnings` removed from
  `response.rs` in favor of structured sampling accounting already carried by
  `BundleArtifactHandle`/manifests (recorded behavior reduction).
- `bounded_resolved_range` becomes the single range-projection entry point;
  `CompactResolvedRange` is its concise tier rather than a parallel path.
- Not folded in: retiring `UniformBounded` acquisition (kept — see design
  decision on statistical bias); pipeline perf (parked `perf-scout-*`).

## Testing

- **Interface (store)**: `crates/krometrail-store/tests/range_resolution.rs` —
  live-session not-yet-elapsed emission with injected fixed clock, ended-session
  suppression, guard-failure suppression. Protects the finding #10b contract.
- **Interface (core)**: `context.rs` tests — epoch partition exactness (one
  change → two epochs; none → one). Protects the metadata-only summary.
- **Interface (MCP)**: `response.rs` tests — per-tier bounds and exact omission
  accounting for a large synthetic range; expanded-bundle range bound; sampling
  accounting without the degradation warning. These are the direct regression
  tests for finding #7's response-size failure.
- **Regression kept**: existing `src/artifacts/service_tests.rs` manifest
  `analysis_sampling` accounting tests stay (manifest truth unchanged).
- **Removal**: `response.rs` tests asserting the `resource_limit_exceeded`
  sampling warning on successful UniformBounded generations are updated or
  removed with the warning.
- Not tested per-unit: serialization plumbing with no branching (projection
  field copies), delegated predicate call sites already covered by epoch tests.

## Risks

- **Round-trip inputs**: tools accepting a complete `ResolvedRange` (e.g.
  `pin_resolved_range`) can no longer be fed from a projected response when the
  range exceeds the full-tier cap; the handle is the supported reference and
  re-resolution is deterministic. Accepted per Current Contract Discipline;
  SPEC wording updated so the contract is not silently contradicted.
- **Cross-boot clock skew**: `ObservedTime` origins from a prior process must
  never be compared with the current monotonic clock. Mitigated by the
  live-session + sanity guards in Unit 1; on any doubt the refinement is
  omitted, never wrong.
- **Store constructor ripple**: injecting the clock into `RecordingStore`
  touches its constructors and tests. Mitigated by injecting at the existing
  composition root (`src/app.rs`) and fixed clocks in tests; no default-clock
  fallback inside the store (injected-core-ports).
- **Full-tier SPEC contradiction**: bounding `full` contradicts the current SPEC
  sentence about complete structures; the doc roll-forward is part of Unit 3's
  acceptance, not a follow-up.
- **Adversarial epoch counts**: per-frame geometry churn yields epochs ==
  frames in the domain vec; presentation is bounded with exact omission counts,
  and the O(n) pass adds no measurable cost at 5k frames.

## Implementation notes

All three child stories implemented in dependency order and closed
2026-07-21; the full gate (fmt, wire-enum schema check, check, test, clippy
`-D warnings`) is green after each story and at feature close.

- **Unit 1 (`…-not-yet-elapsed-tail`)**: `RetentionWarning::
  RequestedEndNotYetElapsed` emitted additively by the resolver from a
  guarded live-session current time (`live_session_now`: live lifecycle, no
  `ended_at`, normalizable origin, `session_now >=` newest retained frame
  time; emission additionally requires `session_now >= resolved.end()` and a
  future requested end). `TemporalRangeResolver` and `RecordingStore` take
  an injected `Arc<dyn MonotonicClock>` — no default clock in the store;
  the composition root passes the process clock and every test injects a
  fixed clock. Bundle header names the tail "not yet elapsed … not evidence
  loss". SPEC rolled forward. The design's optional exact-failure-message
  refinement was skipped as a second emission path for the same signal
  (marked optional in the design).
- **Unit 2 (`…-epoch-capture-summary`)**: `CapturedFrame::same_visual_epoch`
  is the single epoch predicate authority (artifact `same_epoch` delegates);
  `CaptureQuality.epochs: Vec<EpochSummary>` computed in one metadata-only
  O(n) pass. Domain vector exact; presentation bounded by Unit 3.
- **Unit 3 (`…-bounded-projection`)**: `bounded_resolved_range` is the
  single per-tier range projection (counts-only concise; bounded event ids
  + drill-down at expanded; 256-frame head + exact omitted count + paging
  offset at full); generation/bundle/context/video/pin presentations all
  bound identifier enumeration with exact accounting; full inlines bounded
  manifests while canonical manifest resources stay complete. The two
  receiver-accepted behavior reductions are implemented as designed:
  bounded `full` (SPEC/ARCHITECTURE/VISUAL-EVIDENCE + plugin skill rolled
  forward in-stride) and the retirement of the success-path
  `resource_limit_exceeded` sampling warning in favor of structured
  sampling accounting (`sampling_mode` + analyzed/source counts on
  `BundleArtifactHandle`); `Exhaustive` over-limit keeps its hard failure.
- **Simplification outcome**: uniform-sample degradation acquisition was
  kept (statistical-bias rationale in the design decision); what was retired
  is the misreporting warning. `CompactResolvedRange` is now the concise
  tier of the single projection entry point rather than a parallel path.
- Commits: `bdc5a59c` (Unit 1), `83e8e725` (Unit 2), `86ef49f4` (Unit 3).

## Review adjudication (standard weight, cross-model gpt-5.6-sol, one pass)

Clean areas: clock injection and lifecycle guards, full-tier head pagination
composition, epoch-predicate consolidation (behavior-preserving), structured
sampling accounting, canonical manifest completeness, foundation-doc honesty.

Five findings, all accepted; fixes routed to the active implementation worker
(single-writer), closure is fix-verification only:
1. (blocker) `fetch_source_frames` expanded/full still serializes the complete
   canonical range — must project through `bounded_resolved_range` at every
   tier.
2. (significant) Mixed elapsed/future tails: only `[session_now,
   requested_end]` is "not yet elapsed"; the elapsed prefix keeps loss/
   uncertainty language.
3. (significant) Pin-state `retained_frame_ids`/`missing_frame_ids` unbounded
   at every tier — bound with exact omitted counts.
4. (minor) Manifest sampling-index bounding branch dead (tagged
   `ParameterValue` shape mismatch).
5. (minor) Missing over-cap tests (fetch tiers, manifest indices, pin caps,
   video range) that allowed 1/3/4 through.

## Review fixes

- **B1 fixed:** `fetch_source_frames` now projects its resolved range through
  `bounded_resolved_range` for concise, expanded, and full responses; a
  1,000-frame small-selection regression covers the full-tier cap.
- **B2 fixed:** mixed tails now distinguish the elapsed interval through
  `session_now` (which may represent evidence loss or uncertainty) from the
  future interval through the requested end (not yet elapsed and not evidence
  loss). The header regression asserts both claims and stays within its byte
  bound.
- **B3 fixed:** pin request, retained, and missing frame-id vectors are each
  bounded per response tier with separate exact omission counts. PinState and
  PinChange over-cap regressions cover concise, expanded, and full.
- **B4 fixed:** manifest sampling-index projection now traverses the tagged
  `ParameterValue` object/list/value shape; a decimated manifest with 300
  analyzed indices verifies the 256-id cap and exact omission count.
- **B5 fixed:** over-cap coverage now includes fetch ranges, tagged manifest
  sampling indices, pin vectors, and a 1,000-frame temporal-video result range.
