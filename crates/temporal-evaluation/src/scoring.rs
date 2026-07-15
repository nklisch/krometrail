use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    AcceptedClaim, AnswerRegion, AnswerTruth, AnswerValidationContext, ArtifactEvidenceReference,
    ConditionEvidence, ConditionId, ConditionPackage, ContractError, EvaluationStatus,
    EvidenceAvailability, EvidenceReference, EvidenceReferenceKind, FailureRecord,
    GroundTruthDefinition, Judgment, MotionBehavior, RetentionState, ScoringDimensionId,
    StateLabel, TrialIdentity, VIEWPORT_HEIGHT, VIEWPORT_WIDTH, canonical_json,
    parse_interpretation_answer, privacy, sha256_prefixed,
};

pub const SCORER_VERSION: &str = "temporal-evaluation-scorer-v1";
pub const MAX_RAW_ANSWER_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DimensionOutcome {
    Correct,
    Incorrect,
    Inconclusive,
    NotApplicable,
}

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

impl DimensionScore {
    fn new(
        dimension_id: ScoringDimensionId,
        outcome: DimensionOutcome,
        observed_value: impl Into<String>,
        expected_value: impl Into<String>,
        evidence_ids: &[String],
        rationale_code: &'static str,
    ) -> Self {
        Self {
            dimension_id,
            outcome,
            observed_value: observed_value.into(),
            expected_value: expected_value.into(),
            evidence_ids: evidence_ids.to_vec(),
            rationale_code: rationale_code.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TrialScore {
    pub trial_id: String,
    pub condition_id: ConditionId,
    pub case_id: String,
    pub answer: crate::InterpretationAnswer,
    pub answer_digest: String,
    pub raw_answer_ref: String,
    pub dimensions: Vec<DimensionScore>,
    pub accepted_claims: Vec<AcceptedClaim>,
    pub earned_points: u16,
    /// The count of decisive dimensions. Inconclusive and not-applicable dimensions are excluded.
    pub possible_points: u16,
    pub status: EvaluationStatus,
    pub failure: Option<FailureRecord>,
}

impl TrialScore {
    pub fn validate(&self) -> crate::Result<()> {
        privacy::validate_trial_id(&self.trial_id, "score trial id")?;
        privacy::validate_safe_text(&self.case_id, "score case id", privacy::MAX_SHORT_TEXT)?;
        privacy::validate_sha256(&self.answer_digest, "score answer digest")?;
        privacy::validate_opaque_id(&self.raw_answer_ref, "score raw answer reference")?;
        if self.dimensions.len() != ScoringDimensionId::ALL.len()
            || self
                .dimensions
                .iter()
                .map(|dimension| dimension.dimension_id)
                .collect::<Vec<_>>()
                != ScoringDimensionId::ALL
        {
            return Err(ContractError::new(
                "trial score dimensions must match the canonical registry order",
            ));
        }
        if self.earned_points > self.possible_points {
            return Err(ContractError::new(
                "trial score earned points exceed its denominator",
            ));
        }
        let expected_earned = self
            .dimensions
            .iter()
            .filter(|dimension| dimension.outcome == DimensionOutcome::Correct)
            .count() as u16;
        let expected_possible = self
            .dimensions
            .iter()
            .filter(|dimension| {
                matches!(
                    dimension.outcome,
                    DimensionOutcome::Correct | DimensionOutcome::Incorrect
                )
            })
            .count() as u16;
        if self.earned_points != expected_earned || self.possible_points != expected_possible {
            return Err(ContractError::new(
                "trial score points do not match dimension outcomes",
            ));
        }
        validate_claims(&self.accepted_claims)?;
        match (self.status, self.failure.is_some()) {
            (EvaluationStatus::Pass, false)
            | (EvaluationStatus::Fail, true)
            | (EvaluationStatus::Inconclusive, true) => {}
            (EvaluationStatus::Blocked | EvaluationStatus::Skipped, _) => {
                return Err(ContractError::new(
                    "a trial score cannot be blocked or skipped",
                ));
            }
            _ => {
                return Err(ContractError::new(
                    "trial score failure must agree with its status",
                ));
            }
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> crate::Result<Vec<u8>> {
        self.validate()?;
        canonical_json(self)
    }
}

/// Inputs to the pure interpretation scorer. The sidecar is deliberately not accepted as bytes:
/// it is only an opaque handle to an external, operator-controlled retention system.
pub struct ScoreInput<'a> {
    pub trial: &'a TrialIdentity,
    pub package: &'a ConditionPackage,
    pub truth: &'a GroundTruthDefinition,
    pub raw_answer: &'a [u8],
    pub raw_answer_ref: &'a str,
}

pub fn score_interpretation(input: ScoreInput<'_>) -> crate::Result<TrialScore> {
    // This check intentionally precedes JSON parsing. It bounds parser work and prevents the
    // ignored sidecar from becoming an unbounded raw-answer storage path.
    if input.raw_answer.len() > MAX_RAW_ANSWER_BYTES {
        return Err(ContractError::new(format!(
            "raw interpretation answer exceeds the {MAX_RAW_ANSWER_BYTES}-byte limit"
        )));
    }
    privacy::validate_opaque_id(input.raw_answer_ref, "raw interpretation answer reference")?;
    input.package.validate()?;
    input.truth.validate()?;
    validate_trial(input.trial, input.package.condition_id)?;

    let inventory = EvidenceInventory::from_package(input.package);
    let answer = parse_interpretation_answer(
        input.raw_answer,
        AnswerValidationContext {
            unresolved_capture_gap: inventory.has_capture_gap,
            missing_source: inventory.has_missing_source,
        },
    )?;
    validate_answer_region(&answer.affected_region)?;
    validate_answer_evidence(&answer.evidence_refs, &inventory)?;

    let dimensions = score_dimensions(input.package, input.truth, &answer, &inventory);
    let accepted_claims = accepted_claims(&dimensions, &answer.evidence_refs, answer.judgment)?;
    let earned_points = dimensions
        .iter()
        .filter(|dimension| dimension.outcome == DimensionOutcome::Correct)
        .count() as u16;
    let possible_points = dimensions
        .iter()
        .filter(|dimension| {
            matches!(
                dimension.outcome,
                DimensionOutcome::Correct | DimensionOutcome::Incorrect
            )
        })
        .count() as u16;
    let status = if dimensions
        .iter()
        .any(|dimension| dimension.outcome == DimensionOutcome::Incorrect)
    {
        EvaluationStatus::Fail
    } else if dimensions
        .iter()
        .any(|dimension| dimension.outcome == DimensionOutcome::Inconclusive)
    {
        EvaluationStatus::Inconclusive
    } else {
        EvaluationStatus::Pass
    };
    let failure = match status {
        EvaluationStatus::Pass => None,
        EvaluationStatus::Fail => Some(FailureRecord {
            code: crate::RunFailureCode::Threshold,
            phase: "interpretation_scoring".into(),
            reason: "one or more applicable dimensions are incorrect".into(),
            recovery: "review the structured answer against retained evidence".into(),
            retryable: false,
        }),
        EvaluationStatus::Inconclusive => Some(FailureRecord {
            code: inventory.failure_code(),
            phase: "interpretation_scoring".into(),
            reason: inventory.failure_reason().into(),
            recovery: inventory.failure_recovery().into(),
            retryable: true,
        }),
        EvaluationStatus::Blocked | EvaluationStatus::Skipped => unreachable!(),
    };
    let score = TrialScore {
        trial_id: input.trial.trial_id.clone(),
        condition_id: input.trial.condition_id,
        case_id: input.trial.case_id.clone(),
        answer,
        answer_digest: sha256_prefixed(input.raw_answer),
        raw_answer_ref: input.raw_answer_ref.into(),
        dimensions,
        accepted_claims,
        earned_points,
        possible_points,
        status,
        failure,
    };
    score.validate()?;
    Ok(score)
}

fn validate_trial(trial: &TrialIdentity, condition_id: ConditionId) -> crate::Result<()> {
    privacy::validate_trial_id(&trial.trial_id, "trial id")?;
    privacy::validate_safe_text(&trial.case_id, "trial case id", privacy::MAX_SHORT_TEXT)?;
    let definition = crate::BenchmarkDefinition::canonical();
    let case = definition
        .case(&trial.case_id)
        .ok_or_else(|| ContractError::new("trial references an unknown benchmark case"))?;
    if trial.family != case.family
        || !definition.supports_duration(trial.duration_ms)
        || trial.condition_id != condition_id
    {
        return Err(ContractError::new(
            "trial identity contradicts its condition or benchmark case",
        ));
    }
    Ok(())
}

fn validate_answer_region(region: &AnswerRegion) -> crate::Result<()> {
    let AnswerRegion::Rect {
        x,
        y,
        width,
        height,
    } = region
    else {
        return Ok(());
    };
    if *width == 0
        || *height == 0
        || *x > VIEWPORT_WIDTH
        || *y > VIEWPORT_HEIGHT
        || *width > VIEWPORT_WIDTH - *x
        || *height > VIEWPORT_HEIGHT - *y
    {
        return Err(ContractError::new(
            "answer affected_region must be a non-empty rectangle within the 800x450 viewport",
        ));
    }
    Ok(())
}

#[derive(Default)]
struct EvidenceInventory {
    known: BTreeSet<String>,
    retained: BTreeSet<String>,
    retained_source_frames: BTreeSet<String>,
    has_unavailable_evidence: bool,
    has_capture_gap: bool,
    has_missing_source: bool,
    has_historical_presentation: bool,
    has_retained_region_evidence: bool,
}

impl EvidenceInventory {
    fn from_package(package: &ConditionPackage) -> Self {
        let mut inventory = Self {
            has_capture_gap: !package.gap_ids.is_empty(),
            has_missing_source: package.retention != RetentionState::Retained,
            ..Self::default()
        };
        for id in &package.source_frame_ids {
            inventory.known.insert(id.clone());
        }
        for id in &package.gap_ids {
            inventory.known.insert(id.clone());
        }
        match &package.evidence {
            ConditionEvidence::FinalScreenshot {
                final_frame_id,
                current_observation,
            } => {
                inventory.known.insert(final_frame_id.clone());
                inventory.mark_source_retained(final_frame_id);
                inventory.mark_reference(current_observation);
                inventory.has_historical_presentation = false;
            }
            ConditionEvidence::UniformStoryboard { slot_frame_ids } => {
                for id in slot_frame_ids {
                    inventory.known.insert(id.clone());
                    inventory.mark_source_retained(id);
                }
                inventory.has_historical_presentation = slot_frame_ids.len() >= 2;
            }
            ConditionEvidence::ChangeAwareStoryboard { artifacts } => {
                for artifact in artifacts {
                    inventory.mark_artifact(artifact);
                }
                inventory.has_historical_presentation = inventory.retained_source_frames.len() >= 2;
            }
            ConditionEvidence::TemporalBundle(bundle) => {
                inventory.mark_bundle(bundle);
                inventory.has_historical_presentation = inventory.retained_source_frames.len() >= 2;
            }
            ConditionEvidence::ProgressiveSource(evidence) => {
                inventory.mark_bundle(&evidence.bundle);
                for retrieval in &evidence.source_retrievals {
                    inventory.known.insert(retrieval.request_id.clone());
                    inventory.mark_retained(&retrieval.request_id, false);
                    for id in &retrieval.requested_frame_ids {
                        inventory.known.insert(id.clone());
                    }
                    for id in &retrieval.unavailable_frame_ids {
                        inventory.known.insert(id.clone());
                        inventory.has_unavailable_evidence = true;
                    }
                    for reference in &retrieval.returned_frames {
                        inventory.mark_reference(reference);
                        inventory.mark_source_retained(&reference.id);
                    }
                }
                if let Some(filmstrip) = &evidence.region_filmstrip {
                    inventory.mark_artifact(filmstrip);
                }
                inventory.has_historical_presentation = inventory.retained_source_frames.len() >= 2;
            }
        }
        inventory.has_missing_source |= inventory.has_unavailable_evidence;
        inventory.has_retained_region_evidence = !inventory.retained.is_empty();
        inventory
    }

    fn mark_source_retained(&mut self, id: &str) {
        self.mark_retained(id, true);
    }

    fn mark_retained(&mut self, id: &str, source_frame: bool) {
        self.known.insert(id.into());
        self.retained.insert(id.into());
        if source_frame {
            self.retained_source_frames.insert(id.into());
        }
    }

    fn mark_reference(&mut self, reference: &EvidenceReference) {
        self.known.insert(reference.id.clone());
        if reference.availability == EvidenceAvailability::Retained {
            self.mark_retained(
                &reference.id,
                reference.kind == EvidenceReferenceKind::SourceFrame,
            );
        } else {
            self.has_capture_gap |= reference.availability == EvidenceAvailability::Gap;
            self.has_unavailable_evidence = true;
        }
    }

    fn mark_artifact(&mut self, artifact: &ArtifactEvidenceReference) {
        self.mark_reference(&artifact.output);
        for id in &artifact.source_frame_ids {
            self.known.insert(id.clone());
        }
        for id in &artifact.selected_frame_ids {
            self.mark_source_retained(id);
        }
        for id in &artifact.gap_ids {
            self.known.insert(id.clone());
        }
    }

    fn mark_bundle(&mut self, bundle: &crate::TemporalBundleEvidence) {
        self.mark_reference(&bundle.bundle);
        self.mark_reference(&bundle.capture_summary);
        self.mark_reference(&bundle.context_summary);
        for reference in &bundle.evidence_references {
            self.mark_reference(reference);
        }
        for artifact in bundle
            .before_during_after
            .iter()
            .chain(&bundle.storyboards)
            .chain(&bundle.difference_maps)
        {
            self.mark_artifact(artifact);
        }
    }

    fn limitation(&self) -> bool {
        self.has_capture_gap || self.has_missing_source
    }

    fn failure_code(&self) -> crate::RunFailureCode {
        if self.has_capture_gap {
            crate::RunFailureCode::CaptureGap
        } else if self.has_missing_source {
            crate::RunFailureCode::Retention
        } else {
            crate::RunFailureCode::InsufficientEvidence
        }
    }

    fn failure_reason(&self) -> &'static str {
        if self.has_capture_gap {
            "the source interval contains a declared capture gap"
        } else if self.has_missing_source {
            "a required source or artifact is not fully retained"
        } else {
            "the condition does not present enough historical evidence"
        }
    }

    fn failure_recovery(&self) -> &'static str {
        if self.has_capture_gap {
            "recapture a gap-free interval or retain a gap-aware answer"
        } else if self.has_missing_source {
            "retain the required source and artifact evidence before scoring"
        } else {
            "use a condition that presents the required temporal evidence"
        }
    }
}

fn validate_answer_evidence(
    references: &[String],
    inventory: &EvidenceInventory,
) -> crate::Result<()> {
    for reference in references {
        if !inventory.known.contains(reference) {
            return Err(ContractError::new(
                "interpretation answer cites an unknown evidence reference",
            ));
        }
        if !inventory.retained.contains(reference) {
            return Err(ContractError::new(
                "interpretation answer cites unavailable or non-retained evidence",
            ));
        }
    }
    Ok(())
}

fn score_dimensions(
    package: &ConditionPackage,
    truth: &GroundTruthDefinition,
    answer: &crate::InterpretationAnswer,
    inventory: &EvidenceInventory,
) -> Vec<DimensionScore> {
    let evidence = &answer.evidence_refs;
    vec![
        score_transient_defect(package, truth, answer, inventory, evidence),
        score_state_order(truth, answer, inventory, evidence),
        score_region(truth, answer, inventory, evidence),
        score_motion(truth, answer, inventory, evidence),
        score_gap_uncertainty(answer, inventory, evidence),
        score_stable_control(truth, answer, inventory, evidence),
    ]
}

fn score_transient_defect(
    package: &ConditionPackage,
    truth: &GroundTruthDefinition,
    answer: &crate::InterpretationAnswer,
    inventory: &EvidenceInventory,
    evidence: &[String],
) -> DimensionScore {
    let observed = format!(
        "temporary_state={};judgment={}",
        truth_label(answer.temporary_state),
        judgment_label(answer.judgment)
    );
    let expected = format!(
        "temporary_state={};judgment={}",
        truth_label(truth.temporary_state),
        judgment_label(truth.judgment)
    );
    if truth.judgment != Judgment::Defective {
        return DimensionScore::new(
            ScoringDimensionId::TransientDefectIdentification,
            DimensionOutcome::NotApplicable,
            observed,
            "not_applicable",
            evidence,
            "stable-control-separate-dimension",
        );
    }
    if inventory.limitation() {
        return DimensionScore::new(
            ScoringDimensionId::TransientDefectIdentification,
            DimensionOutcome::Inconclusive,
            observed,
            expected,
            evidence,
            "source-limitation",
        );
    }
    if !supports_truth_transition(truth, inventory)
        || matches!(package.evidence, ConditionEvidence::FinalScreenshot { .. })
    {
        return DimensionScore::new(
            ScoringDimensionId::TransientDefectIdentification,
            DimensionOutcome::Incorrect,
            observed,
            expected,
            evidence,
            "historical-presentation-missing",
        );
    }
    let outcome =
        if answer.temporary_state == truth.temporary_state && answer.judgment == truth.judgment {
            DimensionOutcome::Correct
        } else {
            DimensionOutcome::Incorrect
        };
    DimensionScore::new(
        ScoringDimensionId::TransientDefectIdentification,
        outcome,
        observed,
        expected,
        evidence,
        if outcome == DimensionOutcome::Correct {
            "exact-defect-identification"
        } else {
            "defect-identification-mismatch"
        },
    )
}

fn score_state_order(
    truth: &GroundTruthDefinition,
    answer: &crate::InterpretationAnswer,
    inventory: &EvidenceInventory,
    evidence: &[String],
) -> DimensionScore {
    let observed = state_order_label(&answer.state_order);
    let expected = state_order_label(&truth.state_order);
    let (outcome, rationale) = if inventory.limitation() {
        (DimensionOutcome::Inconclusive, "source-limitation")
    } else if !supports_truth_transition(truth, inventory) {
        (
            DimensionOutcome::Incorrect,
            "historical-presentation-missing",
        )
    } else if answer.state_order == truth.state_order {
        (DimensionOutcome::Correct, "exact-state-order")
    } else {
        (DimensionOutcome::Incorrect, "state-order-mismatch")
    };
    DimensionScore::new(
        ScoringDimensionId::StateOrder,
        outcome,
        observed,
        expected,
        evidence,
        rationale,
    )
}

fn score_region(
    truth: &GroundTruthDefinition,
    answer: &crate::InterpretationAnswer,
    inventory: &EvidenceInventory,
    evidence: &[String],
) -> DimensionScore {
    let observed = region_label(&answer.affected_region);
    let expected = region_label(&AnswerRegion::Rect {
        x: truth.affected_region.x,
        y: truth.affected_region.y,
        width: truth.affected_region.width,
        height: truth.affected_region.height,
    });
    let (outcome, rationale) = if !inventory.has_retained_region_evidence {
        (
            DimensionOutcome::Inconclusive,
            "region-evidence-unavailable",
        )
    } else if let AnswerRegion::Rect {
        x,
        y,
        width,
        height,
    } = answer.affected_region
    {
        if (x, y, width, height)
            == (
                truth.affected_region.x,
                truth.affected_region.y,
                truth.affected_region.width,
                truth.affected_region.height,
            )
        {
            (DimensionOutcome::Correct, "exact-viewport-pixel-roi")
        } else {
            (DimensionOutcome::Incorrect, "viewport-pixel-roi-mismatch")
        }
    } else {
        (DimensionOutcome::Incorrect, "viewport-pixel-roi-unknown")
    };
    DimensionScore::new(
        ScoringDimensionId::AffectedRegion,
        outcome,
        observed,
        expected,
        evidence,
        rationale,
    )
}

fn score_motion(
    truth: &GroundTruthDefinition,
    answer: &crate::InterpretationAnswer,
    inventory: &EvidenceInventory,
    evidence: &[String],
) -> DimensionScore {
    let observed = motion_label(answer.motion_behavior);
    let expected = motion_label(truth.motion_behavior);
    let (outcome, rationale) = if inventory.limitation() {
        (DimensionOutcome::Inconclusive, "source-limitation")
    } else if !supports_truth_transition(truth, inventory) {
        (
            DimensionOutcome::Incorrect,
            "historical-presentation-missing",
        )
    } else if answer.motion_behavior == truth.motion_behavior {
        (DimensionOutcome::Correct, "exact-motion-vocabulary")
    } else {
        (DimensionOutcome::Incorrect, "motion-vocabulary-mismatch")
    };
    DimensionScore::new(
        ScoringDimensionId::MotionBehavior,
        outcome,
        observed,
        expected,
        evidence,
        rationale,
    )
}

fn score_gap_uncertainty(
    answer: &crate::InterpretationAnswer,
    inventory: &EvidenceInventory,
    evidence: &[String],
) -> DimensionScore {
    let limitation = inventory.limitation();
    let observed = if answer.judgment == Judgment::Uncertain {
        "calibrated"
    } else if limitation {
        "overclaim"
    } else if answer.uncertainty_reasons.is_empty() {
        "not_applicable"
    } else {
        "underclaim"
    };
    let (outcome, rationale) = if limitation {
        (
            DimensionOutcome::Correct,
            "uncertainty-names-source-limitation",
        )
    } else if answer.judgment == Judgment::Uncertain || !answer.uncertainty_reasons.is_empty() {
        (DimensionOutcome::Incorrect, "unsupported-uncertainty")
    } else {
        (DimensionOutcome::NotApplicable, "no-source-limitation")
    };
    DimensionScore::new(
        ScoringDimensionId::GapUncertainty,
        outcome,
        observed,
        if limitation {
            "calibrated"
        } else {
            "not_applicable"
        },
        evidence,
        rationale,
    )
}

fn supports_truth_transition(truth: &GroundTruthDefinition, inventory: &EvidenceInventory) -> bool {
    inventory.has_historical_presentation
        && inventory.retained_source_frames.len() >= truth.state_order.len()
}

fn score_stable_control(
    truth: &GroundTruthDefinition,
    answer: &crate::InterpretationAnswer,
    inventory: &EvidenceInventory,
    evidence: &[String],
) -> DimensionScore {
    let observed = judgment_label(answer.judgment);
    if truth.judgment != Judgment::Intentional {
        return DimensionScore::new(
            ScoringDimensionId::StableControlFalsePositive,
            DimensionOutcome::NotApplicable,
            observed,
            "not_applicable",
            evidence,
            "defect-case-separate-dimension",
        );
    }
    if inventory.limitation() {
        return DimensionScore::new(
            ScoringDimensionId::StableControlFalsePositive,
            DimensionOutcome::Inconclusive,
            observed,
            "intentional",
            evidence,
            "source-limitation",
        );
    }
    let (outcome, rationale) = match answer.judgment {
        Judgment::Intentional => (DimensionOutcome::Correct, "stable-control-negative"),
        Judgment::Defective => (DimensionOutcome::Incorrect, "stable-control-false-positive"),
        Judgment::Uncertain => (DimensionOutcome::Inconclusive, "stable-control-uncertain"),
    };
    DimensionScore::new(
        ScoringDimensionId::StableControlFalsePositive,
        outcome,
        observed,
        "intentional",
        evidence,
        rationale,
    )
}

fn accepted_claims(
    dimensions: &[DimensionScore],
    evidence_ids: &[String],
    judgment: Judgment,
) -> crate::Result<Vec<AcceptedClaim>> {
    let mut claims = Vec::new();
    for dimension in dimensions {
        if dimension.outcome != DimensionOutcome::Correct
            || (dimension.dimension_id == ScoringDimensionId::GapUncertainty
                && judgment == Judgment::Uncertain)
        {
            continue;
        }
        if evidence_ids.is_empty() {
            return Err(ContractError::new(
                "accepted non-uncertain claims require retained evidence references",
            ));
        }
        claims.push(AcceptedClaim {
            claim_id: format!("dimension_{}", dimension_name(dimension.dimension_id)),
            evidence_ids: evidence_ids.to_vec(),
        });
    }
    Ok(claims)
}

fn validate_claims(claims: &[AcceptedClaim]) -> crate::Result<()> {
    let mut ids = BTreeSet::new();
    for claim in claims {
        privacy::validate_opaque_id(&claim.claim_id, "score claim id")?;
        if !ids.insert(&claim.claim_id) || claim.evidence_ids.is_empty() {
            return Err(ContractError::new(
                "score accepted claims must be unique and evidence-backed",
            ));
        }
        for evidence_id in &claim.evidence_ids {
            privacy::validate_opaque_id(evidence_id, "score claim evidence id")?;
        }
    }
    Ok(())
}

fn dimension_name(dimension: ScoringDimensionId) -> &'static str {
    match dimension {
        ScoringDimensionId::TransientDefectIdentification => "transient_defect_identification",
        ScoringDimensionId::StateOrder => "state_order",
        ScoringDimensionId::AffectedRegion => "affected_region",
        ScoringDimensionId::MotionBehavior => "motion_behavior",
        ScoringDimensionId::GapUncertainty => "gap_uncertainty",
        ScoringDimensionId::StableControlFalsePositive => "stable_control_false_positive",
    }
}

fn truth_label(value: AnswerTruth) -> &'static str {
    match value {
        AnswerTruth::Yes => "yes",
        AnswerTruth::No => "no",
        AnswerTruth::Uncertain => "uncertain",
    }
}

fn judgment_label(value: Judgment) -> &'static str {
    match value {
        Judgment::Defective => "defective",
        Judgment::Intentional => "intentional",
        Judgment::Uncertain => "uncertain",
    }
}

fn motion_label(value: MotionBehavior) -> &'static str {
    match value {
        MotionBehavior::Monotonic => "monotonic",
        MotionBehavior::Reversal => "reversal",
        MotionBehavior::Teleport => "teleport",
        MotionBehavior::Flicker => "flicker",
        MotionBehavior::LayoutShift => "layout_shift",
        MotionBehavior::None => "none",
        MotionBehavior::Uncertain => "uncertain",
    }
}

fn state_order_label(values: &[StateLabel]) -> String {
    let values = values
        .iter()
        .map(|value| match value {
            StateLabel::Baseline => "baseline",
            StateLabel::Changed => "changed",
            StateLabel::Final => "final",
            StateLabel::IntentionalMotion => "intentional_motion",
            StateLabel::Unknown => "unknown",
        })
        .collect::<Vec<_>>();
    format!("[{}]", values.join(","))
}

fn region_label(region: &AnswerRegion) -> String {
    match region {
        AnswerRegion::Unknown => "unknown".into(),
        AnswerRegion::Rect {
            x,
            y,
            width,
            height,
        } => format!("[{x},{y},{width},{height}]"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dimension_registry_order_is_explicit() {
        assert_eq!(
            ScoringDimensionId::ALL
                .into_iter()
                .map(dimension_name)
                .collect::<Vec<_>>(),
            vec![
                "transient_defect_identification",
                "state_order",
                "affected_region",
                "motion_behavior",
                "gap_uncertainty",
                "stable_control_false_positive",
            ]
        );
    }
}
