use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    contract::CandidateContractEvidence,
    error::{SpikeError, SpikeErrorCode},
};

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
            ],
            Self::DisconnectCleanup => &[
                "pending_command_started",
                "pending_calls_closed",
                "subscriptions_closed",
                "pending_command_elapsed_seconds",
                "subscription_elapsed_seconds",
                "close_reason_observed",
            ],
            Self::ExplicitReconnectRebuild => {
                &["connections", "sessions_rebuilt", "elapsed_seconds"]
            }
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
    /// Maximum wall-clock time for the complete real-Chrome qualification operation.
    pub hard_stop_seconds: u64,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_contract: Option<CandidateContractEvidence>,
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
    if value.configuration.minimum_seconds < 60.0
        || value.configuration.minimum_frames < 1_000
        || value.configuration.saturation_seconds < 10.0
        || value.configuration.saturation_attempts < 100
        || value.configuration.hard_stop_seconds == 0
    {
        return Err(SpikeError::new(
            SpikeErrorCode::Evidence,
            "gate configuration has no positive capture or hard-stop threshold",
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
                let aliased =
                    *key == "rss_samples" && gate.measurements.contains_key("rss_sample_count");
                if !gate.measurements.contains_key(*key) && !aliased {
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
    validate_observed_deadlines(value)?;
    if let Some(contract) = &value.candidate_contract {
        if contract.fixtures < 3
            || !contract.connection_survived
            || !is_sha256_digest(&contract.trace_sha256)
        {
            return Err(SpikeError::new(
                SpikeErrorCode::Evidence,
                "candidate wire-contract evidence is incomplete",
            ));
        }
    }
    validate_sanitized_strings(value)?;
    Ok(())
}

fn validate_observed_deadlines(value: &TransportEvidenceV1) -> Result<(), SpikeError> {
    let disconnect = value
        .gates
        .iter()
        .find(|gate| gate.id == TransportGateId::DisconnectCleanup)
        .expect("gate registry was validated before deadline validation");
    if disconnect.status == GateStatus::Pass {
        if disconnect.measurements.contains_key("deadline_seconds") {
            return Err(SpikeError::for_gate(
                SpikeErrorCode::Evidence,
                disconnect.id,
                "nominal deadline_seconds is obsolete; observed termination elapsed time is required",
            ));
        }
        for key in [
            "pending_command_elapsed_seconds",
            "subscription_elapsed_seconds",
        ] {
            let elapsed = disconnect.measurements.get(key).copied().ok_or_else(|| {
                SpikeError::for_gate(
                    SpikeErrorCode::Evidence,
                    disconnect.id,
                    format!("passing gate lacks observed measurement {key}"),
                )
            })?;
            if !elapsed.is_finite() || !(0.0..1.0).contains(&elapsed) {
                return Err(SpikeError::for_gate(
                    SpikeErrorCode::Evidence,
                    disconnect.id,
                    format!("observed {key} is absent, nominal-only, or over one second"),
                ));
            }
        }
        for key in [
            "pending_command_started",
            "pending_calls_closed",
            "subscriptions_closed",
            "close_reason_observed",
        ] {
            if disconnect.measurements.get(key).copied() != Some(1.0) {
                return Err(SpikeError::for_gate(
                    SpikeErrorCode::Evidence,
                    disconnect.id,
                    format!("observed disconnect outcome {key} was not proved"),
                ));
            }
        }
    }

    let rebuild = value
        .gates
        .iter()
        .find(|gate| gate.id == TransportGateId::ExplicitReconnectRebuild)
        .expect("gate registry was validated before deadline validation");
    if rebuild.status == GateStatus::Pass {
        let elapsed = rebuild
            .measurements
            .get("elapsed_seconds")
            .copied()
            .ok_or_else(|| {
                SpikeError::for_gate(
                    SpikeErrorCode::Evidence,
                    rebuild.id,
                    "passing gate lacks observed rebuild elapsed_seconds",
                )
            })?;
        if !elapsed.is_finite() || !(0.0..5.0).contains(&elapsed) {
            return Err(SpikeError::for_gate(
                SpikeErrorCode::Evidence,
                rebuild.id,
                "observed rebuild elapsed_seconds is absent, nominal-only, or over five seconds",
            ));
        }
    }
    Ok(())
}

fn is_sha256_digest(value: &str) -> bool {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return false;
    };
    hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
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
    let samples = measurement("rss_samples")
        .or_else(|| measurement("rss_sample_count"))
        .unwrap_or(0.0);
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
    if let Some(interval) = measurement("rss_sampling_interval_seconds")
        && !(0.75..=1.25).contains(&interval)
    {
        return Err(SpikeError::for_gate(
            SpikeErrorCode::Evidence,
            gate,
            "RSS sampling interval is not approximately one sample per second",
        ));
    }
    if let Some(warmup) = measurement("rss_warmup_seconds")
        && warmup != RSS_WARMUP_SECONDS as f64
    {
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
        || text.contains("/Users/")
        || text.contains("/tmp/")
        || text.contains("\\\\")
        || text.contains("127.0.0.1")
        || text.contains("localhost")
        || text.contains("--user-data-dir")
        || text.contains("--remote-debugging-port")
        || text.contains("PASSWORD=")
        || text.contains("TOKEN=")
        || text.contains("HOME=")
        || text.contains("USER=")
}

const CDPKIT_NAME: &str = "cdpkit";
const CDPKIT_VERSION: &str = "0.4.0";
const CDPKIT_CHECKSUM: &str = "c3fdb566d913b31e0014391a94c0db4ed871dbb76577dd1b2f2c5f6df158bfaa";

fn gate(report: &TransportEvidenceV1, id: TransportGateId) -> Result<&GateResult, SpikeError> {
    report
        .gates
        .iter()
        .find(|candidate| candidate.id == id)
        .ok_or_else(|| {
            SpikeError::for_gate(SpikeErrorCode::Evidence, id, "required gate is missing")
        })
}

fn measurement(gate: &GateResult, key: &str) -> Result<f64, SpikeError> {
    gate.measurements.get(key).copied().ok_or_else(|| {
        SpikeError::for_gate(
            SpikeErrorCode::Evidence,
            gate.id,
            format!("gate lacks required measurement {key}"),
        )
    })
}

fn require_at_least(gate: &GateResult, key: &str, minimum: f64) -> Result<(), SpikeError> {
    let value = measurement(gate, key)?;
    if value < minimum {
        return Err(SpikeError::for_gate(
            SpikeErrorCode::Evidence,
            gate.id,
            format!("measurement {key} is below {minimum}"),
        ));
    }
    Ok(())
}

fn require_equal(gate: &GateResult, key: &str, expected: f64) -> Result<(), SpikeError> {
    let value = measurement(gate, key)?;
    if value != expected {
        return Err(SpikeError::for_gate(
            SpikeErrorCode::Evidence,
            gate.id,
            format!("measurement {key} is {value}, expected {expected}"),
        ));
    }
    Ok(())
}

fn validate_gate_contract(report: &TransportEvidenceV1) -> Result<(), SpikeError> {
    for result in &report.gates {
        if result.status != GateStatus::Pass {
            return Err(SpikeError::for_gate(
                SpikeErrorCode::Evidence,
                result.id,
                "transport decision cannot waive a failed gate",
            ));
        }
    }

    let routing = gate(report, TransportGateId::DeterministicRouting)?;
    require_at_least(routing, "commands", 200.0)?;
    require_at_least(routing, "events", 200.0)?;
    require_equal(routing, "cross_delivery", 0.0)?;

    require_at_least(
        gate(report, TransportGateId::TypedDomains)?,
        "typed_operations",
        5.0,
    )?;

    let sessions = gate(report, TransportGateId::FlatSessionIsolation)?;
    require_at_least(sessions, "sessions", 2.0)?;
    require_at_least(sessions, "commands_per_session", 100.0)?;
    require_at_least(sessions, "events_per_session", 100.0)?;
    require_equal(sessions, "cross_delivery", 0.0)?;

    require_at_least(
        gate(report, TransportGateId::RawBrowserCommand)?,
        "commands",
        1.0,
    )?;
    require_at_least(
        gate(report, TransportGateId::RawSessionCommand)?,
        "commands",
        1.0,
    )?;
    require_at_least(
        gate(report, TransportGateId::NamedRawEventParams)?,
        "named_events",
        1.0,
    )?;

    let drift = gate(report, TransportGateId::ProtocolDriftSurvival)?;
    require_at_least(drift, "fixtures", 3.0)?;
    require_equal(drift, "connection_survived", 1.0)?;
    require_equal(drift, "wildcard_envelope_available", 0.0)?;

    let sustained = gate(report, TransportGateId::SustainedScreencast)?;
    require_at_least(
        sustained,
        "elapsed_seconds",
        report.configuration.minimum_seconds,
    )?;
    require_at_least(
        sustained,
        "frames_received",
        report.configuration.minimum_frames as f64,
    )?;
    let received = measurement(sustained, "frames_received")?;
    require_equal(sustained, "frames_acknowledged", received)?;
    validate_rss_measurements(
        sustained.id,
        &sustained.measurements,
        report.configuration.minimum_seconds,
    )?;

    let acknowledgement = gate(report, TransportGateId::PromptAcknowledgement)?;
    require_equal(acknowledgement, "ack_before_handoff", 1.0)?;
    if measurement(acknowledgement, "ack_latency_ms_p99")? > 250.0
        || measurement(acknowledgement, "ack_latency_ms_max")? > 1_000.0
    {
        return Err(SpikeError::for_gate(
            SpikeErrorCode::Evidence,
            acknowledgement.id,
            "acknowledgement latency exceeds the unchanged threshold",
        ));
    }

    let saturation = gate(report, TransportGateId::BoundedHandoffSaturation)?;
    require_at_least(
        saturation,
        "saturation_seconds",
        report.configuration.saturation_seconds,
    )?;
    require_at_least(
        saturation,
        "handoff_attempts",
        report.configuration.saturation_attempts as f64,
    )?;
    require_at_least(saturation, "handoff_dropped", 1.0)?;
    require_at_least(saturation, "handoff_accepted", 1.0)?;

    let memory = gate(report, TransportGateId::BoundedMemoryProxy)?;
    validate_rss_measurements(
        memory.id,
        &memory.measurements,
        report.configuration.minimum_seconds,
    )?;
    if measurement(memory, "rss_growth_bytes")? > 32.0 * 1024.0 * 1024.0
        || measurement(memory, "rss_theil_sen_bytes_per_minute")? > 8.0 * 1024.0 * 1024.0
    {
        return Err(SpikeError::for_gate(
            SpikeErrorCode::Evidence,
            memory.id,
            "RSS trend exceeds the unchanged threshold",
        ));
    }

    let disconnect = gate(report, TransportGateId::DisconnectCleanup)?;
    require_equal(disconnect, "pending_command_started", 1.0)?;
    require_at_least(disconnect, "pending_calls_closed", 1.0)?;
    require_at_least(disconnect, "subscriptions_closed", 1.0)?;
    require_equal(disconnect, "close_reason_observed", 1.0)?;
    let pending_elapsed = measurement(disconnect, "pending_command_elapsed_seconds")?;
    let subscription_elapsed = measurement(disconnect, "subscription_elapsed_seconds")?;
    if !(0.0..1.0).contains(&pending_elapsed) || !(0.0..1.0).contains(&subscription_elapsed) {
        return Err(SpikeError::for_gate(
            SpikeErrorCode::Evidence,
            disconnect.id,
            "disconnect cleanup observed termination exceeds the one-second deadline",
        ));
    }

    let rebuild = gate(report, TransportGateId::ExplicitReconnectRebuild)?;
    require_at_least(rebuild, "connections", 2.0)?;
    require_at_least(rebuild, "sessions_rebuilt", 2.0)?;
    let elapsed = measurement(rebuild, "elapsed_seconds")?;
    if elapsed <= 0.0 || elapsed >= 5.0 {
        return Err(SpikeError::for_gate(
            SpikeErrorCode::Evidence,
            rebuild.id,
            "explicit reconnect/rebuild observed elapsed time exceeds five seconds",
        ));
    }
    Ok(())
}

fn validate_decisive_report(
    report: &TransportEvidenceV1,
    expected_platform: &str,
) -> Result<(), SpikeError> {
    validate_evidence(report)?;
    if report.environment.platform != expected_platform {
        return Err(SpikeError::new(
            SpikeErrorCode::Evidence,
            format!("report platform is not {expected_platform}"),
        ));
    }
    if !matches!(expected_platform, "linux" | "macos") {
        return Err(SpikeError::new(
            SpikeErrorCode::Evidence,
            "decisive evidence platform must be Linux or macOS",
        ));
    }
    if report.candidate.name != CDPKIT_NAME
        || report.candidate.version != CDPKIT_VERSION
        || report.candidate.checksum != CDPKIT_CHECKSUM
    {
        return Err(SpikeError::new(
            SpikeErrorCode::Evidence,
            "decisive evidence is not the exact published cdpkit 0.4.0 candidate",
        ));
    }
    if report.fixture.name != "cdp-transport-gate"
        || report.fixture.sha256
            != "sha256sum-of-ordered-fixture-files:9b42ae730d12a95772a946bf55e4838a5443b6cb4c536424570219041b6e2a68:84ba666539a996012a781637c1a894d8c7a4789cfca84661bd7cf8b79efa2e13"
    {
        return Err(SpikeError::new(
            SpikeErrorCode::Evidence,
            "decisive evidence uses an unexpected fixture identity",
        ));
    }
    if !report.limitations.iter().any(|limitation| {
        limitation.contains("named event params")
            && limitation.contains("wildcard/full-envelope")
            && limitation.contains("unbounded subscriber")
            && limitation.contains("queue-depth")
    }) {
        return Err(SpikeError::new(
            SpikeErrorCode::Evidence,
            "decisive evidence does not state the named-event and unbounded-subscriber limitations",
        ));
    }
    validate_gate_contract(report)
}

fn sha256_digest(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{:x}", hasher.finalize())
}

fn report_digest(report: &TransportEvidenceV1) -> Result<String, SpikeError> {
    let bytes = serde_json::to_vec(report)
        .map_err(|error| SpikeError::new(SpikeErrorCode::Evidence, error.to_string()))?;
    Ok(sha256_digest(&bytes))
}

pub fn decide(reports: &[TransportEvidenceV1]) -> Result<TransportDecisionV1, SpikeError> {
    if reports.len() != 2 {
        return Err(SpikeError::new(
            SpikeErrorCode::Evidence,
            "decision requires Linux and macOS evidence",
        ));
    }
    for report in reports {
        validate_decisive_report(report, &report.environment.platform)?;
    }
    let digests = reports
        .iter()
        .map(report_digest)
        .collect::<Result<Vec<_>, _>>()?;
    decide_with_digests(reports, &digests)
}

fn decide_with_digests(
    reports: &[TransportEvidenceV1],
    digests: &[String],
) -> Result<TransportDecisionV1, SpikeError> {
    if reports.len() != 2 || digests.len() != reports.len() {
        return Err(SpikeError::new(
            SpikeErrorCode::Evidence,
            "decision requires exactly two reports and two report digests",
        ));
    }
    if reports[0].candidate != reports[1].candidate {
        return Err(SpikeError::new(
            SpikeErrorCode::Evidence,
            "platform reports use different candidates",
        ));
    }
    if reports[0].configuration != reports[1].configuration
        || reports[0].fixture != reports[1].fixture
    {
        return Err(SpikeError::new(
            SpikeErrorCode::Evidence,
            "platform reports use different gate configuration or fixture",
        ));
    }
    let mut platforms = reports
        .iter()
        .map(|report| report.environment.platform.as_str())
        .collect::<BTreeSet<_>>();
    if platforms.len() != 2 || !platforms.remove("linux") || !platforms.remove("macos") {
        return Err(SpikeError::new(
            SpikeErrorCode::Evidence,
            "decision requires one Linux and one macOS report",
        ));
    }
    let mut limitations = BTreeSet::new();
    for report in reports {
        limitations.extend(report.limitations.iter().cloned());
    }
    Ok(TransportDecisionV1 {
        schema_version: EVIDENCE_SCHEMA_VERSION,
        decision: TransportDecision::AdoptCdpkit,
        candidate: reports[0].candidate.clone(),
        evidence: reports
            .iter()
            .zip(digests)
            .map(|(report, sha256)| EvidenceDigest {
                platform: report.environment.platform.clone(),
                sha256: sha256.clone(),
            })
            .collect(),
        gates: reports[0].gates.clone(),
        limitations: limitations.into_iter().collect(),
        rejected_alternatives: vec![
            "chromey 2.52.0 was not tested because cdpkit passed every unchanged gate; revisit it only after a demonstrated cdpkit lifecycle, ordering, or sustained-capture failure".into(),
            "an owned Tokio/tokio-tungstenite transport was not selected because cdpkit preserved the required raw command and named-event boundary without a fork".into(),
        ],
        rationale: "Exact cdpkit 0.4.0 passed all 13 unchanged gates on both Linux and macOS; the named-event-params limitation and unbounded subscriber-depth limitation remain explicit, so the production adapter must preserve Krometrail-owned reconnect, bounded handoff, and backpressure policy behind a replaceable boundary.".into(),
    })
}

pub fn decide_from_files(
    linux_path: &Path,
    macos_path: &Path,
) -> Result<TransportDecisionV1, SpikeError> {
    let paths = [("linux", linux_path), ("macos", macos_path)];
    let mut reports = Vec::with_capacity(paths.len());
    let mut digests = Vec::with_capacity(paths.len());
    for (platform, path) in paths {
        let bytes = std::fs::read(path).map_err(|error| {
            SpikeError::new(
                SpikeErrorCode::Io,
                format!("cannot read {platform} evidence: {error}"),
            )
        })?;
        let report = serde_json::from_slice::<TransportEvidenceV1>(&bytes).map_err(|error| {
            SpikeError::new(
                SpikeErrorCode::Evidence,
                format!("cannot decode {platform} evidence: {error}"),
            )
        })?;
        validate_decisive_report(&report, platform)?;
        reports.push(report);
        digests.push(sha256_digest(&bytes));
    }
    decide_with_digests(&reports, &digests)
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
