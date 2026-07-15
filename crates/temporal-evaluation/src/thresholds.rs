use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    CaseFamily, ConditionEvidence, ConditionId, ConditionPackage, ContractError, DimensionOutcome,
    EvaluationStatus, EvidenceAvailability, FailureRecord, MatrixDefinition, RetentionState,
    RunFailureCode, ScoringDimensionId, TrialScore, UNIFORM_SOURCE_FRAME_SLOTS, privacy,
};

pub const THRESHOLD_PROFILE_VERSION: &str = "temporal-thesis-thresholds-v1";
const MAX_PERCENTAGE_POINTS: u16 = 100;

/// A bounded exact fraction. Rates are never represented as floating-point values.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExactRate {
    pub numerator: u32,
    pub denominator: u32,
}

impl ExactRate {
    pub fn new(numerator: u32, denominator: u32) -> crate::Result<Self> {
        if denominator == 0 {
            return Err(ContractError::new(
                "exact rate denominator must be non-zero",
            ));
        }
        if numerator > denominator {
            return Err(ContractError::new(
                "exact rate numerator must not exceed its denominator",
            ));
        }
        Ok(Self {
            numerator,
            denominator,
        })
    }

    pub fn validate(self) -> crate::Result<()> {
        Self::new(self.numerator, self.denominator).map(|_| ())
    }

    /// Deterministic floor of 100*numerator/denominator.
    pub fn percentage_points(self) -> u16 {
        debug_assert!(self.validate().is_ok());
        ((u64::from(self.numerator) * u64::from(MAX_PERCENTAGE_POINTS))
            / u64::from(self.denominator)) as u16
    }

    /// Returns whether `self` is at least `other` plus the exact percentage-point delta.
    pub fn at_least(self, other: Self, minimum_delta_pp: u16) -> bool {
        self.validate().is_ok()
            && other.validate().is_ok()
            && cross_multiply_delta(self, other, minimum_delta_pp, true)
    }

    /// Returns whether `self` is no more than `other` plus the exact percentage-point delta.
    pub fn delta_at_most(self, other: Self, maximum_delta_pp: u16) -> bool {
        self.validate().is_ok()
            && other.validate().is_ok()
            && cross_multiply_delta(self, other, maximum_delta_pp, false)
    }
}

impl<'de> Deserialize<'de> for ExactRate {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            numerator: u32,
            denominator: u32,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.numerator, wire.denominator).map_err(serde::de::Error::custom)
    }
}

fn cross_multiply_delta(
    self_rate: ExactRate,
    other: ExactRate,
    delta_pp: u16,
    at_least: bool,
) -> bool {
    let left = u128::from(self_rate.numerator)
        * u128::from(other.denominator)
        * u128::from(MAX_PERCENTAGE_POINTS);
    let right = u128::from(other.numerator)
        * u128::from(self_rate.denominator)
        * u128::from(MAX_PERCENTAGE_POINTS)
        + u128::from(delta_pp) * u128::from(self_rate.denominator) * u128::from(other.denominator);
    if at_least {
        left >= right
    } else {
        left <= right
    }
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

impl ThresholdProfile {
    pub fn canonical() -> Self {
        let matrix = MatrixDefinition::canonical();
        Self {
            version: THRESHOLD_PROFILE_VERSION.into(),
            minimum_trials_per_family_condition: 10,
            improvement_over_final_screenshot_pp: 25,
            bundle_vs_uniform_minimum_delta_pp: 0,
            stable_false_positive_delta_max_pp: 10,
            required_families: matrix.required_families,
            stable_controls_required: matrix.stable_controls_required,
        }
    }

    pub fn validate(&self) -> crate::Result<()> {
        if self.version != THRESHOLD_PROFILE_VERSION {
            return Err(ContractError::new(
                "threshold profile version is not the current v1 profile",
            ));
        }
        if self.minimum_trials_per_family_condition == 0 {
            return Err(ContractError::new(
                "threshold profile minimum family coverage must be non-zero",
            ));
        }
        for (value, label) in [
            (
                self.improvement_over_final_screenshot_pp,
                "final screenshot improvement threshold",
            ),
            (
                self.bundle_vs_uniform_minimum_delta_pp,
                "bundle versus uniform threshold",
            ),
            (
                self.stable_false_positive_delta_max_pp,
                "stable false-positive threshold",
            ),
        ] {
            if value > MAX_PERCENTAGE_POINTS {
                return Err(ContractError::new(format!(
                    "{label} must be at most 100 percentage points"
                )));
            }
        }
        if self.required_families != MatrixDefinition::canonical().required_families {
            return Err(ContractError::new(
                "threshold profile required families do not match the matrix registry",
            ));
        }
        if !self.stable_controls_required {
            return Err(ContractError::new("the v1 matrix requires stable controls"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TrialPair {
    pub trial_id: String,
    pub case_id: String,
    pub family: CaseFamily,
    pub source_interval_digest: String,
}

impl TrialPair {
    fn from_score(score: &TrialScore, family: CaseFamily) -> crate::Result<Self> {
        privacy::validate_trial_id(&score.trial_id, "aggregate trial id")?;
        privacy::validate_sha256(
            &score.source_interval_digest,
            "aggregate source interval digest",
        )?;
        Ok(Self {
            trial_id: score.trial_id.clone(),
            case_id: score.case_id.clone(),
            family,
            source_interval_digest: score.source_interval_digest.clone(),
        })
    }

    fn key(&self) -> (String, String, String, String) {
        (
            self.trial_id.clone(),
            self.case_id.clone(),
            format!("{:?}", self.family),
            self.source_interval_digest.clone(),
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DimensionAggregate {
    pub dimension_id: ScoringDimensionId,
    pub rate: Option<ExactRate>,
    pub inconclusive_rows: u32,
    pub not_applicable_rows: u32,
}

impl DimensionAggregate {
    pub fn validate(&self) -> crate::Result<()> {
        if let Some(rate) = self.rate {
            rate.validate()?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConditionAggregate {
    pub condition_id: ConditionId,
    pub trial_count: u32,
    pub decisive_trial_count: u32,
    /// Maximum source-frame tiles normalized against the eight-tile contract.
    pub source_frame_tile_count: ExactRate,
    pub dimensions: Vec<DimensionAggregate>,
    pub family_defect_rates: Vec<(CaseFamily, ExactRate)>,
    pub stable_false_positive_rate: Option<ExactRate>,
    /// Completeness status of the aggregate; answer correctness remains in dimension rates.
    pub status: EvaluationStatus,
    pub failure: Option<FailureRecord>,
    /// Exact pairing keys retained so later threshold checks cannot join different intervals.
    pub paired_trials: Vec<TrialPair>,
}

impl ConditionAggregate {
    pub fn validate(&self) -> crate::Result<()> {
        self.source_frame_tile_count.validate()?;
        let pairing_count = u32::try_from(self.paired_trials.len()).map_err(|_| {
            ContractError::new("condition aggregate pairing keys exceed u32 capacity")
        })?;
        if self.trial_count != pairing_count {
            return Err(ContractError::new(
                "condition aggregate trial count does not match its pairing keys",
            ));
        }
        if self.decisive_trial_count > self.trial_count {
            return Err(ContractError::new(
                "condition aggregate decisive trial count exceeds trial count",
            ));
        }
        if self.dimensions.len() != ScoringDimensionId::ALL.len()
            || self
                .dimensions
                .iter()
                .map(|dimension| dimension.dimension_id)
                .collect::<Vec<_>>()
                != ScoringDimensionId::ALL
        {
            return Err(ContractError::new(
                "condition aggregate dimensions must use the canonical registry order",
            ));
        }
        for dimension in &self.dimensions {
            dimension.validate()?;
        }
        let mut previous = None;
        for pair in &self.paired_trials {
            privacy::validate_trial_id(&pair.trial_id, "aggregate pairing trial id")?;
            privacy::validate_safe_text(
                &pair.case_id,
                "aggregate pairing case id",
                privacy::MAX_SHORT_TEXT,
            )?;
            privacy::validate_sha256(
                &pair.source_interval_digest,
                "aggregate pairing source interval digest",
            )?;
            let key = pair.key();
            if previous.as_ref().is_some_and(|previous| previous >= &key) {
                return Err(ContractError::new(
                    "aggregate pairing keys must be unique and canonicalized",
                ));
            }
            previous = Some(key);
        }
        let mut families = BTreeSet::new();
        let mut previous_family_rank = None;
        for (family, rate) in &self.family_defect_rates {
            if !families.insert(*family) {
                return Err(ContractError::new(
                    "aggregate family defect rates must be unique",
                ));
            }
            let family_rank = CaseFamily::ALL
                .iter()
                .position(|candidate| candidate == family)
                .expect("CaseFamily::ALL contains every family");
            if previous_family_rank.is_some_and(|previous| previous >= family_rank) {
                return Err(ContractError::new(
                    "aggregate family defect rates must use corpus family order",
                ));
            }
            previous_family_rank = Some(family_rank);
            rate.validate()?;
        }
        if let Some(rate) = self.stable_false_positive_rate {
            rate.validate()?;
        }
        validate_status_failure(self.status, self.failure.as_ref())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ThresholdCheck {
    pub observed_rate: Option<ExactRate>,
    pub reference_rate: Option<ExactRate>,
    pub tile_observed_rate: Option<ExactRate>,
    pub tile_reference_rate: Option<ExactRate>,
    pub tile_passed: Option<bool>,
    pub threshold_delta_pp: u16,
    pub passed: bool,
    pub status: EvaluationStatus,
    pub rationale_code: String,
    pub failure: Option<FailureRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FamilyThresholdCheck {
    pub family: CaseFamily,
    pub observed_rate: Option<ExactRate>,
    pub reference_rate: Option<ExactRate>,
    pub threshold_delta_pp: u16,
    pub passed: bool,
    pub status: EvaluationStatus,
    pub rationale_code: String,
    pub failure: Option<FailureRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ThresholdAssessment {
    pub final_vs_bundle: ThresholdCheck,
    pub required_family_improvements: Vec<FamilyThresholdCheck>,
    pub bundle_vs_uniform: ThresholdCheck,
    pub stable_false_positive_delta: ThresholdCheck,
    /// E is reported for visibility and traceability, but is never a gating substitute for D.
    pub progressive_report: ThresholdCheck,
    pub status: EvaluationStatus,
    pub failure: Option<FailureRecord>,
}

pub fn aggregate_condition(
    condition: ConditionId,
    scores: &[TrialScore],
    profile: &ThresholdProfile,
) -> crate::Result<ConditionAggregate> {
    profile.validate()?;
    let definition = crate::BenchmarkDefinition::canonical();
    if scores.is_empty() {
        return empty_aggregate(
            condition,
            EvaluationStatus::Blocked,
            Some(FailureRecord {
                code: RunFailureCode::InsufficientEvidence,
                phase: "threshold_aggregation".into(),
                reason: "no trial scores were supplied for the condition".into(),
                recovery: "provide the required structured trial scores".into(),
                retryable: true,
            }),
        );
    }

    let mut pairs = Vec::with_capacity(scores.len());
    let mut seen_trials = BTreeSet::new();
    let mut families = Vec::with_capacity(scores.len());
    for score in scores {
        score.validate()?;
        if score.condition_id != condition {
            return Err(ContractError::new(
                "trial score condition does not match aggregate condition",
            ));
        }
        let case = definition
            .case(&score.case_id)
            .ok_or_else(|| ContractError::new("aggregate score references an unknown case"))?;
        if !seen_trials.insert(&score.trial_id) {
            return Err(ContractError::new(
                "condition aggregate contains duplicate trial identities",
            ));
        }
        pairs.push(TrialPair::from_score(score, case.family)?);
        families.push(case.family);
    }
    pairs.sort_by_key(TrialPair::key);

    let dimensions = ScoringDimensionId::ALL
        .into_iter()
        .map(|dimension_id| aggregate_dimension(scores, dimension_id))
        .collect::<crate::Result<Vec<_>>>()?;
    let family_defect_rates = aggregate_family_defect_rates(scores, &families)?;
    let stable_false_positive_rate = aggregate_stable_false_positive_rate(scores, &families)?;
    let decisive_trial_count = scores
        .iter()
        .filter(|score| is_decisive_trial(score))
        .count()
        .count_as_u32("decisive trial count")?;
    let source_frame_tile_count = ExactRate::new(
        scores
            .iter()
            .map(|score| u32::from(score.source_frame_tile_count))
            .max()
            .unwrap_or(0),
        UNIFORM_SOURCE_FRAME_SLOTS as u32,
    )?;
    let has_skipped = scores
        .iter()
        .any(|score| score.status == EvaluationStatus::Skipped);
    if has_skipped {
        if scores
            .iter()
            .any(|score| score.status != EvaluationStatus::Skipped)
        {
            return Err(ContractError::new(
                "condition aggregate rejects mixed skipped trial rows",
            ));
        }
        if scores.iter().any(|score| {
            score.failure.as_ref().is_none_or(|failure| {
                failure.code != RunFailureCode::OptionalUnavailable || failure.recovery.is_empty()
            })
        }) {
            return Err(ContractError::new(
                "skipped trial rows require optional-unavailability failures and recovery",
            ));
        }
    }
    let status = aggregate_status(scores);
    let failure = aggregate_failure(status, scores);
    let aggregate = ConditionAggregate {
        condition_id: condition,
        trial_count: scores.len().count_as_u32("trial count")?,
        decisive_trial_count,
        source_frame_tile_count,
        dimensions,
        family_defect_rates,
        stable_false_positive_rate,
        status,
        failure,
        paired_trials: pairs,
    };
    aggregate.validate()?;
    Ok(aggregate)
}

pub fn assess_thresholds(
    aggregates: &[ConditionAggregate],
    packages: &[ConditionPackage],
    profile: &ThresholdProfile,
) -> crate::Result<ThresholdAssessment> {
    profile.validate()?;
    let aggregate_map = aggregate_map(aggregates)?;
    for package in packages {
        package.validate()?;
    }

    let statuses = ConditionId::ALL
        .into_iter()
        .filter_map(|condition| {
            aggregate_map
                .get(&condition)
                .map(|aggregate| aggregate.status)
        })
        .collect::<Vec<_>>();
    let has_skipped = statuses.contains(&EvaluationStatus::Skipped);
    if has_skipped
        && (aggregate_map.len() != ConditionId::ALL.len()
            || statuses
                .iter()
                .any(|status| *status != EvaluationStatus::Skipped))
    {
        return Err(ContractError::new(
            "threshold assessment rejects mixed skipped condition aggregates",
        ));
    }

    let final_vs_bundle = compare_dimension(
        aggregate_map.get(&ConditionId::DTemporalBundle).copied(),
        aggregate_map.get(&ConditionId::AFinalScreenshot).copied(),
        ScoringDimensionId::TransientDefectIdentification,
        profile.improvement_over_final_screenshot_pp,
        Comparison::AtLeast,
        packages,
        "bundle_over_final_screenshot",
    );
    let required_family_improvements = profile
        .required_families
        .iter()
        .copied()
        .map(|family| {
            compare_family(
                aggregate_map.get(&ConditionId::DTemporalBundle).copied(),
                aggregate_map.get(&ConditionId::AFinalScreenshot).copied(),
                family,
                1,
                packages,
            )
        })
        .collect::<Vec<_>>();
    let bundle_vs_uniform = compare_bundle_vs_uniform(
        aggregate_map.get(&ConditionId::DTemporalBundle).copied(),
        aggregate_map.get(&ConditionId::BUniformStoryboard).copied(),
        profile.bundle_vs_uniform_minimum_delta_pp,
        packages,
    );
    let stable_false_positive_delta = compare_stable_false_positive(
        aggregate_map.get(&ConditionId::DTemporalBundle).copied(),
        aggregate_map.get(&ConditionId::AFinalScreenshot).copied(),
        profile.stable_false_positive_delta_max_pp,
        packages,
    );
    let progressive_report = report_progressive(
        aggregate_map.get(&ConditionId::EProgressiveSource).copied(),
        profile,
        packages,
    );

    let required_checks_in_order = std::iter::once(check_status(&final_vs_bundle))
        .chain(required_family_improvements.iter().map(check_status_family))
        .chain(std::iter::once(check_status(&bundle_vs_uniform)))
        .chain(std::iter::once(check_status(&stable_false_positive_delta)))
        .collect::<Vec<_>>();
    let aggregate_status = aggregate_statuses(&aggregate_map);
    let coverage_status = coverage_status(&aggregate_map, profile, packages);
    let comparison_status = statuses_precedence(&required_checks_in_order);
    let status = statuses_precedence(&[aggregate_status, coverage_status, comparison_status]);
    let failure = assessment_failure(status, &aggregate_map, packages);
    Ok(ThresholdAssessment {
        final_vs_bundle,
        required_family_improvements,
        bundle_vs_uniform,
        stable_false_positive_delta,
        progressive_report,
        status,
        failure,
    })
}

fn empty_aggregate(
    condition: ConditionId,
    status: EvaluationStatus,
    failure: Option<FailureRecord>,
) -> crate::Result<ConditionAggregate> {
    let aggregate = ConditionAggregate {
        condition_id: condition,
        trial_count: 0,
        decisive_trial_count: 0,
        source_frame_tile_count: ExactRate::new(0, UNIFORM_SOURCE_FRAME_SLOTS as u32)?,
        dimensions: ScoringDimensionId::ALL
            .into_iter()
            .map(|dimension_id| DimensionAggregate {
                dimension_id,
                rate: None,
                inconclusive_rows: 0,
                not_applicable_rows: 0,
            })
            .collect(),
        family_defect_rates: Vec::new(),
        stable_false_positive_rate: None,
        status,
        failure,
        paired_trials: Vec::new(),
    };
    aggregate.validate()?;
    Ok(aggregate)
}

fn aggregate_map(
    aggregates: &[ConditionAggregate],
) -> crate::Result<BTreeMap<ConditionId, &ConditionAggregate>> {
    let mut map = BTreeMap::new();
    for aggregate in aggregates {
        aggregate.validate()?;
        if map.insert(aggregate.condition_id, aggregate).is_some() {
            return Err(ContractError::new(
                "threshold assessment contains duplicate condition aggregates",
            ));
        }
    }
    Ok(map)
}

fn aggregate_dimension(
    scores: &[TrialScore],
    dimension_id: ScoringDimensionId,
) -> crate::Result<DimensionAggregate> {
    let mut correct = 0u32;
    let mut incorrect = 0u32;
    let mut inconclusive = 0u32;
    let mut not_applicable = 0u32;
    for score in scores {
        let dimension = score
            .dimensions
            .iter()
            .find(|dimension| dimension.dimension_id == dimension_id)
            .ok_or_else(|| ContractError::new("trial score is missing a scoring dimension"))?;
        match dimension.outcome {
            DimensionOutcome::Correct => {
                correct = correct.checked_add(1).ok_or_else(|| {
                    ContractError::new("dimension aggregate correct count overflow")
                })?
            }
            DimensionOutcome::Incorrect => {
                incorrect = incorrect.checked_add(1).ok_or_else(|| {
                    ContractError::new("dimension aggregate incorrect count overflow")
                })?
            }
            DimensionOutcome::Inconclusive => {
                inconclusive = inconclusive.checked_add(1).ok_or_else(|| {
                    ContractError::new("dimension aggregate inconclusive count overflow")
                })?
            }
            DimensionOutcome::NotApplicable => {
                not_applicable = not_applicable.checked_add(1).ok_or_else(|| {
                    ContractError::new("dimension aggregate not-applicable count overflow")
                })?
            }
        }
    }
    let decisive = correct
        .checked_add(incorrect)
        .ok_or_else(|| ContractError::new("dimension aggregate decisive count overflow"))?;
    Ok(DimensionAggregate {
        dimension_id,
        rate: (decisive > 0)
            .then(|| ExactRate::new(correct, decisive))
            .transpose()?,
        inconclusive_rows: inconclusive,
        not_applicable_rows: not_applicable,
    })
}

fn aggregate_family_defect_rates(
    scores: &[TrialScore],
    families: &[CaseFamily],
) -> crate::Result<Vec<(CaseFamily, ExactRate)>> {
    let mut result = Vec::new();
    for family in CaseFamily::ALL {
        let mut correct = 0u32;
        let mut incorrect = 0u32;
        for (score, score_family) in scores.iter().zip(families) {
            if *score_family != family {
                continue;
            }
            let outcome =
                dimension_outcome(score, ScoringDimensionId::TransientDefectIdentification)?;
            match outcome {
                DimensionOutcome::Correct => {
                    correct = correct
                        .checked_add(1)
                        .ok_or_else(|| ContractError::new("family defect correct count overflow"))?
                }
                DimensionOutcome::Incorrect => {
                    incorrect = incorrect.checked_add(1).ok_or_else(|| {
                        ContractError::new("family defect incorrect count overflow")
                    })?
                }
                DimensionOutcome::Inconclusive | DimensionOutcome::NotApplicable => {}
            }
        }
        let decisive = correct
            .checked_add(incorrect)
            .ok_or_else(|| ContractError::new("family defect decisive count overflow"))?;
        if decisive > 0 {
            result.push((family, ExactRate::new(correct, decisive)?));
        }
    }
    Ok(result)
}

fn aggregate_stable_false_positive_rate(
    scores: &[TrialScore],
    families: &[CaseFamily],
) -> crate::Result<Option<ExactRate>> {
    let mut false_positives = 0u32;
    let mut decisive = 0u32;
    for (score, family) in scores.iter().zip(families) {
        if *family != CaseFamily::StableControl {
            continue;
        }
        match dimension_outcome(score, ScoringDimensionId::StableControlFalsePositive)? {
            DimensionOutcome::Correct => {
                decisive = decisive
                    .checked_add(1)
                    .ok_or_else(|| ContractError::new("stable-control decisive count overflow"))?
            }
            DimensionOutcome::Incorrect => {
                decisive = decisive
                    .checked_add(1)
                    .ok_or_else(|| ContractError::new("stable-control decisive count overflow"))?;
                false_positives = false_positives.checked_add(1).ok_or_else(|| {
                    ContractError::new("stable-control false-positive count overflow")
                })?;
            }
            DimensionOutcome::Inconclusive | DimensionOutcome::NotApplicable => {}
        }
    }
    (decisive > 0)
        .then(|| ExactRate::new(false_positives, decisive))
        .transpose()
}

fn dimension_outcome(
    score: &TrialScore,
    dimension_id: ScoringDimensionId,
) -> crate::Result<DimensionOutcome> {
    score
        .dimensions
        .iter()
        .find(|dimension| dimension.dimension_id == dimension_id)
        .map(|dimension| dimension.outcome)
        .ok_or_else(|| ContractError::new("trial score is missing a scoring dimension"))
}

fn is_decisive_trial(score: &TrialScore) -> bool {
    matches!(
        score.status,
        EvaluationStatus::Pass | EvaluationStatus::Fail
    ) && score
        .dimensions
        .iter()
        .all(|dimension| dimension.outcome != DimensionOutcome::Inconclusive)
}

fn aggregate_status(scores: &[TrialScore]) -> EvaluationStatus {
    let statuses = scores.iter().map(|score| score.status).collect::<Vec<_>>();
    if statuses.contains(&EvaluationStatus::Skipped) {
        return EvaluationStatus::Skipped;
    }
    if statuses.contains(&EvaluationStatus::Blocked) {
        return EvaluationStatus::Blocked;
    }
    if statuses.contains(&EvaluationStatus::Inconclusive)
        || scores.iter().any(|score| {
            score
                .dimensions
                .iter()
                .any(|dimension| dimension.outcome == DimensionOutcome::Inconclusive)
        })
    {
        return EvaluationStatus::Inconclusive;
    }
    // Incorrect answers are represented in the dimension rates. The aggregate remains a
    // complete evidence set; threshold checks decide whether those rates meet the thesis rules.
    EvaluationStatus::Pass
}

fn aggregate_failure(status: EvaluationStatus, scores: &[TrialScore]) -> Option<FailureRecord> {
    match status {
        EvaluationStatus::Pass => None,
        EvaluationStatus::Fail => Some(FailureRecord {
            code: RunFailureCode::Threshold,
            phase: "threshold_aggregation".into(),
            reason: "one or more complete trial scores are incorrect".into(),
            recovery: "inspect the complete structured trial scores".into(),
            retryable: false,
        }),
        EvaluationStatus::Inconclusive => Some(FailureRecord {
            code: score_failure_code(scores),
            phase: "threshold_aggregation".into(),
            reason: "one or more trial scores lack decisive evidence".into(),
            recovery: "retain complete source and artifact evidence before aggregating".into(),
            retryable: true,
        }),
        EvaluationStatus::Blocked => Some(FailureRecord {
            code: RunFailureCode::Unavailable,
            phase: "threshold_aggregation".into(),
            reason: "a required trial answer or precondition is unavailable".into(),
            recovery: "provide the required authorized trial input".into(),
            retryable: true,
        }),
        EvaluationStatus::Skipped => Some(FailureRecord {
            code: RunFailureCode::OptionalUnavailable,
            phase: "threshold_aggregation".into(),
            reason: "the optional condition is unavailable".into(),
            recovery: "run the optional condition when its environment is available".into(),
            retryable: true,
        }),
    }
}

fn score_failure_code(scores: &[TrialScore]) -> RunFailureCode {
    for code in [
        RunFailureCode::Authorization,
        RunFailureCode::CaptureGap,
        RunFailureCode::Retention,
        RunFailureCode::CorruptSource,
        RunFailureCode::Unavailable,
        RunFailureCode::InsufficientEvidence,
    ] {
        if scores
            .iter()
            .filter_map(|score| score.failure.as_ref())
            .any(|failure| failure.code == code)
        {
            return code;
        }
    }
    RunFailureCode::InsufficientEvidence
}

fn validate_status_failure(
    status: EvaluationStatus,
    failure: Option<&FailureRecord>,
) -> crate::Result<()> {
    if status == EvaluationStatus::Pass && failure.is_some() {
        return Err(ContractError::new(
            "passing condition aggregate cannot carry a failure",
        ));
    }
    if status != EvaluationStatus::Pass && failure.is_none() {
        return Err(ContractError::new(
            "non-passing condition aggregate requires a failure",
        ));
    }
    if let Some(failure) = failure {
        privacy::validate_safe_text(
            &failure.phase,
            "aggregate failure phase",
            privacy::MAX_SHORT_TEXT,
        )?;
        privacy::validate_safe_text(
            &failure.reason,
            "aggregate failure reason",
            privacy::MAX_LONG_TEXT,
        )?;
        privacy::validate_safe_text(
            &failure.recovery,
            "aggregate failure recovery",
            privacy::MAX_LONG_TEXT,
        )?;
    }
    Ok(())
}

fn coverage_status(
    aggregates: &BTreeMap<ConditionId, &ConditionAggregate>,
    profile: &ThresholdProfile,
    packages: &[ConditionPackage],
) -> EvaluationStatus {
    let statuses = ConditionId::ALL
        .into_iter()
        .map(|condition| {
            let Some(aggregate) = aggregates.get(&condition) else {
                return EvaluationStatus::Inconclusive;
            };
            if matches!(
                aggregate.status,
                EvaluationStatus::Blocked | EvaluationStatus::Skipped
            ) {
                return aggregate.status;
            }
            if aggregate_coverage_ready(aggregate, profile, packages) {
                aggregate.status
            } else {
                EvaluationStatus::Inconclusive
            }
        })
        .collect::<Vec<_>>();
    statuses_precedence(&statuses)
}

fn aggregate_coverage_ready(
    aggregate: &ConditionAggregate,
    profile: &ThresholdProfile,
    packages: &[ConditionPackage],
) -> bool {
    let family_counts = aggregate.paired_trials.iter().fold(
        BTreeMap::<CaseFamily, u32>::new(),
        |mut counts, pair| {
            let entry = counts.entry(pair.family).or_default();
            *entry = entry.saturating_add(1);
            counts
        },
    );
    let required_families_ready = profile.required_families.iter().all(|family| {
        family_counts.get(family).copied().unwrap_or_default()
            >= u32::from(profile.minimum_trials_per_family_condition)
    });
    let stable_ready = !profile.stable_controls_required
        || family_counts
            .get(&CaseFamily::StableControl)
            .copied()
            .unwrap_or_default()
            >= u32::from(profile.minimum_trials_per_family_condition);
    let dimensions_complete = aggregate
        .dimensions
        .iter()
        .all(|dimension| dimension.inconclusive_rows == 0);
    aggregate.trial_count > 0
        && aggregate.decisive_trial_count == aggregate.trial_count
        && required_families_ready
        && stable_ready
        && dimensions_complete
        && packages_cover(aggregate, packages)
}

fn package_is_traceable(package: &ConditionPackage) -> bool {
    if package.retention != RetentionState::Retained || !package.gap_ids.is_empty() {
        return false;
    }
    match &package.evidence {
        ConditionEvidence::FinalScreenshot {
            current_observation,
            ..
        } => current_observation.availability == EvidenceAvailability::Retained,
        ConditionEvidence::UniformStoryboard { .. } => true,
        ConditionEvidence::ChangeAwareStoryboard { artifacts } => {
            artifacts.iter().all(artifact_is_traceable)
        }
        ConditionEvidence::TemporalBundle(bundle) => bundle_is_traceable(bundle),
        ConditionEvidence::ProgressiveSource(evidence) => {
            bundle_is_traceable(&evidence.bundle)
                && evidence.source_retrievals.iter().all(|retrieval| {
                    retrieval.unavailable_frame_ids.is_empty()
                        && retrieval.returned_frames.iter().all(|reference| {
                            reference.availability == EvidenceAvailability::Retained
                        })
                })
                && evidence
                    .region_filmstrip
                    .as_ref()
                    .is_none_or(artifact_is_traceable)
        }
    }
}

fn artifact_is_traceable(artifact: &crate::ArtifactEvidenceReference) -> bool {
    artifact.output.availability == EvidenceAvailability::Retained
}

fn bundle_is_traceable(bundle: &crate::TemporalBundleEvidence) -> bool {
    bundle.bundle.availability == EvidenceAvailability::Retained
        && bundle.capture_summary.availability == EvidenceAvailability::Retained
        && bundle.context_summary.availability == EvidenceAvailability::Retained
        && bundle
            .evidence_references
            .iter()
            .all(|reference| reference.availability == EvidenceAvailability::Retained)
        && bundle.before_during_after.iter().all(artifact_is_traceable)
        && bundle.storyboards.iter().all(artifact_is_traceable)
        && bundle.difference_maps.iter().all(artifact_is_traceable)
}

fn packages_cover(aggregate: &ConditionAggregate, packages: &[ConditionPackage]) -> bool {
    aggregate.paired_trials.iter().all(|pair| {
        packages.iter().any(|package| {
            package.condition_id == aggregate.condition_id
                && package.source_interval_digest == pair.source_interval_digest
                && package_is_traceable(package)
        })
    })
}

#[derive(Clone, Copy)]
enum Comparison {
    AtLeast,
}

fn compare_dimension(
    observed: Option<&ConditionAggregate>,
    reference: Option<&ConditionAggregate>,
    dimension_id: ScoringDimensionId,
    threshold_delta_pp: u16,
    comparison: Comparison,
    packages: &[ConditionPackage],
    rationale: &'static str,
) -> ThresholdCheck {
    let Some(observed) = observed else {
        return unavailable_check("missing-observed-condition");
    };
    let Some(reference) = reference else {
        return unavailable_check("missing-reference-condition");
    };
    if let Some(status) = pair_status(observed, reference) {
        return status_check(status, status_rationale(status));
    }
    if !packages_cover(observed, packages) || !packages_cover(reference, packages) {
        return unavailable_check("missing-retained-traceability");
    }
    if !same_pairs(observed, reference) {
        return unavailable_check("trial-or-interval-pair-mismatch");
    }
    let observed_rate = dimension_rate(observed, dimension_id);
    let reference_rate = dimension_rate(reference, dimension_id);
    let passed = match (observed_rate, reference_rate, comparison) {
        (Some(observed), Some(reference), Comparison::AtLeast) => {
            observed.at_least(reference, threshold_delta_pp)
        }
        _ => false,
    };
    let status = if observed_rate.is_none() || reference_rate.is_none() {
        EvaluationStatus::Inconclusive
    } else if passed {
        EvaluationStatus::Pass
    } else {
        EvaluationStatus::Fail
    };
    ThresholdCheck {
        observed_rate,
        reference_rate,
        tile_observed_rate: None,
        tile_reference_rate: None,
        tile_passed: None,
        threshold_delta_pp,
        passed,
        status,
        rationale_code: rationale.into(),
        failure: check_failure(status, rationale),
    }
}

fn compare_family(
    observed: Option<&ConditionAggregate>,
    reference: Option<&ConditionAggregate>,
    family: CaseFamily,
    threshold_delta_pp: u16,
    packages: &[ConditionPackage],
) -> FamilyThresholdCheck {
    let missing = |rationale: &'static str, status: EvaluationStatus| FamilyThresholdCheck {
        family,
        observed_rate: None,
        reference_rate: None,
        threshold_delta_pp,
        passed: false,
        status,
        rationale_code: rationale.into(),
        failure: check_failure(status, rationale),
    };
    let Some(observed) = observed else {
        return missing("missing-observed-condition", EvaluationStatus::Inconclusive);
    };
    let Some(reference) = reference else {
        return missing(
            "missing-reference-condition",
            EvaluationStatus::Inconclusive,
        );
    };
    if let Some(status) = pair_status(observed, reference) {
        return missing(status_rationale(status), status);
    }
    if !packages_cover(observed, packages) || !packages_cover(reference, packages) {
        return missing(
            "missing-retained-traceability",
            EvaluationStatus::Inconclusive,
        );
    }
    if !same_family_pairs(observed, reference, family) {
        return missing(
            "trial-or-interval-pair-mismatch",
            EvaluationStatus::Inconclusive,
        );
    }
    let observed_rate = family_rate(observed, family);
    let reference_rate = family_rate(reference, family);
    let passed = matches!((observed_rate, reference_rate), (Some(observed), Some(reference)) if observed.at_least(reference, threshold_delta_pp));
    let status = if observed_rate.is_none() || reference_rate.is_none() {
        EvaluationStatus::Inconclusive
    } else if passed {
        EvaluationStatus::Pass
    } else {
        EvaluationStatus::Fail
    };
    FamilyThresholdCheck {
        family,
        observed_rate,
        reference_rate,
        threshold_delta_pp,
        passed,
        status,
        rationale_code: "required-family-improvement".into(),
        failure: check_failure(status, "required-family-improvement"),
    }
}

fn compare_bundle_vs_uniform(
    observed: Option<&ConditionAggregate>,
    reference: Option<&ConditionAggregate>,
    threshold_delta_pp: u16,
    packages: &[ConditionPackage],
) -> ThresholdCheck {
    let mut check = compare_dimension(
        observed,
        reference,
        ScoringDimensionId::TransientDefectIdentification,
        threshold_delta_pp,
        Comparison::AtLeast,
        packages,
        "bundle_over_uniform",
    );
    if matches!(
        check.status,
        EvaluationStatus::Pass | EvaluationStatus::Fail
    ) {
        let Some(observed) = observed else {
            unreachable!()
        };
        let Some(reference) = reference else {
            unreachable!()
        };
        let tile_passed = rate_at_most(
            observed.source_frame_tile_count,
            reference.source_frame_tile_count,
        );
        check.tile_observed_rate = Some(observed.source_frame_tile_count);
        check.tile_reference_rate = Some(reference.source_frame_tile_count);
        check.tile_passed = Some(tile_passed);
        check.passed &= tile_passed;
        if !tile_passed {
            check.status = EvaluationStatus::Fail;
            check.failure = check_failure(check.status, "bundle_tile_budget");
        }
        check.rationale_code = "bundle_over_uniform_and_tile_budget".into();
    }
    check
}

fn compare_stable_false_positive(
    observed: Option<&ConditionAggregate>,
    reference: Option<&ConditionAggregate>,
    threshold_delta_pp: u16,
    packages: &[ConditionPackage],
) -> ThresholdCheck {
    let Some(observed) = observed else {
        return unavailable_check("missing-observed-condition");
    };
    let Some(reference) = reference else {
        return unavailable_check("missing-reference-condition");
    };
    if let Some(status) = pair_status(observed, reference) {
        return status_check(status, status_rationale(status));
    }
    if !packages_cover(observed, packages) || !packages_cover(reference, packages) {
        return unavailable_check("missing-retained-traceability");
    }
    if !same_family_pairs(observed, reference, CaseFamily::StableControl) {
        return unavailable_check("trial-or-interval-pair-mismatch");
    }
    let observed_rate = observed.stable_false_positive_rate;
    let reference_rate = reference.stable_false_positive_rate;
    let passed = matches!((observed_rate, reference_rate), (Some(observed), Some(reference)) if observed.delta_at_most(reference, threshold_delta_pp));
    let status = if observed_rate.is_none() || reference_rate.is_none() {
        EvaluationStatus::Inconclusive
    } else if passed {
        EvaluationStatus::Pass
    } else {
        EvaluationStatus::Fail
    };
    ThresholdCheck {
        observed_rate,
        reference_rate,
        tile_observed_rate: None,
        tile_reference_rate: None,
        tile_passed: None,
        threshold_delta_pp,
        passed,
        status,
        rationale_code: "stable_false_positive_delta".into(),
        failure: check_failure(status, "stable_false_positive_delta"),
    }
}

fn report_progressive(
    aggregate: Option<&ConditionAggregate>,
    profile: &ThresholdProfile,
    packages: &[ConditionPackage],
) -> ThresholdCheck {
    let Some(aggregate) = aggregate else {
        return unavailable_check("missing-progressive-condition");
    };
    if aggregate.status == EvaluationStatus::Blocked {
        return unavailable_check("progressive-condition-blocked");
    }
    if aggregate.status == EvaluationStatus::Skipped {
        return unavailable_check("progressive-condition-skipped");
    }
    if aggregate.status == EvaluationStatus::Inconclusive
        || !aggregate_coverage_ready(aggregate, profile, packages)
    {
        return unavailable_check("progressive-condition-inconclusive");
    }
    let rate = dimension_rate(aggregate, ScoringDimensionId::TransientDefectIdentification);
    let status = if rate.is_some() {
        aggregate.status
    } else {
        EvaluationStatus::Inconclusive
    };
    ThresholdCheck {
        observed_rate: rate,
        reference_rate: None,
        tile_observed_rate: None,
        tile_reference_rate: None,
        tile_passed: None,
        threshold_delta_pp: 0,
        passed: status == EvaluationStatus::Pass,
        status,
        rationale_code: "reported_not_gating".into(),
        failure: check_failure(status, "reported_not_gating"),
    }
}

fn rate_at_most(observed: ExactRate, reference: ExactRate) -> bool {
    u128::from(observed.numerator) * u128::from(reference.denominator)
        <= u128::from(reference.numerator) * u128::from(observed.denominator)
}

fn dimension_rate(
    aggregate: &ConditionAggregate,
    dimension_id: ScoringDimensionId,
) -> Option<ExactRate> {
    aggregate
        .dimensions
        .iter()
        .find(|dimension| dimension.dimension_id == dimension_id)
        .and_then(|dimension| dimension.rate)
}

fn family_rate(aggregate: &ConditionAggregate, family: CaseFamily) -> Option<ExactRate> {
    aggregate
        .family_defect_rates
        .iter()
        .find(|(candidate, _)| *candidate == family)
        .map(|(_, rate)| *rate)
}

fn same_pairs(left: &ConditionAggregate, right: &ConditionAggregate) -> bool {
    pair_keys(left) == pair_keys(right)
}

fn same_family_pairs(
    left: &ConditionAggregate,
    right: &ConditionAggregate,
    family: CaseFamily,
) -> bool {
    pair_keys_for_family(left, family) == pair_keys_for_family(right, family)
}

fn pair_keys(aggregate: &ConditionAggregate) -> BTreeSet<(String, String, String, String)> {
    aggregate.paired_trials.iter().map(TrialPair::key).collect()
}

fn pair_keys_for_family(
    aggregate: &ConditionAggregate,
    family: CaseFamily,
) -> BTreeSet<(String, String, String, String)> {
    aggregate
        .paired_trials
        .iter()
        .filter(|pair| pair.family == family)
        .map(TrialPair::key)
        .collect()
}

fn pair_status(left: &ConditionAggregate, right: &ConditionAggregate) -> Option<EvaluationStatus> {
    let status = statuses_precedence(&[left.status, right.status]);
    (!matches!(status, EvaluationStatus::Pass | EvaluationStatus::Fail)).then_some(status)
}

fn status_rationale(status: EvaluationStatus) -> &'static str {
    match status {
        EvaluationStatus::Blocked => "paired-condition-blocked",
        EvaluationStatus::Skipped => "paired-condition-skipped",
        EvaluationStatus::Inconclusive => "paired-condition-inconclusive",
        EvaluationStatus::Pass | EvaluationStatus::Fail => "paired-condition-complete",
    }
}

fn unavailable_check(rationale: &'static str) -> ThresholdCheck {
    status_check(EvaluationStatus::Inconclusive, rationale)
}

fn status_check(status: EvaluationStatus, rationale: &'static str) -> ThresholdCheck {
    ThresholdCheck {
        observed_rate: None,
        reference_rate: None,
        tile_observed_rate: None,
        tile_reference_rate: None,
        tile_passed: None,
        threshold_delta_pp: 0,
        passed: false,
        status,
        rationale_code: rationale.into(),
        failure: check_failure(status, rationale),
    }
}

fn check_failure(status: EvaluationStatus, rationale: &str) -> Option<FailureRecord> {
    match status {
        EvaluationStatus::Pass => None,
        EvaluationStatus::Fail => Some(FailureRecord {
            code: RunFailureCode::Threshold,
            phase: "threshold_assessment".into(),
            reason: format!("threshold check failed: {rationale}"),
            recovery: "inspect the complete paired condition aggregates".into(),
            retryable: false,
        }),
        EvaluationStatus::Inconclusive => Some(FailureRecord {
            code: RunFailureCode::InsufficientEvidence,
            phase: "threshold_assessment".into(),
            reason: format!("threshold check is inconclusive: {rationale}"),
            recovery: "provide complete retained paired condition evidence".into(),
            retryable: true,
        }),
        EvaluationStatus::Blocked => Some(FailureRecord {
            code: RunFailureCode::Unavailable,
            phase: "threshold_assessment".into(),
            reason: format!("threshold check is blocked: {rationale}"),
            recovery: "provide the required authorized condition input".into(),
            retryable: true,
        }),
        EvaluationStatus::Skipped => Some(FailureRecord {
            code: RunFailureCode::OptionalUnavailable,
            phase: "threshold_assessment".into(),
            reason: format!("threshold check is skipped: {rationale}"),
            recovery: "run the optional condition when available".into(),
            retryable: true,
        }),
    }
}

fn check_status(check: &ThresholdCheck) -> EvaluationStatus {
    check.status
}

fn check_status_family(check: &FamilyThresholdCheck) -> EvaluationStatus {
    check.status
}

fn aggregate_statuses(map: &BTreeMap<ConditionId, &ConditionAggregate>) -> EvaluationStatus {
    let mut statuses = ConditionId::ALL
        .into_iter()
        .map(|condition| {
            map.get(&condition)
                .map_or(EvaluationStatus::Inconclusive, |aggregate| aggregate.status)
        })
        .collect::<Vec<_>>();
    if map.len() != ConditionId::ALL.len() {
        statuses.push(EvaluationStatus::Inconclusive);
    }
    statuses_precedence(&statuses)
}

fn statuses_precedence(statuses: &[EvaluationStatus]) -> EvaluationStatus {
    if statuses.contains(&EvaluationStatus::Blocked) {
        EvaluationStatus::Blocked
    } else if statuses.contains(&EvaluationStatus::Skipped) {
        EvaluationStatus::Skipped
    } else if statuses.contains(&EvaluationStatus::Inconclusive) {
        EvaluationStatus::Inconclusive
    } else if statuses.contains(&EvaluationStatus::Fail) {
        EvaluationStatus::Fail
    } else {
        EvaluationStatus::Pass
    }
}

fn assessment_failure(
    status: EvaluationStatus,
    aggregates: &BTreeMap<ConditionId, &ConditionAggregate>,
    packages: &[ConditionPackage],
) -> Option<FailureRecord> {
    let code = match status {
        EvaluationStatus::Pass => return None,
        EvaluationStatus::Fail => RunFailureCode::Threshold,
        EvaluationStatus::Blocked => RunFailureCode::Unavailable,
        EvaluationStatus::Skipped => RunFailureCode::OptionalUnavailable,
        EvaluationStatus::Inconclusive => aggregates
            .values()
            .filter_map(|aggregate| aggregate.failure.as_ref())
            .map(|failure| failure.code)
            .find(|code| {
                matches!(
                    code,
                    RunFailureCode::Authorization
                        | RunFailureCode::CaptureGap
                        | RunFailureCode::Retention
                        | RunFailureCode::CorruptSource
                        | RunFailureCode::Unavailable
                )
            })
            .or_else(|| {
                (!packages.iter().all(package_is_traceable)).then_some(RunFailureCode::Retention)
            })
            .unwrap_or(RunFailureCode::InsufficientEvidence),
    };
    let (reason, recovery, retryable) = match status {
        EvaluationStatus::Fail => (
            "complete paired evidence is below one or more v1 thresholds",
            "inspect the complete condition and family aggregates",
            false,
        ),
        EvaluationStatus::Blocked => (
            "a required condition answer or precondition is blocked",
            "provide the required authorized condition input",
            true,
        ),
        EvaluationStatus::Skipped => (
            "all condition rows are explicitly optional and skipped",
            "run the optional configuration when available",
            true,
        ),
        EvaluationStatus::Inconclusive | EvaluationStatus::Pass => (
            "coverage, pairing, traceability, or decisive evidence is incomplete",
            "retain complete paired source and artifact evidence",
            true,
        ),
    };
    Some(FailureRecord {
        code,
        phase: "threshold_assessment".into(),
        reason: reason.into(),
        recovery: recovery.into(),
        retryable,
    })
}

trait CountAsU32 {
    fn count_as_u32(self, label: &str) -> crate::Result<u32>;
}

impl CountAsU32 for usize {
    fn count_as_u32(self, label: &str) -> crate::Result<u32> {
        u32::try_from(self)
            .map_err(|_| ContractError::new(format!("{label} exceeds u32 count capacity")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_rates_use_cross_multiplication_at_boundaries() {
        let base = ExactRate::new(1, 3).unwrap();
        let improved = ExactRate::new(2, 3).unwrap();
        assert!(improved.at_least(base, 33));
        assert!(!improved.at_least(base, 34));
        assert!(improved.delta_at_most(base, 34));
        assert!(!improved.delta_at_most(base, 33));
        assert_eq!(ExactRate::new(1, 3).unwrap().percentage_points(), 33);
    }
}
