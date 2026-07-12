use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use serde::{Deserialize, Serialize};

use super::error::{SpikeError, SpikeErrorCode};

pub const EVIDENCE_SCHEMA_VERSION: u16 = 1;

/// RSS samples begin after a short setup interval so the first window reflects steady-state
/// capture rather than browser startup. The manual gate contract asserts these values too.
pub const RSS_WARMUP_SECONDS: u64 = 10;
pub const RSS_SAMPLE_INTERVAL_SECONDS: u64 = 1;
pub const RSS_MIN_SAMPLES_FOR_60_SECONDS: u64 = 50;

fn minimum_rss_samples(minimum_seconds: f64) -> u64 {
    let post_warmup_seconds = (minimum_seconds - RSS_WARMUP_SECONDS as f64).max(0.0);
    let contract_minimum = if minimum_seconds >= 60.0 {
        RSS_MIN_SAMPLES_FOR_60_SECONDS
    } else {
        1
    };
    (post_warmup_seconds.floor() as u64).max(contract_minimum)
}

#[derive(
    Clone,
    Copy,
    Debug,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum TransportGateId {
    DeterministicRouting,
    TypedDomains,
    FlatSessionIsolation,
    RawBrowserCommand,
    RawSessionCommand,
    NamedRawEventParams,
    ProtocolDriftSurvival,
    SustainedScreencast,
    PromptAcknowledgement,
    BoundedHandoffSaturation,
    BoundedMemoryProxy,
    DisconnectCleanup,
    ExplicitReconnectRebuild,
}

impl TransportGateId {
    pub const ALL: [Self; 13] = [
        Self::DeterministicRouting,
        Self::TypedDomains,
        Self::FlatSessionIsolation,
        Self::RawBrowserCommand,
        Self::RawSessionCommand,
        Self::NamedRawEventParams,
        Self::ProtocolDriftSurvival,
        Self::SustainedScreencast,
        Self::PromptAcknowledgement,
        Self::BoundedHandoffSaturation,
        Self::BoundedMemoryProxy,
        Self::DisconnectCleanup,
        Self::ExplicitReconnectRebuild,
    ];

    pub const fn measurement_keys(self) -> &'static [&'static str] {
        match self {
            Self::DeterministicRouting => &["commands", "events", "cross_delivery"],
            Self::TypedDomains => &["typed_operations"],
            Self::FlatSessionIsolation => &["sessions", "cross_delivery"],
            Self::RawBrowserCommand | Self::RawSessionCommand => &["commands"],
            Self::NamedRawEventParams => &["named_events"],
            Self::ProtocolDriftSurvival => &["fixtures", "connection_survived"],
            Self::SustainedScreencast => {
                // A sustained pass is not valid without the same RSS evidence used by the
                // bounded-memory proxy; otherwise a missing sampler can still look successful.
                &[
                    "elapsed_seconds",
                    "frames_received",
                    "frames_acknowledged",
                    "rss_samples",
                    "rss_peak_bytes",
                    "rss_first_window_median_bytes",
                    "rss_last_window_median_bytes",
                    "rss_theil_sen_bytes_per_minute",
                    "rss_sampling_interval_seconds",
                    "rss_warmup_seconds",
                ]
            }
            Self::PromptAcknowledgement => &["ack_before_handoff"],
            Self::BoundedHandoffSaturation => &["handoff_attempts", "handoff_dropped"],
            Self::BoundedMemoryProxy => &[
                "rss_samples",
                "rss_growth_bytes",
                "rss_peak_bytes",
                "rss_first_window_median_bytes",
                "rss_last_window_median_bytes",
                "rss_theil_sen_bytes_per_minute",
                "rss_sampling_interval_seconds",
                "rss_warmup_seconds",
            ],
            Self::DisconnectCleanup => &["pending_calls_closed", "subscriptions_closed"],
            Self::ExplicitReconnectRebuild => &["connections", "sessions_rebuilt"],
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GateStatus {
    Pass,
    Fail,
    Blocked,
    NotRun,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CandidateIdentity {
    pub name: String,
    pub version: String,
    pub checksum: String,
}

impl CandidateIdentity {
    pub fn fake() -> Self {
        Self {
            name: "fake".into(),
            version: "deterministic".into(),
            checksum: "local-scripted-peer".into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceIdentity {
    pub git_revision: String,
    pub protocol_revision: String,
    pub rust_version: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SanitizedEnvironment {
    pub platform: String,
    pub architecture: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrowserEvidence {
    pub product: String,
    pub protocol: String,
    pub revision: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FixtureEvidence {
    pub name: String,
    pub sha256: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GateConfiguration {
    pub minimum_seconds: f64,
    pub minimum_frames: u64,
    pub saturation_seconds: f64,
    pub saturation_attempts: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GateResult {
    pub id: TransportGateId,
    pub status: GateStatus,
    pub summary: String,
    pub measurements: BTreeMap<String, f64>,
    pub failure: Option<SpikeError>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransportEvidenceV1 {
    pub schema_version: u16,
    pub candidate: CandidateIdentity,
    pub source: SourceIdentity,
    pub environment: SanitizedEnvironment,
    pub browser: BrowserEvidence,
    pub fixture: FixtureEvidence,
    pub configuration: GateConfiguration,
    pub gates: Vec<GateResult>,
    pub limitations: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TransportDecision {
    AdoptCdpkit,
    AdoptChromey,
    OwnTransport,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EvidenceDigest {
    pub platform: String,
    pub sha256: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransportDecisionV1 {
    pub schema_version: u16,
    pub decision: TransportDecision,
    pub candidate: CandidateIdentity,
    pub evidence: Vec<EvidenceDigest>,
    pub gates: Vec<GateResult>,
    pub limitations: Vec<String>,
    pub rejected_alternatives: Vec<String>,
    pub rationale: String,
}

pub fn validate_evidence(value: &TransportEvidenceV1) -> Result<(), SpikeError> {
    if value.schema_version != EVIDENCE_SCHEMA_VERSION {
        return Err(SpikeError::new(
            SpikeErrorCode::Evidence,
            "unsupported evidence schema version",
        ));
    }
    if !value.configuration.minimum_seconds.is_finite()
        || !value.configuration.saturation_seconds.is_finite()
    {
        return Err(SpikeError::new(
            SpikeErrorCode::Evidence,
            "gate configuration contains a non-finite measurement",
        ));
    }
    if value.configuration.minimum_seconds <= 0.0 || value.configuration.minimum_frames == 0 {
        return Err(SpikeError::new(
            SpikeErrorCode::Evidence,
            "gate configuration has no positive capture threshold",
        ));
    }
    if value.gates.len() != TransportGateId::ALL.len() {
        return Err(SpikeError::new(
            SpikeErrorCode::Evidence,
            "gate registry is incomplete or contains duplicates",
        ));
    }

    let expected: BTreeSet<_> = TransportGateId::ALL.into_iter().collect();
    let actual: BTreeSet<_> = value.gates.iter().map(|gate| gate.id).collect();
    if actual != expected {
        return Err(SpikeError::new(
            SpikeErrorCode::Evidence,
            "gate registry is incomplete or contains duplicates",
        ));
    }
    for gate in &value.gates {
        for measurement in gate.measurements.values() {
            if !measurement.is_finite() {
                return Err(SpikeError::for_gate(
                    SpikeErrorCode::Evidence,
                    gate.id,
                    "measurement is not finite",
                ));
            }
        }
        if gate.status == GateStatus::Pass {
            for key in gate.id.measurement_keys() {
                if !gate.measurements.contains_key(*key) {
                    return Err(SpikeError::for_gate(
                        SpikeErrorCode::Evidence,
                        gate.id,
                        format!("passing gate lacks measurement {key}"),
                    ));
                }
            }
            if matches!(
                gate.id,
                TransportGateId::SustainedScreencast | TransportGateId::BoundedMemoryProxy
            ) {
                validate_rss_measurements(
                    gate.id,
                    &gate.measurements,
                    value.configuration.minimum_seconds,
                )?;
            }
        }
    }
    validate_sanitized_strings(value)?;
    Ok(())
}

pub fn rss_measurements_are_valid(
    measurements: &BTreeMap<String, f64>,
    minimum_seconds: f64,
) -> bool {
    validate_rss_measurements(
        TransportGateId::BoundedMemoryProxy,
        measurements,
        minimum_seconds,
    )
    .is_ok()
}

fn validate_rss_measurements(
    gate: TransportGateId,
    measurements: &BTreeMap<String, f64>,
    minimum_seconds: f64,
) -> Result<(), SpikeError> {
    let measurement = |key: &str| measurements.get(key).copied();
    let samples = measurement("rss_samples").unwrap_or(0.0);
    let required_samples = minimum_rss_samples(minimum_seconds) as f64;
    if samples < required_samples || samples.fract() != 0.0 {
        return Err(SpikeError::for_gate(
            SpikeErrorCode::Evidence,
            gate,
            format!("RSS sample count is below the required {required_samples:.0}"),
        ));
    }
    for key in [
        "rss_peak_bytes",
        "rss_first_window_median_bytes",
        "rss_last_window_median_bytes",
    ] {
        if measurement(key).unwrap_or(0.0) <= 0.0 {
            return Err(SpikeError::for_gate(
                SpikeErrorCode::Evidence,
                gate,
                format!("RSS measurement {key} is absent or zero"),
            ));
        }
    }
    let interval = measurement("rss_sampling_interval_seconds").unwrap_or(0.0);
    if !(0.75..=1.25).contains(&interval) {
        return Err(SpikeError::for_gate(
            SpikeErrorCode::Evidence,
            gate,
            "RSS sampling interval is not approximately one sample per second",
        ));
    }
    let warmup = measurement("rss_warmup_seconds").unwrap_or(0.0);
    if warmup != RSS_WARMUP_SECONDS as f64 {
        return Err(SpikeError::for_gate(
            SpikeErrorCode::Evidence,
            gate,
            "RSS warmup does not match the declared gate contract",
        ));
    }
    Ok(())
}

/// Validate an evidence object before it crosses into the committed evidence tree.
/// Machine-local paths, endpoints, and secret-bearing process details are rejected rather
/// than silently normalized into an ambiguous report.
pub fn sanitize_evidence(value: TransportEvidenceV1) -> Result<TransportEvidenceV1, SpikeError> {
    validate_evidence(&value)?;
    Ok(value)
}

fn validate_sanitized_strings(value: &TransportEvidenceV1) -> Result<(), SpikeError> {
    let strings = [
        &value.candidate.name,
        &value.candidate.version,
        &value.candidate.checksum,
        &value.source.git_revision,
        &value.source.protocol_revision,
        &value.source.rust_version,
        &value.environment.platform,
        &value.environment.architecture,
        &value.browser.product,
        &value.browser.protocol,
        &value.browser.revision,
        &value.fixture.name,
        &value.fixture.sha256,
    ];
    for text in strings {
        if contains_machine_detail(text) {
            return Err(SpikeError::new(
                SpikeErrorCode::Evidence,
                "evidence contains a path or endpoint",
            ));
        }
    }
    for gate in &value.gates {
        if contains_machine_detail(&gate.summary) {
            return Err(SpikeError::for_gate(
                SpikeErrorCode::Evidence,
                gate.id,
                "gate summary contains a path or endpoint",
            ));
        }
    }
    for text in &value.limitations {
        if contains_machine_detail(text) {
            return Err(SpikeError::new(
                SpikeErrorCode::Evidence,
                "limitation contains a path or endpoint",
            ));
        }
    }
    Ok(())
}

fn contains_machine_detail(text: &str) -> bool {
    text.contains("ws://")
        || text.contains("wss://")
        || text.contains("file://")
        || text.contains("/home/")
        || text.contains("/tmp/")
        || text.contains("\\\\")
        || text.contains("--user-data-dir")
        || text.contains("PASSWORD=")
        || text.contains("TOKEN=")
}

pub fn decide(reports: &[TransportEvidenceV1]) -> Result<TransportDecisionV1, SpikeError> {
    if reports.len() != 2 {
        return Err(SpikeError::new(
            SpikeErrorCode::Evidence,
            "decision requires Linux and macOS evidence",
        ));
    }
    for report in reports {
        validate_evidence(report)?;
        if report
            .gates
            .iter()
            .any(|gate| gate.status != GateStatus::Pass)
        {
            return Err(SpikeError::new(
                SpikeErrorCode::Evidence,
                "transport decision cannot waive a failed gate",
            ));
        }
    }
    if reports[0].candidate != reports[1].candidate {
        return Err(SpikeError::new(
            SpikeErrorCode::Evidence,
            "platform reports use different candidates",
        ));
    }
    let decision = match reports[0].candidate.name.as_str() {
        "cdpkit" => TransportDecision::AdoptCdpkit,
        "chromey" => TransportDecision::AdoptChromey,
        _ => TransportDecision::OwnTransport,
    };
    let gates = reports[0].gates.clone();
    Ok(TransportDecisionV1 {
        schema_version: EVIDENCE_SCHEMA_VERSION,
        decision,
        candidate: reports[0].candidate.clone(),
        evidence: reports
            .iter()
            .map(|report| EvidenceDigest {
                platform: report.environment.platform.clone(),
                sha256: report.fixture.sha256.clone(),
            })
            .collect(),
        gates,
        limitations: reports[0].limitations.clone(),
        rejected_alternatives: vec![
            "selection remains behind the transport adapter boundary".into(),
        ],
        rationale: "all registered gates passed on both decisive platforms".into(),
    })
}

pub fn write_json_schema(path: &Path) -> Result<(), SpikeError> {
    let schema = schemars::schema_for!(TransportEvidenceV1);
    let encoded = serde_json::to_vec_pretty(&schema)
        .map_err(|error| SpikeError::new(SpikeErrorCode::Evidence, error.to_string()))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| SpikeError::new(SpikeErrorCode::Io, error.to_string()))?;
    }
    std::fs::write(path, encoded)
        .map_err(|error| SpikeError::new(SpikeErrorCode::Io, error.to_string()))
}
