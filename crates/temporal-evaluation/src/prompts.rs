use std::collections::HashSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ContractError, Result, canonical_json, sha256_prefixed};

const PROMPT_SET_VERSION: &str = "1";
const MAX_PROMPT_CHARS: usize = 4_096;
const MAX_TASK_CHARS: usize = 8_192;
const MAX_ANSWER_TEXT_CHARS: usize = 512;
const MAX_STATE_LABELS: usize = 8;
const MAX_EVIDENCE_REFS: usize = 32;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum PromptId {
    #[serde(rename = "interpretation-v1")]
    Interpretation,
    #[serde(rename = "debugging-v1")]
    Debugging,
}

impl PromptId {
    pub const ALL: [Self; 2] = [Self::Interpretation, Self::Debugging];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Interpretation => "interpretation-v1",
            Self::Debugging => "debugging-v1",
        }
    }
}

impl std::fmt::Display for PromptId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AnswerKind {
    Interpretation,
    Debugging,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum AnswerTruth {
    Yes,
    No,
    Uncertain,
}

#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum StateLabel {
    Baseline,
    Changed,
    Final,
    IntentionalMotion,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MotionBehavior {
    Monotonic,
    Reversal,
    Teleport,
    Flicker,
    LayoutShift,
    None,
    Uncertain,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Judgment {
    Defective,
    Intentional,
    Uncertain,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UncertaintyReason {
    CaptureGap,
    MissingSource,
    InsufficientDetail,
    Other,
}

/// A model-supplied localization in the same coordinate space as the benchmark ground truth.
///
/// Rectangles use half-open captured-viewport pixels from the top-left of the fixed 800x450
/// screenshot at device scale one. They are not stage-relative offsets or canvas-local drawing
/// coordinates.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AnswerRegion {
    Unknown,
    Rect {
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InterpretationAnswer {
    pub temporary_state: AnswerTruth,
    pub state_order: Vec<StateLabel>,
    pub affected_region: AnswerRegion,
    pub motion_behavior: MotionBehavior,
    pub judgment: Judgment,
    pub uncertainty_reasons: Vec<UncertaintyReason>,
    pub evidence_refs: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DebuggingAnswer {
    pub reproduced: AnswerTruth,
    pub diagnosis: String,
    pub patch_applied: AnswerTruth,
    pub final_state_verified: AnswerTruth,
    pub temporal_behavior_verified: AnswerTruth,
    pub evidence_refs: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AnswerValidationContext {
    pub unresolved_capture_gap: bool,
    pub missing_source: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PromptTemplate {
    pub id: PromptId,
    pub version: String,
    pub answer_kind: AnswerKind,
    pub system_prompt: String,
    pub task_prompt: String,
    pub sha256: String,
}

impl PromptTemplate {
    pub fn validate(&self) -> Result<()> {
        if self.version != PROMPT_SET_VERSION {
            return Err(ContractError::new(format!(
                "prompt {} has unsupported version {}",
                self.id, self.version
            )));
        }
        if self.system_prompt.is_empty() || self.system_prompt.chars().count() > MAX_PROMPT_CHARS {
            return Err(ContractError::new("system prompt is empty or too long"));
        }
        if self.task_prompt.is_empty() || self.task_prompt.chars().count() > MAX_TASK_CHARS {
            return Err(ContractError::new("task prompt is empty or too long"));
        }
        if contains_model_metadata(&self.system_prompt)
            || contains_model_metadata(&self.task_prompt)
        {
            return Err(ContractError::new(
                "model-facing prompt contains fixture or expected-answer metadata",
            ));
        }
        validate_sha256(&self.sha256)?;
        if self.sha256 != self.computed_sha256()? {
            return Err(ContractError::new(format!(
                "prompt {} hash does not match its exact input",
                self.id
            )));
        }
        Ok(())
    }

    pub fn computed_sha256(&self) -> Result<String> {
        let input = PromptHashInput {
            id: self.id,
            version: &self.version,
            answer_kind: self.answer_kind,
            system_prompt: &self.system_prompt,
            task_prompt: &self.task_prompt,
        };
        Ok(sha256_prefixed(&canonical_json(&input)?))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PromptSet {
    pub version: String,
    pub templates: Vec<PromptTemplate>,
}

impl PromptSet {
    pub fn canonical() -> Self {
        Self {
            version: PROMPT_SET_VERSION.into(),
            templates: vec![
                canonical_prompt(
                    PromptId::Interpretation,
                    AnswerKind::Interpretation,
                    "You inspect browser evidence as an observation task. Describe only visible sequence, region, and uncertainty supported by supplied evidence. Do not infer causes, diagnoses, or hidden implementation details. If a capture gap or missing source prevents a claim, use uncertain and name the limitation. Return only the JSON object required by the answer contract.",
                    "Inspect the supplied browser evidence and return JSON. State whether a temporary visible state occurred; list visible state order; identify the affected region or unknown; classify motion or visual behavior; judge defective, intentional, or uncertain; list uncertainty reasons; and cite opaque evidence references. Rectangle coordinates are half-open viewport pixels from the top-left of the fixed 800x450 screenshot at device scale one, not stage-relative or canvas-local coordinates. Do not use labels not visible in the evidence.",
                ),
                canonical_prompt(
                    PromptId::Debugging,
                    AnswerKind::Debugging,
                    "You are completing a bounded browser debugging task after inspecting supplied evidence. Keep observation, diagnosis, patch, and verification separate. Do not claim a cause not supported by the repository or evidence. Return only the JSON object required by the answer contract.",
                    "Reproduce the reported browser behavior, record whether reproduction succeeded, summarize the supported diagnosis, record whether a focused patch was applied, and state whether final-state and temporal verification succeeded. Use opaque evidence references and uncertainty when verification is unavailable.",
                ),
            ],
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self != &Self::canonical() {
            return Err(ContractError::new(
                "prompt set does not match the current benchmark contract",
            ));
        }
        for template in &self.templates {
            template.validate()?;
        }
        Ok(())
    }

    pub fn template(&self, id: PromptId) -> Option<&PromptTemplate> {
        self.templates.iter().find(|template| template.id == id)
    }
}

pub fn parse_interpretation_answer(
    bytes: &[u8],
    context: AnswerValidationContext,
) -> Result<InterpretationAnswer> {
    let answer: InterpretationAnswer = serde_json::from_slice(bytes)?;
    validate_interpretation_answer(&answer, context)?;
    Ok(answer)
}

pub fn validate_interpretation_answer(
    answer: &InterpretationAnswer,
    context: AnswerValidationContext,
) -> Result<()> {
    if answer.state_order.is_empty() || answer.state_order.len() > MAX_STATE_LABELS {
        return Err(ContractError::new(
            "state_order must contain between one and eight labels",
        ));
    }
    let mut states = HashSet::new();
    if answer.state_order.iter().any(|state| !states.insert(state)) {
        return Err(ContractError::new(
            "state_order labels must be unique and ordered observations",
        ));
    }
    if answer.uncertainty_reasons.len() > MAX_STATE_LABELS {
        return Err(ContractError::new("too many uncertainty reasons"));
    }
    let mut reasons = HashSet::new();
    if answer
        .uncertainty_reasons
        .iter()
        .any(|reason| !reasons.insert(reason))
    {
        return Err(ContractError::new("uncertainty reasons must be unique"));
    }
    validate_evidence_refs(&answer.evidence_refs)?;
    if context.unresolved_capture_gap || context.missing_source {
        if answer.judgment != Judgment::Uncertain {
            return Err(ContractError::new(
                "unresolved source evidence requires an uncertain judgment",
            ));
        }
        let required_reason = if context.unresolved_capture_gap {
            UncertaintyReason::CaptureGap
        } else {
            UncertaintyReason::MissingSource
        };
        if !answer.uncertainty_reasons.contains(&required_reason) {
            return Err(ContractError::new(
                "answer must name the source limitation behind its uncertainty",
            ));
        }
    }
    Ok(())
}

pub fn validate_debugging_answer(answer: &DebuggingAnswer) -> Result<()> {
    if answer.diagnosis.is_empty() || answer.diagnosis.chars().count() > MAX_ANSWER_TEXT_CHARS {
        return Err(ContractError::new("diagnosis is empty or too long"));
    }
    validate_evidence_refs(&answer.evidence_refs)
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct PromptHashInput<'a> {
    id: PromptId,
    version: &'a str,
    answer_kind: AnswerKind,
    system_prompt: &'a str,
    task_prompt: &'a str,
}

fn canonical_prompt(
    id: PromptId,
    answer_kind: AnswerKind,
    system_prompt: &str,
    task_prompt: &str,
) -> PromptTemplate {
    let mut template = PromptTemplate {
        id,
        version: PROMPT_SET_VERSION.into(),
        answer_kind,
        system_prompt: system_prompt.into(),
        task_prompt: task_prompt.into(),
        sha256: String::new(),
    };
    template.sha256 = template
        .computed_sha256()
        .expect("canonical prompt hash must serialize");
    template
}

fn contains_model_metadata(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    [
        "movement-reversal",
        "movement reversal",
        "flicker",
        "transient-layout",
        "transient layout",
        "dom-opaque",
        "dom opaque",
        "stable-control",
        "stable control",
        "case_id",
        "case id",
        "variant",
        "expected answer",
        "ground truth",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn validate_evidence_refs(refs: &[String]) -> Result<()> {
    if refs.len() > MAX_EVIDENCE_REFS {
        return Err(ContractError::new("too many evidence references"));
    }
    let mut unique = HashSet::new();
    for reference in refs {
        if !unique.insert(reference) {
            return Err(ContractError::new(
                "evidence references must be unique and ordered",
            ));
        }
        if reference.is_empty()
            || reference.chars().count() > 128
            || reference
                .chars()
                .any(|character| !(character.is_ascii_alphanumeric() || "._-".contains(character)))
        {
            return Err(ContractError::new(
                "evidence references must be bounded opaque identifiers",
            ));
        }
    }
    Ok(())
}

fn validate_sha256(value: &str) -> Result<()> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return Err(ContractError::new("prompt hash must use sha256:<64 hex>"));
    };
    if hex.len() != 64
        || hex.bytes().any(|byte| !byte.is_ascii_hexdigit())
        || hex.bytes().any(|byte| byte.is_ascii_uppercase())
    {
        return Err(ContractError::new(
            "prompt hash must use 64 lowercase hexadecimal characters",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_prompt_hash_is_stable() {
        let set = PromptSet::canonical();
        set.validate().unwrap();
        assert_eq!(set.templates.len(), 2);
        assert_eq!(
            set.templates[0].sha256,
            set.templates[0].computed_sha256().unwrap()
        );
    }

    #[test]
    fn model_metadata_is_rejected_before_prompt_hashing() {
        let mut prompt = PromptSet::canonical().templates[0].clone();
        prompt.task_prompt.push_str(" case_id=secret");
        assert!(prompt.validate().is_err());
    }

    #[test]
    fn unresolved_evidence_requires_uncertain_interpretation() {
        let answer = InterpretationAnswer {
            temporary_state: AnswerTruth::Yes,
            state_order: vec![StateLabel::Baseline, StateLabel::Changed],
            affected_region: AnswerRegion::Unknown,
            motion_behavior: MotionBehavior::Uncertain,
            judgment: Judgment::Defective,
            uncertainty_reasons: vec![],
            evidence_refs: vec!["frame_1".into()],
        };
        assert!(
            validate_interpretation_answer(
                &answer,
                AnswerValidationContext {
                    unresolved_capture_gap: true,
                    missing_source: false,
                }
            )
            .is_err()
        );
    }

    #[test]
    fn debugging_answer_bounds_free_text_and_references() {
        let answer = DebuggingAnswer {
            reproduced: AnswerTruth::Yes,
            diagnosis: "focused diagnosis".into(),
            patch_applied: AnswerTruth::Yes,
            final_state_verified: AnswerTruth::Yes,
            temporal_behavior_verified: AnswerTruth::Yes,
            evidence_refs: vec!["artifact_1".into()],
        };
        validate_debugging_answer(&answer).unwrap();
        let mut invalid = answer;
        invalid.evidence_refs = vec!["/private/path".into()];
        assert!(validate_debugging_answer(&invalid).is_err());
    }
}
