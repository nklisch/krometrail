---
id: epic-agent-surface-simplification-bounded-temporal-bundles
kind: feature
stage: implementing
tags: [agent-ux, visual]
parent: epic-agent-surface-simplification
depends_on: [epic-agent-surface-simplification-response-detail]
release_binding: null
gate_origin: null
created: 2026-07-18
updated: 2026-07-18
---

# Bounded temporal bundle defaults

## Brief

Default `temporal_debug_bundle` generation to the one visual epoch containing the effective artifact anchor, with deterministic nearest/earlier selection when the anchor lies between spans. Preserve the full resolved range, gaps, and epoch provenance while generating at most the meaningful outputs for that selected epoch. Let agents explicitly request `all` epochs when investigating geometry transitions. Generic explicit artifact generation retains its all-epoch behavior.

Integrate with concise/expanded/full responses: concise publishes the primary handle/resource and exact outcome/epoch omission counts, expanded publishes every generated compact handle/resource, and full exposes complete structures. Do not read inline artifact bytes unless images were requested.

## Epic context

- Parent epic: `epic-agent-surface-simplification`
- Position in epic: temporal acquisition and presentation consumer of response detail

## Simplification opportunity

Delete frozen v1 bundle-policy version fields/tests, default epoch/output Cartesian work, singleton low-information generation, and default artifact read-then-discard I/O while retaining canonical retained-resource authority.

## Foundation references

- `docs/VISUAL-EVIDENCE.md` — Input Sequence and artifact provenance
- `docs/SPEC.md` — Temporal Query and Artifact Operations
- `docs/EVALUATION.md` — Condition D temporal bundle

## Design decisions

- **Public scope name**: `epochs: "anchor" | "all"`, default `anchor`; it describes visual-epoch acquisition directly.
- **Selection authority**: epoch selection happens after canonical frame validation/planning and before output-limit calculation, cache lookup, or generation. The retained descriptor keeps its original index.
- **Anchor outside spans**: choose the epoch with the smallest distance to the effective artifact anchor; equal distances choose the earlier epoch deterministically.
- **Generic generation**: `generate_artifacts` continues selecting all epochs. Only the debug-bundle service supplies anchor selection by default.
- **Singleton evidence**: the anchor epoch remains eligible even with one frame so the response can truthfully report available/limited evidence; generators decide their normal unavailable/degraded semantics rather than fabricating motion.

## Architectural choice

Three approaches were considered. Truncating MCP output would hide retained artifacts after paying all generation cost. Special-casing singleton outputs inside each generator would not bound multi-frame epoch fanout. The chosen approach adds one internal epoch-selection value to `ArtifactGenerationContext`, allowing the bundle to narrow planned epochs once while generic artifact requests retain all-epoch behavior. The response layer then projects only what was actually generated.

The trickiest unit is deterministic anchor selection because epoch descriptors are geometry partitions rather than explicit time ranges. Selection derives each plan span from its first and last frame session times, preserves the original descriptor index, and operates only after exact frame validation.

## Implementation Units

### Unit 1: Bundle epoch scope contract

**Files**: `crates/krometrail-core/src/debug_bundle.rs`, `src/debug_bundle/policy.rs`
**Story**: `epic-agent-surface-simplification-bounded-temporal-bundles-anchor-scope`

```rust
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BundleEpochScope {
    #[default]
    Anchor,
    All,
}

pub struct TemporalDebugBundleRequest {
    query: TemporalQueryRequest,
    caller_markers: Vec<ArtifactMarker>,
    orientation: OrientationPolicy,
    epochs: BundleEpochScope,
}

pub struct EffectiveBundlePolicy {
    pub artifact_anchor: SessionTime,
    pub epoch_scope: BundleEpochScope,
    // existing concrete policy fields; no version field
}
```

**Implementation notes**:
- Extend validated wire construction, getters, `into_parts`, and schema.
- Delete `TEMPORAL_DEBUG_BUNDLE_POLICY_VERSION`, `EffectiveBundlePolicy.version`, `policy_version`, and tests/comments that freeze the obsolete v1 bundle policy.
- Concrete generators, anchor, scope, failure policy, filters, and focus times remain truthful provenance.

**Acceptance criteria**:
- [ ] Omitted `epochs` validates to `anchor`; explicit `all` round-trips in generated schema and effective policy.
- [ ] No legacy policy-version field or alias remains.

### Unit 2: Select planned epochs before work

**Files**: `crates/krometrail-core/src/artifacts.rs`, `src/artifacts/service.rs`, `src/debug_bundle/service.rs`
**Story**: `epic-agent-surface-simplification-bounded-temporal-bundles-anchor-scope`

```rust
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ArtifactEpochSelection {
    #[default]
    All,
    Anchor(SessionTime),
}

pub struct ArtifactGenerationContext {
    pub deadline: Option<Instant>,
    pub cancellation: Option<Arc<dyn CancellationSignal>>,
    pub epoch_selection: ArtifactEpochSelection,
}

fn select_epoch_plans(
    plans: Vec<EpochPlan>,
    selection: ArtifactEpochSelection,
) -> Result<Vec<EpochPlan>>;
```

**Implementation notes**:
- Apply `select_epoch_plans` immediately after `validate_and_plan` and before `potential_outputs`.
- `Anchor` returns exactly one original plan. Empty plans remain the existing validation error.
- Bundle service maps `BundleEpochScope::Anchor` to the resolved effective artifact anchor and `All` to the default context.
- Update every explicit `ArtifactGenerationContext` literal; do not add a compatibility constructor.

**Acceptance criteria**:
- [ ] A five-epoch default bundle schedules at most the selected epoch's generator outputs.
- [ ] `epochs: all` produces all planned epochs with unchanged original indexes and complete range provenance.
- [ ] Direct generic artifact generation remains all-epoch by default.

### Unit 3: Progressive temporal response and lazy pixels

**Files**: `crates/krometrail-mcp/src/response.rs`, `crates/krometrail-mcp/src/registry.rs`, `plugin/skills/krometrail/SKILL.md`, `plugin/skills/krometrail/references/evidence.md`
**Story**: `epic-agent-surface-simplification-bounded-temporal-bundles-response`

```rust
fn project_temporal_bundle(
    bundle: TemporalDebugBundle,
    detail: ResponseDetail,
) -> Result<TemporalBundleProjection, ResponseInvariantError>;
```

**Implementation notes**:
- Concise returns the resolved range identity, gaps/degradations, primary artifact handle/resource, selected epoch count, available/unavailable outcome counts, and omitted outcome/resource counts.
- Expanded returns compact handles/resources for every generated outcome; full returns complete bundle structures.
- Move `read_inline_artifact` behind the explicit inline-image branch so default concise does no artifact byte read.
- Preserve every canonical resource URI for projected retained outputs at the appropriate expanded/full level; concise identifies exact omissions and the explicit expansion path.

**Acceptance criteria**:
- [ ] Concise default returns one primary retained artifact reference and bounded counts without reading inline bytes.
- [ ] Expanded/full expose every artifact generated by the selected epoch scope.
- [ ] Warnings, degradations, gaps, range identity, and resource authority are never mislabeled or silently lost.

### Unit 4: Foundation and evaluation alignment

**Files**: `docs/VISUAL-EVIDENCE.md`, `docs/SPEC.md`, `docs/ARCHITECTURE.md`, `docs/EVALUATION.md`, generated `docs/public/llms-full.txt`
**Story**: `epic-agent-surface-simplification-bounded-temporal-bundles-response`

**Implementation notes**:
- Document anchor-epoch default and explicit all-epoch geometry investigation.
- Keep the non-stretching visual-epoch rule and full-range/gap provenance.
- Regenerate public docs through `bun run docs:build`.

**Acceptance criteria**:
- [ ] Foundation and skill guidance describe the implemented scope and expansion vocabulary without historical policy prose.

## Implementation Order

1. `epic-agent-surface-simplification-bounded-temporal-bundles-anchor-scope`
2. `epic-agent-surface-simplification-bounded-temporal-bundles-response`

## Simplification

- Delete bundle policy version constants, fields, builders, exact-v1 comments, and byte-freeze tests.
- Remove default epoch/output Cartesian work and default inline artifact read-then-discard behavior.
- Replace compact/full temporal-specific public controls with the shared response detail vocabulary.

## Testing

- Add artifact-service two-epoch selection tests protecting pre-work narrowing, original indexes, nearest/earlier ties, and generic all default.
- Extend debug-bundle contract/service tests for default anchor and explicit all with full range/gap provenance.
- Replace legacy compact/full MCP tests with concise/expanded/full resource-count and no-inline-read assertions.
- Keep generator-level geometry incompatibility tests; this feature narrows epochs but never stretches them.

## Risks

Filtering after output-limit calculation would preserve the current failure and cost, so ordering is contract-critical. Concise resources must not imply that omitted generated artifacts do not exist; exact counts and expansion guidance prevent that ambiguity.
