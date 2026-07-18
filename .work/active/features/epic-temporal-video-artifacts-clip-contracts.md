---
id: epic-temporal-video-artifacts-clip-contracts
kind: feature
stage: done
tags: [visual, agent-ux, security]
parent: epic-temporal-video-artifacts
depends_on: []
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Temporal video clip contracts

## Brief

Define the deterministic, browser-independent contract that turns one resolved retained range into a bounded temporal-video presentation plan. It covers real-time and model-optimized timing, ordered source mapping, explicit gap slates, visual-epoch boundaries, output ceilings, encoder input, result identity, and typed video provenance. The same canonical plan must drive encoding and the manifest so a held frame or gap can never be represented differently across those surfaces.

This is the shared foundation for the process adapter and retained generation service. It does not discover or launch FFmpeg, publish bytes, register MCP tools, upload to providers, or add video responsibilities to `temporal-vision`'s still-image manifest.

## Epic context

- Parent epic: `epic-temporal-video-artifacts`
- Position in epic: foundation feature — the FFmpeg runtime and retained generation service consume its contracts independently

## Simplification opportunity

- Reuse the existing resolved-range, source-frame, gap, visual-epoch, cancellation, artifact-identity, and validated-wire-contract vocabulary; introduce only the video-specific timing and encoder provenance needed to keep the still visual crate process-free.

## Foundation references

- `docs/VISION.md` — Visual Evidence and Product Boundaries
- `docs/SPEC.md` — Temporal Queries, Artifact Provenance, and Exclusions
- `docs/ARCHITECTURE.md` — Artifact Generation, Capability Registry, and Dependency Direction
- `docs/VISUAL-EVIDENCE.md` — Temporal Video Clip, Capture Gaps, and Provenance
- `docs/EVALUATION.md` — Optional video conditions and Temporal video evaluation

## Parent decisions inherited

- Both presentation policies ship under one versioned deterministic plan.
- Video provenance is typed without importing process or codec concerns into `temporal-vision`.
- Numeric ceilings are hard server boundaries; callers may only request values within them.
- No UI surfaces or mockups apply.

## Design decisions

- **One plan per visual epoch**: a `VideoPresentationPlan` accepts exactly one `VisualEpoch`; a range spanning geometry/device-scale changes produces multiple ordered clips in the later service. Combining epochs into one padded movie was rejected because it would obscure the source geometry boundary and make pixel interpretation ambiguous.
- **Application-owned planning**: validated values and the encoder port live in `krometrail-core`, while the pure planning algorithm lives in the root application's new `video` module. Putting policy in the FFmpeg adapter would couple provenance to process behavior; putting it in `temporal-vision` would make the still-image crate own Krometrail video/output contracts.
- **Conservative v1 ceilings**: one plan accepts at most 30 seconds of source time, 120 retained frames, 12 meaningful-frame anchors, 512 presentation segments, 1920×1080 output, 512 MiB of encoded image inputs, 60 seconds of presentation time, and 64 MiB of MP4 output. A later increase is additive; requests never bypass the server ceilings.
- **Explicit derived timing**: real-time mode preserves observed frame deltas, except that zero-length/tied segments receive a recorded 1 ms minimum and the terminal frame receives a recorded 250 ms hold. Model-optimized mode starts from that plan, holds declared meaningful frames for at least 1 second, and holds gap slates for at least 500 ms. Every adjustment has a typed reason in the plan and manifest.
- **No silent fitting**: if the deterministic plan exceeds a limit it fails with `resource_limit_exceeded`; it is never truncated, rescaled in time, or thinned behind the caller's back. The later service can resolve a narrower range and try again.
- **Gap slates are plan elements**: overlapping gaps are clipped to the epoch's presented source range, deterministically sorted/coalesced, and emitted as segments containing every contributing `GapId`. No source frame is synthesized or interpolated. A gap replaces only the mapped presentation interval; source frames remain available independently.
- **Meaningful selection is injected evidence**: the planner accepts a unique ordered subset of at most 12 frame IDs selected by the later generation service. It does not duplicate the storyboard selector or infer importance. The plan and manifest preserve those IDs and the selector/version will enter the retained artifact cache identity.
- **Fixed media contract**: the port accepts encoded JPEG/PNG stills plus generated PNG gap slates and produces silent MP4/H.264 using `yuv420p`. Output geometry preserves aspect ratio without upscaling and pads at most one right/bottom pixel to reach even dimensions; the transform is explicit provenance.
- **Privacy-safe exact encoder identity**: provenance stores a bounded encoder implementation-version label, a SHA-256 of the complete build report, selected encoder name, adapter version, and argument-policy version. It never stores the executable path, temporary paths, raw stderr, or an unredacted configure line.
- **Stable failure vocabulary**: add `video_encoder_unavailable` for missing/changed qualified authority and `video_encoding_failed` for a failed qualified invocation. Cancellation and resource ceilings retain the existing shared codes.

## Architectural choice

Three approaches were considered:

1. **Core contracts + pure application planner + injected encoder port (chosen).** Core owns invariant-bearing values and provenance; one deterministic root module owns timing policy; adapters only encode. This matches injected-core ports, keeps schemas constructor-backed, and makes a fake encoder sufficient for deterministic service tests.
2. **Add video generation to `temporal-vision`.** This would make the planner reusable outside Krometrail, but it would also force a still-image computation crate to own MP4/H.264 profiles, external encoder identity, Krometrail retention semantics, and process-facing input shapes. The generality is not earned by a current consumer.
3. **Build the plan inside the FFmpeg adapter or MCP handler.** This has fewer initial types, but it lets process details or transport routing become the timing/provenance authority and makes cache/manifests capable of disagreeing with actual encoder input. It violates both the port and registry patterns.

The chosen design is intentionally not a generic media framework. It defines one closed silent MP4/H.264 contract and one source-derived temporal-plan version.

## Implementation units

### Unit 1: Validated temporal-video domain

**Files**: `crates/krometrail-core/src/video/mod.rs`, `crates/krometrail-core/src/video/plan.rs`, `crates/krometrail-core/src/lib.rs`

**Story**: `epic-temporal-video-artifacts-clip-contracts-domain-and-encoder-port`

```rust
pub const TEMPORAL_VIDEO_PLAN_VERSION: &str = "temporal-video-plan-v1";
pub const MAX_VIDEO_SOURCE_DURATION: Duration = Duration::from_secs(30);
pub const MAX_VIDEO_PRESENTATION_DURATION: Duration = Duration::from_secs(60);
pub const MAX_VIDEO_SOURCE_FRAMES: usize = 120;
pub const MAX_VIDEO_MEANINGFUL_FRAMES: usize = 12;
pub const MAX_VIDEO_PRESENTATION_SEGMENTS: usize = 512;
pub const MAX_VIDEO_WIDTH: u32 = 1_920;
pub const MAX_VIDEO_HEIGHT: u32 = 1_080;
pub const MAX_VIDEO_ENCODED_INPUT_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_VIDEO_ENCODED_OUTPUT_BYTES: u64 = 64 * 1024 * 1024;

pub enum VideoPresentationPolicy {
    RealTime,
    ModelOptimized,
}

pub struct PresentationTime(u64);

pub struct PresentationRange {
    start: PresentationTime,
    end: PresentationTime,
}

pub struct VideoOutputGeometry {
    source: PixelDimensions,
    scaled: PixelDimensions,
    canvas: PixelDimensions,
    pad_right: u8,
    pad_bottom: u8,
}

pub enum VideoSegmentSource {
    SourceFrame { frame_id: FrameId, session_time: SessionTime },
    GapSlate { gap_ids: Vec<GapId>, source_range: SessionRange },
}

pub enum VideoTimingBasis {
    RecordedDelta,
    MinimumVisibleFrame,
    TerminalHold,
    RecordedGap,
    ModelMeaningfulHold,
    ModelGapHold,
}

pub struct VideoPresentationSegment {
    index: u32,
    source: VideoSegmentSource,
    presentation: PresentationRange,
    timing_basis: VideoTimingBasis,
}

pub struct VideoPlanInput {
    range: ResolvedRange,
    epoch: VisualEpoch,
    frames: Vec<CapturedFrame>,
    meaningful_frame_ids: Vec<FrameId>,
    output: VideoOutputGeometry,
    policy: VideoPresentationPolicy,
}

pub struct VideoPresentationPlan {
    version: NonEmptyText,
    policy: VideoPresentationPolicy,
    requested_range: SessionRange,
    resolved_range: SessionRange,
    presented_source_range: SessionRange,
    epoch: VisualEpoch,
    input_frame_ids: Vec<FrameId>,
    meaningful_frame_ids: Vec<FrameId>,
    segments: Vec<VideoPresentationSegment>,
    output: VideoOutputGeometry,
    duration: PresentationTime,
}

impl VideoPlanInput {
    pub fn new(
        range: ResolvedRange,
        epoch: VisualEpoch,
        frames: Vec<CapturedFrame>,
        meaningful_frame_ids: Vec<FrameId>,
        output: VideoOutputGeometry,
        policy: VideoPresentationPolicy,
    ) -> Result<Self>;
}

impl VideoPresentationPlan {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        policy: VideoPresentationPolicy,
        requested_range: SessionRange,
        resolved_range: SessionRange,
        presented_source_range: SessionRange,
        epoch: VisualEpoch,
        input_frame_ids: Vec<FrameId>,
        meaningful_frame_ids: Vec<FrameId>,
        segments: Vec<VideoPresentationSegment>,
        output: VideoOutputGeometry,
    ) -> Result<Self>;
}
```

**Implementation notes**:

- Use constructor-backed Serde and delegated JSON Schema for externally reusable policy/limit values; reject unknown fields and invalid enum spellings.
- `PresentationRange` is half-open and non-empty. Plan segments start at presentation zero, remain contiguous, have strictly increasing indices, and end exactly at `duration`.
- `VideoPlanInput::new` revalidates exact session/target/frame order, strict capture ordinals, range containment, one geometry/device-scale epoch, meaningful-frame subset/order, output aspect/no-upscale rules, and all hard ceilings.
- `VideoOutputGeometry` allows only aspect-preserving integer/rational fitting and zero/one-pixel right/bottom padding for even `yuv420p` dimensions; no hidden crop or stretch.

**Acceptance criteria**:

- [ ] Invalid, duplicate, out-of-range, cross-target, cross-epoch, over-limit, noncontiguous, or path-bearing values cannot construct or deserialize.
- [ ] One complete registry/round-trip test fixes the stable policy and timing-basis names without a second hand-maintained list.
- [ ] A plan cannot claim source frames, gaps, dimensions, or duration that contradict its exact input epoch and resolved range.

### Unit 2: Narrow video-encoder port

**Files**: `crates/krometrail-core/src/ports/video.rs`, `crates/krometrail-core/src/ports/mod.rs`, `crates/krometrail-core/src/error.rs`, `crates/krometrail-core/src/lib.rs`

**Story**: `epic-temporal-video-artifacts-clip-contracts-domain-and-encoder-port`

```rust
pub struct VideoEncoderIdentity {
    implementation_version: NonEmptyText,
    build_report_sha256: [u8; 32],
    encoder_name: NonEmptyText,
    adapter_version: NonEmptyText,
    argument_policy_version: NonEmptyText,
}

pub struct VideoEncodingProfile {
    geometry: VideoOutputGeometry,
    max_encoded_bytes: u64,
}

pub struct VideoEncodeFrame {
    segment_index: u32,
    format: ImageFormat,
    dimensions: PixelDimensions,
    bytes: Arc<[u8]>,
}

pub struct VideoEncodeRequest {
    plan: VideoPresentationPlan,
    frames: Vec<VideoEncodeFrame>,
    profile: VideoEncodingProfile,
}

pub struct VideoEncodedClip {
    identity: VideoEncoderIdentity,
    profile: VideoEncodingProfile,
    output_hash: temporal_vision::OutputHash,
    encoded_bytes: Arc<[u8]>,
}

pub struct VideoEncodingContext {
    pub deadline: Instant,
    pub cancellation: Arc<dyn CancellationSignal>,
}

pub trait TemporalVideoEncoder: Send + Sync {
    fn identity(&self) -> &VideoEncoderIdentity;
    fn encode(
        &self,
        request: VideoEncodeRequest,
        context: VideoEncodingContext,
    ) -> PortFuture<'_, Result<VideoEncodedClip>>;
}
```

**Implementation notes**:

- Constructors enforce one non-empty encoded image per plan segment, exact segment-index order, matching source dimensions/formats, aggregate input bytes within the hard ceiling, fixed `video/mp4` + H.264 + `yuv420p` + no-audio semantics, and output bytes/hash within the request profile.
- `VideoEncoderIdentity` stores only the bounded version label and hashes/allowlisted names; reject path separators and control characters from visible fields.
- Add `ErrorCode::VideoEncoderUnavailable` (`video_encoder_unavailable`) with `AfterRecovery` retry advice and `ErrorCode::VideoEncodingFailed` (`video_encoding_failed`) with `Safe` retry advice. Adapters keep private causes in sanitized logs.
- Do not add discovery, qualification, subprocess, filesystem, or Tokio process types to core.

**Acceptance criteria**:

- [ ] The port is object-safe and can be exercised with a deterministic fake containing no FFmpeg, filesystem, or Tokio process dependency.
- [ ] Request construction rejects missing/reordered segments, empty/oversized input, geometry drift, unsupported image formats, and a profile that differs from the plan.
- [ ] Result construction verifies exact output bytes, SHA-256, identity, profile, and size without claiming to parse MP4/H.264; the production adapter owns that proof.
- [ ] Stable error-code serialization and retry defaults are exhaustive.

### Unit 3: Deterministic presentation planner

**Files**: `src/video/mod.rs`, `src/video/plan.rs`, `src/video/tests.rs`, `src/main.rs`

**Story**: `epic-temporal-video-artifacts-clip-contracts-presentation-planner`

```rust
pub(crate) fn build_presentation_plan(input: VideoPlanInput) -> Result<VideoPresentationPlan>;

fn coalesce_epoch_gaps(
    gaps: &[CaptureGap],
    presented_source_range: SessionRange,
) -> Result<Vec<CoalescedGap>>;

fn real_time_segments(
    input: &VideoPlanInput,
    gaps: &[CoalescedGap],
) -> Result<Vec<VideoPresentationSegment>>;

fn optimize_for_model(
    segments: Vec<VideoPresentationSegment>,
    meaningful_frame_ids: &[FrameId],
) -> Result<Vec<VideoPresentationSegment>>;
```

**Implementation notes**:

- The presented source range is the first through last retained frame of the epoch; the broader requested/resolved ranges remain provenance and are never rewritten to fit render-local evidence.
- Sort gaps by clipped start/end and stable `GapId`, coalesce overlap, retain every contributor ID, and split frame holds at gap boundaries. Never interpolate between frames or infer a gap from timestamp/ordinal distance.
- Real-time uses observed deltas, a 1 ms minimum only for zero/tied frame segments, and a 250 ms terminal hold. Model-optimized replaces durations only where the typed 1-second meaningful-frame or 500 ms gap minimum is larger.
- Recompute presentation offsets from zero after each explicit adjustment. Return `resource_limit_exceeded` if the result exceeds 60 seconds or 512 segments; never drop a segment to make it fit.
- The algorithm has no I/O, image decoding, FFmpeg knowledge, provider sampling heuristics beyond the declared holds, or mutable global state.

**Acceptance criteria**:

- [ ] Identical frames, capture order, selected IDs, gaps, policy, and geometry produce byte-identical serialized plans regardless of caller gap ordering.
- [ ] Real-time and model-optimized table cases prove ordinary deltas, tied timestamps, single-frame terminal hold, selected-state holds, gap clipping/coalescing, and explicit timing-basis labels.
- [ ] Every input frame remains in ordered source provenance; every visible gap slate maps to exact contributing IDs/range; neither policy invents an intermediate frame.
- [ ] Cross-epoch input and every exact/next-unit limit boundary fail or pass deterministically with the expected stable code.

### Unit 4: Typed manifest and cache transcript

**Files**: `crates/krometrail-core/src/video/manifest.rs`, `crates/krometrail-core/src/video/mod.rs`, `crates/krometrail-core/src/lib.rs`

**Story**: `epic-temporal-video-artifacts-clip-contracts-provenance-manifest`

```rust
pub const TEMPORAL_VIDEO_MANIFEST_SCHEMA_VERSION: u32 = 1;

pub struct TemporalVideoManifest {
    schema_version: u32,
    artifact_id: ArtifactId,
    session_id: SessionId,
    target_id: TargetId,
    requested_range: SessionRange,
    resolved_range: SessionRange,
    plan: VideoPresentationPlan,
    encoder: VideoEncoderIdentity,
    profile: VideoEncodingProfile,
    media_type: NonEmptyText,
    codec: NonEmptyText,
    pixel_format: NonEmptyText,
    has_audio: bool,
    encoded_byte_len: u64,
    output_hash: temporal_vision::OutputHash,
}

impl TemporalVideoManifest {
    pub fn new(
        artifact_id: ArtifactId,
        scope: &ResolvedRange,
        plan: VideoPresentationPlan,
        encoded: &VideoEncodedClip,
    ) -> Result<Self>;
}

pub fn canonical_video_cache_parameters(
    plan: &VideoPresentationPlan,
    identity: &VideoEncoderIdentity,
    profile: &VideoEncodingProfile,
) -> Result<Arc<[u8]>>;
```

**Implementation notes**:

- Constructor-backed deserialization revalidates the manifest rather than trusting persisted JSON. Media values are closed to `video/mp4`, `h264`, `yuv420p`, and `has_audio == false`.
- The exact `VideoPresentationPlan` is embedded, not reconstructed from an independent segment list. Visible annotations and encoder input therefore share one authority.
- Canonical cache parameters serialize a versioned struct containing plan, geometry, limits, encoder build hash/name, adapter/argument-policy versions, and fixed media profile. They contain no source bytes, local paths, stderr, or unordered maps. Source fingerprints remain the storage feature's separate cache-key input.

**Acceptance criteria**:

- [ ] A manifest round-trip preserves the complete plan, meaningful holds, gaps, encoder identity, media profile, and output hash and rejects any contradiction.
- [ ] Canonical bytes change when any timing, source selection, gap, geometry, ceiling, build hash, encoder, adapter, or argument-policy input changes and remain equal for identical values.
- [ ] Serialized manifests contain no executable/temp paths, raw build report, stderr, source pixels, or provider-specific fields.

## Implementation order

1. `epic-temporal-video-artifacts-clip-contracts-domain-and-encoder-port`
2. `epic-temporal-video-artifacts-clip-contracts-presentation-planner`
3. `epic-temporal-video-artifacts-clip-contracts-provenance-manifest`

The stories are durable checkpoints inside one cohesive feature implementation bundle, not separate worker assignments.

## Simplification

- Reuse `ResolvedRange`, `CaptureGap`, `VisualEpoch`, `CapturedFrame`, `PixelDimensions`, `ImageFormat`, `CancellationSignal`, `PortFuture`, `NonEmptyText`, and `temporal_vision::OutputHash` instead of adding parallel timing, frame, geometry, cancellation, or digest authorities.
- Keep one closed media profile rather than registries for hypothetical containers, codecs, audio, provider uploads, or video filters.
- Keep policy out of the adapter and MCP handler, and keep FFmpeg/process vocabulary out of `temporal-vision` and core ports.
- Do not change the existing still `ArtifactManifest` alias in this feature; the retained-generation feature will introduce the additive persisted-manifest envelope where the actual compatibility boundary lives.

## Testing

- Core interface tests protect constructor-backed wire/schema behavior, stable enum/error identities, port object safety, exact segment/frame matching, limits, hashes, and validated manifest deserialization.
- Pure planner table tests protect the novel temporal logic: tied timestamps, one frame, ordered frames, real-time deltas, model holds, intersecting/overlapping gaps, narrow epoch clipping, timing-basis disclosure, and exact boundary failures.
- One canonical serialization sensitivity test protects cache/manifests from omitting policy, timing, gap, geometry, limit, or encoder identity fields. It compares values/bytes rather than maintaining a duplicate hand-authored schema fixture.
- No FFmpeg, process, filesystem, browser, network, or paid-model test belongs here. Those are adapter and end-to-end qualification responsibilities in later features.
- No existing tests are removed; current artifact/range tests remain the authority for still-image behavior and source-range validation.

## Risks

- **Presentation can be mistaken for observation.** Minimum/terminal/model holds deliberately differ from recorded duration. Mitigation: every segment carries `VideoTimingBasis`, the manifest embeds the exact plan, and the real-time exceptions remain narrow and versioned.
- **Gap and frame boundaries can overlap.** A capture gap records known loss, not total absence of observations. Mitigation: clipping/coalescing never deletes source provenance, slate segments carry all contributing IDs, and planner tests cover boundary equality and frames inside gap ranges.
- **Meaningful-frame selection can drift from storyboard semantics.** This feature intentionally accepts selected IDs instead of copying image analysis. The retained-generation design must bind selector name/version and exact selected IDs into cache/provenance.
- **Hard ceilings are stable caller behavior.** The initial limits are intentionally conservative; increasing them is additive, while lowering them requires compatibility review. Exact-boundary tests prevent accidental tightening.
- **MP4/H.264 correctness is not proven by core bytes.** Core validates identity, size, and hash only. The FFmpeg feature must qualify and validate the real container/codec before constructing a production result.

## Other agent review

- Invoked because: stable timing/provenance and encoder-port contracts are high-risk and feed two parallel downstream features.
- Skipped/degraded: the active autopilot delegation explicitly prohibits nested agents and peeragent. This non-blocking design-time degradation is offset by constructor invariants, a source-grounded pre-mortem, and the unchanged standard feature/final completion review requirements.

## Review findings (2026-07-18)

**Review weight**: `standard` (default) — one same-harness fresh-context pass after the preferred Claude cross-model path failed on expired OAuth. Closure after correction is fix verification only; do not run a second independent pass.

**Receiver-confirmed blockers**:

- Bind every encoded source input to the exact source-frame identity claimed by its plan segment so same-geometry frame bytes cannot be swapped or duplicated under false provenance.
- Make constructor-backed and persisted plan validation reject policy/timing-basis/source-kind, duration, and canonical v1 timing contradictions rather than validating structure alone.
- Prove every gap slate's contributor IDs are exactly the canonical intersecting gaps and preserve enough typed gap evidence for deserialization to revalidate that claim.
- Align generated JSON Schema with the strict wire decoders and bounded numeric/hash values; this is included in the blocker fix because the feature acceptance explicitly promises source-aligned constructor-backed schemas.

No separate active findings were created: these corrections remain cohesive with the feature's core contract and test surface. The reviewer accepted later-feature ownership of FFmpeg, storage publication, MCP registration, and selector-version binding.

## Review (2026-07-18)

**Verdict**: Approve after verified corrections

**Blockers**: none — exact frame/source binding, durable canonical timing validation, exact persisted gap evidence, and strict source-aligned schemas were fixed in `725fe60`.
**Important**: none
**Nits**: none
**Rejected**: none

**Notes**: `standard` review used one same-harness fresh-context pass after the preferred Claude peer was unavailable because its OAuth token had expired. Per the single-pass closure policy, the receiver inspected the correction and reran the focused core/root video suites without a second independent review. The worker also reported full workspace format, check, tests, and Clippy with warnings denied as green.

## Implementation summary

- Execution capability: GPT-5.6 Sol at xhigh reasoning, coordinated as one cohesive feature worker under autopilot.
- Review weight: standard. This implementation is ready for the independent feature review; the implementing worker did not self-close the feature.
- Completed child stories and commits:
  - `epic-temporal-video-artifacts-clip-contracts-domain-and-encoder-port` — `220661f`
  - `epic-temporal-video-artifacts-clip-contracts-presentation-planner` — `f7e74a6`
  - `epic-temporal-video-artifacts-clip-contracts-provenance-manifest` — `b4d66a5`
- Core now owns the validated temporal-video plan vocabulary, fixed H.264 encoding profile, privacy-safe encoder identity, object-safe injected encoder port, stable encoder errors, typed manifest, and canonical cache transcript. The root application owns the deterministic pure presentation planner; no process, filesystem, FFmpeg, browser, MCP, or provider concerns entered these contracts.
- The planner deterministically preserves source provenance, maps declared gaps without interpolation, applies disclosed real-time/model timing rules, and fails rather than silently fitting beyond hard ceilings. The manifest embeds that exact plan and binds it to the exact resolved scope, output identity, encoder identity, fixed media contract, and output profile.
- Existing `ImageFormat` is a closed enum, so unsupported image variants remain unconstructable rather than requiring another format registry. Existing public-field `VisualEpoch` values are revalidated at video boundaries; it gained only an additive `JsonSchema` derive because the externally reusable video-plan schema embeds it.
- A selected meaningful frame that is fully obscured by an explicit gap replacement is rejected with `invalid_input`; the planner does not claim a meaningful hold that has no visible source segment. All input frame IDs still remain in ordered provenance.
- The root planner has a narrowly scoped `dead_code` allowance until the dependent retained-generation service consumes it. No broader warning suppression or compatibility shim was added.
- Simplification completed as designed: existing range, frame, gap, epoch, geometry, cancellation, identity, and hash types remain the authorities; the media contract is deliberately closed; no adjacent issue was discovered or parked.

## Verification evidence

- `cargo fmt --all -- --check` — passed.
- `cargo check --workspace --all-targets --locked` — passed.
- `cargo test --workspace --all-targets --locked` — passed across the workspace; existing manual/performance tests remained intentionally ignored.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` — passed.
- No existing test was removed, weakened, or skipped to obtain these results.
- `.work/bin/work-view` is an x86-64 Linux executable in this checkout and cannot execute on the macOS host. Dependency readiness and child stages were therefore verified directly from item frontmatter; this did not block implementation or verification.

## Review correction verification (2026-07-18)

- **Exact encoder source identity**: `VideoEncodeFrame` now carries the exact `VideoSegmentSource` it materializes, and `VideoEncodeRequest` requires equality with the corresponding plan segment in addition to index, format, dimensions, and byte bounds. An adversarial two-frame test proves that swapping same-geometry inputs is rejected rather than encoded under false `FrameId` provenance.
- **Durable canonical timing**: plans now persist the ordered session time for every input frame and rebind every visible source segment to that exact `(FrameId, SessionTime)` pair. Constructor and serde validation enforce the 30-second source ceiling, 60-second presentation ceiling, source-kind/timing-basis matrix, real-time versus model policy compatibility, exact v1 terminal/tied/meaningful/gap holds, ordered coalesced gap ranges, and exact terminal-frame placement. Canonical timing constants moved into core so the root planner and durable validator share one authority.
- **Canonical gap evidence**: manifests now carry constructor-backed `VideoGapEvidence` for every retained gap intersecting the presented range. Construction derives the clipped, sorted evidence from the exact `ResolvedRange`; construction and deserialization independently coalesce it and require the plan's gap slates, ranges, and complete contributor IDs to match exactly. Adversarial tests reject substituted plan IDs, substituted evidence IDs, and omitted evidence.
- **Source-aligned schemas**: public video schemas now delegate to the strict `deny_unknown_fields` wire shapes. They publish exact plan/media versions, no-audio, array ceilings, presentation/output geometry limits, byte ceilings, bounded privacy-safe encoder labels, and exact lowercase SHA-256 patterns. Schema regression tests inspect generated output so relaxed refs or omitted constraints fail visibly.
- Focused verification passed: `cargo test -p krometrail-core --locked` (135 passed), `cargo test --bin krometrail video:: --locked` (7 passed), and `cargo clippy -p krometrail-core --all-targets --locked -- -D warnings`.
- Full verification passed: `cargo fmt --all -- --check`, `cargo check --workspace --all-targets --locked`, `cargo test --workspace --all-targets --locked`, and `cargo clippy --workspace --all-targets --locked -- -D warnings`. Only the pre-existing explicitly manual performance tests remained ignored.
- All four receiver-confirmed blockers are corrected within the existing clip-contract boundary. No FFmpeg, storage publication, MCP registration, selector-version binding, provider, or new adjacent scope was introduced. No new issue was parked.
- The shared modified `.work/bin/work-view` remained untouched and is excluded from this feature commit.
