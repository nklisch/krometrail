---
id: epic-prove-temporal-advantage-deterministic-scoring-and-artifact-conditions-canonical-result-records-and-traceability
kind: story
stage: done
tags: [testing, visual, storage]
parent: epic-prove-temporal-advantage-deterministic-scoring-and-artifact-conditions
depends_on: [epic-prove-temporal-advantage-deterministic-scoring-and-artifact-conditions-threshold-aggregation-and-status]
release_binding: 1.0.0
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Emit canonical evaluation result records

## Checkpoint

Add the one current v1 scorer-result contract. It references the existing run manifest and exact
evidence authorities without copying a run manifest, temporal-vision provenance, artifact cache,
source payload, or browser record.

## Contract

Add `crates/temporal-evaluation/src/result.rs` with `EvaluationResultRecord`,
`TrialResultRecord`, `EvidenceLayer`, `ThesisEligibility`, and the single `NonClaimId` registry:

```rust
pub const RESULT_SCHEMA_VERSION: u16 = 1;
pub const RESULT_KIND: &str = "temporal_benchmark_evaluation_result";

pub fn EvaluationResultRecord::from_scores(
    manifest_input_digest: String,
    evidence_layer: EvidenceLayer,
    packages: &[ConditionPackage],
    scores: &[TrialScore],
    aggregates: Vec<ConditionAggregate>,
    thresholds: ThresholdAssessment,
) -> Result<Self, ContractError>;

impl EvaluationResultRecord {
    pub fn validate(&self) -> Result<(), ContractError>;
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ContractError>;
    pub fn digest(&self) -> Result<String, ContractError>;
}
```

Each trial record preserves trial/package/source-interval digests, ordered source IDs, gap and
retention state, accepted claim/evidence IDs, structured answer, exact raw-answer digest and
opaque ignored sidecar reference, output/manifest hashes, algorithm/version identities, and
existing cache keys. A record may reference only retained, source-linked evidence. It never stores
raw prose, image bytes, paths, URLs, page bodies, segment offsets, or a copied artifact manifest.

`RunManifest` remains the input/environment reproducibility authority; the result binds to its
`input_digest` and does not duplicate its browser/platform/model/capture configuration. The
`deterministic-ci` evidence layer sets `thesis_eligibility=NotEligible`, even if synthetic test
scores satisfy all arithmetic thresholds. Fixed non-claims cover no Chrome capture, network,
paid/model call, model comprehension, product-thesis, causal diagnosis, deterministic replay,
cross-model generalization, unobserved-frame claim, and ground-truth-from-artifact claim.

Generate `evaluation-result.schema.json` and a canonical `sample-evaluation-result.json`, update
the versioned evidence README, and keep all per-run results under ignored
`target/temporal-evaluation/`.

## Acceptance evidence

- [x] Generated schema/sample and repeated result canonicalization are byte-stable with unknown fields, unsorted semantic arrays, invalid hashes, duplicate IDs, and unsafe references rejected.
- [x] Every accepted claim traces through retained evidence to exact source IDs, gaps, retention, output/manifest hashes, algorithm/version, and cache identity.
- [x] Mixed intervals, status/threshold contradictions, unavailable evidence, and deterministic-CI attempts to claim thesis `Pass` fail validation.
- [x] Result round-trips preserve all dimension outcomes, aggregate rates, threshold checks, non-claims, and failure/recovery data without raw answer prose or bytes.
- [x] README explicitly separates deterministic scoring output from live capture, platform, model, debugging, and product-thesis evidence.

## Ordering

This checkpoint consumes the completed scorer and threshold aggregates. The final story adds fake
clock/source/store qualification and clean-checkout boundary verification.

## Implementation notes

- Execution capability: inline implementation; this checkpoint has one browser-agnostic Rust crate, one generator, and one versioned evidence directory with no independent runtime lane.
- Review weight: standard default; child-story checkpoints do not enter review.
- Files changed: `crates/temporal-evaluation/src/result.rs`, `src/lib.rs`, `src/bin/generate-evaluation-result.rs`, `Cargo.toml`, `tests/result.rs`, `src/scoring.rs`, `tests/thresholds.rs`, and the v1 result schema/sample/README.
- Tests added/removed: canonical result byte/digest and round-trip tests, unknown/unsafe/duplicate/unsorted/contradictory input tests, retained/unavailable trace checks, and generated-artifact integration checks; no tests were removed.
- Simplification: result records retain bounded trace projections and identities rather than copying manifests, configuration, artifact bytes, raw answer prose, paths, URLs, or cache storage; no browser/model/qualification lane was added.
- Discrepancies from design: added `EvidenceTraceRecord` as the named bounded projection needed to carry artifact output/manifest hashes, source IDs, algorithm/version identities, and cache identities without duplicating temporal-vision provenance; deterministic constructor keeps live/manual thesis eligibility inconclusive until their authorization lanes exist.
- Adjacent issues parked: none.
- Verification: Rust 1.85 locked workspace fmt, check, test, and clippy passed; all three versioned generators reproduced committed artifacts byte-for-byte.
