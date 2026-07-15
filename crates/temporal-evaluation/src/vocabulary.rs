use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ContractError, Result};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ScoringDimensionId {
    TransientDefectIdentification,
    StateOrder,
    AffectedRegion,
    MotionBehavior,
    GapUncertainty,
    StableControlFalsePositive,
}

impl ScoringDimensionId {
    pub const ALL: [Self; 6] = [
        Self::TransientDefectIdentification,
        Self::StateOrder,
        Self::AffectedRegion,
        Self::MotionBehavior,
        Self::GapUncertainty,
        Self::StableControlFalsePositive,
    ];
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScoringDimension {
    pub id: ScoringDimensionId,
    pub allowed_values: Vec<String>,
    pub requires_ground_truth: bool,
    pub contributes_to_thesis: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScoringVocabulary {
    pub version: String,
    pub dimensions: Vec<ScoringDimension>,
}

impl ScoringVocabulary {
    pub fn canonical() -> Self {
        Self {
            version: "1".into(),
            dimensions: vec![
                dimension(
                    ScoringDimensionId::TransientDefectIdentification,
                    &["defective", "intentional", "uncertain"],
                    true,
                    true,
                ),
                dimension(
                    ScoringDimensionId::StateOrder,
                    &[
                        "baseline",
                        "changed",
                        "final",
                        "intentional_motion",
                        "unknown",
                    ],
                    true,
                    true,
                ),
                dimension(
                    ScoringDimensionId::AffectedRegion,
                    &["match", "mismatch", "unknown"],
                    true,
                    true,
                ),
                dimension(
                    ScoringDimensionId::MotionBehavior,
                    &[
                        "monotonic",
                        "reversal",
                        "teleport",
                        "flicker",
                        "layout_shift",
                        "none",
                        "uncertain",
                    ],
                    true,
                    true,
                ),
                dimension(
                    ScoringDimensionId::GapUncertainty,
                    &["calibrated", "overclaim", "underclaim", "not_applicable"],
                    false,
                    true,
                ),
                dimension(
                    ScoringDimensionId::StableControlFalsePositive,
                    &["positive", "negative", "uncertain"],
                    true,
                    true,
                ),
            ],
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self != &Self::canonical() {
            return Err(ContractError::new(
                "scoring vocabulary does not match the current benchmark contract",
            ));
        }
        Ok(())
    }
}

fn dimension(
    id: ScoringDimensionId,
    allowed_values: &[&str],
    requires_ground_truth: bool,
    contributes_to_thesis: bool,
) -> ScoringDimension {
    ScoringDimension {
        id,
        allowed_values: allowed_values.iter().map(|value| (*value).into()).collect(),
        requires_ground_truth,
        contributes_to_thesis,
    }
}
