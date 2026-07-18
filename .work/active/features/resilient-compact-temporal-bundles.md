---
id: resilient-compact-temporal-bundles
kind: feature
stage: implementing
tags: [agent-ux, visual]
parent: null
depends_on: [truthful-screencast-geometry]
release_binding: null
gate_origin: null
created: 2026-07-17
updated: 2026-07-17
---

# Resilient compact temporal bundles

## Brief

Make the default temporal debug bundle a reliable, context-sized investigation entry point at realistic capture sizes. A request extending slightly beyond the newest captured frame currently fails with `not_found` but provides no recovery or safe retry guidance. A gap-free 53-frame, 1200×705 CSS, DPR 2 interval currently resolves yet loses all default artifacts to the decoded-sequence memory limit. A successful 52-frame bundle can inline roughly 44,000 tokens of complete artifact manifests even though canonical resource links already support progressive provenance access.

Preserve the stable 1.x range, artifact, and provenance contracts while making capture-edge errors actionable, fitting default high-DPI bundle work within bounded resources, and projecting only context-sized artifact metadata inline. Full manifests and source evidence must remain available through canonical resources.

## Reproduced findings

- A click-relative request for 500 ms before and 6 seconds after extended about 1.06 seconds beyond the newest captured frame. `AllowPartial` still returned `not_found` with null recovery and `retry: never`; 4.9 seconds after succeeded.
- A 5.4-second, 53-frame DPR-2 interval returned `resource_limit_exceeded` for decoded sequence bytes with no recovery guidance.
- A successful five-second, 52-frame bundle generated nine resources but expanded to roughly 44,000 structured-response tokens because every full artifact manifest was repeated inline.

## Simplification opportunity

Use the retained-range resolver and canonical resource boundary as the two authorities: one place should describe the nearest valid time bounds, and one compact artifact projection should link to full persisted provenance instead of maintaining two equally verbose delivery paths.

## Design decisions

- **Never-captured bounds**: keep the specification's explicit failure rather than treating `AllowPartial` as permission to silently clamp future/never-captured time. Return the authoritative captured bounds and a safe adjusted-request recovery.
- **High-DPI work**: preserve bounded memory and all resolved source frames. Fit normalization against the combined request budget and admit the reproduced sequence under a larger but still fixed decoded ceiling; do not thin evidence or merely raise every memory limit.
- **Manifest delivery**: the default bundle projection is compact because the documented bundle is a progressive entry point. Full manifests move behind a canonical additive text resource; the compact handle retains identity, type, output geometry/hash, counts, and the manifest URI.
- **Compatibility rationale**: persisted manifests, generic artifact-generation results, artifact image resources, and request schemas remain valid. The verbose default bundle projection is corrected to the documented progressive contract while a manifest resource preserves complete data.
- **Dispatch rationale**: one read-only generic explorer mapped the range, artifact scheduler, bundle projection, and resource seams; the host verified the load-bearing source files.

## Architectural choice

Keep the range resolver, artifact service, and MCP presentation/resource boundary as separate authorities. Enrich never-captured range errors at the resolver; make `FitLimits` reserve memory using decoded+normalized+output estimates under fixed caps; and introduce a compact MCP-only bundle projection whose full provenance links resolve through the existing retained-artifact store.

Alternatives rejected:

- Silently clamping beyond captured bounds would conflate retention loss with evidence that never existed and contradict the range contract.
- Frame thinning would weaken difference-map completeness and require a new evidence-selection contract.
- Raising decoded and combined limits without making scale selection budget-aware would move the failure to the scheduler and permit avoidable memory pressure.
- Dropping inline manifests without a manifest resource would break progressive provenance access.

## Implementation Units

### Unit 1: Actionable captured-bound failures

**Story**: `resilient-compact-temporal-bundles-guide-captured-bounds`

**Files**: `crates/krometrail-core/src/timeline/range.rs`, `crates/krometrail-store/tests/temporal_queries.rs`

```rust
fn range_not_found(
    message: impl Into<String>,
    seed: &RangeSeed,
    requested: SessionRange,
    retained_bounds: Option<SessionRange>,
) -> KrometrailError;
```

**Implementation notes**:

- When requested bounds extend outside `FrameAvailability.retained_bounds` without an eviction intersection, include captured start/end session nanoseconds in recovery and set `RetryAdvice::AfterRecovery`.
- Recovery tells the caller to retry with a range contained by those bounds; context continues to carry the original requested range/session/target.
- Do not change `AllowPartial` eviction semantics or resolve a range that includes uncaptured time.

**Acceptance criteria**:

- [ ] A future-edge request fails `not_found` with exact captured bounds, concrete adjusted-request recovery, and retry-after-recovery advice.
- [ ] Retention eviction and require-complete cases retain their existing semantics.

### Unit 2: Budget-aware high-DPI artifact fitting

**Story**: `resilient-compact-temporal-bundles-fit-high-dpi`

**Files**: `src/artifacts/scheduler.rs`, `src/artifacts/epoch.rs`, `src/artifacts/generators.rs`, `src/artifacts/service.rs`, `src/artifacts/service_tests.rs`, `src/debug_bundle/error.rs`, `src/debug_bundle/tests.rs`

```rust
fn fit_scale(
    request: NormalizationRequest,
    epoch: &EpochPlan,
    limits: ArtifactWorkLimits,
    reserved_output_bytes: usize,
) -> Result<AnalysisScale>;

fn recoverable_artifact_limit(error: KrometrailError, range: &ResolvedRange) -> KrometrailError;
```

**Implementation notes**:

- Raise only `max_decoded_bytes` enough for the reproduced 717,408,000-byte sequence while leaving the 1 GiB combined cap fixed.
- Materialize `FitLimits` using remaining combined capacity after the epoch's decoded estimate and bounded output reservation, so the selected exact integer downscale makes the eventual scheduler reservation admissible.
- Keep all source frames and declared gaps. Cache identity already includes materialized normalization and adapter version; bump the adapter version if the canonical scale changes.
- If an interval still cannot fit, the bundle's per-artifact error must recommend shortening the interval or using progressive source-frame evidence and use retry-after-recovery rather than leaving recovery null.

**Acceptance criteria**:

- [ ] The reproduced 53-frame 2400×1410 sequence selects a bounded exact scale and generates default storyboard/difference-map evidence below the unchanged combined cap.
- [ ] Work above the fixed envelope fails before allocation with actionable recovery.
- [ ] Cache keys, manifests, and output dimensions reflect the effective scale deterministically.

### Unit 3: Compact bundle handles and canonical manifest resources

**Story**: `resilient-compact-temporal-bundles-project-manifests`

**Files**: `crates/krometrail-mcp/src/response.rs`, `crates/krometrail-mcp/src/resources.rs`, `crates/krometrail-mcp/src/server.rs`

```rust
#[derive(Serialize, JsonSchema)]
struct BundleArtifactHandle {
    artifact_id: ArtifactId,
    cache: ArtifactCacheDisposition,
    media_type: String,
    encoded_byte_len: u64,
    artifact_kind: ArtifactKind,
    evidence_class: EvidenceClass,
    source_frame_count: u32,
    selected_frame_count: u32,
    omitted_frame_count: u32,
    output_dimensions: PixelDimensions,
    output_hash: String,
    manifest_uri: String,
}

enum ResourceKind {
    Artifact,
    ArtifactManifest,
    SourceFrame,
}
```

**Implementation notes**:

- Project only temporal-debug-bundle artifact outcomes; generic `generate_artifacts` continues returning its existing full result.
- Preserve epoch/generator/status/error fields and replace each available bundle artifact's full manifest with the compact handle above.
- Add a canonical manifest URI adjacent to each artifact URI. Reading it retrieves the retained artifact through the same scope validation and returns the exact serialized full manifest as JSON text.
- Keep the primary inline image and image resource behavior unchanged. Resource links remain bounded descriptors.

**Acceptance criteria**:

- [ ] A nine-artifact bundle no longer repeats source-frame ID arrays and parameters inline.
- [ ] Every compact handle exposes enough identity and summary provenance for immediate reasoning and links to byte-equivalent retained full provenance.
- [ ] Cross-session/target artifact access remains rejected by the existing scope boundary.

## Implementation Order

1. Add captured-bound recovery.
2. Make fitting respect the fixed combined memory envelope and qualify the reproduced high-DPI sequence.
3. Add manifest resources and switch only the default bundle's MCP projection to compact handles.

## Simplification

- Keep one range authority and enrich its existing error rather than adding bundle-specific preflight.
- Reuse scheduler estimates for scale selection instead of maintaining a second hidden memory policy.
- Reuse retained artifact lookup for both image and manifest resources; avoid duplicating provenance storage.

## Testing

- Store/core range regressions protect exact recovery fields and unchanged retention behavior.
- Artifact service regression uses the reproduced physical dimensions/frame count and asserts reservation, deterministic scale, and successful outputs.
- MCP contract test bounds serialized structured content and round-trips every compact manifest URI to the full manifest.

## Risks

- A larger decoded ceiling increases potential resident memory, so the unchanged combined semaphore and scale-selection proof are release blockers, not optional optimization.
- Resource URI expansion is public and must use the canonical parser/builder rather than string concatenation.
- Compact projection must not affect generic artifact-generation consumers; tests should compare both surfaces explicitly.
