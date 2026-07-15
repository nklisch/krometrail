---
id: epic-prove-temporal-advantage-deterministic-scoring-and-artifact-conditions
kind: feature
stage: implementing
tags: [testing, visual]
parent: epic-prove-temporal-advantage
depends_on:
  - epic-prove-temporal-advantage-benchmark-corpus-and-manifest-contracts
  - epic-prove-temporal-advantage-benchmark-corpus-and-manifest-contracts-region-coordinate-and-skip-status-review-fix
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Deterministic Scoring and Artifact Conditions

## Brief

Deliver the CI-safe evaluation harness for the five evidence conditions: final screenshot,
uniform storyboard, change-aware storyboard, temporal bundle, and progressive source access. It
consumes the committed corpus and ground truth, uses deterministic source sequences or fake
capture/storage ports where live infrastructure is unnecessary, and scores temporal-state recall,
ordering, region localization, reversal/direction description, uncertainty under gaps, false
defects on stable controls, and source-frame traceability. It also validates that artifact outputs
and manifests are reproducible for identical inputs and algorithm versions.

This feature owns condition packaging and the structured scorer, not model calls. It must prove
that the benchmark can distinguish the conditions and that a reported claim can be traced to exact
retained source identities without treating a generated artifact as ground truth. It does not turn
a green fake or synthetic run into a real-Chrome capture claim, a model-comprehension claim, or a
product-thesis pass.

## Epic context

- Parent epic: `epic-prove-temporal-advantage`
- Position in epic: deterministic evidence foundation — live collection and manual model lanes consume its condition and scoring contracts
- Depends on: `epic-prove-temporal-advantage-benchmark-corpus-and-manifest-contracts`

## Execution boundary

- Runs in ordinary locked Rust CI with no browser installation, network, paid Codex invocation, or external model.
- Uses explicit source-frame records, fake monotonic time, declared gaps, retention state, existing artifact cache metadata, and bounded evidence handles only at the boundaries already represented by the product contracts.
- The temporal-evaluation crate remains browser-agnostic and does not depend on `krometrail-core`, CDP, MCP, or the product binary. A narrow adapter snapshot converts existing core, temporal-vision, bundle, progressive, and store authorities into the evaluation package; it never replaces them.
- No product CLI command, runtime wiring, browser launcher, benchmark server, network client, model client, paid-work path, or generated VitePress documentation is added.

## Design decisions

- **One interval authority**: construct one immutable `SourceInterval` from the exact ordered source identities, source/observed/session times, capture ordinals, declared gaps, and retention truth. Every A–E package carries its digest and exact frame set. A condition cannot recapture, widen, re-resolve, infer missing frames, or substitute a different interval.
- **One ROI coordinate contract**: `affected_region` is a fixed viewport-pixel ROI: top-left origin `(0,0)`, integer half-open bounds `[x,x+width) × [y,y+height)`, measured in the captured 800×450 viewport image. It is not CSS, DOM, canvas/logical, device-independent, source-frame-after-scaling, or element geometry. The benchmark-contract review fix must clarify the existing benchmark `Rect` as one fixed viewport-pixel ROI and align every canonical region with actual fixture pixels before Unit 2 scoring begins. The scorer consumes only that corrected benchmark contract and never interprets pre-fix values itself.
- **Pure contract plus adapter snapshot**: `temporal-evaluation` owns validated condition/package/scoring/result values. An adapter supplies bounded projections of existing `ResolvedRange`, `temporal_vision::ArtifactManifest`, `ArtifactCacheMetadata`, `TemporalDebugBundle`, progressive handles, and store availability. The adapter projection is an evaluation reference, not a second artifact manifest or provenance authority.
- **Uniform baseline is intentionally simple**: B selects exactly eight distinct source-frame references by capture-order position `floor(i * (n - 1) / 7)` for `i=0..7`. It does not call `temporal-vision` measurement or selection and does not render a new storyboard algorithm. Fewer than eight retained frames is an explicit unavailable/inconclusive package, never duplicated tiles or interpolation.
- **Existing authorities remain authoritative**: C accepts only the existing change-aware storyboard manifest and its typed selection trace; D accepts the existing temporal debug-bundle result and exact nested artifact/context outcomes; E starts from D and records existing progressive source-frame and region-filmstrip results. The packager never regenerates, remeasures, parses free-form manifest parameters, or treats a bundle header as ground truth.
- **Hidden truth is extended in place**: add an evaluator-owned `GroundTruthDefinition` field to each existing `CaseDefinition` and regenerate the one current benchmark definition/schema. The field is withheld from condition packages and model-facing prompts. There is no second truth file, answer alias, legacy schema, migration reader, or compatibility shape.
- **Scoring is integer and descriptive**: score each bounded structured answer per dimension with exact integer points and count-based rates. Threshold comparisons use checked integer cross-multiplication and percentage-point rules; there are no confidence intervals, p-values, generic statistics dependencies, random resampling, or statistics framework.
- **Manifest and result are different authorities**: `RunManifest` remains the reproducibility/input/environment record. A new canonical `EvaluationResultRecord` is the scorer output and references the manifest input digest; it does not copy visual provenance, source bytes, cache tables, or environment fields. This is two contracts with distinct responsibilities, not two versions of one artifact format.
- **Skipped-run closure**: the benchmark-contract review fix tightens `Skipped` validation so a skipped manifest has every row explicitly `Skipped`, each row carries its own optional-unavailability failure, and no pass/fail/inconclusive row can be hidden inside an aggregate skip. The scorer and result validator consume that stricter rule and never normalize mixed row states.
- **Synthetic qualification is not thesis evidence**: deterministic CI can pass packaging, scorer, canonicalization, cache-identity, and status-contract checks. Its result layer is `deterministic-ci` and its thesis eligibility is explicitly `inconclusive`; only the later authorized live/model lanes can make capture, interpretation, platform, or thesis claims.
- **Prepublic one-contract policy**: all new contracts are current v1 surfaces. Remove superseded local shapes directly, deny unknown fields, and do not add aliases, migrations, fallback providers, or shims for unpublished consumers.

## Architectural choice

Three approaches were considered:

1. **A benchmark-only renderer and duplicated provenance model** would make B easy to visualize, but it would add a second visual algorithm, cache identity, gap model, and artifact manifest. It would make condition comparisons look deterministic while drifting from the product's artifacts. Rejected.
2. **Make `temporal-evaluation` call core/store/bundle/progressive services directly** would provide direct access to live authorities, but it would reverse the workspace dependency direction, pull runtime concerns into a browser-agnostic contract crate, and make ordinary CI depend on infrastructure. Rejected.
3. **A pure package/scorer contract with a narrow authority snapshot adapter (chosen)** keeps evaluation data reusable and CI-safe while forcing every later live/manual consumer to provide exact existing evidence identities. The adapter carries references and hashes, not bytes or copied provenance; existing services remain the only producers of artifacts, bundles, progressive reads, gaps, retention, and cache metadata.

## Tricky unit first: one source interval and honest claim support

The highest-risk unit is condition packaging, not arithmetic. A condition can appear to improve a score by receiving a different frame interval, a regenerated artifact, a gap hidden by a summary, or a cache hit whose source identity was not checked. The package validator therefore establishes the common interval first, then validates every evidence reference against it:

```text
ResolvedRange + exact source/store metadata
        │
        ▼
immutable SourceInterval (ordered IDs, clocks, ordinals, gaps, retention, digest)
        │
        ├── A: final retained source reference + current-observation handle
        ├── B: eight deterministic uniform source references
        ├── C: existing temporal-storyboard manifest/trace
        ├── D: existing bundle artifacts/context/capture summary
        └── E: D + bounded existing progressive retrieval records
```

A scorer derives claim support from the package and hidden case truth. A final-only package cannot
support a historical claim; a package with a gap crossing the relevant truth interval cannot
support a gap-free claim; an evicted or corrupt evidence reference cannot back an accepted claim.
The answer is still parsed and recorded, but the affected dimension is `inconclusive` or the
unsupported confident claim fails its uncertainty-calibration dimension. No missing evidence is
converted into a negative observation.

## Implementation Units

### Unit 1: source interval and exact A–E condition packaging

**Files**:

- `crates/temporal-evaluation/src/interval.rs` (new)
- `crates/temporal-evaluation/src/packaging.rs` (new)
- `crates/temporal-evaluation/src/conditions.rs` (extend the existing registry only)
- `crates/temporal-evaluation/src/lib.rs`
- `crates/temporal-evaluation/tests/conditions.rs` (new)

The pure package boundary is:

```rust
pub const CONDITION_PACKAGER_VERSION: &str = "temporal-condition-packager-v1";
pub const UNIFORM_SOURCE_FRAME_SLOTS: usize = 8;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceInterval {
    pub interval_id: String,
    pub session_scope: ScopeIdentity,
    pub requested_range: TimeRangeNs,
    pub resolved_range: TimeRangeNs,
    pub anchor_session_time_ns: u64,
    pub frames: Vec<SourceFrameEvidence>,
    pub gaps: Vec<GapEvidence>,
    pub retention: RetentionState,
    pub digest: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceFrameEvidence {
    pub id: String,
    pub capture_ordinal: u64,
    pub source_time_ns: Option<u64>,
    pub observed_time_ns: u64,
    pub session_time_ns: u64,
    pub encoded_sha256: String,
    pub availability: EvidenceAvailability,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GapEvidence {
    pub id: String,
    pub start_session_time_ns: u64,
    pub end_session_time_ns: u64,
    pub reason: String,
    pub estimated_missing_frames: Option<u64>,
}

impl SourceInterval {
    pub fn new(/* exact fields above */) -> Result<Self, ContractError>;
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ContractError>;
    pub fn digest(&self) -> Result<String, ContractError>;
    pub fn frame(&self, id: &str) -> Option<&SourceFrameEvidence>;
    pub fn has_unresolved_gap(&self) -> bool;
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EvidenceReference {
    pub id: String,
    pub kind: EvidenceReferenceKind,
    pub sha256: Option<String>,
    pub availability: EvidenceAvailability,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactEvidenceReference {
    pub output: EvidenceReference,
    pub manifest_sha256: String,
    pub source_frame_ids: Vec<String>,
    pub selected_frame_ids: Vec<String>,
    pub gap_ids: Vec<String>,
    pub algorithm_versions: Vec<NamedVersion>,
    pub cache: ArtifactCacheIdentity,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactCacheIdentity {
    pub cache_schema_version: u32,
    pub cache_key: String,
    pub source_fingerprint: String,
    pub parameter_hash: String,
    pub visual_epoch_hash: String,
    pub adapter_version: NamedVersion,
    pub generator: NamedVersion,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemporalBundleEvidence {
    pub bundle: EvidenceReference,
    pub before_during_after: Vec<ArtifactEvidenceReference>,
    pub storyboards: Vec<ArtifactEvidenceReference>,
    pub difference_maps: Vec<ArtifactEvidenceReference>,
    pub capture_summary: EvidenceReference,
    pub context_summary: EvidenceReference,
    pub evidence_references: Vec<EvidenceReference>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProgressiveRetrievalRecord {
    pub request_id: String,
    pub requested_frame_ids: Vec<String>,
    pub returned_frames: Vec<EvidenceReference>,
    pub unavailable_frame_ids: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProgressiveConditionEvidence {
    pub bundle: TemporalBundleEvidence,
    pub source_retrievals: Vec<ProgressiveRetrievalRecord>,
    pub region_filmstrip: Option<ArtifactEvidenceReference>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub enum ConditionEvidence {
    FinalScreenshot {
        final_frame_id: String,
        current_observation: EvidenceReference,
    },
    UniformStoryboard {
        slot_frame_ids: Vec<String>,
    },
    ChangeAwareStoryboard {
        artifacts: Vec<ArtifactEvidenceReference>,
    },
    TemporalBundle(TemporalBundleEvidence),
    ProgressiveSource(ProgressiveConditionEvidence),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConditionPackage {
    pub packager_version: String,
    pub condition_id: ConditionId,
    pub source_interval_digest: String,
    pub source_frame_ids: Vec<String>,
    pub gap_ids: Vec<String>,
    pub retention: RetentionState,
    pub evidence: ConditionEvidence,
    pub non_claims: Vec<NonClaimId>,
    pub digest: String,
}

pub struct ConditionPackager;
impl ConditionPackager {
    pub fn final_screenshot(
        interval: &SourceInterval,
        final_frame_id: &str,
        current_observation: EvidenceReference,
    ) -> Result<ConditionPackage, ContractError>;
    pub fn uniform_storyboard(
        interval: &SourceInterval,
    ) -> Result<ConditionPackage, ContractError>;
    pub fn change_aware_storyboard(
        interval: &SourceInterval,
        artifacts: Vec<ArtifactEvidenceReference>,
    ) -> Result<ConditionPackage, ContractError>;
    pub fn temporal_bundle(
        interval: &SourceInterval,
        bundle: TemporalBundleEvidence,
    ) -> Result<ConditionPackage, ContractError>;
    pub fn progressive_source(
        interval: &SourceInterval,
        evidence: ProgressiveConditionEvidence,
    ) -> Result<ConditionPackage, ContractError>;
}

pub fn require_one_source_interval(
    packages: &[ConditionPackage],
) -> Result<String, ContractError>;
```

`SourceInterval::new` requires non-empty opaque IDs, strict capture-ordinal ordering, nondecreasing
session time, separate source/observed/session clocks, canonical lowercase hashes, in-range
non-overlapping gaps, and a retention state that agrees with source availability. It does not
infer gaps from ordinal arithmetic. `ConditionPackage` requires the exact interval digest and
source IDs, validates every reference against the interval, preserves unavailable/corrupt/evicted
references instead of deleting them, and accepts only the fixed A–E condition registry.

The exact condition rules are:

- **A** has one final source-frame reference, chosen as the last retained frame in the interval,
  plus the bounded current-observation reference. It has no historical retrieval and makes no
  temporal claim from the final image.
- **B** has exactly eight distinct source-frame IDs from the same interval. For `n >= 8`, slot
  `i` is `frames[floor(i * (n - 1) / 7)]`, with source capture order as the only ordering input.
  It has no generated artifact, measurement, or cache identity. Fewer than eight frames is an
  explicit unavailable package.
- **C** accepts only existing temporal-vision storyboard/orientation manifest projections with
  `temporal-storyboard/1.1.0`, at most eight selected source-frame IDs per output, exact source
  IDs, exact manifest/output hashes, declared gaps, and the existing cache metadata. It does not
  call `select_storyboard_frames` or render a uniform alternative.
- **D** accepts the existing `TemporalDebugBundle` projection only when its resolved range equals
  the interval. It carries before/during/after, change-aware storyboard, difference-map,
  capture-summary, context, and evidence references in the existing bundle/artifact order. A
  partial or unavailable nested outcome remains explicit and makes affected claims incomplete.
- **E** starts with the exact D evidence and records at most two existing progressive source-frame
  requests, at most four requested frames per request, and at most one existing fixed-region
  filmstrip. Requested, returned, and unavailable IDs remain in request order. Returned frames
  and the region artifact must be retained, source-linked, and same-interval; no tracking,
  re-resolution, or logical-element claim is added.

The authority adapter creates these projections from the exact existing values: `ResolvedRange`,
`ArtifactManifest`/`ArtifactHandle`, `ArtifactCacheMetadata`, `TemporalDebugBundle`,
`SourceFrameHandle`/`SourceFrameBatch`, `RegionFilmstripEvidence`, and `RecordingStore` retention
truth. `ArtifactCacheIdentity` is a bounded record of existing cache metadata for validation; it
is not a cache key implementation or a new provenance format. No condition may accept a hand-made
algorithm/version/cache identity in place of those authority-derived values.

**Acceptance criteria**:

- [ ] A–E packages all have one byte-stable source interval digest and reject mixed ranges, reordered source identities, undeclared gaps, evicted/corrupt evidence presented as retained, and accepted claims backed by unavailable evidence.
- [ ] B's eight-slot formula is deterministic, integer-only, distinct, independent of temporal-vision selection, and explicitly incomplete below eight source frames.
- [ ] C verifies the existing storyboard descriptor, selected/source IDs, manifest/output hashes, gap IDs, cache key metadata, and version identity without a second selector or renderer.
- [ ] D preserves existing bundle/artifact/context outcomes and exact range; E preserves bounded progressive request/return/unavailable identities and fixed-region semantics.
- [ ] Package canonical bytes/digests are stable across repeated runs and do not contain image bytes, base64, paths, URLs, page text, ground truth, or model answers.

### Unit 2: hidden ground truth and bounded structured scorer

This unit is blocked until `epic-prove-temporal-advantage-benchmark-corpus-and-manifest-contracts-region-coordinate-and-skip-status-review-fix` is done. Scoring must consume the corrected benchmark `Rect` with fixed-viewport-pixel semantics; it must not read or reinterpret pre-fix values.

**Files**:

- `crates/temporal-evaluation/src/corpus.rs` (extend `CaseDefinition` in place)
- `crates/temporal-evaluation/src/scoring.rs` (new)
- `crates/temporal-evaluation/src/prompts.rs` (only answer-boundary helpers, if required)
- `crates/temporal-evaluation/src/lib.rs`
- `docs/evidence/temporal-evaluation/v1/benchmark-definition.json`
- `docs/evidence/temporal-evaluation/v1/benchmark-definition.schema.json`
- `crates/temporal-evaluation/tests/scoring.rs` (new)

Add this evaluator-owned field to the current corpus definition; it is never emitted in a
condition package or supplied to an evaluated model:

```rust
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GroundTruthDefinition {
    pub temporary_state: AnswerTruth,
    pub state_order: Vec<StateLabel>,
    pub affected_region: Rect,
    pub motion_behavior: MotionBehavior,
    pub judgment: Judgment,
}
```

Every one of the 13 cases gets an explicit value in the committed definition. The defect cases
use `temporary_state=yes`, `[baseline, changed, final]`, their corrected exact fixed-viewport-pixel
ROI, and `judgment=defective`; movement/path reversal use `reversal`, flicker variants and the
canvas sprite use `flicker`, layout variants use `layout_shift`, teleport uses `teleport`. The
stable smooth panel uses `[intentional_motion, final]`, `monotonic`, `intentional`; loading uses
`[intentional_motion, final]`, `none`, `intentional`; caret uses `[intentional_motion]`, `none`,
`intentional`. Tests assert every value is authored and validated, not derived from a case-ID
match in the scorer. The existing phase timeline, final state, corrected viewport-pixel ROI, and
intent remain independent corpus truth; the scorer never reads a captured pixel or generated
artifact as truth. The ROI contract is global and exact: coordinates are fixed captured viewport
pixels at the canonical 800×450 benchmark geometry, with no CSS/device-scale conversion inside
the scorer.

The scorer contract is:

```rust
pub const SCORER_VERSION: &str = "temporal-evaluation-scorer-v1";
pub const MAX_RAW_ANSWER_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum DimensionOutcome { Correct, Incorrect, Inconclusive, NotApplicable }

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DimensionScore {
    pub dimension_id: ScoringDimensionId,
    pub outcome: DimensionOutcome,
    pub observed_value: String,
    pub expected_value: String,
    pub evidence_ids: Vec<String>,
    pub rationale_code: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TrialScore {
    pub trial_id: String,
    pub condition_id: ConditionId,
    pub case_id: String,
    pub answer: InterpretationAnswer,
    pub answer_digest: String,
    pub raw_answer_ref: String,
    pub dimensions: Vec<DimensionScore>,
    pub accepted_claims: Vec<AcceptedClaim>,
    pub earned_points: u16,
    pub possible_points: u16,
    pub status: EvaluationStatus,
    pub failure: Option<FailureRecord>,
}

pub struct ScoreInput<'a> {
    pub trial: &'a TrialIdentity,
    pub package: &'a ConditionPackage,
    pub truth: &'a GroundTruthDefinition,
    pub raw_answer: &'a [u8],
    pub raw_answer_ref: &'a str,
}

pub fn score_interpretation(input: ScoreInput<'_>) -> Result<TrialScore, ContractError>;
```

`score_interpretation` rejects raw answers over 16 KiB before JSON parsing, uses the existing
`parse_interpretation_answer` with `deny_unknown_fields` and the package-derived gap/missing-source
context, computes the SHA-256 of the exact bounded raw bytes, and retains only the bounded
structured answer plus an opaque ignored sidecar reference. It rejects evidence references that
do not exist in the package or are not retained. A non-uncertain accepted claim must have at
least one retained source/artifact reference; an uncertain answer must cite the relevant
`capture_gap`, `missing_source`, or `insufficient_detail` reason.

The six dimensions use the one existing `ScoringDimensionId` registry:

- `transient_defect_identification` compares `judgment` with hidden truth when the package shows
  the relevant non-final state. A final-only or presentation-missing package records a complete
  answer but scores this dimension as `Incorrect` for a defect that was not identified; a known
  capture/retention gap crossing the truth interval makes it `Inconclusive` instead of silently
  treating the defect as absent.
- `state_order` requires the exact declared `state_order` after the package contains source
  evidence for every required visible truth state and no unresolved gap crosses a transition.
  Missing historical presentation is an incorrect condition result; missing retained source or a
  crossing gap is inconclusive.
- `affected_region` is `Correct` only for exact integer equality between the answer and the hidden
  fixed viewport-pixel ROI after the upstream contract fix. The answer uses the same top-left,
  half-open 800×450 pixel space; `unknown` or a mismatch is incorrect when the condition supplies
  retained region evidence. There is no CSS/logical-coordinate conversion, hidden IoU/tolerance,
  clipping, or statistics framework.
- `motion_behavior` requires exact vocabulary equality with hidden truth whenever the package
  contains enough retained selected/retrieved frames to support the truth transition; otherwise
  it follows the same unavailable-versus-gap distinction as state order.
- `gap_uncertainty` is applicable only when a gap, evicted source, corrupt source, or unavailable
  retrieval limits a claim. It is correct only when the answer is uncertain and names the exact
  limitation; confident overclaim is incorrect and unsupported caution on a complete package is
  underclaim. It is not a reason to pass a missing source.
- `stable_control_false_positive` is applicable only to stable-control truth. `judgment=defective`
  is a false positive; `intentional` is correct when supported, and `uncertain` is retained as a
  non-positive but not a positive diagnosis. Defect cases receive `NotApplicable` for this
  dimension.

Dimension points are one for `Correct`, zero for `Incorrect`, and omitted from the denominator for
`Inconclusive`/`NotApplicable`. The row is `Pass` only when all applicable dimensions are correct,
`Fail` when complete evidence has an incorrect applicable dimension, and `Inconclusive` when a
required evidence limitation prevents a decisive dimension. A missing answer/authorization is
`Blocked`; it is never represented by a fabricated empty answer.

**Acceptance criteria**:

- [ ] The upstream benchmark-contract review fix has aligned every canonical ROI with actual fixture geometry and exposes one fixed viewport-pixel, top-left, half-open 800×450 coordinate contract; the scorer has no fallback for the pre-fix ambiguous shape.
- [ ] The canonical definition contains explicit hidden truth for every case, generated schemas match, and model-facing condition packages/prompts contain no truth, case family, variant, or expected-answer metadata.
- [ ] Oversized, unknown-field, malformed, unbounded, or unsupported structured answers fail before scoring; raw answer bytes never enter canonical result records.
- [ ] Scoring distinguishes unavailable presentation from capture/retention gaps, does not convert gaps into stable observations, and requires retained evidence references for accepted claims.
- [ ] All six dimensions have deterministic exact outcomes, rationale codes, point/denominator rules, and no model, pixel-analysis, IoU framework, confidence interval, or causal-diagnosis dependency.
- [ ] Repeated scoring of identical package/truth/answer bytes produces identical structured answer digest, dimension scores, accepted claims, status, and canonical bytes.

### Unit 3: dimension rates, thesis thresholds, and status aggregation

**Files**:

- `crates/temporal-evaluation/src/thresholds.rs` (new)
- `crates/temporal-evaluation/src/scoring.rs` (aggregate helpers)
- `crates/temporal-evaluation/src/matrix.rs` (coverage helpers only)
- `crates/temporal-evaluation/src/lib.rs`
- `crates/temporal-evaluation/tests/thresholds.rs` (new)

Use exact count types and no floating-point aggregation:

```rust
pub const THRESHOLD_PROFILE_VERSION: &str = "temporal-thesis-thresholds-v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExactRate {
    pub numerator: u32,
    pub denominator: u32,
}

impl ExactRate {
    pub fn new(numerator: u32, denominator: u32) -> Result<Self, ContractError>;
    pub fn percentage_points(self) -> u16; // deterministic floor of 100*numerator/denominator
    pub fn at_least(self, other: Self, minimum_delta_pp: u16) -> bool;
    pub fn delta_at_most(self, other: Self, maximum_delta_pp: u16) -> bool;
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ThresholdProfile {
    pub version: String,
    pub minimum_trials_per_family_condition: u16,
    pub improvement_over_final_screenshot_pp: u16,
    pub bundle_vs_uniform_minimum_delta_pp: u16,
    pub stable_false_positive_delta_max_pp: u16,
    pub required_families: Vec<CaseFamily>,
    pub stable_controls_required: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DimensionAggregate {
    pub dimension_id: ScoringDimensionId,
    pub rate: Option<ExactRate>,
    pub inconclusive_rows: u32,
    pub not_applicable_rows: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConditionAggregate {
    pub condition_id: ConditionId,
    pub trial_count: u32,
    pub decisive_trial_count: u32,
    pub source_frame_tile_count: ExactRate,
    pub dimensions: Vec<DimensionAggregate>,
    pub family_defect_rates: Vec<(CaseFamily, ExactRate)>,
    pub stable_false_positive_rate: Option<ExactRate>,
    pub status: EvaluationStatus,
    pub failure: Option<FailureRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ThresholdAssessment {
    pub final_vs_bundle: ThresholdCheck,
    pub required_family_improvements: Vec<FamilyThresholdCheck>,
    pub bundle_vs_uniform: ThresholdCheck,
    pub stable_false_positive_delta: ThresholdCheck,
    pub progressive_report: ThresholdCheck, // reported only; never substitutes for D
    pub status: EvaluationStatus,
    pub failure: Option<FailureRecord>,
}

pub fn aggregate_condition(
    condition: ConditionId,
    scores: &[TrialScore],
    profile: &ThresholdProfile,
) -> Result<ConditionAggregate, ContractError>;

pub fn assess_thresholds(
    aggregates: &[ConditionAggregate],
    packages: &[ConditionPackage],
    profile: &ThresholdProfile,
) -> Result<ThresholdAssessment, ContractError>;
```

The canonical threshold profile is minimum ten interpretation rows per required family/condition,
25 percentage points improvement for D over A on defect identification, positive D-over-A gain in
each required movement, flicker, and transient-layout family, D defect-identification rate at
least B on the same paired trials, D's maximum source-frame tile count no greater than B's, and D
stable-control false-positive rate no more than ten percentage points above A. C and E receive full
dimension aggregates; E's progressive retrieval result is reported but does not substitute for D
in a thesis gate. The required family set and stable-control requirement come from the existing
matrix registry, not a scorer-local list.

Pairs are matched by the exact trial identity and source-interval digest, so a condition cannot
borrow a different case, repetition, duration, or interval. A threshold comparison is decisive
only when both sides have the minimum complete rows, retained source/artifact identities, no
unresolved gaps for the dimension, and the required family/control coverage. A complete below-
threshold comparison is `Fail`; missing rows, missing source, gaps, retention loss, corrupt
artifacts, unsupported condition output, or insufficient minimums are `Inconclusive`; a missing
required precondition/answer is `Blocked`; only the explicitly optional Linux Chromium row can be
`Skipped`. A skipped manifest is valid only when every row is also `Skipped` with its own explicit
optional-unavailability failure. `Pass` requires every applicable threshold and identity rule.

Aggregate ordering is fixed as A, B, C, D, E; dimensions use `ScoringDimensionId::ALL`; families
use the corpus family registry order; trial rows retain the realized benchmark order. No hash-map,
filesystem, host, or execution completion order may affect counts or output bytes. Threshold math
uses `u128` cross multiplication and rejects denominator zero/overflow. The scorer records no
confidence claim and no model generalization.

**Acceptance criteria**:

- [ ] Exact-rate constructors reject zero denominators, numerators above denominators, and overflow; repeated aggregation is byte/order-identical across hosts and worker completion order.
- [ ] A–E condition aggregates preserve dimension, family, stable-control, tile-budget, inconclusive, blocked, and optional-skipped distinctions.
- [ ] D-vs-A, per-required-family gain, D-vs-B, tile-count, stable-false-positive, and E-report checks use the exact stated thresholds and same-trial/source-interval pairing.
- [ ] Complete below-threshold data is `Fail`; incomplete, gapped, evicted, corrupt, or unauthorized data is never promoted to `Pass` or silently discarded, and a `Skipped` manifest rejects any row that is not `Skipped`.
- [ ] No generic statistics dependency/framework, random resampling, model invocation, browser/network operation, or product-thesis claim is introduced.

### Unit 4: canonical result records and traceability output

**Files**:

- `crates/temporal-evaluation/src/result.rs` (new)
- `crates/temporal-evaluation/src/lib.rs`
- `docs/evidence/temporal-evaluation/v1/evaluation-result.schema.json` (generated)
- `docs/evidence/temporal-evaluation/v1/sample-evaluation-result.json` (canonical contract sample)
- `docs/evidence/temporal-evaluation/v1/README.md` (narrow CI claim update)
- `crates/temporal-evaluation/tests/result.rs` (new)

The scorer output has one current canonical shape:

```rust
pub const RESULT_SCHEMA_VERSION: u16 = 1;
pub const RESULT_KIND: &str = "temporal_benchmark_evaluation_result";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum EvidenceLayer { DeterministicCi, LiveCapture, ManualModel }

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum ThesisEligibility { Eligible, NotEligible, Inconclusive }

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum NonClaimId {
    NoChromeCaptureClaim,
    NoNetworkClaim,
    NoPaidModelClaim,
    NoModelComprehensionClaim,
    NoProductThesisClaim,
    NoCausalDiagnosisClaim,
    NoDeterministicReplayClaim,
    NoCrossModelGeneralizationClaim,
    NoUnobservedFrameClaim,
    NoGroundTruthFromArtifactClaim,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TrialResultRecord {
    pub score: TrialScore,
    pub package_digest: String,
    pub source_interval_digest: String,
    pub source_frame_ids: Vec<String>,
    pub gap_ids: Vec<String>,
    pub retention: RetentionState,
    pub evidence_ids: Vec<String>,
    pub cache_keys: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EvaluationResultRecord {
    pub schema_version: u16,
    pub kind: String,
    pub benchmark_id: String,
    pub run_manifest_input_digest: String,
    pub scorer: ScorerIdentity,
    pub evidence_layer: EvidenceLayer,
    pub thesis_eligibility: ThesisEligibility,
    pub threshold_profile: ThresholdProfile,
    pub trials: Vec<TrialResultRecord>,
    pub conditions: Vec<ConditionAggregate>,
    pub thresholds: ThresholdAssessment,
    pub status: EvaluationStatus,
    pub non_claims: Vec<NonClaimId>,
    pub failure: Option<FailureRecord>,
}

impl EvaluationResultRecord {
    pub fn from_scores(/* manifest input digest, layer, packages, scores, aggregates */)
        -> Result<Self, ContractError>;
    pub fn validate(&self) -> Result<(), ContractError>;
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ContractError>;
    pub fn digest(&self) -> Result<String, ContractError>;
}
```

The record stores the bounded structured answer, exact raw-answer digest/opaque sidecar reference,
dimension outcomes, accepted claim-to-evidence IDs, package/source interval digests, gap and
retention state, artifact output IDs, manifest/output hashes, algorithm/version identities, and
existing cache keys needed to audit a score. It never stores raw model prose, image bytes,
filesystem paths, page bodies, URLs, endpoints, segment offsets, or a copied temporal-vision
manifest. A result cannot reference an evidence ID unless the package proves it is retained and
source-linked; it cannot claim a cache hit/output without the exact existing cache metadata
projection and manifest/output hash.

`EvaluationResultRecord` uses `run_manifest_input_digest` to bind the result to the existing
reproducibility manifest. It does not repeat browser/platform/model/capture configuration. The
`deterministic-ci` layer always includes the fixed non-claim registry above and sets
`thesis_eligibility=NotEligible`; even synthetic answers that make every arithmetic threshold pass
cannot produce a product-thesis `Pass`. Later live/manual features may use this exact result shape
with a different evidence layer and their own recorded authorization/availability metadata.

The generated schema and canonical sample are checked exactly like the existing definition and
run-manifest artifacts. The README states that CI may prove package/scorer/result determinism and
traceability only; it does not claim Chrome capture, artifact usefulness, model comprehension,
platform qualification, or thesis improvement.

**Acceptance criteria**:

- [ ] Result schema/sample generation is byte-stable, unknown fields are rejected, semantic arrays retain declared order, hashes are lowercase canonical SHA-256, and raw answers/private machine details cannot enter the record.
- [ ] Every accepted claim names retained evidence; every artifact record preserves source IDs, gaps, manifest/output hashes, algorithm/version, and exact cache identity without defining a second provenance/cache authority.
- [ ] Duplicate/missing trial IDs, mixed source intervals, status/threshold contradictions, unsupported `Pass`, unavailable evidence, and deterministic-CI thesis eligibility violations are rejected.
- [ ] A canonical result round-trip preserves dimension scores, threshold checks, gap/retention state, cache identities, non-claims, and failure/recovery semantics exactly.
- [ ] The README and sample explicitly distinguish deterministic contract/scoring output from live Chrome, platform, manual model, debugging, and product-thesis evidence.

### Unit 5: fake-clock/source qualification and strict CI boundary

**Files**:

- `crates/temporal-evaluation/tests/qualification.rs` (new)
- `crates/temporal-evaluation/tests/support/` (test-only deterministic source/authority records)
- `crates/krometrail-store/tests/temporal_evaluation_qualification.rs` (new thin store-seam checks)
- `crates/temporal-evaluation/tests/contracts.rs` (extend the existing CI boundary checks)
- `docs/evidence/temporal-evaluation/v1/README.md`

Qualification uses deterministic in-memory `SourceInterval` records, an injected step/fake
monotonic clock, the existing source-frame ordering/gap contracts, existing temporal-vision
artifact manifests and generator descriptors, and the existing `RecordingStore` progressive/
artifact/retention fixtures where a persistence seam is needed. It does not create a benchmark
store, benchmark renderer, fake Chrome, or alternate cache. Store checks may use temporary ignored
`target/temporal-evaluation/` directories and the existing coherent source/artifact/pinning
contracts; they never publish generated evidence into Git.

The focused CI suite proves:

1. the benchmark-contract review fix is complete before scoring tests run: every canonical ROI is a
   fixed viewport-pixel, top-left, half-open rectangle aligned to the actual 800×450 fixture
   geometry, and mixed skipped manifests are rejected because every row must be `Skipped`;
2. repeated interval construction with fixed IDs/times/ordinals/gaps/retention is canonical and
   independent of fake-clock call count, host wall clock, filesystem order, or task completion order;
3. all five packages share one interval digest and exactly the prescribed A–E evidence budgets;
4. B's uniform slots differ from C's existing change-aware selected IDs for a corpus sequence where
   the algorithms make different choices, without invoking a new image algorithm;
5. C's descriptor/manifest/output/cache identity checks reject one-field mutations in source
   content, source order, time, format, dimensions/scale epoch, parameters, cache schema,
   adapter version, generator version, manifest hash, or output hash;
6. D's package is exactly the existing bundle range and nested artifact/context outcomes, while E
   adds only bounded progressive retrieval records and never changes D's source interval;
7. gap, retention, missing-source, corrupt-output, unavailable-retrieval, and partial-bundle
   fixtures remain explicit `inconclusive`/`blocked` states and cannot score a confident claim as
   retained evidence;
8. exact answer/dimension/rate/threshold/result canonicalization is repeatable and output hashes
   survive reordering of parallel test execution; and
9. the dependency/output boundary contains no Chrome/network/model/paid-agent path, no new product
   command, no `docs/public/llms-full.txt` mutation, and no committed per-run source/artifact/raw
   answer output.

The test suite may use synthetic answers equal to hidden truth to test scorer arithmetic, but its
result is marked `deterministic-ci`, `thesis_eligibility=NotEligible`, and carries all strict
non-claims. A green fake source, fake clock, or existing store fixture proves only the contract,
packaging, scorer, provenance-reference, cache-identity, and status machinery. It cannot satisfy
the live capture envelope, platform matrix, model interpretation, or product-thesis thresholds.

**Acceptance criteria**:

- [ ] Locked Rust CI passes the deterministic package/scorer/result suite with no browser, network, paid model, or product CLI path and with no ignored-output comparison used as truth.
- [ ] Fake-clock/source/store seam tests prove normalized ordering, gap/retention propagation, no stale source/artifact claim, deterministic A–E packaging, and cache/version identity sensitivity.
- [ ] A clean checkout regenerates the definition/schema/run-manifest/result-schema/result-sample artifacts byte-for-byte; generated docs remain untouched.
- [ ] Every non-passing state records a bounded reason and recovery action; no unavailable, skipped, timeout, gap, eviction, corruption, or unauthorized input becomes a synthetic `Pass`.
- [ ] Rust 1.85 locked fmt/check/test/clippy gates are the only ordinary CI qualification; no performance stopwatch or model-quality assertion is added to this feature.

## Implementation Order

1. `epic-prove-temporal-advantage-deterministic-scoring-and-artifact-conditions-condition-packaging-and-source-interval` — immutable interval and exact A–E package validators; depends on `epic-prove-temporal-advantage-benchmark-corpus-and-manifest-contracts`.
2. `epic-prove-temporal-advantage-deterministic-scoring-and-artifact-conditions-structured-scorer-and-ground-truth` — explicit hidden truth and bounded answer scoring; depends on Unit 1.
3. `epic-prove-temporal-advantage-deterministic-scoring-and-artifact-conditions-threshold-aggregation-and-status` — exact rates, family/condition aggregates, and threshold decisions; depends on Unit 2.
4. `epic-prove-temporal-advantage-deterministic-scoring-and-artifact-conditions-canonical-result-records-and-traceability` — result schema/sample, evidence/cache/source trace, and CI-only eligibility; depends on Unit 3.
5. `epic-prove-temporal-advantage-deterministic-scoring-and-artifact-conditions-deterministic-ci-qualification-and-non-claims` — fake seams, clean generation checks, and strict non-claim boundary; depends on Unit 4.

These are sequential implementation checkpoints for one feature owner, not five worker
assignments. The shared write set and dependency chain favor one cohesive implementation bundle;
stories preserve the contract boundaries and evidence required for later live/manual features.

## Simplification

- Keep all condition IDs, artifact kinds, scoring dimensions, statuses, thresholds, and non-claims in existing or single new registries; do not re-enumerate them in tests or future adapters.
- Reuse `SourceInterval`/`ResolvedRange` identity, `temporal_vision::ArtifactManifest`, the existing storyboard trace, core artifact cache metadata, temporal debug bundle, progressive handles, and store retention truth. Do not add a visual algorithm, renderer, decoder, frame cache, cache table, manifest/provenance format, or gap inference rule.
- Keep `RunManifest` as the input/environment reproducibility authority and add only the distinct derived result contract; do not create a second run manifest or silently mutate old serialized shapes.
- Keep raw answers, artifacts, frames, logs, transcripts, and aggregate outputs under ignored `target/temporal-evaluation/`; commit only definitions, schemas, canonical samples, contract code, and tests.
- Leave the product CLI, MCP surface, browser transport, capture runtime, paid/manual lane, and foundation documents unchanged. No foundation assertion is contradicted by this design; implementation uses the code-first documentation rule if a later behavior makes one stale.

## Testing

- **Condition interface tests** protect one-interval identity, A–E shape/budget rules, uniform versus existing change-aware selection, bundle/progressive authority preservation, and unavailable evidence semantics.
- **Complex scorer tests** protect bounded answer parsing, hidden-truth projection, exact state/region/motion/judgment rules, evidence-reference requirements, and gap/retention calibration.
- **Aggregate tests** protect integer rates, exact percentage-point comparisons, family/control coverage, pair identity, status precedence, and no-pass-on-incomplete behavior.
- **Canonical result tests** protect schema generation, canonical bytes/digests, source/artifact/gap/cache traceability, raw-answer privacy, typed non-claims, and deterministic ordering.
- **Seam qualification** protects fake-clock independence, existing source/store/artifact/progressive contracts, cache invalidation sensitivity, and no Chrome/network/model/CLI path.
- No tests invoke a model, inspect a paid run, launch Chrome, render a new visual artifact, snapshot large images/SQL, or assert line coverage/trivial getters.

## Risks

- **Condition drift**: a future adapter could silently rebuild C/D/E from new algorithms. The package inputs require authority-derived manifest/cache/bundle/progressive identities and reject hand-authored substitutes; clean tests mutate each identity field.
- **ROI contract drift**: the old corpus coordinates may be logical fixture coordinates rather than captured pixels. The upstream review-fix story owns the correction and blocks scoring; this feature has no conversion heuristic or compatibility interpretation. A changed ROI contract requires a new benchmark-definition digest and result version.
- **Mixed skipped rows**: aggregate status could hide useful-looking rows under an optional skip. The upstream manifest fix and this result validator require every row to be `Skipped` and preserve row-level recovery reasons.
- **A/B evidence asymmetry**: the final screenshot and uniform slots could accidentally come from a different interval. One interval digest and same-trial pairing are mandatory, and A/B package tests include deliberately mismatched ranges.
- **Gaps and retention look like negative evidence**: claim support is dimension-specific; crossing gaps and missing sources yield inconclusive support, while confident unsupported answers fail uncertainty calibration. No scorer branch says “no defect observed” when the evidence is absent.
- **Cache metadata is not available in every compact bundle handle**: the adapter obtains it from the existing artifact store authority when constructing the package; if it cannot, the artifact remains unavailable/inconclusive instead of being assigned a fake key.
- **Exact region/state rules may be strict for future model answers**: strict equality is intentional for this deterministic contract and avoids an unrecorded tolerance/statistics framework. A later product decision can version the scoring vocabulary/profile rather than silently relaxing v1.
- **Synthetic threshold fixtures may be mistaken for thesis evidence**: result records carry an explicit evidence layer, `NotEligible` thesis state, fixed non-claims, and README language; CI tests assert that synthetic all-pass answers cannot produce a thesis claim.
- **Result records could become a second provenance surface**: they retain only references, hashes, source/gap/retention/cache identities, and scorer outcomes. Exact visual provenance remains the temporal-vision manifest and exact handles remain core/progressive authorities.

## Blockers

The prerequisite benchmark corpus/manifest feature has implemented, schema-backed condition,
prompt, scoring, status, matrix, and run-manifest contracts, but its review-fix story must first
align ROI semantics and tighten skipped-row closure. The current temporal-vision artifact
APIs/tests, typed storyboard trace, artifact cache metadata, bundle contract, progressive handles,
and store/fake seams provide the authorities this design consumes. No external research, model
authorization, browser installation, network access, or schema migration is required for this
CI-safe feature.
