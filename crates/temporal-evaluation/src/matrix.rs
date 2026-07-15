use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{CaseDefinition, CaseFamily, ContractError, Result};

pub const MATRIX_SEED: u64 = 0x4b524f4d45545241;
pub const CAPTURE_REPETITIONS: u16 = 30;
pub const INTERPRETATION_REPETITIONS: u16 = 10;
pub const LIVE_QUALIFICATION_PROFILE: &str = "live-qualification-v1";

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EvaluationStatus {
    Pass,
    Fail,
    Inconclusive,
    Blocked,
    Skipped,
}

impl EvaluationStatus {
    pub const ALL: [Self; 5] = [
        Self::Pass,
        Self::Fail,
        Self::Inconclusive,
        Self::Blocked,
        Self::Skipped,
    ];

    /// Returns the status that must win when independent evidence layers disagree.
    pub const fn precedence(self) -> u8 {
        match self {
            Self::Pass => 0,
            Self::Fail => 1,
            Self::Inconclusive => 2,
            Self::Skipped => 3,
            Self::Blocked => 4,
        }
    }
}

/// The qualification gate registry is the sole source of gate identity and order.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QualificationGateId {
    CaptureEnvelope,
    TimingIntegrity,
    MovementSequence,
    ControlReliability,
    Retention,
    Recovery,
    ResourceUsage,
    TemporalQueryLatency,
    ArtifactLatency,
    Cleanup,
}

impl QualificationGateId {
    pub const ALL: [Self; 10] = [
        Self::CaptureEnvelope,
        Self::TimingIntegrity,
        Self::MovementSequence,
        Self::ControlReliability,
        Self::Retention,
        Self::Recovery,
        Self::ResourceUsage,
        Self::TemporalQueryLatency,
        Self::ArtifactLatency,
        Self::Cleanup,
    ];
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MatrixOrder {
    FamilyCaseDurationRepetition,
    SeededFisherYates,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StatusRules {
    pub complete: EvaluationStatus,
    pub threshold_failure: EvaluationStatus,
    pub insufficient_evidence: EvaluationStatus,
    pub blocked_precondition: EvaluationStatus,
    pub optional_unavailable: EvaluationStatus,
}

impl StatusRules {
    pub fn canonical() -> Self {
        Self {
            complete: EvaluationStatus::Pass,
            threshold_failure: EvaluationStatus::Fail,
            insufficient_evidence: EvaluationStatus::Inconclusive,
            blocked_precondition: EvaluationStatus::Blocked,
            optional_unavailable: EvaluationStatus::Skipped,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self != &Self::canonical() {
            return Err(ContractError::new(
                "status rules do not match the current benchmark contract",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MatrixDefinition {
    pub seed: u64,
    pub capture_repetitions: u16,
    pub interpretation_repetitions: u16,
    pub capture_order: MatrixOrder,
    pub interpretation_order: MatrixOrder,
    pub required_families: Vec<CaseFamily>,
    pub supplemental_families: Vec<CaseFamily>,
    pub stable_controls_required: bool,
    pub status_rules: StatusRules,
}

impl MatrixDefinition {
    pub fn canonical() -> Self {
        Self {
            seed: MATRIX_SEED,
            capture_repetitions: CAPTURE_REPETITIONS,
            interpretation_repetitions: INTERPRETATION_REPETITIONS,
            capture_order: MatrixOrder::FamilyCaseDurationRepetition,
            interpretation_order: MatrixOrder::SeededFisherYates,
            required_families: vec![
                CaseFamily::MovementReversal,
                CaseFamily::Flicker,
                CaseFamily::TransientLayout,
            ],
            supplemental_families: vec![CaseFamily::DomOpaqueMotion],
            stable_controls_required: true,
            status_rules: StatusRules::canonical(),
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self != &Self::canonical() {
            return Err(ContractError::new(
                "matrix definition does not match the current benchmark contract",
            ));
        }
        self.status_rules.validate()?;
        if self.required_families.len() != 3
            || self.supplemental_families.len() != 1
            || self
                .required_families
                .iter()
                .any(|family| self.supplemental_families.contains(family))
        {
            return Err(ContractError::new(
                "matrix family coverage is incomplete or overlapping",
            ));
        }
        Ok(())
    }

    /// Builds the fixed capture order without consulting host, process, or filesystem order.
    pub fn capture_trials(
        &self,
        cases: &[CaseDefinition],
        durations_ms: &[u16],
    ) -> Result<Vec<CaptureTrial>> {
        self.validate()?;
        let mut ordered = cases.iter().collect::<Vec<_>>();
        ordered.sort_by(|left, right| {
            family_rank(left.family)
                .cmp(&family_rank(right.family))
                .then_with(|| left.case_id.cmp(&right.case_id))
        });

        let mut trials = Vec::with_capacity(
            ordered
                .len()
                .saturating_mul(durations_ms.len())
                .saturating_mul(usize::from(self.capture_repetitions)),
        );
        for case in ordered {
            for &duration_ms in durations_ms {
                for repetition in 0..self.capture_repetitions {
                    trials.push(CaptureTrial {
                        trial_id: format!("capture:{}/{duration_ms}/{repetition}", case.case_id),
                        case_id: case.case_id.clone(),
                        family: case.family,
                        duration_ms,
                        repetition,
                    });
                }
            }
        }
        Ok(trials)
    }

    /// Builds all case/duration/condition trials, then applies a platform-independent shuffle.
    pub fn interpretation_trials(
        &self,
        cases: &[CaseDefinition],
        durations_ms: &[u16],
        condition_ids: &[crate::ConditionId],
    ) -> Result<Vec<InterpretationTrial>> {
        self.validate()?;
        let mut trials = Vec::new();
        let mut ordered = cases.iter().collect::<Vec<_>>();
        ordered.sort_by(|left, right| {
            family_rank(left.family)
                .cmp(&family_rank(right.family))
                .then_with(|| left.case_id.cmp(&right.case_id))
        });
        let mut conditions = condition_ids.to_vec();
        conditions.sort_by_key(|condition| condition.rank());

        for case in ordered {
            for &duration_ms in durations_ms {
                for condition_id in &conditions {
                    for repetition in 0..self.interpretation_repetitions {
                        trials.push(InterpretationTrial {
                            trial_id: format!(
                                "interpretation:{}/{duration_ms}/{condition_id}/{repetition}",
                                case.case_id
                            ),
                            case_id: case.case_id.clone(),
                            family: case.family,
                            duration_ms,
                            condition_id: *condition_id,
                            repetition,
                        });
                    }
                }
            }
        }
        fisher_yates(&mut trials, self.seed);
        Ok(trials)
    }

    pub fn coverage_status(&self, required: bool, observed: u16, minimum: u16) -> EvaluationStatus {
        if observed >= minimum {
            self.status_rules.complete
        } else if observed == 0 && !required {
            self.status_rules.optional_unavailable
        } else if observed == 0 {
            self.status_rules.blocked_precondition
        } else {
            self.status_rules.insufficient_evidence
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaptureTrial {
    pub trial_id: String,
    pub case_id: String,
    pub family: CaseFamily,
    pub duration_ms: u16,
    pub repetition: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InterpretationTrial {
    pub trial_id: String,
    pub case_id: String,
    pub family: CaseFamily,
    pub duration_ms: u16,
    pub condition_id: crate::ConditionId,
    pub repetition: u16,
}

fn family_rank(family: CaseFamily) -> usize {
    match family {
        CaseFamily::MovementReversal => 0,
        CaseFamily::Flicker => 1,
        CaseFamily::TransientLayout => 2,
        CaseFamily::DomOpaqueMotion => 3,
        CaseFamily::StableControl => 4,
    }
}

fn fisher_yates<T>(values: &mut [T], seed: u64) {
    let mut state = seed;
    for index in (1..values.len()).rev() {
        state = splitmix64(state);
        let swap_index = (state % (index as u64 + 1)) as usize;
        values.swap(index, swap_index);
    }
}

fn splitmix64(state: u64) -> u64 {
    let mut value = state.wrapping_add(0x9e3779b97f4a7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shuffle_is_repeatable_and_seeded() {
        let mut first = [0_u8, 1, 2, 3, 4, 5];
        let mut second = first;
        fisher_yates(&mut first, MATRIX_SEED);
        fisher_yates(&mut second, MATRIX_SEED);
        assert_eq!(first, second);
        assert_ne!(first, [0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn family_order_is_total_and_stable() {
        assert_eq!(family_rank(CaseFamily::MovementReversal), 0);
        assert_eq!(family_rank(CaseFamily::StableControl), 4);
        assert_eq!(
            family_rank(CaseFamily::Flicker).cmp(&family_rank(CaseFamily::Flicker)),
            std::cmp::Ordering::Equal
        );
    }
}
