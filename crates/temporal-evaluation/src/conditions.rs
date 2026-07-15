use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ContractError, Result, ScoringDimensionId};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ConditionId {
    #[serde(rename = "A-final-screenshot")]
    AFinalScreenshot,
    #[serde(rename = "B-uniform-storyboard")]
    BUniformStoryboard,
    #[serde(rename = "C-change-aware-storyboard")]
    CChangeAwareStoryboard,
    #[serde(rename = "D-temporal-bundle")]
    DTemporalBundle,
    #[serde(rename = "E-progressive-source")]
    EProgressiveSource,
}

impl ConditionId {
    pub const ALL: [Self; 5] = [
        Self::AFinalScreenshot,
        Self::BUniformStoryboard,
        Self::CChangeAwareStoryboard,
        Self::DTemporalBundle,
        Self::EProgressiveSource,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AFinalScreenshot => "A-final-screenshot",
            Self::BUniformStoryboard => "B-uniform-storyboard",
            Self::CChangeAwareStoryboard => "C-change-aware-storyboard",
            Self::DTemporalBundle => "D-temporal-bundle",
            Self::EProgressiveSource => "E-progressive-source",
        }
    }

    pub const fn rank(self) -> u8 {
        match self {
            Self::AFinalScreenshot => 0,
            Self::BUniformStoryboard => 1,
            Self::CChangeAwareStoryboard => 2,
            Self::DTemporalBundle => 3,
            Self::EProgressiveSource => 4,
        }
    }
}

impl std::fmt::Display for ConditionId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SourceIntervalPolicy {
    SameCapturedSourceInterval,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConditionInput {
    FinalScreenshot,
    CurrentPageObservation,
    UniformSourceStoryboard,
    ChangeAwareSourceStoryboard,
    BeforeDuringAfterComposite,
    DifferenceMap,
    CaptureSummary,
    EvidenceReferences,
    SourceFrameRetrieval,
    RegionFilmstripRetrieval,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    FinalScreenshot,
    UniformStoryboard,
    ChangeAwareStoryboard,
    BeforeDuringAfter,
    DifferenceMap,
    TemporalDebugBundle,
    SourceFrame,
    RegionFilmstrip,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactContract {
    pub kind: ArtifactKind,
    pub algorithm: String,
    pub version: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RetrievalBudget {
    pub source_frame_requests: u16,
    pub source_frames_per_request: u16,
    pub region_filmstrips: u16,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EvidenceCondition {
    pub condition_id: ConditionId,
    pub source_interval_policy: SourceIntervalPolicy,
    pub inputs: Vec<ConditionInput>,
    pub artifacts: Vec<ArtifactContract>,
    pub initial_source_frame_tile_limit: u16,
    pub retrieval_budget: RetrievalBudget,
    pub prompt_id: crate::PromptId,
    pub scoring_dimension_ids: Vec<ScoringDimensionId>,
}

impl EvidenceCondition {
    pub fn validate(&self) -> Result<()> {
        let Some(expected) = canonical_conditions()
            .into_iter()
            .find(|condition| condition.condition_id == self.condition_id)
        else {
            return Err(ContractError::new("unknown evidence condition"));
        };
        if self != &expected {
            return Err(ContractError::new(format!(
                "condition {} does not match the current benchmark contract",
                self.condition_id
            )));
        }
        Ok(())
    }
}

pub fn canonical_conditions() -> Vec<EvidenceCondition> {
    let scoring_dimension_ids = ScoringDimensionId::ALL.to_vec();
    vec![
        EvidenceCondition {
            condition_id: ConditionId::AFinalScreenshot,
            source_interval_policy: SourceIntervalPolicy::SameCapturedSourceInterval,
            inputs: vec![
                ConditionInput::FinalScreenshot,
                ConditionInput::CurrentPageObservation,
            ],
            artifacts: vec![ArtifactContract {
                kind: ArtifactKind::FinalScreenshot,
                algorithm: "current-observation".into(),
                version: "1.0.0".into(),
            }],
            initial_source_frame_tile_limit: 1,
            retrieval_budget: RetrievalBudget {
                source_frame_requests: 0,
                source_frames_per_request: 0,
                region_filmstrips: 0,
            },
            prompt_id: crate::PromptId::Interpretation,
            scoring_dimension_ids: scoring_dimension_ids.clone(),
        },
        EvidenceCondition {
            condition_id: ConditionId::BUniformStoryboard,
            source_interval_policy: SourceIntervalPolicy::SameCapturedSourceInterval,
            inputs: vec![ConditionInput::UniformSourceStoryboard],
            artifacts: vec![ArtifactContract {
                kind: ArtifactKind::UniformStoryboard,
                algorithm: "uniform-source-storyboard".into(),
                version: "1.0.0".into(),
            }],
            initial_source_frame_tile_limit: 8,
            retrieval_budget: RetrievalBudget {
                source_frame_requests: 0,
                source_frames_per_request: 0,
                region_filmstrips: 0,
            },
            prompt_id: crate::PromptId::Interpretation,
            scoring_dimension_ids: scoring_dimension_ids.clone(),
        },
        EvidenceCondition {
            condition_id: ConditionId::CChangeAwareStoryboard,
            source_interval_policy: SourceIntervalPolicy::SameCapturedSourceInterval,
            inputs: vec![ConditionInput::ChangeAwareSourceStoryboard],
            artifacts: vec![ArtifactContract {
                kind: ArtifactKind::ChangeAwareStoryboard,
                algorithm: "temporal-storyboard".into(),
                version: "1.1.0".into(),
            }],
            initial_source_frame_tile_limit: 8,
            retrieval_budget: RetrievalBudget {
                source_frame_requests: 0,
                source_frames_per_request: 0,
                region_filmstrips: 0,
            },
            prompt_id: crate::PromptId::Interpretation,
            scoring_dimension_ids: scoring_dimension_ids.clone(),
        },
        EvidenceCondition {
            condition_id: ConditionId::DTemporalBundle,
            source_interval_policy: SourceIntervalPolicy::SameCapturedSourceInterval,
            inputs: vec![
                ConditionInput::BeforeDuringAfterComposite,
                ConditionInput::ChangeAwareSourceStoryboard,
                ConditionInput::DifferenceMap,
                ConditionInput::CaptureSummary,
                ConditionInput::EvidenceReferences,
            ],
            artifacts: vec![
                ArtifactContract {
                    kind: ArtifactKind::BeforeDuringAfter,
                    algorithm: "temporal-storyboard".into(),
                    version: "1.1.0".into(),
                },
                ArtifactContract {
                    kind: ArtifactKind::ChangeAwareStoryboard,
                    algorithm: "temporal-storyboard".into(),
                    version: "1.1.0".into(),
                },
                ArtifactContract {
                    kind: ArtifactKind::DifferenceMap,
                    algorithm: "temporal-difference-map".into(),
                    version: "v1".into(),
                },
                ArtifactContract {
                    kind: ArtifactKind::TemporalDebugBundle,
                    algorithm: "temporal-debug-bundle".into(),
                    version: "1.0.0".into(),
                },
            ],
            initial_source_frame_tile_limit: 8,
            retrieval_budget: RetrievalBudget {
                source_frame_requests: 0,
                source_frames_per_request: 0,
                region_filmstrips: 0,
            },
            prompt_id: crate::PromptId::Interpretation,
            scoring_dimension_ids: scoring_dimension_ids.clone(),
        },
        EvidenceCondition {
            condition_id: ConditionId::EProgressiveSource,
            source_interval_policy: SourceIntervalPolicy::SameCapturedSourceInterval,
            inputs: vec![
                ConditionInput::BeforeDuringAfterComposite,
                ConditionInput::ChangeAwareSourceStoryboard,
                ConditionInput::DifferenceMap,
                ConditionInput::CaptureSummary,
                ConditionInput::EvidenceReferences,
                ConditionInput::SourceFrameRetrieval,
                ConditionInput::RegionFilmstripRetrieval,
            ],
            artifacts: vec![
                ArtifactContract {
                    kind: ArtifactKind::TemporalDebugBundle,
                    algorithm: "temporal-debug-bundle".into(),
                    version: "1.0.0".into(),
                },
                ArtifactContract {
                    kind: ArtifactKind::SourceFrame,
                    algorithm: "retained-source-frame".into(),
                    version: "1.0.0".into(),
                },
                ArtifactContract {
                    kind: ArtifactKind::RegionFilmstrip,
                    algorithm: "region-filmstrip".into(),
                    version: "1.0.0".into(),
                },
            ],
            initial_source_frame_tile_limit: 8,
            retrieval_budget: RetrievalBudget {
                source_frame_requests: 2,
                source_frames_per_request: 4,
                region_filmstrips: 1,
            },
            prompt_id: crate::PromptId::Interpretation,
            scoring_dimension_ids,
        },
    ]
}
