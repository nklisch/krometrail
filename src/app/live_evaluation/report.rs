//! Final assembly and atomic publication for the test-only live qualification.
//!
//! This module is intentionally the only place that turns the independent production
//! measurements into a `RunManifest`. It does not manufacture measurements: absent observations
//! remain explicit non-passing evidence, and every decisive status is derived from rows and the
//! ordered qualification-gate registry.

use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use krometrail_core::{
    BrowserProduct as CoreBrowserProduct, BrowserVersion, DiskBudgetBytes, ErrorCode,
    KrometrailError, Result,
};
use serde::Serialize;
use temporal_evaluation::{
    Architecture, ArtifactIdentity, BenchmarkDefinition, BrowserAvailability, BrowserProduct,
    CacheDisposition, CaptureOrdinalRange, CaptureQualificationMeasurements, ConditionId,
    ControlQualificationMeasurements, DURATIONS_MS, EnvironmentIdentity, EvaluationStatus,
    EvidenceAvailability, FailureRecord, ImageFormat, KrometrailIdentity, LIVE_NON_CLAIMS,
    LIVE_QUALIFICATION_PROFILE, LatencyQualificationMeasurements, LiveQualification, MANIFEST_KIND,
    MANIFEST_SCHEMA_VERSION, ManifestFixture, ManifestPrompt, ManifestRow, MatrixOrder,
    ModelAvailability, NamedVersion, QualificationEvidenceMode, QualificationGateId,
    QualificationGateResult, RecoveryQualificationMeasurements, ResourceQualificationMeasurements,
    RetentionQualificationMeasurements, RetentionState, RevisionIdentity, RunConfiguration,
    RunFailureCode, RunManifest, ScorerIdentity, ScoringDimensionId, ScoringIdentity,
    TrialIdentity, VIEWPORT_HEIGHT, VIEWPORT_WIDTH, Viewport,
};

use super::{
    CleanupObservation, LiveQualificationConfig,
    capture::{CaptureQualificationRun, CaptureTrialMeasurement, canonical_manifest_trials},
    control::{ControlQualificationRun, control_scenarios},
    latency::{ArtifactIdentityObservation, LatencyObservation},
    live_error,
    recovery::RecoveryObservation,
    resource_usage::ResourceObservation,
    retention::RetentionObservation,
};

const MANIFEST_FILE: &str = "run-manifest.json";
const PARTIAL_FILE: &str = "run-manifest.json.partial";
const FINALIZATION_ERROR_FILE: &str = "finalization-error.json";
const NOT_COLLECTED_ID: &str = "not-collected";
const ZERO_REVISION: &str = "0000000000000000000000000000000000000000";
const ZERO_SHA256: &str = "sha256:0000000000000000000000000000000000000000000000000000000000000000";

/// All inputs required by final assembly. The optional fields are an explicit representation of a
/// preflight or runtime failure; they are not permission to synthesize a passing measurement.
#[derive(Clone, Debug)]
pub struct QualificationObservations {
    pub environment: Option<EnvironmentIdentity>,
    pub harness: Option<RevisionIdentity>,
    pub krometrail: Option<KrometrailIdentity>,
    pub browser: BrowserAvailability,
    pub optional_configuration: bool,
    pub evidence_mode: QualificationEvidenceMode,
    pub retention_budget: DiskBudgetBytes,
    pub capture: Option<CaptureQualificationRun>,
    pub control: Option<ControlQualificationRun>,
    pub retention: Option<RetentionObservation>,
    pub recovery: Option<RecoveryObservation>,
    pub resources: Option<ResourceObservation>,
    pub latency: Option<LatencyObservation>,
    pub cleanup: CleanupObservation,
}

impl Default for QualificationObservations {
    fn default() -> Self {
        Self {
            environment: None,
            harness: None,
            krometrail: None,
            browser: BrowserAvailability::Blocked {
                reason: "required browser qualification precondition is unavailable".into(),
                recovery: "provide the declared local browser and retry the authorized run".into(),
            },
            optional_configuration: false,
            evidence_mode: QualificationEvidenceMode::CodeHarness,
            retention_budget: DiskBudgetBytes::default(),
            capture: None,
            control: None,
            retention: None,
            recovery: None,
            resources: None,
            latency: None,
            cleanup: CleanupObservation::default(),
        }
    }
}

impl QualificationObservations {
    /// A useful test-only identity seed. It contains no host paths and deliberately describes no
    /// browser or measurement; callers still have to provide observed production rows.
    pub fn contract_seed() -> Self {
        Self {
            environment: Some(EnvironmentIdentity {
                platform: temporal_evaluation::Platform::Linux,
                architecture: Architecture::X86_64,
                os_release_class: "qualification-test-host".into(),
            }),
            harness: Some(RevisionIdentity {
                git_revision: ZERO_REVISION.into(),
                sha256: ZERO_SHA256.into(),
            }),
            krometrail: Some(KrometrailIdentity {
                git_revision: ZERO_REVISION.into(),
                cargo_lock_sha256: ZERO_SHA256.into(),
                rust_toolchain: "rustc-1.85.0".into(),
                capture_config: temporal_evaluation::CaptureConfigIdentity {
                    queue_capacity: 1,
                    max_active_streams: 1,
                    ack_timeout_ms: 1_000,
                    shutdown_timeout_ms: 1_000,
                },
                adapter_versions: Vec::new(),
            }),
            ..Self::default()
        }
    }
}

/// Convert the connector's observed `Browser.getVersion` projection without retaining executable
/// paths, user-agent text, or other adapter details.
pub fn observed_browser(
    version: &BrowserVersion,
    capability_id: impl Into<String>,
) -> BrowserAvailability {
    let product = match version.product {
        CoreBrowserProduct::Chrome => BrowserProduct::Chrome,
        CoreBrowserProduct::Chromium => BrowserProduct::Chromium,
        CoreBrowserProduct::ElectronRenderer | CoreBrowserProduct::OtherChromium => {
            BrowserProduct::OtherChromium
        }
    };
    BrowserAvailability::Observed {
        product,
        product_version: version.product_version().as_str().into(),
        protocol_version: version.protocol_version().into(),
        revision: version.revision().into(),
        capability_id: capability_id.into(),
    }
}

/// Build the one canonical manifest from observed production authorities and canonical benchmark
/// definitions. No output is written by this function.
pub fn assemble_manifest(observations: QualificationObservations) -> Result<RunManifest> {
    let definition = BenchmarkDefinition::canonical();
    definition
        .validate()
        .map_err(|_| report_error("canonical benchmark definition is invalid"))?;
    let ordered_trials = canonical_manifest_trials(&definition)?;
    let environment = observations
        .environment
        .ok_or_else(|| report_error("qualification environment identity is missing"))?;
    let harness = observations
        .harness
        .ok_or_else(|| report_error("qualification harness identity is missing"))?;
    let krometrail = observations
        .krometrail
        .ok_or_else(|| report_error("qualification Krometrail identity is missing"))?;

    let missing = missing_observation_status(&observations.browser);
    let capture = observations.capture.as_ref();
    let (capture_measurements, rows, row_status, row_failure) = assemble_capture(
        capture,
        &ordered_trials,
        missing.clone(),
        observations.retention_budget.get(),
    )?;
    let (control_measurements, control_status, control_failure) =
        assemble_control(observations.control.as_ref(), missing.clone());
    let (retention_measurements, retention_status, retention_failure) = assemble_retention(
        observations.retention.as_ref(),
        observations.retention_budget.get(),
        missing.clone(),
    );
    let (recovery_measurements, recovery_status, recovery_failure) =
        assemble_recovery(observations.recovery.as_ref(), missing.clone());
    let (resource_measurements, resource_status, resource_failure) =
        assemble_resources(observations.resources.as_ref(), missing.clone());
    let (latency_measurements, latency_status, latency_failure) =
        assemble_latency(observations.latency.as_ref(), missing.clone());
    let no_measurements = observations.capture.is_none()
        && observations.control.is_none()
        && observations.retention.is_none()
        && observations.recovery.is_none()
        && observations.resources.is_none()
        && observations.latency.is_none();
    let cleanup_status =
        if no_measurements && matches!(observations.browser, BrowserAvailability::Skipped { .. }) {
            EvaluationStatus::Skipped
        } else if observations.cleanup.is_clean() {
            EvaluationStatus::Pass
        } else {
            EvaluationStatus::Inconclusive
        };
    let cleanup_failure = if cleanup_status == EvaluationStatus::Pass {
        None
    } else if cleanup_status == EvaluationStatus::Skipped {
        Some(missing.1.clone())
    } else {
        Some(cleanup_failure())
    };

    let gate_inputs = [
        (
            QualificationGateId::CaptureEnvelope,
            row_status,
            row_failure.clone(),
        ),
        (
            QualificationGateId::TimingIntegrity,
            capture_status(capture, missing.clone()),
            capture_failure(capture, missing.clone()),
        ),
        (
            QualificationGateId::MovementSequence,
            capture_status(capture, missing.clone()),
            capture_failure(capture, missing.clone()),
        ),
        (
            QualificationGateId::ControlReliability,
            control_status,
            control_failure.clone(),
        ),
        (
            QualificationGateId::Retention,
            retention_status,
            retention_failure.clone(),
        ),
        (
            QualificationGateId::Recovery,
            recovery_status,
            recovery_failure.clone(),
        ),
        (
            QualificationGateId::ResourceUsage,
            resource_status,
            resource_failure.clone(),
        ),
        (
            QualificationGateId::TemporalQueryLatency,
            latency_status,
            latency_failure.clone(),
        ),
        (
            QualificationGateId::ArtifactLatency,
            latency_status,
            latency_failure.clone(),
        ),
        (
            QualificationGateId::Cleanup,
            cleanup_status,
            cleanup_failure.clone(),
        ),
    ];
    let gates = gate_inputs
        .into_iter()
        .map(|(gate, status, failure)| QualificationGateResult {
            gate,
            status,
            failure: failure_for_status(status, failure, gate_phase(gate)),
        })
        .collect::<Vec<_>>();
    debug_assert_eq!(
        gates.iter().map(|gate| gate.gate).collect::<Vec<_>>(),
        QualificationGateId::ALL
    );

    let qualification = LiveQualification {
        profile: LIVE_QUALIFICATION_PROFILE.into(),
        evidence_mode: observations.evidence_mode,
        gates,
        capture: capture_measurements,
        control: control_measurements,
        retention: retention_measurements,
        recovery: recovery_measurements,
        resources: resource_measurements,
        latency: latency_measurements,
        cleanup: temporal_evaluation::CleanupQualificationMeasurements {
            server_stopped: observations.cleanup.server_stopped,
            profile_deleted: observations.cleanup.profile_deleted,
            store_flushed: observations.cleanup.store_flushed,
            lock_released: observations.cleanup.lock_released,
            output_finalized: false,
            remaining_managed_resources: observations.cleanup.remaining_managed_resources,
        },
    };
    let status = aggregate_status(
        rows.iter().map(|row| row.status),
        qualification.gates.iter().map(|gate| gate.status),
    );
    let failure = first_failure(status, &rows, &qualification.gates)
        .or_else(|| browser_failure(&observations.browser, status));

    let mut manifest = RunManifest {
        schema_version: MANIFEST_SCHEMA_VERSION,
        kind: MANIFEST_KIND.into(),
        benchmark_id: temporal_evaluation::BENCHMARK_ID.into(),
        benchmark_definition: RevisionIdentity {
            git_revision: ZERO_REVISION.into(),
            sha256: definition
                .definition_digest()
                .map_err(|_| report_error("canonical benchmark digest is invalid"))?,
        },
        harness,
        scorer: ScorerIdentity {
            git_revision: ZERO_REVISION.into(),
            version: "qualification-not-used".into(),
        },
        fixture: ManifestFixture {
            root_relative_path: temporal_evaluation::FIXTURE_ROOT.into(),
            ordered_files: definition.fixture.files.clone(),
            definition_sha256: definition
                .definition_digest()
                .map_err(|_| report_error("canonical fixture digest is invalid"))?,
        },
        run: RunConfiguration {
            seed: temporal_evaluation::MATRIX_SEED,
            order_policy: MatrixOrder::FamilyCaseDurationRepetition,
            ordered_trials,
            condition_id: ConditionId::AFinalScreenshot,
            duration_ms: DURATIONS_MS.to_vec(),
            repetitions: temporal_evaluation::CAPTURE_REPETITIONS,
            optional_configuration: observations.optional_configuration,
            viewport: Viewport {
                width: VIEWPORT_WIDTH,
                height: VIEWPORT_HEIGHT,
            },
            device_scale_factor: temporal_evaluation::DEVICE_SCALE_FACTOR_MILLI,
            image_format: ImageFormat::Png,
            image_quality: None,
            retention_budget_bytes: observations.retention_budget.get(),
            threshold_profile: LIVE_QUALIFICATION_PROFILE.into(),
        },
        environment,
        browser: observations.browser,
        krometrail,
        model: ModelAvailability::NotRequired,
        prompt: qualification_prompt()?,
        artifact: artifact_identity(
            capture,
            observations.latency.as_ref(),
            ConditionId::AFinalScreenshot,
        )?,
        scoring: ScoringIdentity {
            rubric_version: "qualification-not-applicable".into(),
            dimension_ids: ScoringDimensionId::ALL.to_vec(),
            aggregate_method: "qualification-gates".into(),
            rationale_policy: "not_applicable".into(),
        },
        rows,
        qualification: Some(qualification),
        status,
        non_claims: LIVE_NON_CLAIMS
            .iter()
            .map(|claim| (*claim).into())
            .collect(),
        failure,
    };
    if manifest.status == EvaluationStatus::Pass {
        manifest
            .qualification
            .as_mut()
            .expect("qualification is present")
            .cleanup
            .output_finalized = false;
    }
    manifest
        .validate()
        .map_err(|_| report_error("assembled live qualification failed canonical validation"))?;
    Ok(manifest)
}

fn assemble_capture(
    run: Option<&CaptureQualificationRun>,
    ordered_trials: &[TrialIdentity],
    missing: (EvaluationStatus, FailureRecord),
    retention_budget: u64,
) -> Result<(
    CaptureQualificationMeasurements,
    Vec<ManifestRow>,
    EvaluationStatus,
    Option<FailureRecord>,
)> {
    let Some(run) = run else {
        let (status, failure) = missing;
        let measurements = missing_capture(status, retention_budget);
        let rows = ordered_trials
            .iter()
            .map(|trial| empty_row(trial, status, failure.clone()))
            .collect();
        return Ok((measurements, rows, status, Some(failure)));
    };
    if run.manifest_trials != ordered_trials || run.measurements.len() != ordered_trials.len() {
        return Err(report_error(
            "capture measurements do not match the canonical trial matrix",
        ));
    }
    let rows = run
        .measurements
        .iter()
        .map(row_from_capture)
        .collect::<Result<Vec<_>>>()?;
    let status = aggregate_status(std::iter::empty(), rows.iter().map(|row| row.status));
    let failure = rows
        .iter()
        .find(|row| row.status != EvaluationStatus::Pass)
        .and_then(|row| row.failure.clone());
    Ok((run.capture.clone(), rows, status, failure))
}

fn row_from_capture(measurement: &CaptureTrialMeasurement) -> Result<ManifestRow> {
    let trial = TrialIdentity {
        trial_id: measurement.trial.trial_id.clone(),
        case_id: measurement.trial.case_id.clone(),
        family: measurement.trial.family,
        duration_ms: measurement.trial.duration_ms,
        repetition: measurement.trial.repetition,
        condition_id: ConditionId::AFinalScreenshot,
    };
    let interval = measurement.interval.as_ref();
    let frames = interval.map(|value| value.frames.as_slice()).unwrap_or(&[]);
    let source_time_range =
        range_from_values(frames.iter().filter_map(|frame| frame.source_time_ns));
    let observed_time_range = range_from_values(frames.iter().map(|frame| frame.observed_time_ns));
    let session_time_range = range_from_values(frames.iter().map(|frame| frame.session_time_ns));
    Ok(ManifestRow {
        trial_id: trial.trial_id.clone(),
        case_id: trial.case_id,
        family: trial.family,
        duration_ms: trial.duration_ms,
        repetition: trial.repetition,
        condition_id: trial.condition_id,
        capture_ordinal_range: ordinal_range(frames),
        source_time_range,
        observed_time_range,
        session_time_range,
        gap_ids: interval.map(|value| value.gap_ids()).unwrap_or_default(),
        retention_state: interval
            .map(|value| value.retention)
            .unwrap_or(RetentionState::Unavailable),
        artifact_ids: Vec::new(),
        accepted_claims: Vec::new(),
        answer_digest: None,
        raw_answer_ref: None,
        score: None,
        scoring_rationale: None,
        status: measurement.status,
        failure: failure_for_status(measurement.status, measurement.failure.clone(), "capture"),
    })
}

fn empty_row(
    trial: &TrialIdentity,
    status: EvaluationStatus,
    failure: FailureRecord,
) -> ManifestRow {
    ManifestRow {
        trial_id: trial.trial_id.clone(),
        case_id: trial.case_id.clone(),
        family: trial.family,
        duration_ms: trial.duration_ms,
        repetition: trial.repetition,
        condition_id: trial.condition_id,
        capture_ordinal_range: None,
        source_time_range: None,
        observed_time_range: None,
        session_time_range: None,
        gap_ids: Vec::new(),
        retention_state: RetentionState::Unavailable,
        artifact_ids: Vec::new(),
        accepted_claims: Vec::new(),
        answer_digest: None,
        raw_answer_ref: None,
        score: None,
        scoring_rationale: None,
        status,
        failure: Some(failure),
    }
}

fn assemble_control(
    value: Option<&ControlQualificationRun>,
    missing: (EvaluationStatus, FailureRecord),
) -> (
    ControlQualificationMeasurements,
    EvaluationStatus,
    Option<FailureRecord>,
) {
    value.map_or_else(
        || {
            let (status, failure) = missing;
            (
                ControlQualificationMeasurements {
                    scenario_ids: control_scenarios()
                        .into_iter()
                        .map(|scenario| scenario.scenario_id)
                        .collect(),
                    attempts: 0,
                    successes: 0,
                    failed_observation_ids: Vec::new(),
                    success_rate_basis_points: 0,
                },
                status,
                Some(failure),
            )
        },
        |value| {
            let failure = if value.status == EvaluationStatus::Pass {
                None
            } else {
                Some(generic_failure(value.status, "control_reliability"))
            };
            (value.control.clone(), value.status, failure)
        },
    )
}

fn assemble_retention(
    value: Option<&RetentionObservation>,
    budget: u64,
    missing: (EvaluationStatus, FailureRecord),
) -> (
    RetentionQualificationMeasurements,
    EvaluationStatus,
    Option<FailureRecord>,
) {
    value.map_or_else(
        || {
            let (status, failure) = missing;
            (
                RetentionQualificationMeasurements {
                    budget_bytes: budget,
                    peak_usage_bytes: 0,
                    pinned_interval_preserved: false,
                    evicted_frame_count: 0,
                    capture_paused_when_pinned: false,
                    capture_resumed_after_unpin: false,
                    cleanup_removed_frame_count: 0,
                },
                status,
                Some(failure),
            )
        },
        |value| {
            (
                value.measurements.clone(),
                value.status,
                value.failure.clone(),
            )
        },
    )
}

fn assemble_recovery(
    value: Option<&RecoveryObservation>,
    missing: (EvaluationStatus, FailureRecord),
) -> (
    RecoveryQualificationMeasurements,
    EvaluationStatus,
    Option<FailureRecord>,
) {
    value.map_or_else(
        || {
            let (status, failure) = missing;
            (
                RecoveryQualificationMeasurements {
                    reopened: false,
                    reconciled: false,
                    recovered_frame_count: 0,
                    removed_frame_count: 0,
                    trailing_segment_repaired: false,
                    staged_artifacts_recovered: false,
                },
                status,
                Some(failure),
            )
        },
        |value| {
            (
                value.measurements.clone(),
                value.status,
                value.failure.clone(),
            )
        },
    )
}

fn assemble_resources(
    value: Option<&ResourceObservation>,
    missing: (EvaluationStatus, FailureRecord),
) -> (
    ResourceQualificationMeasurements,
    EvaluationStatus,
    Option<FailureRecord>,
) {
    value.map_or_else(
        || {
            let (status, failure) = missing;
            (
                ResourceQualificationMeasurements {
                    sample_count: 0,
                    rss_bytes: Vec::new(),
                    cpu_millis: Vec::new(),
                    browser_child_accounting_available: false,
                    unavailable_reason: Some(
                        "qualification resource measurement was not collected".into(),
                    ),
                },
                status,
                Some(failure),
            )
        },
        |value| {
            (
                value.measurements.clone(),
                value.status,
                value.failure.clone(),
            )
        },
    )
}

fn assemble_latency(
    value: Option<&LatencyObservation>,
    missing: (EvaluationStatus, FailureRecord),
) -> (
    LatencyQualificationMeasurements,
    EvaluationStatus,
    Option<FailureRecord>,
) {
    value.map_or_else(
        || {
            let (status, failure) = missing;
            (
                LatencyQualificationMeasurements {
                    source_interval_id: NOT_COLLECTED_ID.into(),
                    viewport: Viewport {
                        width: 1_920,
                        height: 1_080,
                    },
                    frame_width: 0,
                    frame_height: 0,
                    warm_cache: CacheDisposition::Unavailable,
                    temporal_query_elapsed_ms: Vec::new(),
                    artifact_elapsed_ms: Vec::new(),
                    sample_count: 0,
                    threshold_profile_ids: Vec::new(),
                },
                status,
                Some(failure),
            )
        },
        |value| {
            (
                value.measurements.clone(),
                value.status,
                value.failure.clone(),
            )
        },
    )
}

fn missing_capture(
    status: EvaluationStatus,
    _retention_budget: u64,
) -> CaptureQualificationMeasurements {
    CaptureQualificationMeasurements {
        requested_durations_ms: DURATIONS_MS.to_vec(),
        repetitions: temporal_evaluation::CAPTURE_REPETITIONS,
        observed_viewport: Viewport {
            width: 0,
            height: 0,
        },
        observed_device_scale_factor: 0,
        source_frame_count: 0,
        observed_frame_count: 0,
        source_time_sample_count: 0,
        gap_ids: Vec::new(),
        gap_count: 0,
        per_duration: DURATIONS_MS
            .iter()
            .map(
                |duration_ms| temporal_evaluation::DurationQualificationMeasurement {
                    duration_ms: *duration_ms,
                    eligible_count: 0,
                    observed_count: 0,
                    eligibility_rate_basis_points: 0,
                    coverage_rate_basis_points: 0,
                    status,
                },
            )
            .collect(),
    }
}

fn capture_status(
    capture: Option<&CaptureQualificationRun>,
    missing: (EvaluationStatus, FailureRecord),
) -> EvaluationStatus {
    capture.map_or(missing.0, |capture| capture.status)
}

fn capture_failure(
    capture: Option<&CaptureQualificationRun>,
    missing: (EvaluationStatus, FailureRecord),
) -> Option<FailureRecord> {
    capture
        .and_then(|capture| {
            capture
                .measurements
                .iter()
                .find(|measurement| measurement.status != EvaluationStatus::Pass)
                .and_then(|measurement| measurement.failure.clone())
        })
        .or_else(|| (missing.0 != EvaluationStatus::Pass).then(|| missing.1.clone()))
}

fn aggregate_status(
    rows: impl Iterator<Item = EvaluationStatus>,
    gates: impl Iterator<Item = EvaluationStatus>,
) -> EvaluationStatus {
    rows.chain(gates)
        .max_by_key(|status| status.precedence())
        .unwrap_or(EvaluationStatus::Inconclusive)
}

fn first_failure(
    status: EvaluationStatus,
    rows: &[ManifestRow],
    gates: &[QualificationGateResult],
) -> Option<FailureRecord> {
    gates
        .iter()
        .find(|gate| gate.status == status && gate.failure.is_some())
        .and_then(|gate| gate.failure.clone())
        .or_else(|| {
            rows.iter()
                .find(|row| row.status == status && row.failure.is_some())
                .and_then(|row| row.failure.clone())
        })
}

fn failure_for_status(
    status: EvaluationStatus,
    failure: Option<FailureRecord>,
    phase: &str,
) -> Option<FailureRecord> {
    if status == EvaluationStatus::Pass {
        None
    } else {
        Some(failure.unwrap_or_else(|| generic_failure(status, phase)))
    }
}

fn generic_failure(status: EvaluationStatus, phase: &str) -> FailureRecord {
    let (code, reason, recovery) = match status {
        EvaluationStatus::Pass => unreachable!("passing status has no failure"),
        EvaluationStatus::Fail => (
            RunFailureCode::Threshold,
            "complete qualification evidence is below its declared threshold",
            "inspect the exact production measurements and retry the declared configuration",
        ),
        EvaluationStatus::Inconclusive => (
            RunFailureCode::InsufficientEvidence,
            "qualification evidence is incomplete or unavailable",
            "collect the missing production evidence and retry the declared configuration",
        ),
        EvaluationStatus::Blocked => (
            RunFailureCode::Unavailable,
            "a required qualification precondition is unavailable",
            "resolve the declared local blocker and retry the authorized run",
        ),
        EvaluationStatus::Skipped => (
            RunFailureCode::OptionalUnavailable,
            "the optional Linux Chromium configuration is unavailable",
            "install the optional configuration before collecting it",
        ),
    };
    FailureRecord {
        code,
        phase: phase.into(),
        reason: reason.into(),
        recovery: recovery.into(),
        retryable: true,
    }
}

fn cleanup_failure() -> FailureRecord {
    FailureRecord {
        code: RunFailureCode::Cleanup,
        phase: "cleanup".into(),
        reason: "managed qualification resources remain after cleanup".into(),
        recovery: "remove the remaining managed resources before retrying".into(),
        retryable: true,
    }
}

fn browser_failure(
    browser: &BrowserAvailability,
    status: EvaluationStatus,
) -> Option<FailureRecord> {
    match browser {
        BrowserAvailability::Blocked { reason, recovery }
        | BrowserAvailability::Unavailable { reason, recovery } => Some(FailureRecord {
            code: if status == EvaluationStatus::Blocked {
                RunFailureCode::Unavailable
            } else {
                RunFailureCode::InsufficientEvidence
            },
            phase: "browser_preflight".into(),
            reason: reason.clone(),
            recovery: recovery.clone(),
            retryable: true,
        }),
        BrowserAvailability::Skipped {
            reason, recovery, ..
        } => Some(FailureRecord {
            code: RunFailureCode::OptionalUnavailable,
            phase: "browser_preflight".into(),
            reason: reason.clone(),
            recovery: recovery.clone(),
            retryable: true,
        }),
        _ => None,
    }
}

fn missing_observation_status(browser: &BrowserAvailability) -> (EvaluationStatus, FailureRecord) {
    match browser {
        BrowserAvailability::Blocked { reason, recovery }
        | BrowserAvailability::Unavailable { reason, recovery } => (
            EvaluationStatus::Blocked,
            FailureRecord {
                code: RunFailureCode::Unavailable,
                phase: "browser_preflight".into(),
                reason: reason.clone(),
                recovery: recovery.clone(),
                retryable: true,
            },
        ),
        BrowserAvailability::Skipped {
            reason, recovery, ..
        } => (
            EvaluationStatus::Skipped,
            FailureRecord {
                code: RunFailureCode::OptionalUnavailable,
                phase: "browser_preflight".into(),
                reason: reason.clone(),
                recovery: recovery.clone(),
                retryable: true,
            },
        ),
        _ => (
            EvaluationStatus::Inconclusive,
            generic_failure(EvaluationStatus::Inconclusive, "qualification"),
        ),
    }
}

fn gate_phase(gate: QualificationGateId) -> &'static str {
    match gate {
        QualificationGateId::CaptureEnvelope => "capture_envelope",
        QualificationGateId::TimingIntegrity => "timing_integrity",
        QualificationGateId::MovementSequence => "movement_sequence",
        QualificationGateId::ControlReliability => "control_reliability",
        QualificationGateId::Retention => "retention",
        QualificationGateId::Recovery => "recovery",
        QualificationGateId::ResourceUsage => "resource_usage",
        QualificationGateId::TemporalQueryLatency => "temporal_query_latency",
        QualificationGateId::ArtifactLatency => "artifact_latency",
        QualificationGateId::Cleanup => "cleanup",
    }
}

fn qualification_prompt() -> Result<ManifestPrompt> {
    let template = temporal_evaluation::PromptSet::canonical()
        .template(temporal_evaluation::PromptId::CaptureQualification)
        .cloned()
        .ok_or_else(|| report_error("capture qualification prompt is not registered"))?;
    Ok(ManifestPrompt {
        prompt_set_id: template.id,
        prompt_version: template.version,
        system_prompt: template.system_prompt,
        task_prompt: template.task_prompt,
        sha256: template.sha256,
    })
}

fn artifact_identity(
    capture: Option<&CaptureQualificationRun>,
    latency: Option<&LatencyObservation>,
    condition_id: ConditionId,
) -> Result<ArtifactIdentity> {
    let source_interval = latency
        .map(|value| value.source_interval_id.clone())
        .or_else(|| {
            capture.and_then(|value| {
                value.measurements.iter().find_map(|measurement| {
                    measurement
                        .interval
                        .as_ref()
                        .map(|interval| interval.interval_id.clone())
                })
            })
        })
        .unwrap_or_else(|| NOT_COLLECTED_ID.into());
    let selected_interval = capture.and_then(|value| {
        value.measurements.iter().find_map(|measurement| {
            measurement
                .interval
                .as_ref()
                .filter(|interval| interval.interval_id == source_interval)
        })
    });
    let mut output_ids = BTreeMap::<String, temporal_evaluation::OutputIdentity>::new();
    let mut algorithms = BTreeMap::<String, String>::new();
    let mut parameters = BTreeMap::new();
    if let Some(latency) = latency {
        for sample in &latency.samples {
            for identity in &sample.identities {
                add_artifact_identity(identity, &mut output_ids, &mut algorithms, &mut parameters)?;
            }
        }
    }
    if let Some(interval) = selected_interval {
        parameters.insert("source_interval_digest".into(), interval.digest.clone());
    }
    let source_frame_ids = selected_interval
        .map(|interval| {
            interval
                .frames
                .iter()
                .map(|frame| temporal_evaluation::OutputIdentity {
                    id: frame.id.clone(),
                    sha256: match frame.availability {
                        EvidenceAvailability::Retained | EvidenceAvailability::Corrupt => {
                            Some(frame.encoded_sha256.clone())
                        }
                        _ => None,
                    },
                    availability: frame.availability,
                })
                .collect()
        })
        .unwrap_or_default();
    let gap_ids = selected_interval
        .map(|interval| interval.gap_ids())
        .unwrap_or_default();
    Ok(ArtifactIdentity {
        condition_id,
        algorithm_versions: algorithms
            .into_iter()
            .map(|(name, version)| NamedVersion { name, version })
            .collect(),
        parameters,
        source_interval_id: source_interval,
        output_ids: output_ids.into_values().collect(),
        source_frame_ids,
        gap_ids,
    })
}

fn add_artifact_identity(
    identity: &ArtifactIdentityObservation,
    output_ids: &mut BTreeMap<String, temporal_evaluation::OutputIdentity>,
    algorithms: &mut BTreeMap<String, String>,
    parameters: &mut BTreeMap<String, String>,
) -> Result<()> {
    let artifact_id = identity.artifact_id.to_string();
    let output_hash = identity.manifest.output_hash().to_string();
    output_ids
        .entry(artifact_id.clone())
        .or_insert(temporal_evaluation::OutputIdentity {
            id: artifact_id,
            sha256: Some(output_hash),
            availability: EvidenceAvailability::Retained,
        });
    let algorithm = identity.manifest.algorithm();
    if let Some(previous) = algorithms.insert(algorithm.name().into(), algorithm.version().into())
        && previous != algorithm.version()
    {
        return Err(report_error(
            "authority artifact identities disagree on algorithm version",
        ));
    }
    for (key, value) in identity.manifest.parameters().iter() {
        let value = serde_json::to_string(value)
            .map_err(|_| report_error("authority artifact parameters could not be serialized"))?;
        if let Some(previous) = parameters.insert(key.into(), value.clone())
            && previous != value
        {
            return Err(report_error(
                "authority artifact identities disagree on parameters",
            ));
        }
    }
    Ok(())
}

fn range_from_values(
    values: impl Iterator<Item = u64>,
) -> Option<temporal_evaluation::TimeRangeMs> {
    let values = values.collect::<Vec<_>>();
    let (min, max) = (values.iter().min()?, values.iter().max()?);
    Some(temporal_evaluation::TimeRangeMs {
        start_ms: min / 1_000_000,
        end_ms: max / 1_000_000,
    })
}

fn ordinal_range(
    frames: &[temporal_evaluation::SourceFrameEvidence],
) -> Option<CaptureOrdinalRange> {
    Some(CaptureOrdinalRange {
        first: frames.first()?.capture_ordinal,
        last: frames.last()?.capture_ordinal,
    })
}

fn report_error(message: &'static str) -> KrometrailError {
    live_error(ErrorCode::InvalidInput, message)
}

/// Atomically publish a finalized manifest below the ignored live run boundary.
///
/// Cleanup is applied before validation and the final rename. A cleanup failure therefore writes a
/// non-passing manifest with explicit cleanup evidence. Filesystem/canonicalization failures write
/// only a safe fixed-shape error report; no private path or adapter error is copied into it.
pub fn finalize_manifest_at(
    mut run: RunManifest,
    cleanup: CleanupObservation,
    path: &Path,
) -> Result<PathBuf> {
    if !is_safe_manifest_path(path) {
        return Err(report_error(
            "live output path is outside the ignored qualification boundary",
        ));
    }
    let result = finalize_manifest_inner(&mut run, cleanup, path);
    if let Err(error) = &result {
        write_safe_error_report(path, error.code);
    }
    result
}

fn finalize_manifest_inner(
    run: &mut RunManifest,
    cleanup: CleanupObservation,
    path: &Path,
) -> Result<PathBuf> {
    let Some(qualification) = run.qualification.as_mut() else {
        return Err(report_error(
            "live manifest finalization requires qualification measurements",
        ));
    };
    qualification.cleanup = temporal_evaluation::CleanupQualificationMeasurements {
        server_stopped: cleanup.server_stopped,
        profile_deleted: cleanup.profile_deleted,
        store_flushed: cleanup.store_flushed,
        lock_released: cleanup.lock_released,
        output_finalized: true,
        remaining_managed_resources: cleanup.remaining_managed_resources,
    };
    if !cleanup.is_clean() {
        if let Some(gate) = qualification
            .gates
            .iter_mut()
            .find(|gate| gate.gate == QualificationGateId::Cleanup)
        {
            gate.status = EvaluationStatus::Inconclusive;
            gate.failure = Some(cleanup_failure());
        }
        run.status = aggregate_status(
            run.rows.iter().map(|row| row.status),
            qualification.gates.iter().map(|gate| gate.status),
        );
        run.failure = first_failure(run.status, &run.rows, &qualification.gates)
            .or_else(|| Some(cleanup_failure()));
    }
    run.validate()
        .map_err(|_| report_error("live manifest failed final canonical validation"))?;
    let bytes = run
        .canonical_bytes()
        .map_err(|_| report_error("live manifest could not be canonicalized"))?;
    atomic_write(path, &bytes)?;
    Ok(path.to_owned())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| report_error("live output boundary is invalid"))?;
    fs::create_dir_all(parent).map_err(|_| {
        live_error(
            ErrorCode::PersistenceFailed,
            "live output boundary could not be prepared",
        )
    })?;
    let partial = parent.join(PARTIAL_FILE);
    let write_result = (|| -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&partial)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&partial, path)?;
        if let Ok(directory) = OpenOptions::new().read(true).open(parent) {
            let _ = directory.sync_all();
        }
        Ok(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&partial);
        return Err(live_error(
            ErrorCode::PersistenceFailed,
            "live manifest could not be finalized",
        ));
    }
    Ok(())
}

fn write_safe_error_report(path: &Path, code: ErrorCode) {
    let Some(parent) = path.parent() else { return };
    if !is_safe_manifest_path(path) || fs::create_dir_all(parent).is_err() {
        return;
    }
    #[derive(Serialize)]
    struct SafeErrorReport {
        kind: &'static str,
        code: String,
        recovery: &'static str,
    }
    let report = SafeErrorReport {
        kind: "temporal_qualification_finalization_error",
        code: code.as_str().into(),
        recovery: "inspect the local qualification output boundary and retry after cleanup",
    };
    let Ok(bytes) = serde_json::to_vec(&report) else {
        return;
    };
    let error_path = parent.join(FINALIZATION_ERROR_FILE);
    let partial = parent.join("finalization-error.json.partial");
    let result = (|| -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&partial)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(&partial, error_path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(partial);
    }
}

fn is_safe_manifest_path(path: &Path) -> bool {
    let mut components = path.components().rev();
    if components
        .next()
        .and_then(|component| component.as_os_str().to_str())
        != Some(MANIFEST_FILE)
    {
        return false;
    }
    let Some(run_id) = components
        .next()
        .and_then(|component| component.as_os_str().to_str())
    else {
        return false;
    };
    let Some(product) = components
        .next()
        .and_then(|component| component.as_os_str().to_str())
    else {
        return false;
    };
    if !safe_component(run_id) || !safe_component(product) {
        return false;
    }
    if path.to_string_lossy().contains('\\') || path.to_string_lossy().contains("..") {
        return false;
    }
    let candidate = if path.is_absolute() {
        path.to_owned()
    } else {
        let Ok(current) = std::env::current_dir() else {
            return false;
        };
        current.join(path)
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/temporal-evaluation/live");
    candidate.starts_with(root)
}

fn safe_component(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
}

/// Finalize a run using the canonical `<product>/<run-id>` output boundary.
pub async fn finalize_manifest(run: RunManifest, cleanup: CleanupObservation) -> Result<PathBuf> {
    if super::OptInDecision::from_environment() != super::OptInDecision::Authorized {
        return Err(live_error(
            ErrorCode::InvalidLifecycleTransition,
            "live manifest finalization requires both explicit opt-in gates",
        ));
    }
    let config = LiveQualificationConfig::default();
    finalize_manifest_at(run, cleanup, &config.output_path())
}

/// Return the canonical output path for a configuration without exposing that path in a manifest.
pub fn output_path(config: &LiveQualificationConfig) -> PathBuf {
    config.output_path()
}

#[cfg(test)]
mod tests {
    use super::*;
    use temporal_evaluation::BrowserProduct;

    #[test]
    fn gate_statuses_follow_registry_order_and_precedence() {
        assert_eq!(
            aggregate_status(
                [EvaluationStatus::Pass].into_iter(),
                [EvaluationStatus::Fail].into_iter()
            ),
            EvaluationStatus::Fail
        );
        assert_eq!(
            aggregate_status(
                [EvaluationStatus::Fail].into_iter(),
                [EvaluationStatus::Inconclusive].into_iter()
            ),
            EvaluationStatus::Inconclusive
        );
        assert_eq!(
            aggregate_status(
                [EvaluationStatus::Inconclusive].into_iter(),
                [EvaluationStatus::Skipped].into_iter()
            ),
            EvaluationStatus::Skipped
        );
        assert_eq!(
            aggregate_status(
                [EvaluationStatus::Skipped].into_iter(),
                [EvaluationStatus::Blocked].into_iter()
            ),
            EvaluationStatus::Blocked
        );
    }

    #[test]
    fn blocked_and_optional_skip_assembly_never_writes_a_passing_result() {
        let blocked = assemble_manifest(QualificationObservations::contract_seed()).unwrap();
        assert_eq!(blocked.status, EvaluationStatus::Blocked);
        assert!(blocked.failure.is_some());
        assert!(
            blocked
                .qualification
                .as_ref()
                .unwrap()
                .gates
                .iter()
                .all(|gate| {
                    gate.status == EvaluationStatus::Blocked
                        || gate.status == EvaluationStatus::Inconclusive
                })
        );

        let mut skipped_input = QualificationObservations::contract_seed();
        skipped_input.optional_configuration = true;
        skipped_input.browser = BrowserAvailability::Skipped {
            product: BrowserProduct::Chromium,
            reason: "optional Linux Chromium is unavailable".into(),
            recovery: "install the optional Linux Chromium configuration before collecting it"
                .into(),
        };
        let skipped = assemble_manifest(skipped_input).unwrap();
        assert_eq!(skipped.status, EvaluationStatus::Skipped);
        assert!(
            skipped
                .qualification
                .as_ref()
                .unwrap()
                .gates
                .iter()
                .all(|gate| gate.status == EvaluationStatus::Skipped)
        );
        skipped.validate().unwrap();
    }

    #[test]
    fn canonical_manifest_is_atomically_written_and_finalization_failures_are_safe() {
        let run_root = PathBuf::from("target/temporal-evaluation/live/chrome/report-atomic-test");
        let _ = fs::remove_dir_all(&run_root);
        let run = assemble_manifest(QualificationObservations::contract_seed()).unwrap();
        let path = run_root.join(MANIFEST_FILE);
        let written = finalize_manifest_at(
            run.clone(),
            CleanupObservation {
                server_stopped: true,
                profile_deleted: true,
                store_flushed: true,
                lock_released: true,
                output_finalized: false,
                remaining_managed_resources: 0,
            },
            &path,
        )
        .unwrap();
        assert_eq!(written, path);
        let bytes = fs::read(&path).unwrap();
        let decoded = RunManifest::from_canonical_json(&bytes).unwrap();
        let mut finalized = run.clone();
        finalized.qualification.as_mut().unwrap().cleanup =
            temporal_evaluation::CleanupQualificationMeasurements {
                server_stopped: true,
                profile_deleted: true,
                store_flushed: true,
                lock_released: true,
                output_finalized: true,
                remaining_managed_resources: 0,
            };
        assert_eq!(decoded, finalized);
        assert!(!run_root.join(PARTIAL_FILE).exists());

        let mut invalid = run;
        invalid.qualification = None;
        assert!(finalize_manifest_at(invalid, CleanupObservation::default(), &path).is_err());
        let report = fs::read_to_string(run_root.join(FINALIZATION_ERROR_FILE)).unwrap();
        assert!(report.contains("temporal_qualification_finalization_error"));
        assert!(!report.contains("target/"));
        let _ = fs::remove_dir_all(run_root);
    }

    #[test]
    fn unsafe_output_paths_are_rejected_without_a_write() {
        let path = PathBuf::from("/tmp/private/run-manifest.json");
        assert!(!is_safe_manifest_path(&path));
    }

    #[test]
    fn safe_error_report_contains_no_path_or_private_detail() {
        let path =
            PathBuf::from("target/temporal-evaluation/live/chrome/report-test/run-manifest.json");
        let _ = fs::remove_dir_all(path.parent().unwrap());
        write_safe_error_report(&path, ErrorCode::PersistenceFailed);
        let bytes = fs::read(path.parent().unwrap().join(FINALIZATION_ERROR_FILE)).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("temporal_qualification_finalization_error"));
        assert!(!text.contains("target/"));
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }
}
