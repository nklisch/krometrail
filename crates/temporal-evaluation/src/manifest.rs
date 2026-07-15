use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    AnswerKind, BenchmarkDefinition, CaseFamily, ConditionId, EvaluationStatus, FixtureFile,
    MatrixOrder, PromptId, PromptTemplate, Result, ScoringDimensionId, canonical_json,
    conditions::canonical_conditions,
    matrix::{
        CAPTURE_REPETITIONS, INTERPRETATION_REPETITIONS, LIVE_NON_CLAIMS,
        LIVE_QUALIFICATION_PROFILE, MATRIX_SEED,
    },
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
                PromptId::CaptureQualification => AnswerKind::CaptureQualification,
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
    Cleanup,
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

/// Aggregate cache state for a qualification measurement.
///
/// This is derived from the exact disposition of every returned artifact, not from whether a
/// request was the first or a repeated invocation. `Cold` includes cache-miss regeneration after
/// invalidation; the per-artifact record retains that more specific disposition.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CacheDisposition {
    Cold,
    Warm,
    Mixed,
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DurationQualificationMeasurement {
    pub duration_ms: u16,
    pub eligible_count: u32,
    pub observed_count: u32,
    pub eligibility_rate_basis_points: u16,
    pub coverage_rate_basis_points: u16,
    pub status: EvaluationStatus,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CaptureQualificationMeasurements {
    pub requested_durations_ms: Vec<u16>,
    pub repetitions: u16,
    pub observed_viewport: Viewport,
    pub observed_device_scale_factor: u16,
    pub source_frame_count: u64,
    pub observed_frame_count: u64,
    pub source_time_sample_count: u64,
    pub gap_ids: Vec<String>,
    pub gap_count: u64,
    pub per_duration: Vec<DurationQualificationMeasurement>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ControlQualificationMeasurements {
    pub scenario_ids: Vec<String>,
    pub attempts: u64,
    pub successes: u64,
    pub failed_observation_ids: Vec<String>,
    pub success_rate_basis_points: u16,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RetentionQualificationMeasurements {
    pub budget_bytes: u64,
    pub peak_usage_bytes: u64,
    pub pinned_interval_preserved: bool,
    pub evicted_frame_count: u64,
    pub capture_paused_when_pinned: bool,
    pub capture_resumed_after_unpin: bool,
    pub cleanup_removed_frame_count: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecoveryQualificationMeasurements {
    pub reopened: bool,
    pub reconciled: bool,
    pub recovered_frame_count: u64,
    pub removed_frame_count: u64,
    pub trailing_segment_repaired: bool,
    pub staged_artifacts_recovered: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ResourceQualificationMeasurements {
    pub sample_count: u64,
    pub rss_bytes: Vec<u64>,
    pub cpu_millis: Vec<u64>,
    pub browser_child_accounting_available: bool,
    pub unavailable_reason: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LatencyQualificationMeasurements {
    pub source_interval_id: String,
    pub viewport: Viewport,
    pub frame_width: u32,
    pub frame_height: u32,
    pub warm_cache: CacheDisposition,
    pub temporal_query_elapsed_ms: Vec<u64>,
    pub artifact_elapsed_ms: Vec<u64>,
    pub sample_count: u64,
    pub threshold_profile_ids: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CleanupQualificationMeasurements {
    pub server_stopped: bool,
    pub profile_deleted: bool,
    pub store_flushed: bool,
    pub lock_released: bool,
    pub output_finalized: bool,
    pub remaining_managed_resources: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QualificationGateResult {
    pub gate: crate::QualificationGateId,
    pub status: EvaluationStatus,
    pub failure: Option<FailureRecord>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QualificationEvidenceMode {
    CodeHarness,
    OperatorAuthorizedLiveCapture,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LiveQualification {
    pub profile: String,
    pub evidence_mode: QualificationEvidenceMode,
    pub gates: Vec<QualificationGateResult>,
    pub capture: CaptureQualificationMeasurements,
    pub control: ControlQualificationMeasurements,
    pub retention: RetentionQualificationMeasurements,
    pub recovery: RecoveryQualificationMeasurements,
    pub resources: ResourceQualificationMeasurements,
    pub latency: LatencyQualificationMeasurements,
    pub cleanup: CleanupQualificationMeasurements,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qualification: Option<LiveQualification>,
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
        for key in ["rows", "qualification", "status", "non_claims", "failure"] {
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
            qualification: None,
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
            ManifestRunMode::Qualification => PromptId::CaptureQualification,
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
        validate_qualification(self)?;
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
    Qualification,
}

fn run_mode(value: &RunConfiguration) -> Result<ManifestRunMode> {
    match value.threshold_profile.as_str() {
        "contract-only" => Ok(ManifestRunMode::Contract),
        "capture-v1" => Ok(ManifestRunMode::Capture),
        "interpretation-v1" => Ok(ManifestRunMode::Interpretation),
        "debugging-v1" => Ok(ManifestRunMode::Debugging),
        LIVE_QUALIFICATION_PROFILE => Ok(ManifestRunMode::Qualification),
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
        ManifestRunMode::Qualification => MatrixOrder::FamilyCaseDurationRepetition,
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
    if matches!(run_mode(value)?, ManifestRunMode::Qualification)
        && value.device_scale_factor != 1_000
    {
        return Err(ContractError::new(
            "live qualification requires device scale one",
        ));
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

fn validate_qualification(manifest: &RunManifest) -> Result<()> {
    let mode = run_mode(&manifest.run)?;
    match (&manifest.qualification, mode) {
        (None, ManifestRunMode::Qualification) => Err(ContractError::new(
            "live qualification profile requires qualification measurements",
        )),
        (Some(_), mode) if mode != ManifestRunMode::Qualification => Err(ContractError::new(
            "qualification measurements are only valid for the live qualification profile",
        )),
        (None, _) => Ok(()),
        (Some(value), ManifestRunMode::Qualification) => validate_live_qualification(value),
        (Some(_), _) => Err(ContractError::new(
            "qualification measurements are only valid for the live qualification profile",
        )),
    }
}

fn validate_live_qualification(value: &LiveQualification) -> Result<()> {
    if value.profile != LIVE_QUALIFICATION_PROFILE {
        return Err(ContractError::new(
            "qualification profile is not registered",
        ));
    }
    if value.gates.len() != crate::QualificationGateId::ALL.len()
        || value.gates.iter().map(|gate| gate.gate).collect::<Vec<_>>()
            != crate::QualificationGateId::ALL
    {
        return Err(ContractError::new(
            "qualification gates must appear exactly once in registry order",
        ));
    }
    for gate in &value.gates {
        match (&gate.status, &gate.failure) {
            (EvaluationStatus::Pass, None) => {}
            (EvaluationStatus::Pass, Some(_)) => {
                return Err(ContractError::new(
                    "passing qualification gate cannot carry a failure",
                ));
            }
            (_, None) => {
                return Err(ContractError::new(
                    "non-passing qualification gate requires a failure",
                ));
            }
            (_, Some(failure)) => validate_failure(failure, "qualification.gate.failure")?,
        }
    }
    let capture_gate = value
        .gates
        .iter()
        .find(|gate| gate.gate == crate::QualificationGateId::CaptureEnvelope)
        .expect("gate registry validation guarantees capture envelope");
    validate_capture_measurements(&value.capture, capture_gate.status)?;
    let canonical_capture_observation = value.capture.observed_viewport.width == VIEWPORT_WIDTH
        && value.capture.observed_viewport.height == VIEWPORT_HEIGHT
        && value.capture.observed_device_scale_factor == 1_000;
    if !canonical_capture_observation
        && !matches!(
            capture_gate.status,
            EvaluationStatus::Blocked | EvaluationStatus::Skipped
        )
    {
        return Err(ContractError::new(
            "noncanonical capture observation must block or skip the capture envelope",
        ));
    }
    validate_control_measurements(&value.control)?;
    validate_retention_measurements(&value.retention)?;
    validate_recovery_measurements(&value.recovery)?;
    validate_resource_measurements(&value.resources)?;
    validate_latency_measurements(&value.latency)?;
    Ok(())
}

fn validate_rate(value: u16, label: &str) -> Result<()> {
    if value > 10_000 {
        return Err(ContractError::new(format!(
            "{label} must be at most 10000 basis points"
        )));
    }
    Ok(())
}

fn validate_capture_measurements(
    value: &CaptureQualificationMeasurements,
    gate_status: EvaluationStatus,
) -> Result<()> {
    if value.requested_durations_ms != DURATIONS_MS || value.repetitions == 0 {
        return Err(ContractError::new(
            "qualification capture matrix is not canonical",
        ));
    }
    if (value.observed_viewport.width == 0
        || value.observed_viewport.height == 0
        || value.observed_device_scale_factor == 0)
        && matches!(gate_status, EvaluationStatus::Pass | EvaluationStatus::Fail)
    {
        return Err(ContractError::new(
            "decisive qualification capture observation must report a positive viewport and scale",
        ));
    }
    if value.gap_count != value.gap_ids.len() as u64 {
        return Err(ContractError::new(
            "qualification gap count does not match gap identifiers",
        ));
    }
    validate_unique_ids(&value.gap_ids, "qualification.gap_ids")?;
    if value.per_duration.len() != DURATIONS_MS.len()
        || value
            .per_duration
            .iter()
            .map(|measurement| measurement.duration_ms)
            .collect::<Vec<_>>()
            != DURATIONS_MS
    {
        return Err(ContractError::new(
            "qualification duration measurements must cover the canonical matrix exactly once",
        ));
    }
    for measurement in &value.per_duration {
        if !DURATIONS_MS.contains(&measurement.duration_ms) {
            return Err(ContractError::new(
                "qualification duration is not in the canonical matrix",
            ));
        }
        validate_rate(
            measurement.eligibility_rate_basis_points,
            "qualification eligibility rate",
        )?;
        validate_rate(
            measurement.coverage_rate_basis_points,
            "qualification coverage rate",
        )?;
    }
    Ok(())
}

fn validate_control_measurements(value: &ControlQualificationMeasurements) -> Result<()> {
    validate_unique_trial_ids(&value.scenario_ids, "qualification.control.scenario_ids")?;
    validate_unique_trial_ids(
        &value.failed_observation_ids,
        "qualification.control.failed_observation_ids",
    )?;
    if value.successes > value.attempts {
        return Err(ContractError::new(
            "qualification control successes exceed attempts",
        ));
    }
    validate_rate(
        value.success_rate_basis_points,
        "qualification control success rate",
    )
}

fn validate_retention_measurements(value: &RetentionQualificationMeasurements) -> Result<()> {
    if value.peak_usage_bytes < value.budget_bytes && value.capture_paused_when_pinned {
        return Err(ContractError::new(
            "qualification retention pause contradicts its budget observation",
        ));
    }
    Ok(())
}

fn validate_recovery_measurements(_value: &RecoveryQualificationMeasurements) -> Result<()> {
    Ok(())
}

fn validate_resource_measurements(value: &ResourceQualificationMeasurements) -> Result<()> {
    if value.sample_count == 0 && value.unavailable_reason.is_none() {
        return Err(ContractError::new(
            "qualification resources need samples or an unavailable reason",
        ));
    }
    if let Some(reason) = &value.unavailable_reason {
        privacy::validate_safe_text(
            reason,
            "qualification.resources.unavailable_reason",
            privacy::MAX_LONG_TEXT,
        )?;
    }
    Ok(())
}

fn validate_latency_measurements(value: &LatencyQualificationMeasurements) -> Result<()> {
    privacy::validate_opaque_id(
        &value.source_interval_id,
        "qualification.latency.source_interval_id",
    )?;
    validate_unique_ids(
        &value.threshold_profile_ids,
        "qualification.latency.threshold_profile_ids",
    )?;
    if value.sample_count == 0
        && (!value.temporal_query_elapsed_ms.is_empty() || !value.artifact_elapsed_ms.is_empty())
    {
        return Err(ContractError::new(
            "qualification latency sample count is inconsistent",
        ));
    }
    Ok(())
}

fn validate_complete_qualification(value: &LiveQualification) -> Result<()> {
    if value.capture.source_frame_count == 0
        || value.capture.observed_frame_count == 0
        || value.capture.source_time_sample_count == 0
        || value.capture.gap_count != 0
        || value.capture.per_duration.iter().any(|measurement| {
            measurement.status != EvaluationStatus::Pass
                || measurement.eligible_count == 0
                || measurement.observed_count == 0
        })
    {
        return Err(ContractError::new(
            "a passing qualification requires complete gap-free capture measurements",
        ));
    }
    if value.control.attempts == 0
        || value.control.successes != value.control.attempts
        || !value.control.failed_observation_ids.is_empty()
        || value.control.success_rate_basis_points != 10_000
    {
        return Err(ContractError::new(
            "a passing qualification requires successful observed control operations",
        ));
    }
    if value.retention.peak_usage_bytes > value.retention.budget_bytes
        || !value.retention.pinned_interval_preserved
    {
        return Err(ContractError::new(
            "a passing qualification requires bounded retention and preserved pinned evidence",
        ));
    }
    if !value.recovery.reopened
        || !value.recovery.reconciled
        || !value.recovery.staged_artifacts_recovered
    {
        return Err(ContractError::new(
            "a passing qualification requires successful recovery and reconciliation",
        ));
    }
    if value.resources.sample_count == 0
        || value.resources.rss_bytes.len() != value.resources.sample_count as usize
        || value.resources.cpu_millis.len() != value.resources.sample_count as usize
        || !value.resources.browser_child_accounting_available
        || value.resources.unavailable_reason.is_some()
    {
        return Err(ContractError::new(
            "a passing qualification requires complete resource measurements",
        ));
    }
    if value.latency.frame_width != 1_920
        || value.latency.frame_height != 1_080
        || value.latency.warm_cache != CacheDisposition::Warm
        || value.latency.sample_count == 0
        || value.latency.temporal_query_elapsed_ms.is_empty()
        || value.latency.artifact_elapsed_ms.is_empty()
    {
        return Err(ContractError::new(
            "a passing qualification requires complete scoped latency measurements",
        ));
    }
    if !value.cleanup.server_stopped
        || !value.cleanup.profile_deleted
        || !value.cleanup.store_flushed
        || !value.cleanup.lock_released
        || value.cleanup.remaining_managed_resources != 0
    {
        return Err(ContractError::new(
            "a passing qualification requires complete cleanup evidence",
        ));
    }
    Ok(())
}

fn validate_unique_trial_ids(values: &[String], label: &str) -> Result<()> {
    let mut ids = BTreeSet::new();
    for value in values {
        privacy::validate_trial_id(value, label)?;
        if !ids.insert(value) {
            return Err(ContractError::new(format!("{label} must be unique")));
        }
    }
    Ok(())
}

fn validate_unique_ids(values: &[String], label: &str) -> Result<()> {
    let mut ids = BTreeSet::new();
    for value in values {
        privacy::validate_opaque_id(value, label)?;
        if !ids.insert(value) {
            return Err(ContractError::new(format!("{label} must be unique")));
        }
    }
    Ok(())
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
        ManifestRunMode::Qualification => CAPTURE_REPETITIONS,
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
    let qualification_gates_failed = manifest
        .qualification
        .as_ref()
        .is_some_and(|qualification| {
            qualification
                .gates
                .iter()
                .any(|gate| gate.status == EvaluationStatus::Fail)
        });
    let qualification_gates_incomplete =
        manifest
            .qualification
            .as_ref()
            .is_some_and(|qualification| {
                qualification.gates.iter().any(|gate| {
                    matches!(
                        gate.status,
                        EvaluationStatus::Inconclusive
                            | EvaluationStatus::Blocked
                            | EvaluationStatus::Skipped
                    )
                })
            });
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
    if mode == ManifestRunMode::Qualification {
        let expected = manifest
            .rows
            .iter()
            .map(|row| row.status)
            .chain(
                manifest
                    .qualification
                    .as_ref()
                    .expect("qualification mode validation guarantees measurements")
                    .gates
                    .iter()
                    .map(|gate| gate.status),
            )
            .max_by_key(|status| status.precedence())
            .unwrap_or(EvaluationStatus::Inconclusive);
        if manifest.status != expected {
            return Err(ContractError::new(
                "qualification status does not follow blocked/skipped/inconclusive/fail/pass precedence",
            ));
        }
        if expected == EvaluationStatus::Pass {
            validate_complete_qualification(
                manifest
                    .qualification
                    .as_ref()
                    .expect("qualification mode validation guarantees measurements"),
            )?;
        }
    }
    match manifest.status {
        EvaluationStatus::Pass => {
            if manifest.failure.is_some()
                || rows_incomplete
                || !manifest
                    .rows
                    .iter()
                    .all(|row| row.status == EvaluationStatus::Pass)
                || qualification_gates_incomplete
                || manifest
                    .qualification
                    .as_ref()
                    .is_some_and(|qualification| {
                        qualification
                            .gates
                            .iter()
                            .any(|gate| gate.status != EvaluationStatus::Pass)
                    })
            {
                return Err(ContractError::new(
                    "a passing run must contain complete passing rows and qualification gates with no failure",
                ));
            }
        }
        EvaluationStatus::Fail => {
            let failure = manifest
                .failure
                .as_ref()
                .ok_or_else(|| ContractError::new("a failed run requires an explicit failure"))?;
            if rows_incomplete
                || qualification_gates_incomplete
                || !matches!(
                    failure.code,
                    RunFailureCode::Threshold | RunFailureCode::Validation
                )
                || (!rows_failed && !qualification_gates_failed)
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
                && !qualification_gates_incomplete
                && !matches!(
                    failure.code,
                    RunFailureCode::InsufficientEvidence
                        | RunFailureCode::Unavailable
                        | RunFailureCode::Retention
                        | RunFailureCode::CaptureGap
                        | RunFailureCode::CorruptSource
                        | RunFailureCode::Cleanup
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
                    .any(|row| row.status == EvaluationStatus::Blocked)
                && !manifest
                    .qualification
                    .as_ref()
                    .is_some_and(|qualification| {
                        qualification
                            .gates
                            .iter()
                            .any(|gate| gate.status == EvaluationStatus::Blocked)
                    }))
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
                || !manifest.rows.iter().all(|row| {
                    row.status == EvaluationStatus::Skipped
                        && row.failure.as_ref().is_some_and(|failure| {
                            failure.code == RunFailureCode::OptionalUnavailable
                        })
                })
                || (manifest.qualification.is_some()
                    && !manifest
                        .qualification
                        .as_ref()
                        .is_some_and(|qualification| {
                            qualification.gates.iter().all(|gate| {
                                gate.status == EvaluationStatus::Skipped
                                    && gate.failure.as_ref().is_some_and(|failure| {
                                        failure.code == RunFailureCode::OptionalUnavailable
                                    })
                            })
                        }))
            {
                return Err(ContractError::new(
                    "only the optional Linux Chromium configuration with explicitly skipped rows and optional-unavailability failures may be skipped",
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
    if mode == ManifestRunMode::Qualification
        && manifest.non_claims
            != LIVE_NON_CLAIMS
                .iter()
                .map(|claim| (*claim).to_owned())
                .collect::<Vec<_>>()
    {
        return Err(ContractError::new(
            "live qualification non-claims do not match the canonical registry",
        ));
    }
    Ok(())
}

pub fn run_manifest_schema() -> schemars::Schema {
    schemars::schema_for!(RunManifest)
}

pub fn sample_manifest() -> RunManifest {
    RunManifest::sample()
}
