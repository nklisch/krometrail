---
id: epic-prove-temporal-advantage-deterministic-scoring-and-artifact-conditions-threshold-aggregation-and-status
kind: story
stage: done
tags: [testing, visual]
parent: epic-prove-temporal-advantage-deterministic-scoring-and-artifact-conditions
depends_on: [epic-prove-temporal-advantage-deterministic-scoring-and-artifact-conditions-structured-scorer-and-ground-truth]
release_binding: null
gate_origin: null
created: 2026-07-14
updated: 2026-07-14
---

# Aggregate dimensions and apply exact status thresholds

## Checkpoint

Add integer-only condition/family aggregation and the product-thesis threshold evaluator over
completed `TrialScore` records. This story owns threshold math and status precedence; it does not
collect captures, invoke models, or decide what an artifact means.

## Contract

Add `crates/temporal-evaluation/src/thresholds.rs` and aggregate helpers in `scoring.rs`:

```rust
pub const THRESHOLD_PROFILE_VERSION: &str = "temporal-thesis-thresholds-v1";

pub struct ExactRate { pub numerator: u32, pub denominator: u32 }
impl ExactRate {
    pub fn new(numerator: u32, denominator: u32) -> Result<Self, ContractError>;
    pub fn percentage_points(self) -> u16;
    pub fn at_least(self, other: Self, minimum_delta_pp: u16) -> bool;
    pub fn delta_at_most(self, other: Self, maximum_delta_pp: u16) -> bool;
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

Use `u128` cross multiplication and no floating-point or generic statistics framework. Aggregate
in the fixed A, B, C, D, E condition order, `ScoringDimensionId::ALL` dimension order, corpus family
order, and realized trial order. Pair comparisons by exact trial identity and source-interval
digest; a condition cannot borrow rows from a different interval, case, duration, repetition, or
ROI definition.

The v1 profile is the existing matrix's minimum ten interpretation rows per required family and
condition, at least 25 percentage points D-over-A defect-identification improvement, positive
D-over-A improvement in each required movement/flicker/layout family, D at least B on the same
paired trials, D's maximum source-frame tile count no greater than B's, and stable-control false
positives no more than ten percentage points above A. C and E get complete dimension aggregates;
E's progressive result is reported but cannot substitute for D. Stable controls are required by
the matrix registry.

## Status semantics

- `Pass` requires complete required rows, retained source/artifact traceability, minimum coverage,
  no unresolved dimension gaps, and every applicable threshold.
- `Fail` means complete decisive evidence measured below a threshold or a complete row answer is
  incorrect; it is never used for missing evidence.
- `Inconclusive` means gaps, eviction, corruption, unsupported outputs, insufficient rows, or
  incomplete pairing prevents a decisive result.
- `Blocked` means a required answer or precondition was not available.
- `Skipped` remains only for optional Linux Chromium and is valid only when every row in the
  manifest is also `Skipped`, each with its own `OptionalUnavailable` failure/recovery. Mixed row
  states are rejected rather than collapsed into the aggregate.

## Acceptance evidence

- [ ] Exact-rate constructors reject zero denominators, numerator overflow, and invalid numerators; repeated aggregation is stable across execution order.
- [ ] A–E dimension/family/control/tile aggregates preserve pass/fail/inconclusive/blocked/skipped distinctions and never drop incomplete rows.
- [ ] D-vs-A, family gains, D-vs-B, tile-budget, stable-false-positive, and E-report checks use the exact thresholds and same-trial/interval pairing.
- [ ] Complete below-threshold evidence is `Fail`; gaps, retention loss, corruption, missing rows, unauthorized inputs, and mixed skipped rows cannot become `Pass`.
- [ ] Threshold tests use synthetic structured scores only and make no capture, model-comprehension, platform, or product-thesis claim.

## Implementation evidence

- Added `thresholds.rs` with integer-only `ExactRate` comparisons using `u128` cross
  multiplication, the canonical v1 threshold profile, fixed-order dimension/family/condition
  aggregates, exact trial/source-interval pairing, tile-budget accounting, and threshold checks.
- Threshold assessment enforces D-over-A 25 percentage points overall, positive gains in each
  required family, D at least B on paired trials, D's tile budget no greater than B's, stable
  false-positive delta at most 10 percentage points, complete A-E coverage/retained traceability,
  and an E report that never substitutes for D.
- Status precedence preserves blocked, skipped, inconclusive, fail, and pass outcomes; mixed
  skipped rows/conditions are rejected. Gaps, partial retention, corrupt outputs, missing coverage,
  and pair mismatches remain non-decisive rather than passing.
- `TrialScore` now retains source-interval identity and bounded tile count so threshold pairing and
  budget checks do not infer provenance from condition IDs. No result records, CI qualification,
  browser capture, model invocation, network access, or visual algorithm was added.
- Added deterministic synthetic tests for exact boundaries, fixed ordering, complete pass/fail,
  missing coverage, pair mismatch, gap, retention, corruption, blocked, and mixed-skipped states.
- Verification passed with Rust 1.85: locked workspace fmt, check, test, and clippy gates.

## Ordering

This checkpoint depends on the corrected scorer. Canonical result records consume its ordered
aggregates and status/failure vocabulary next.
