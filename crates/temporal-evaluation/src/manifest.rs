use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    AnswerKind, BenchmarkDefinition, CaseFamily, ConditionId, EvaluationStatus, FixtureFile,
    MatrixOrder, PromptId, PromptTemplate, Result, ScoringDimensionId, canonical_json,
    conditions::canonical_conditions,
    matrix::{CAPTURE_REPETITIONS, INTERPRETATION_REPETITIONS, MATRIX_SEED},
    privacy,
};
use crate::{ContractError, DURATIONS_MS, FIXTURE_ROOT, VIEWPORT_HEIGHT, VIEWPORT_WIDTH};

pub const MANIFEST_SCHEMA_VERSION: u16 = 1;
pub const MANIFEST_KIND: &str = "temporal_benchmark_run";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RevisionIdentity {
    pub git_revision: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScorerIdentity {
    pub git_revision: String,
    pub version: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ManifestFixture {
    pub root_relative_path: String,
    pub ordered_files: Vec<FixtureFile>,
    pub definition_sha256: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ImageFormat {
    Png,
    Jpeg,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunConfiguration {
    pub seed: u64,
    pub order_policy: MatrixOrder,
    pub ordered_trials: Vec<TrialIdentity>,
    pub condition_id: ConditionId,
    pub duration_ms: Vec<u16>,
    pub repetitions: u16,
    pub optional_configuration: bool,
    pub viewport: Viewport,
    /// Device scale factor represented in thousandths to keep the contract integer-only.
    pub device_scale_factor: u16,
    pub image_format: ImageFormat,
    pub image_quality: Option<u8>,
    pub retention_budget_bytes: u64,
    pub threshold_profile: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TrialIdentity {
    pub trial_id: String,
    pub case_id: String,
    pub family: CaseFamily,
    pub duration_ms: u16,
    pub repetition: u16,
    pub condition_id: ConditionId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Linux,
    Macos,
    Windows,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Architecture {
    X86_64,
    Aarch64,
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentIdentity {
    pub platform: Platform,
    pub architecture: Architecture,
    pub os_release_class: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BrowserProduct {
    Chromium,
    Chrome,
    OtherChromium,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "availability", rename_all = "snake_case", deny_unknown_fields)]
pub enum BrowserAvailability {
    NotRequired,
    Observed {
        product: BrowserProduct,
        product_version: String,
        protocol_version: String,
        revision: String,
        capability_id: String,
    },
    Unavailable {
        reason: String,
        recovery: String,
    },
    Blocked {
        reason: String,
        recovery: String,
    },
    Skipped {
        product: BrowserProduct,
        reason: String,
        recovery: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModelInputLimits {
    pub max_input_tokens: Option<u64>,
    pub max_images: Option<u16>,
    pub max_input_bytes: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "availability", rename_all = "snake_case", deny_unknown_fields)]
pub enum ModelAvailability {
    NotRequired,
    Observed {
        provider: String,
        model_id: String,
        model_version_or_dated_alias: String,
        invocation_date: String,
        authorization_ref: String,
        tools: Vec<String>,
        input_limits: ModelInputLimits,
    },
    Unavailable {
        reason: String,
        recovery: String,
    },
    Blocked {
        reason: String,
        recovery: String,
    },
    Skipped {
        reason: String,
        recovery: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ManifestPrompt {
    pub prompt_set_id: PromptId,
    pub prompt_version: String,
    pub system_prompt: String,
    pub task_prompt: String,
    pub sha256: String,
}

impl ManifestPrompt {
    fn as_template(&self) -> PromptTemplate {
        PromptTemplate {
            id: self.prompt_set_id,
            version: self.prompt_version.clone(),
            answer_kind: match self.prompt_set_id {
                PromptId::Interpretation => AnswerKind::Interpretation,
                PromptId::Debugging => AnswerKind::Debugging,
            },
            system_prompt: self.system_prompt.clone(),
            task_prompt: self.task_prompt.clone(),
            sha256: self.sha256.clone(),
        }
    }

    pub fn validate(&self) -> Result<()> {
        let template = self.as_template();
        template.validate()?;
        let expected = crate::PromptSet::canonical()
            .templates
            .into_iter()
            .find(|candidate| candidate.id == self.prompt_set_id)
            .ok_or_else(|| ContractError::new("manifest prompt is not registered"))?;
        if template != expected {
            return Err(ContractError::new(
                "manifest prompt does not match the canonical prompt registry",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NamedVersion {
    pub name: String,
    pub version: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CaptureConfigIdentity {
    pub queue_capacity: u16,
    pub max_active_streams: u16,
    pub ack_timeout_ms: u64,
    pub shutdown_timeout_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KrometrailIdentity {
    pub git_revision: String,
    pub cargo_lock_sha256: String,
    pub rust_toolchain: String,
    pub capture_config: CaptureConfigIdentity,
    pub adapter_versions: Vec<NamedVersion>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceAvailability {
    Retained,
    Evicted,
    NotCollected,
    Corrupt,
    Gap,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputIdentity {
    pub id: String,
    pub sha256: Option<String>,
    pub availability: EvidenceAvailability,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactIdentity {
    pub condition_id: ConditionId,
    pub algorithm_versions: Vec<NamedVersion>,
    pub parameters: BTreeMap<String, String>,
    pub source_interval_id: String,
    pub output_ids: Vec<OutputIdentity>,
    pub source_frame_ids: Vec<OutputIdentity>,
    pub gap_ids: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScoringIdentity {
    pub rubric_version: String,
    pub dimension_ids: Vec<ScoringDimensionId>,
    pub aggregate_method: String,
    pub rationale_policy: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RetentionState {
    NotApplicable,
    Retained,
    PartiallyRetained,
    Evicted,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimeRangeMs {
    pub start_ms: u64,
    pub end_ms: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CaptureOrdinalRange {
    pub first: u64,
    pub last: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AcceptedClaim {
    pub claim_id: String,
    pub evidence_ids: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ManifestRow {
    pub trial_id: String,
    pub case_id: String,
    pub family: CaseFamily,
    pub duration_ms: u16,
    pub repetition: u16,
    pub condition_id: ConditionId,
    pub capture_ordinal_range: Option<CaptureOrdinalRange>,
    pub source_time_range: Option<TimeRangeMs>,
    pub observed_time_range: Option<TimeRangeMs>,
    pub session_time_range: Option<TimeRangeMs>,
    pub gap_ids: Vec<String>,
    pub retention_state: RetentionState,
    pub artifact_ids: Vec<String>,
    pub accepted_claims: Vec<AcceptedClaim>,
    pub answer_digest: Option<String>,
    pub raw_answer_ref: Option<String>,
    pub score: Option<u8>,
    pub scoring_rationale: Option<String>,
    pub status: EvaluationStatus,
    pub failure: Option<FailureRecord>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunFailureCode {
    Validation,
    Threshold,
    InsufficientEvidence,
    Unavailable,
    Authorization,
    Unsupported,
    Retention,
    CaptureGap,
    CorruptSource,
    OptionalUnavailable,
    Internal,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FailureRecord {
    pub code: RunFailureCode,
    pub phase: String,
    pub reason: String,
    pub recovery: String,
    pub retryable: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunManifest {
    pub schema_version: u16,
    pub kind: String,
    pub benchmark_id: String,
    pub benchmark_definition: RevisionIdentity,
    pub harness: RevisionIdentity,
    pub scorer: ScorerIdentity,
    pub fixture: ManifestFixture,
    pub run: RunConfiguration,
    pub environment: EnvironmentIdentity,
    pub browser: BrowserAvailability,
    pub krometrail: KrometrailIdentity,
    pub model: ModelAvailability,
    pub prompt: ManifestPrompt,
    pub artifact: ArtifactIdentity,
    pub scoring: ScoringIdentity,
    pub rows: Vec<ManifestRow>,
    pub status: EvaluationStatus,
    pub non_claims: Vec<String>,
    pub failure: Option<FailureRecord>,
}

impl RunManifest {
    pub fn from_canonical_json(bytes: &[u8]) -> Result<Self> {
        let manifest: Self = serde_json::from_slice(bytes)?;
        manifest.validate_inner()?;
        crate::canonical::require_canonical(bytes, &manifest)?;
        privacy::sanitize_serialized(&manifest)?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<()> {
        self.validate_inner()?;
        privacy::sanitize_serialized(self)
    }

    /// Manifest sanitization is intentionally validation-only: unsafe details are rejected,
    /// not silently rewritten, so a producer cannot accidentally publish a changed identity.
    pub fn sanitize(&self) -> Result<()> {
        self.validate()
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        canonical_json(self)
    }

    pub fn digest(&self) -> Result<String> {
        Ok(crate::sha256_prefixed(&self.canonical_bytes()?))
    }

    /// Hashes benchmark inputs and identities while excluding measured rows and outcomes.
    pub fn input_digest(&self) -> Result<String> {
        self.validate()?;
        let mut value = serde_json::to_value(self)?;
        let object = value
            .as_object_mut()
            .ok_or_else(|| ContractError::new("run manifest did not serialize as an object"))?;
        for key in ["rows", "status", "non_claims", "failure"] {
            object.remove(key);
        }
        Ok(crate::sha256_prefixed(&canonical_json(&value)?))
    }

    pub fn sample() -> Self {
        let definition = BenchmarkDefinition::canonical();
        let conditions = canonical_conditions();
        let condition = conditions
            .iter()
            .find(|condition| condition.condition_id == ConditionId::AFinalScreenshot)
            .expect("condition A is part of the canonical registry");
        let case = definition
            .cases
            .iter()
            .find(|case| case.case_id == "movement-reversal/basic")
            .expect("movement case is part of the canonical corpus");
        let trial_id = "contract/movement-reversal-basic/16/0".to_owned();
        let template = crate::PromptSet::canonical()
            .templates
            .iter()
            .find(|template| template.id == PromptId::Interpretation)
            .expect("interpretation prompt is part of the canonical prompt registry")
            .clone();
        let prompt = ManifestPrompt {
            prompt_set_id: template.id,
            prompt_version: template.version,
            system_prompt: template.system_prompt,
            task_prompt: template.task_prompt,
            sha256: template.sha256,
        };
        let zero_revision = "0000000000000000000000000000000000000000".to_owned();
        let zero_hash = "sha256:0000000000000000000000000000000000000000000000000000000000000000";

        Self {
            schema_version: MANIFEST_SCHEMA_VERSION,
            kind: MANIFEST_KIND.to_owned(),
            benchmark_id: crate::BENCHMARK_ID.to_owned(),
            benchmark_definition: RevisionIdentity {
                git_revision: zero_revision.clone(),
                sha256: definition
                    .definition_digest()
                    .expect("canonical definition is valid"),
            },
            harness: RevisionIdentity {
                git_revision: zero_revision.clone(),
                sha256: zero_hash.to_owned(),
            },
            scorer: ScorerIdentity {
                git_revision: zero_revision.clone(),
                version: "contract-only".to_owned(),
            },
            fixture: ManifestFixture {
                root_relative_path: FIXTURE_ROOT.to_owned(),
                ordered_files: definition.fixture.files.clone(),
                definition_sha256: definition
                    .definition_digest()
                    .expect("canonical definition is valid"),
            },
            run: RunConfiguration {
                seed: MATRIX_SEED,
                order_policy: MatrixOrder::FamilyCaseDurationRepetition,
                ordered_trials: vec![TrialIdentity {
                    trial_id: trial_id.clone(),
                    case_id: case.case_id.clone(),
                    family: case.family,
                    duration_ms: 16,
                    repetition: 0,
                    condition_id: ConditionId::AFinalScreenshot,
                }],
                condition_id: ConditionId::AFinalScreenshot,
                duration_ms: DURATIONS_MS.to_vec(),
                repetitions: 1,
                optional_configuration: false,
                viewport: Viewport {
                    width: VIEWPORT_WIDTH,
                    height: VIEWPORT_HEIGHT,
                },
                device_scale_factor: 1_000,
                image_format: ImageFormat::Png,
                image_quality: None,
                retention_budget_bytes: 0,
                threshold_profile: "contract-only".to_owned(),
            },
            environment: EnvironmentIdentity {
                platform: Platform::Linux,
                architecture: Architecture::X86_64,
                os_release_class: "contract-fixture".to_owned(),
            },
            browser: BrowserAvailability::NotRequired,
            krometrail: KrometrailIdentity {
                git_revision: zero_revision.clone(),
                cargo_lock_sha256: zero_hash.to_owned(),
                rust_toolchain: "rustc-1.85.0".to_owned(),
                capture_config: CaptureConfigIdentity {
                    queue_capacity: 1,
                    max_active_streams: 1,
                    ack_timeout_ms: 1_000,
                    shutdown_timeout_ms: 1_000,
                },
                adapter_versions: Vec::new(),
            },
            model: ModelAvailability::NotRequired,
            prompt,
            artifact: ArtifactIdentity {
                condition_id: condition.condition_id,
                algorithm_versions: vec![NamedVersion {
                    name: "contract".to_owned(),
                    version: "1".to_owned(),
                }],
                parameters: BTreeMap::new(),
                source_interval_id: "contract-interval".to_owned(),
                output_ids: Vec::new(),
                source_frame_ids: Vec::new(),
                gap_ids: Vec::new(),
            },
            scoring: ScoringIdentity {
                rubric_version: "1".to_owned(),
                dimension_ids: ScoringDimensionId::ALL.to_vec(),
                aggregate_method: "contract-only".to_owned(),
                rationale_policy: "not_applicable".to_owned(),
            },
            rows: vec![ManifestRow {
                trial_id,
                case_id: case.case_id.clone(),
                family: case.family,
                duration_ms: 16,
                repetition: 0,
                condition_id: ConditionId::AFinalScreenshot,
                capture_ordinal_range: None,
                source_time_range: None,
                observed_time_range: None,
                session_time_range: None,
                gap_ids: Vec::new(),
                retention_state: RetentionState::NotApplicable,
                artifact_ids: Vec::new(),
                accepted_claims: Vec::new(),
                answer_digest: None,
                raw_answer_ref: None,
                score: None,
                scoring_rationale: None,
                status: EvaluationStatus::Pass,
                failure: None,
            }],
            status: EvaluationStatus::Pass,
            non_claims: vec![
                "This sample validates the contract only.".to_owned(),
                "It is not browser, model, or temporal-evidence measurement.".to_owned(),
            ],
            failure: None,
        }
    }

    fn validate_inner(&self) -> Result<()> {
        if self.schema_version != MANIFEST_SCHEMA_VERSION {
            return Err(ContractError::new(
                "unsupported run manifest schema version",
            ));
        }
        if self.kind != MANIFEST_KIND {
            return Err(ContractError::new(
                "run manifest kind is not temporal_benchmark_run",
            ));
        }
        if self.benchmark_id != crate::BENCHMARK_ID {
            return Err(ContractError::new(
                "run manifest benchmark identity is unknown",
            ));
        }
        validate_revision(&self.benchmark_definition, "benchmark_definition")?;
        validate_revision(&self.harness, "harness")?;
        privacy::validate_git_revision(&self.scorer.git_revision, "scorer.git_revision")?;
        privacy::validate_safe_text(
            &self.scorer.version,
            "scorer.version",
            privacy::MAX_SHORT_TEXT,
        )?;
        validate_fixture(&self.fixture)?;
        if self.benchmark_definition.sha256 != self.fixture.definition_sha256 {
            return Err(ContractError::new(
                "benchmark definition and fixture definition digests disagree",
            ));
        }
        validate_run(&self.run)?;
        validate_trials(&self.run.ordered_trials, &self.run)?;
        validate_environment(&self.environment)?;
        validate_browser(&self.browser)?;
        validate_krometrail(&self.krometrail)?;
        validate_model(&self.model)?;
        self.prompt.validate()?;
        let expected_prompt = match run_mode(&self.run)? {
            ManifestRunMode::Debugging => PromptId::Debugging,
            ManifestRunMode::Contract
            | ManifestRunMode::Capture
            | ManifestRunMode::Interpretation => canonical_conditions()
                .iter()
                .find(|condition| condition.condition_id == self.run.condition_id)
                .map(|condition| condition.prompt_id)
                .ok_or_else(|| ContractError::new("run condition is not registered"))?,
        };
        if self.prompt.prompt_set_id != expected_prompt {
            return Err(ContractError::new(
                "prompt does not match run kind and condition",
            ));
        }
        validate_artifact(&self.artifact, &self.run)?;
        validate_scoring(&self.scoring)?;
        validate_rows(self)?;
        validate_outcome(self)
    }
}

fn validate_revision(value: &RevisionIdentity, label: &str) -> Result<()> {
    privacy::validate_git_revision(&value.git_revision, &format!("{label}.git_revision"))?;
    privacy::validate_sha256(&value.sha256, &format!("{label}.sha256"))
}

fn validate_fixture(value: &ManifestFixture) -> Result<()> {
    privacy::validate_relative_path(&value.root_relative_path, "fixture.root_relative_path")?;
    privacy::validate_sha256(&value.definition_sha256, "fixture.definition_sha256")?;
    if value.root_relative_path != FIXTURE_ROOT {
        return Err(ContractError::new(
            "fixture root does not match the canonical benchmark",
        ));
    }
    let definition = BenchmarkDefinition::canonical();
    if value.ordered_files != definition.fixture.files {
        return Err(ContractError::new(
            "fixture file order does not match the canonical benchmark",
        ));
    }
    if value.definition_sha256 != definition.definition_digest()? {
        return Err(ContractError::new(
            "fixture definition digest does not match the canonical benchmark",
        ));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ManifestRunMode {
    Contract,
    Capture,
    Interpretation,
    Debugging,
}

fn run_mode(value: &RunConfiguration) -> Result<ManifestRunMode> {
    match value.threshold_profile.as_str() {
        "contract-only" => Ok(ManifestRunMode::Contract),
        "capture-v1" => Ok(ManifestRunMode::Capture),
        "interpretation-v1" => Ok(ManifestRunMode::Interpretation),
        "debugging-v1" => Ok(ManifestRunMode::Debugging),
        _ => Err(ContractError::new(
            "run threshold profile is not registered",
        )),
    }
}

fn validate_run(value: &RunConfiguration) -> Result<()> {
    let expected_order = match run_mode(value)? {
        ManifestRunMode::Contract | ManifestRunMode::Capture => {
            MatrixOrder::FamilyCaseDurationRepetition
        }
        ManifestRunMode::Interpretation | ManifestRunMode::Debugging => {
            MatrixOrder::SeededFisherYates
        }
    };
    if value.seed != MATRIX_SEED || value.order_policy != expected_order {
        return Err(ContractError::new(
            "run matrix seed or order policy is not canonical",
        ));
    }
    if value.duration_ms != DURATIONS_MS {
        return Err(ContractError::new(
            "run duration matrix does not match the canonical matrix",
        ));
    }
    if value.repetitions == 0 {
        return Err(ContractError::new("run repetitions must be positive"));
    }
    if value.viewport.width != VIEWPORT_WIDTH || value.viewport.height != VIEWPORT_HEIGHT {
        return Err(ContractError::new(
            "run viewport does not match the canonical viewport",
        ));
    }
    if value.device_scale_factor == 0 {
        return Err(ContractError::new("device scale factor must be positive"));
    }
    if matches!(value.image_format, ImageFormat::Jpeg)
        && !matches!(value.image_quality, Some(1..=100))
    {
        return Err(ContractError::new(
            "JPEG image quality must be between 1 and 100",
        ));
    }
    if matches!(value.image_format, ImageFormat::Png) && value.image_quality.is_some() {
        return Err(ContractError::new("PNG cannot declare JPEG image quality"));
    }
    privacy::validate_safe_text(
        &value.threshold_profile,
        "run.threshold_profile",
        privacy::MAX_SHORT_TEXT,
    )
}

fn validate_trials(values: &[TrialIdentity], run: &RunConfiguration) -> Result<()> {
    if values.is_empty() {
        return Err(ContractError::new(
            "run manifest must declare ordered trials",
        ));
    }
    if values.len() > 100_000 {
        return Err(ContractError::new(
            "ordered trial list exceeds its contract bound",
        ));
    }
    let definition = BenchmarkDefinition::canonical();
    let mut ids = BTreeSet::new();
    for trial in values {
        privacy::validate_trial_id(&trial.trial_id, "trial.trial_id")?;
        if !ids.insert(&trial.trial_id) {
            return Err(ContractError::new(
                "ordered trial identifiers must be unique",
            ));
        }
        let case = definition
            .cases
            .iter()
            .find(|case| case.case_id == trial.case_id)
            .ok_or_else(|| ContractError::new("trial references an unknown case"))?;
        if case.family != trial.family
            || !DURATIONS_MS.contains(&trial.duration_ms)
            || trial.condition_id != run.condition_id
            || trial.repetition >= run.repetitions
        {
            return Err(ContractError::new(
                "trial identity contradicts its run configuration",
            ));
        }
        privacy::validate_safe_text(&trial.case_id, "trial.case_id", privacy::MAX_SHORT_TEXT)?;
    }
    Ok(())
}

fn validate_environment(value: &EnvironmentIdentity) -> Result<()> {
    privacy::validate_safe_text(
        &value.os_release_class,
        "environment.os_release_class",
        privacy::MAX_SHORT_TEXT,
    )
}

fn validate_reason(reason: &str, recovery: &str, label: &str) -> Result<()> {
    privacy::validate_safe_text(reason, &format!("{label}.reason"), privacy::MAX_LONG_TEXT)?;
    privacy::validate_safe_text(
        recovery,
        &format!("{label}.recovery"),
        privacy::MAX_LONG_TEXT,
    )
}

fn validate_browser(value: &BrowserAvailability) -> Result<()> {
    match value {
        BrowserAvailability::NotRequired => Ok(()),
        BrowserAvailability::Observed {
            product_version,
            protocol_version,
            revision,
            capability_id,
            ..
        } => {
            privacy::validate_safe_text(
                product_version,
                "browser.product_version",
                privacy::MAX_SHORT_TEXT,
            )?;
            privacy::validate_safe_text(
                protocol_version,
                "browser.protocol_version",
                privacy::MAX_SHORT_TEXT,
            )?;
            privacy::validate_safe_text(revision, "browser.revision", privacy::MAX_SHORT_TEXT)?;
            privacy::validate_opaque_id(capability_id, "browser.capability_id")
        }
        BrowserAvailability::Unavailable { reason, recovery }
        | BrowserAvailability::Blocked { reason, recovery } => {
            validate_reason(reason, recovery, "browser")
        }
        BrowserAvailability::Skipped {
            reason, recovery, ..
        } => validate_reason(reason, recovery, "browser"),
    }
}

fn validate_model(value: &ModelAvailability) -> Result<()> {
    match value {
        ModelAvailability::NotRequired => Ok(()),
        ModelAvailability::Observed {
            provider,
            model_id,
            model_version_or_dated_alias,
            invocation_date,
            authorization_ref,
            tools,
            input_limits,
        } => {
            privacy::validate_safe_text(provider, "model.provider", privacy::MAX_SHORT_TEXT)?;
            privacy::validate_safe_text(model_id, "model.model_id", privacy::MAX_SHORT_TEXT)?;
            privacy::validate_safe_text(
                model_version_or_dated_alias,
                "model.model_version_or_dated_alias",
                privacy::MAX_SHORT_TEXT,
            )?;
            privacy::validate_safe_text(
                invocation_date,
                "model.invocation_date",
                privacy::MAX_SHORT_TEXT,
            )?;
            privacy::validate_opaque_id(authorization_ref, "model.authorization_ref")?;
            validate_sorted_unique_texts(tools.clone(), "model.tools")?;
            if input_limits.max_input_tokens == Some(0)
                || input_limits.max_images == Some(0)
                || input_limits.max_input_bytes == Some(0)
            {
                return Err(ContractError::new(
                    "model input limits must be positive when present",
                ));
            }
            Ok(())
        }
        ModelAvailability::Unavailable { reason, recovery }
        | ModelAvailability::Blocked { reason, recovery }
        | ModelAvailability::Skipped { reason, recovery } => {
            validate_reason(reason, recovery, "model")
        }
    }
}

fn validate_sorted_unique_texts(values: Vec<String>, label: &str) -> Result<()> {
    let mut previous = None;
    for value in values {
        privacy::validate_opaque_id(&value, label)?;
        if previous
            .as_ref()
            .is_some_and(|item: &String| item >= &value)
        {
            return Err(ContractError::new(format!(
                "{label} must be sorted and unique"
            )));
        }
        previous = Some(value);
    }
    Ok(())
}

fn validate_krometrail(value: &KrometrailIdentity) -> Result<()> {
    privacy::validate_git_revision(&value.git_revision, "krometrail.git_revision")?;
    privacy::validate_sha256(&value.cargo_lock_sha256, "krometrail.cargo_lock_sha256")?;
    privacy::validate_safe_text(
        &value.rust_toolchain,
        "krometrail.rust_toolchain",
        privacy::MAX_SHORT_TEXT,
    )?;
    if value.capture_config.queue_capacity == 0 || value.capture_config.max_active_streams == 0 {
        return Err(ContractError::new(
            "capture config capacities must be positive",
        ));
    }
    if value.capture_config.ack_timeout_ms == 0 || value.capture_config.shutdown_timeout_ms == 0 {
        return Err(ContractError::new(
            "capture config timeouts must be positive",
        ));
    }
    let names = value
        .adapter_versions
        .iter()
        .map(|version| version.name.clone())
        .collect();
    validate_sorted_unique_texts(names, "krometrail.adapter_versions.name")?;
    for version in &value.adapter_versions {
        privacy::validate_safe_text(
            &version.version,
            "krometrail.adapter_versions.version",
            privacy::MAX_SHORT_TEXT,
        )?;
    }
    Ok(())
}

fn validate_artifact(value: &ArtifactIdentity, run: &RunConfiguration) -> Result<()> {
    if value.condition_id != run.condition_id {
        return Err(ContractError::new(
            "artifact condition does not match the run condition",
        ));
    }
    privacy::validate_opaque_id(&value.source_interval_id, "artifact.source_interval_id")?;
    let mut output_ids = BTreeSet::new();
    for output in value.output_ids.iter().chain(value.source_frame_ids.iter()) {
        privacy::validate_opaque_id(&output.id, "artifact output id")?;
        if !output_ids.insert(&output.id) {
            return Err(ContractError::new(
                "artifact evidence identifiers must be unique",
            ));
        }
        match (output.availability, output.sha256.as_deref()) {
            (EvidenceAvailability::Retained, Some(digest))
            | (EvidenceAvailability::Corrupt, Some(digest)) => {
                privacy::validate_sha256(digest, "artifact output sha256")?;
            }
            (EvidenceAvailability::Retained, None) => {
                return Err(ContractError::new(
                    "retained evidence must declare a digest",
                ));
            }
            (EvidenceAvailability::Corrupt, None) => {
                return Err(ContractError::new(
                    "corrupt evidence must declare its observed digest",
                ));
            }
            (_, Some(_)) => {
                return Err(ContractError::new(
                    "unavailable evidence cannot declare a digest",
                ));
            }
            (_, None) => {}
        }
    }
    for gap_id in &value.gap_ids {
        privacy::validate_opaque_id(gap_id, "artifact.gap_id")?;
    }
    for parameter in value.parameters.keys() {
        privacy::validate_opaque_id(parameter, "artifact.parameter name")?;
    }
    for parameter in value.parameters.values() {
        privacy::validate_safe_text(
            parameter,
            "artifact.parameter value",
            privacy::MAX_SHORT_TEXT,
        )?;
    }
    let names = value
        .algorithm_versions
        .iter()
        .map(|version| version.name.clone())
        .collect();
    validate_sorted_unique_texts(names, "artifact.algorithm_versions.name")?;
    for version in &value.algorithm_versions {
        privacy::validate_safe_text(
            &version.version,
            "artifact.algorithm_versions.version",
            privacy::MAX_SHORT_TEXT,
        )?;
    }
    Ok(())
}

fn validate_scoring(value: &ScoringIdentity) -> Result<()> {
    privacy::validate_safe_text(
        &value.rubric_version,
        "scoring.rubric_version",
        privacy::MAX_SHORT_TEXT,
    )?;
    if value.dimension_ids != ScoringDimensionId::ALL {
        return Err(ContractError::new(
            "scoring dimensions do not match the canonical registry",
        ));
    }
    privacy::validate_opaque_id(&value.aggregate_method, "scoring.aggregate_method")?;
    privacy::validate_opaque_id(&value.rationale_policy, "scoring.rationale_policy")
}

fn validate_range(value: &TimeRangeMs, label: &str) -> Result<()> {
    if value.start_ms > value.end_ms {
        return Err(ContractError::new(format!("{label} has inverted bounds")));
    }
    Ok(())
}

fn validate_ordinals(value: &CaptureOrdinalRange) -> Result<()> {
    if value.first > value.last {
        return Err(ContractError::new(
            "capture ordinal range has inverted bounds",
        ));
    }
    Ok(())
}

fn validate_failure(value: &FailureRecord, label: &str) -> Result<()> {
    privacy::validate_safe_text(
        &value.phase,
        &format!("{label}.phase"),
        privacy::MAX_SHORT_TEXT,
    )?;
    validate_reason(&value.reason, &value.recovery, label)
}

fn validate_rows(manifest: &RunManifest) -> Result<()> {
    if manifest.rows.is_empty() {
        return Err(ContractError::new(
            "run manifest must declare measured rows",
        ));
    }
    if manifest.rows.len() > 100_000 {
        return Err(ContractError::new(
            "manifest row list exceeds its contract bound",
        ));
    }
    let trials: BTreeMap<&str, &TrialIdentity> = manifest
        .run
        .ordered_trials
        .iter()
        .map(|trial| (trial.trial_id.as_str(), trial))
        .collect();
    let evidence: BTreeMap<&str, &OutputIdentity> = manifest
        .artifact
        .output_ids
        .iter()
        .chain(manifest.artifact.source_frame_ids.iter())
        .map(|output| (output.id.as_str(), output))
        .collect();
    let mut row_ids = BTreeSet::new();
    for row in &manifest.rows {
        privacy::validate_trial_id(&row.trial_id, "row.trial_id")?;
        if !row_ids.insert(&row.trial_id) {
            return Err(ContractError::new(
                "manifest row identifiers must be unique",
            ));
        }
        let trial = trials.get(row.trial_id.as_str()).ok_or_else(|| {
            ContractError::new("manifest row does not reference an ordered trial")
        })?;
        if row.case_id != trial.case_id
            || row.family != trial.family
            || row.duration_ms != trial.duration_ms
            || row.repetition != trial.repetition
            || row.condition_id != trial.condition_id
        {
            return Err(ContractError::new(
                "manifest row contradicts its trial identity",
            ));
        }
        for range in [
            row.source_time_range,
            row.observed_time_range,
            row.session_time_range,
        ]
        .into_iter()
        .flatten()
        {
            validate_range(&range, "row time range")?;
        }
        if let Some(ordinals) = row.capture_ordinal_range {
            validate_ordinals(&ordinals)?;
        }
        for gap_id in &row.gap_ids {
            privacy::validate_opaque_id(gap_id, "row.gap_id")?;
            if !manifest.artifact.gap_ids.contains(gap_id) {
                return Err(ContractError::new(
                    "row references an undeclared artifact gap",
                ));
            }
        }
        for artifact_id in &row.artifact_ids {
            privacy::validate_opaque_id(artifact_id, "row.artifact_id")?;
            if !evidence.contains_key(artifact_id.as_str()) {
                return Err(ContractError::new(
                    "row references an undeclared artifact output",
                ));
            }
        }
        let mut claim_ids = BTreeSet::new();
        if row.accepted_claims.len() > 32 {
            return Err(ContractError::new(
                "accepted claim list exceeds its contract bound",
            ));
        }
        for claim in &row.accepted_claims {
            privacy::validate_opaque_id(&claim.claim_id, "claim.claim_id")?;
            if !claim_ids.insert(&claim.claim_id) || claim.evidence_ids.is_empty() {
                return Err(ContractError::new(
                    "accepted claims must be unique and evidence-backed",
                ));
            }
            for evidence_id in &claim.evidence_ids {
                privacy::validate_opaque_id(evidence_id, "claim.evidence_id")?;
                let output = evidence.get(evidence_id.as_str()).ok_or_else(|| {
                    ContractError::new("accepted claim references unknown evidence")
                })?;
                if output.availability != EvidenceAvailability::Retained {
                    return Err(ContractError::new(
                        "accepted claims may reference retained evidence only",
                    ));
                }
            }
        }
        match (&row.answer_digest, &row.raw_answer_ref) {
            (Some(digest), Some(reference)) => {
                privacy::validate_sha256(digest, "row.answer_digest")?;
                privacy::validate_opaque_id(reference, "row.raw_answer_ref")?;
            }
            (None, None) => {}
            _ => {
                return Err(ContractError::new(
                    "answer digest and raw answer reference are paired fields",
                ));
            }
        }
        if let Some(score) = row.score {
            if row.scoring_rationale.is_none() {
                return Err(ContractError::new("a score must include scoring rationale"));
            }
            if score > 100 {
                return Err(ContractError::new("score must be between 0 and 100"));
            }
        }
        if let Some(rationale) = &row.scoring_rationale {
            privacy::validate_safe_text(
                rationale,
                "row.scoring_rationale",
                privacy::MAX_LONG_TEXT,
            )?;
        }
        if let Some(failure) = &row.failure {
            validate_failure(failure, "row.failure")?;
        }
        if row.status == EvaluationStatus::Pass && row.failure.is_some() {
            return Err(ContractError::new("a passing row cannot carry a failure"));
        }
        if row.status != EvaluationStatus::Pass && row.failure.is_none() {
            return Err(ContractError::new(
                "a non-passing row requires an explicit failure",
            ));
        }
    }
    Ok(())
}

fn required_repetitions(mode: ManifestRunMode) -> u16 {
    match mode {
        ManifestRunMode::Contract => 1,
        ManifestRunMode::Capture => CAPTURE_REPETITIONS,
        ManifestRunMode::Interpretation | ManifestRunMode::Debugging => INTERPRETATION_REPETITIONS,
    }
}

fn availability_is_required(mode: ManifestRunMode) -> bool {
    !matches!(mode, ManifestRunMode::Contract)
}

fn browser_is_observed(value: &BrowserAvailability) -> bool {
    matches!(value, BrowserAvailability::Observed { .. })
}

fn model_is_observed(value: &ModelAvailability) -> bool {
    matches!(value, ModelAvailability::Observed { .. })
}

fn browser_is_unavailable(value: &BrowserAvailability) -> bool {
    matches!(
        value,
        BrowserAvailability::Unavailable { .. }
            | BrowserAvailability::Blocked { .. }
            | BrowserAvailability::Skipped { .. }
    )
}

fn model_is_unavailable(value: &ModelAvailability) -> bool {
    matches!(
        value,
        ModelAvailability::Unavailable { .. }
            | ModelAvailability::Blocked { .. }
            | ModelAvailability::Skipped { .. }
    )
}

fn validate_outcome(manifest: &RunManifest) -> Result<()> {
    let rows_incomplete = manifest.rows.iter().any(|row| {
        matches!(
            row.status,
            EvaluationStatus::Inconclusive | EvaluationStatus::Blocked | EvaluationStatus::Skipped
        )
    });
    let rows_failed = manifest
        .rows
        .iter()
        .any(|row| row.status == EvaluationStatus::Fail);
    let mode = run_mode(&manifest.run)?;
    if mode != ManifestRunMode::Contract
        && manifest.status == EvaluationStatus::Pass
        && manifest.run.repetitions < required_repetitions(mode)
    {
        return Err(ContractError::new(
            "a passing run does not meet its repetition minimum",
        ));
    }
    if availability_is_required(mode)
        && matches!(
            manifest.status,
            EvaluationStatus::Pass | EvaluationStatus::Fail
        )
        && !browser_is_observed(&manifest.browser)
    {
        return Err(ContractError::new(
            "a decisive run requires observed browser identity",
        ));
    }
    if matches!(
        mode,
        ManifestRunMode::Interpretation | ManifestRunMode::Debugging
    ) && matches!(
        manifest.status,
        EvaluationStatus::Pass | EvaluationStatus::Fail
    ) && !model_is_observed(&manifest.model)
    {
        return Err(ContractError::new(
            "a passing interpretation requires observed model identity",
        ));
    }
    match manifest.status {
        EvaluationStatus::Pass => {
            if manifest.failure.is_some()
                || rows_incomplete
                || !manifest
                    .rows
                    .iter()
                    .all(|row| row.status == EvaluationStatus::Pass)
            {
                return Err(ContractError::new(
                    "a passing run must contain complete passing rows and no failure",
                ));
            }
        }
        EvaluationStatus::Fail => {
            let failure = manifest
                .failure
                .as_ref()
                .ok_or_else(|| ContractError::new("a failed run requires an explicit failure"))?;
            if rows_incomplete
                || !matches!(
                    failure.code,
                    RunFailureCode::Threshold | RunFailureCode::Validation
                )
                || !rows_failed
            {
                return Err(ContractError::new(
                    "a failed run must be complete and below threshold or invalid",
                ));
            }
            validate_failure(failure, "failure")?;
        }
        EvaluationStatus::Inconclusive => {
            let failure = manifest.failure.as_ref().ok_or_else(|| {
                ContractError::new("an inconclusive run requires an explicit failure")
            })?;
            if !rows_incomplete
                && !matches!(
                    failure.code,
                    RunFailureCode::InsufficientEvidence
                        | RunFailureCode::Unavailable
                        | RunFailureCode::Retention
                        | RunFailureCode::CaptureGap
                        | RunFailureCode::CorruptSource
                )
            {
                return Err(ContractError::new(
                    "inconclusive status must name incomplete evidence",
                ));
            }
            validate_failure(failure, "failure")?;
        }
        EvaluationStatus::Blocked => {
            let failure = manifest
                .failure
                .as_ref()
                .ok_or_else(|| ContractError::new("a blocked run requires an explicit failure"))?;
            if !matches!(
                failure.code,
                RunFailureCode::Unavailable
                    | RunFailureCode::Authorization
                    | RunFailureCode::Unsupported
            ) || (!browser_is_unavailable(&manifest.browser)
                && !model_is_unavailable(&manifest.model)
                && !manifest
                    .rows
                    .iter()
                    .any(|row| row.status == EvaluationStatus::Blocked))
            {
                return Err(ContractError::new(
                    "blocked status must name an unavailable dependency",
                ));
            }
            validate_failure(failure, "failure")?;
        }
        EvaluationStatus::Skipped => {
            let failure = manifest
                .failure
                .as_ref()
                .ok_or_else(|| ContractError::new("a skipped run requires an explicit failure"))?;
            if !manifest.run.optional_configuration
                || failure.code != RunFailureCode::OptionalUnavailable
                || manifest.environment.platform != Platform::Linux
                || !matches!(
                    manifest.browser,
                    BrowserAvailability::Skipped {
                        product: BrowserProduct::Chromium,
                        ..
                    }
                )
            {
                return Err(ContractError::new(
                    "only the optional Linux Chromium configuration may be skipped",
                ));
            }
            validate_failure(failure, "failure")?;
        }
    }
    if let Some(failure) = &manifest.failure {
        validate_failure(failure, "failure")?;
        if manifest.status == EvaluationStatus::Pass {
            return Err(ContractError::new(
                "passing manifests cannot carry a failure",
            ));
        }
    }
    if manifest.non_claims.is_empty() {
        return Err(ContractError::new("manifest must state its non-claims"));
    }
    for non_claim in &manifest.non_claims {
        privacy::validate_safe_text(non_claim, "manifest.non_claims", privacy::MAX_LONG_TEXT)?;
    }
    Ok(())
}

pub fn run_manifest_schema() -> schemars::Schema {
    schemars::schema_for!(RunManifest)
}

pub fn sample_manifest() -> RunManifest {
    RunManifest::sample()
}
