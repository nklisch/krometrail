---
id: epic-prove-temporal-advantage-deterministic-scoring-and-artifact-conditions-structured-scorer-and-ground-truth
kind: story
stage: implementing
tags: [testing, visual]
parent: epic-prove-temporal-advantage-deterministic-scoring-and-artifact-conditions
depends_on:
  - epic-prove-temporal-advantage-deterministic-scoring-and-artifact-conditions-condition-packaging-and-source-interval
  - epic-prove-temporal-advantage-benchmark-corpus-and-manifest-contracts-region-coordinate-and-skip-status-review-fix
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Score bounded answers against corrected hidden truth

## Checkpoint

Extend the current `temporal-evaluation` corpus with explicit evaluator-owned
`GroundTruthDefinition` values and implement the deterministic structured scorer. This story is
blocked until the benchmark-contract review fix is done: the scorer must consume the corrected
benchmark `Rect` contract, not reinterpret pre-fix ambiguous values.

## Ground truth and ROI semantics

The canonical definition gains one explicit truth record per case with temporary-state truth, state
order, fixed viewport-pixel ROI, motion behavior, and judgment. The ROI is always top-left origin,
integer half-open `[x,x+width) × [y,y+height)` in the captured 800×450 viewport image. It is not
CSS, DOM, canvas/logical, device-independent, post-scale source-image, or element geometry. The
upstream review fix aligns all values with actual fixture pixels and updates the generated
definition/schema; this story does not add a conversion heuristic or compatibility interpretation.

The committed truth values are explicit: defect cases use baseline/changed/final and defective;
stable smooth/loading/caret cases use intentional-motion ordering and intentional judgment; motion
vocabulary follows the case's authored behavior. No scorer branch derives truth from a case-ID
string, artifact pixels, visual measurements, or a model answer.

## Scorer contract

Add `crates/temporal-evaluation/src/scoring.rs` with:

```rust
pub const SCORER_VERSION: &str = "temporal-evaluation-scorer-v1";
pub const MAX_RAW_ANSWER_BYTES: usize = 16 * 1024;

pub fn score_interpretation(input: ScoreInput<'_>) -> Result<TrialScore, ContractError>;
```

`ScoreInput` contains one ordered trial, one validated `ConditionPackage`, the hidden truth, the
bounded raw answer bytes, and an opaque ignored sidecar reference. The scorer rejects oversize
bytes before parsing, uses the existing strict `parse_interpretation_answer`, hashes exact raw
bytes, retains only the bounded structured answer, and requires accepted non-uncertain claims to
cite retained evidence references. Unknown evidence IDs, unavailable/corrupt/evicted evidence,
unknown fields, malformed enum values, certainty across a gap, and unsafe sidecar references fail
at the boundary.

Produce one `DimensionScore` for each registry dimension and a `TrialScore` with exact accepted
claims, points, denominator, row status, failure/recovery, answer digest, and raw-answer handle.
Score region localization only by exact equality in the corrected fixed viewport-pixel ROI space;
there is no CSS conversion, clipping, IoU tolerance, pixel re-analysis, or statistics dependency.
Distinguish missing presentation from a source/retention gap: unsupported historical claims are
not treated as observations, and a gap-crossing claim is inconclusive rather than a false stable
result. Stable-control false positives and uncertainty calibration remain separate dimensions.

## Acceptance evidence

- [ ] The benchmark-contract review fix is a hard prerequisite and all 13 truth records use the corrected benchmark `Rect` as a fixed viewport-pixel ROI with no fallback to pre-fix ambiguity.
- [ ] Strict bounded answer parsing rejects unknown/oversized/malformed answers before scoring; raw prose is never canonical output.
- [ ] Dimension outcomes distinguish correct, incorrect, inconclusive, and not-applicable; gaps and retention loss cannot become negative visual evidence.
- [ ] Every accepted claim maps to retained source/artifact evidence, with source interval, gap, retention, manifest/output, algorithm/version, and cache identities still available to the later result record.
- [ ] Repeated scoring of identical package/truth/answer bytes is byte-stable and never calls Chrome, the network, a model, or a visual algorithm.

## Ordering

This story follows the package contract and the upstream ROI/skipped-manifest review fix. Threshold
aggregation may begin only after its `TrialScore` and dimension/status semantics are complete.
