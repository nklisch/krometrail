use std::{
    collections::{BTreeMap, BTreeSet},
    net::IpAddr,
    path::Path,
    process::Command,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    contract::{
        CandidateContractEvidence, canonical_fixture_digest, canonical_trace_digest,
        recompute_candidate_contract_results, validate_candidate_contract_trace,
    },
    error::{SpikeError, SpikeErrorCode},
    scripted_peer::committed_protocol_fixtures,
};

pub const EVIDENCE_SCHEMA_VERSION: u16 = 2;

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
            Self::FlatSessionIsolation => &[
                "sessions",
                "commands_per_session",
                "events_per_session",
                "cross_delivery",
            ],
            Self::RawBrowserCommand | Self::RawSessionCommand => &["commands"],
            Self::NamedRawEventParams => &["named_events"],
            Self::ProtocolDriftSurvival => &[
                "fixtures",
                "connection_survived",
                "wildcard_envelope_available",
            ],
            Self::SustainedScreencast => {
                // A sustained pass is not valid without the same RSS evidence used by the
                // bounded-memory proxy; otherwise a missing sampler can still look successful.
                &[
                    "capture_elapsed_seconds",
                    "frames_received",
                    "frames_acknowledged",
                    "rss_samples",
                    "rss_sampling_interval_seconds",
                    "rss_warmup_seconds",
                    "rss_peak_bytes",
                    "rss_first_window_median_bytes",
                    "rss_last_window_median_bytes",
                    "rss_theil_sen_bytes_per_minute",
                    "handoff_accepted",
                    "handoff_dropped",
                    "handoff_elapsed_seconds",
                    "saturation_attempts",
                    "ack_latency_ms_p50",
                    "ack_latency_ms_p95",
                    "ack_latency_ms_p99",
                    "ack_latency_ms_max",
                    "upstream_queue_depth_available",
                ]
            }
            Self::PromptAcknowledgement => &[
                "ack_before_handoff",
                "ack_latency_ms_p50",
                "ack_latency_ms_p95",
                "ack_latency_ms_p99",
                "ack_latency_ms_max",
            ],
            Self::BoundedHandoffSaturation => &[
                "handoff_attempts",
                "handoff_accepted",
                "handoff_dropped",
                "handoff_elapsed_seconds",
            ],
            Self::BoundedMemoryProxy => &[
                "rss_samples",
                "rss_sampling_interval_seconds",
                "rss_warmup_seconds",
                "rss_growth_bytes",
                "rss_peak_bytes",
                "rss_first_window_median_bytes",
                "rss_last_window_median_bytes",
                "rss_theil_sen_bytes_per_minute",
                "upstream_queue_depth_available",
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
    /// Absolute maximum wall-clock time for the complete real-Chrome qualification operation.
    /// This global hard stop remains authoritative when the frame minimum is unmet; it is not
    /// derived from an assumed frame rate.
    pub hard_stop_seconds: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GateProvenance {
    /// The immutable source revision of the gate implementation used by the runner.
    pub implementation_revision: String,
    /// Digest of the exact serialized GateConfiguration used by the runner.
    pub configuration_sha256: String,
    /// Deterministic attestation of every tracked input to the qualification gate.
    /// This is optional only for non-decisive failure/unit evidence.
    pub source_attestation: Option<SourceAttestation>,
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceFileAttestation {
    pub path: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceAttestation {
    pub revision: String,
    pub digest: String,
    pub files: Vec<SourceFileAttestation>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransportEvidenceV2 {
    pub schema_version: u16,
    pub candidate: CandidateIdentity,
    pub source: SourceIdentity,
    pub environment: SanitizedEnvironment,
    pub browser: BrowserEvidence,
    pub fixture: FixtureEvidence,
    pub configuration: GateConfiguration,
    pub gate_provenance: GateProvenance,
    pub gates: Vec<GateResult>,
    pub limitations: Vec<String>,
    /// Required on decisive reports; optional for candidate-neutral unit evidence.
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlatformEvidence {
    pub platform: String,
    pub sha256: String,
    pub gates: Vec<GateResult>,
    pub candidate_contract: CandidateContractEvidence,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TransportDecisionV2 {
    pub schema_version: u16,
    pub decision: TransportDecision,
    pub candidate: CandidateIdentity,
    /// Platform-labelled results prevent a Linux-only rollup from hiding macOS values.
    pub evidence: Vec<PlatformEvidence>,
    pub limitations: Vec<String>,
    pub rejected_alternatives: Vec<String>,
    pub rationale: String,
}

// Rust aliases keep the spike's internal call sites source-compatible while the serialized
// contract is explicitly version 2. They are not accepted as legacy JSON schema aliases.
pub type TransportEvidenceV1 = TransportEvidenceV2;
pub type TransportDecisionV1 = TransportDecisionV2;

const RELEVANT_SOURCE_PATHS: &[&str] = &[
    "Cargo.toml",
    "Cargo.lock",
    "crates/krometrail-cdp/Cargo.toml",
    "crates/krometrail-cdp/src/bin/cdp-transport-gate.rs",
    "scripts/cdp-transport-gate-cross-worktree.sh",
    ".github/workflows/cdp-transport-gate.yml",
];
const RELEVANT_SOURCE_PREFIXES: &[&str] = &[
    "crates/krometrail-cdp/src/spike/",
    "crates/krometrail-cdp/tests/",
    "tests/fixtures/browser/cdp-transport-gate/",
];

pub fn is_git_revision(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

/// Resolve the repository from an explicit CLI root or the caller's current directory.
///
/// The build directory is deliberately not consulted: a binary may have been compiled in a
/// disposable worktree and then reused from a shared `CARGO_TARGET_DIR` after that worktree was
/// deleted. Git validates the root and the marker files prevent accidentally attesting an
/// unrelated repository with a coincidentally matching commit.
pub fn resolve_repository_root(explicit: Option<&Path>) -> Result<std::path::PathBuf, SpikeError> {
    let start = match explicit {
        Some(path) => path.to_owned(),
        None => std::env::current_dir().map_err(|error| {
            SpikeError::new(
                SpikeErrorCode::Evidence,
                format!("cannot determine the current directory: {error}"),
            )
        })?,
    };
    let start = std::fs::canonicalize(&start).map_err(|error| {
        SpikeError::new(
            SpikeErrorCode::Evidence,
            format!(
                "repository root does not exist: {} ({error})",
                start.display()
            ),
        )
    })?;
    if !start.is_dir() {
        return Err(SpikeError::new(
            SpikeErrorCode::Evidence,
            format!("repository root is not a directory: {}", start.display()),
        ));
    }
    let git_root = git_output(&start, &["rev-parse", "--show-toplevel"])?;
    let root = std::fs::canonicalize(Path::new(&git_root)).map_err(|error| {
        SpikeError::new(
            SpikeErrorCode::Evidence,
            format!("git repository root cannot be resolved: {error}"),
        )
    })?;
    for marker in [
        "Cargo.toml",
        "crates/krometrail-cdp/Cargo.toml",
        "tests/fixtures/browser/cdp-transport-gate/index.html",
    ] {
        if !root.join(marker).is_file() {
            return Err(SpikeError::new(
                SpikeErrorCode::Evidence,
                format!("repository root is missing required marker {marker}"),
            ));
        }
    }
    Ok(root)
}

/// Enumerate and hash the source that can affect qualification using an explicit runtime root.
/// The expected commit is used as the byte source so a report cannot bind a digest to a merely
/// similar working tree.
pub fn attest_relevant_source(expected_revision: &str) -> Result<SourceAttestation, SpikeError> {
    let root = resolve_repository_root(None)?;
    attest_relevant_source_at(&root, expected_revision)
}

pub fn attest_relevant_source_at(
    repository_root: &Path,
    expected_revision: &str,
) -> Result<SourceAttestation, SpikeError> {
    if !is_git_revision(expected_revision) {
        return Err(SpikeError::new(
            SpikeErrorCode::Evidence,
            "expected source revision must be exactly 40 lowercase hexadecimal characters",
        ));
    }
    let root = resolve_repository_root(Some(repository_root))?;
    let resolved = git_output(&root, &["rev-parse", "--verify", "HEAD"])?;
    if resolved != expected_revision {
        return Err(SpikeError::new(
            SpikeErrorCode::Evidence,
            "qualification checkout is not at the expected source revision",
        ));
    }
    let commit_ref = format!("{expected_revision}^{{commit}}");
    let commit = git_output(&root, &["rev-parse", "--verify", &commit_ref])?;
    if commit != expected_revision {
        return Err(SpikeError::new(
            SpikeErrorCode::Evidence,
            "expected source revision does not resolve to the checked-out commit",
        ));
    }

    let paths = relevant_paths_at_revision(&root, expected_revision)?;
    if paths.is_empty() {
        return Err(SpikeError::new(
            SpikeErrorCode::Evidence,
            "relevant qualification source set is empty",
        ));
    }
    let pathspecs = relevant_pathspecs();
    let mut diff_args = vec!["diff", "--quiet", expected_revision, "--"];
    diff_args.extend(pathspecs.iter().map(String::as_str));
    let mut cached_diff_args = vec!["diff", "--cached", "--quiet", expected_revision, "--"];
    cached_diff_args.extend(pathspecs.iter().map(String::as_str));
    if git_status(&root, &diff_args) || git_status(&root, &cached_diff_args) {
        return Err(SpikeError::new(
            SpikeErrorCode::Evidence,
            "relevant tracked qualification source differs from the expected revision",
        ));
    }
    let mut untracked_args = vec!["ls-files", "--others", "--exclude-standard", "--"];
    untracked_args.extend(pathspecs.iter().map(String::as_str));
    let untracked = git_output_allow_empty(&root, &untracked_args)?;
    if !untracked.is_empty() {
        return Err(SpikeError::new(
            SpikeErrorCode::Evidence,
            "relevant untracked qualification source is present",
        ));
    }

    let mut files = Vec::with_capacity(paths.len());
    for path in paths {
        let expected = git_bytes(&root, expected_revision, &path)?;
        let current_path = root.join(&path);
        let current = std::fs::read(&current_path).map_err(|error| {
            SpikeError::new(
                SpikeErrorCode::Io,
                format!("cannot read relevant qualification source: {error}"),
            )
        })?;
        if current != expected {
            return Err(SpikeError::new(
                SpikeErrorCode::Evidence,
                "relevant tracked qualification source differs from the expected revision",
            ));
        }
        files.push(SourceFileAttestation {
            path,
            sha256: sha256_digest(&expected),
        });
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    let digest = source_attestation_digest(expected_revision, &files)?;
    Ok(SourceAttestation {
        revision: expected_revision.into(),
        digest,
        files,
    })
}

pub fn validate_source_attestation(attestation: &SourceAttestation) -> Result<(), SpikeError> {
    if !is_git_revision(&attestation.revision)
        || !is_sha256_digest(&attestation.digest)
        || attestation.files.is_empty()
        || attestation
            .files
            .windows(2)
            .any(|files| files[0].path >= files[1].path)
        || attestation
            .files
            .iter()
            .any(|file| !is_relevant_source_path(&file.path) || !is_sha256_digest(&file.sha256))
        || source_attestation_digest(&attestation.revision, &attestation.files)?
            != attestation.digest
    {
        return Err(SpikeError::new(
            SpikeErrorCode::Evidence,
            "source attestation is malformed or its digest does not match",
        ));
    }
    Ok(())
}

fn source_attestation_digest(
    revision: &str,
    files: &[SourceFileAttestation],
) -> Result<String, SpikeError> {
    let encoded = serde_json::to_vec(&(revision, files))
        .map_err(|error| SpikeError::new(SpikeErrorCode::Evidence, error.to_string()))?;
    Ok(sha256_digest(&encoded))
}

fn relevant_pathspecs() -> Vec<String> {
    RELEVANT_SOURCE_PATHS
        .iter()
        .chain(RELEVANT_SOURCE_PREFIXES)
        .map(|path| (*path).to_owned())
        .collect()
}

fn relevant_paths_at_revision(root: &Path, revision: &str) -> Result<Vec<String>, SpikeError> {
    let pathspecs = relevant_pathspecs();
    let mut args = vec!["ls-tree", "-r", "--name-only", revision, "--"];
    args.extend(pathspecs.iter().map(String::as_str));
    let output = git_command(root, &args)?;
    let output = String::from_utf8(output).map_err(|error| {
        SpikeError::new(
            SpikeErrorCode::Evidence,
            format!("git returned non-UTF-8 qualification paths: {error}"),
        )
    })?;
    let mut paths = output.lines().map(str::to_owned).collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn is_relevant_source_path(path: &str) -> bool {
    RELEVANT_SOURCE_PATHS.contains(&path)
        || RELEVANT_SOURCE_PREFIXES
            .iter()
            .any(|prefix| path.starts_with(prefix))
}

fn git_bytes(root: &Path, revision: &str, path: &str) -> Result<Vec<u8>, SpikeError> {
    git_command_bytes(root, &["show", &format!("{revision}:{path}")])
}

fn git_output(root: &Path, args: &[&str]) -> Result<String, SpikeError> {
    let output = git_command(root, args)?;
    Ok(String::from_utf8_lossy(&output).trim().to_owned())
}

fn git_output_allow_empty(root: &Path, args: &[&str]) -> Result<String, SpikeError> {
    git_output(root, args)
}

fn git_status(root: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .current_dir(root)
        .args(args)
        .status()
        .map(|status| !status.success())
        .unwrap_or(true)
}

fn git_command(root: &Path, args: &[&str]) -> Result<Vec<u8>, SpikeError> {
    git_command_bytes(root, args)
}

fn git_command_bytes(root: &Path, args: &[&str]) -> Result<Vec<u8>, SpikeError> {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .map_err(|error| SpikeError::new(SpikeErrorCode::Io, format!("cannot run git: {error}")))?;
    if !output.status.success() {
        return Err(SpikeError::new(
            SpikeErrorCode::Evidence,
            "git qualification provenance command failed",
        ));
    }
    Ok(output.stdout)
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
        let has_failure = gate.failure.is_some();
        if (gate.status == GateStatus::Pass) == has_failure {
            return Err(SpikeError::for_gate(
                SpikeErrorCode::Evidence,
                gate.id,
                "gate status and failure payload are inconsistent",
            ));
        }
        if let Some(failure) = &gate.failure {
            if failure.gate != Some(gate.id) {
                return Err(SpikeError::for_gate(
                    SpikeErrorCode::Evidence,
                    gate.id,
                    "gate failure payload is bound to a different gate",
                ));
            }
        }
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
                        format!("passing gate lacks canonical measurement {key}"),
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
    if let Some(attestation) = &value.gate_provenance.source_attestation {
        validate_source_attestation(attestation)?;
        if attestation.revision != value.source.git_revision {
            return Err(SpikeError::new(
                SpikeErrorCode::Evidence,
                "source attestation revision does not match report provenance",
            ));
        }
    }
    let failure_report = !value.gates.is_empty()
        && value
            .gates
            .iter()
            .all(|gate| gate.status != GateStatus::Pass);
    let unavailable_provenance = value.source.git_revision == "unavailable"
        && value.gate_provenance.implementation_revision == "unavailable";
    let exact_provenance = is_git_revision(&value.source.git_revision)
        && is_git_revision(&value.gate_provenance.implementation_revision);
    if !(exact_provenance || (failure_report && unavailable_provenance)) {
        return Err(SpikeError::new(
            SpikeErrorCode::Evidence,
            "evidence source and implementation revisions must be lowercase full 40-hex SHAs",
        ));
    }
    if let Some(contract) = &value.candidate_contract {
        validate_candidate_contract(contract)?;
    }
    if value.gate_provenance.implementation_revision.is_empty()
        || value.gate_provenance.implementation_revision != value.source.git_revision
        || !is_sha256_digest(&value.gate_provenance.configuration_sha256)
        || configuration_digest(&value.configuration) != value.gate_provenance.configuration_sha256
    {
        return Err(SpikeError::new(
            SpikeErrorCode::Evidence,
            "gate provenance does not identify the exact implementation and configuration",
        ));
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

fn validate_candidate_contract(contract: &CandidateContractEvidence) -> Result<(), SpikeError> {
    let fixtures = committed_protocol_fixtures()?;
    let expected_fixtures = fixtures
        .iter()
        .map(|fixture| super::contract::CanonicalProtocolFixture {
            name: fixture.name.clone(),
            method: fixture.method.clone(),
            params: fixture.params.clone(),
        })
        .collect::<Vec<_>>();
    validate_candidate_contract_trace(&contract.trace)?;
    if contract.trace.fixtures != expected_fixtures {
        return Err(SpikeError::new(
            SpikeErrorCode::Evidence,
            "candidate trace fixture methods, params, or order differ from committed fixtures",
        ));
    }
    let expected_fixture_sha256 = canonical_fixture_digest(&contract.trace.fixtures)?;
    let expected_trace_sha256 = canonical_trace_digest(&contract.trace)?;
    let expected_results = recompute_candidate_contract_results(&contract.trace)?;
    if contract.fixture_sha256 != expected_fixture_sha256
        || contract.trace_sha256 != expected_trace_sha256
        || contract.trace_observations != contract.trace.observations.len() as u64
        || contract.results != expected_results
        || !is_sha256_digest(&contract.fixture_sha256)
        || !is_sha256_digest(&contract.trace_sha256)
        || contract.results.wire.drift_fixtures != fixtures.len() as u64
        || !contract.results.wire.connection_survived
        || contract.results.wire.routing_commands < 200
        || contract.results.wire.routing_events < 200
        || contract.results.wire.routing_cross_delivery != 0
        || !contract.results.wire.event_before_response
        || !contract.results.wire.detach_during_pending
        || !contract.results.wire.socket_closed
        || contract.results.wire.reconnect_connections < 2
        || contract.results.wire.sessions_rebuilt < 2
        || !contract.results.runtime.pending_calls_closed
        || !contract.results.runtime.subscriptions_closed
    {
        return Err(SpikeError::new(
            SpikeErrorCode::Evidence,
            "candidate wire-contract evidence is incomplete, mismatched, or failed",
        ));
    }
    Ok(())
}

fn is_sha256_digest(value: &str) -> bool {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return false;
    };
    hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub fn configuration_digest(configuration: &GateConfiguration) -> String {
    let encoded = serde_json::to_vec(configuration).expect("gate configuration is serializable");
    sha256_digest(&encoded)
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
    let interval = measurement("rss_sampling_interval_seconds").ok_or_else(|| {
        SpikeError::for_gate(
            SpikeErrorCode::Evidence,
            gate,
            "RSS sampling cadence is absent",
        )
    })?;
    if interval != RSS_SAMPLE_INTERVAL_SECONDS as f64 {
        return Err(SpikeError::for_gate(
            SpikeErrorCode::Evidence,
            gate,
            "RSS sampling interval does not match the canonical one-second cadence",
        ));
    }
    let warmup = measurement("rss_warmup_seconds").ok_or_else(|| {
        SpikeError::for_gate(SpikeErrorCode::Evidence, gate, "RSS warmup is absent")
    })?;
    if warmup != RSS_WARMUP_SECONDS as f64 {
        return Err(SpikeError::for_gate(
            SpikeErrorCode::Evidence,
            gate,
            "RSS warmup does not match the canonical gate contract",
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
    let encoded = serde_json::to_value(value)
        .map_err(|error| SpikeError::new(SpikeErrorCode::Evidence, error.to_string()))?;
    fn walk(value: &serde_json::Value) -> bool {
        match value {
            serde_json::Value::String(text) => contains_machine_detail(text),
            serde_json::Value::Array(values) => values.iter().any(walk),
            serde_json::Value::Object(values) => values.iter().any(|(key, value)| {
                // Canonical CDP routing uses numeric command tokens and the deterministic
                // session-a/session-b event token. Those are evidence fields, not credentials.
                // Other secret-shaped keys remain denied recursively.
                if is_sensitive_key(key) && !is_safe_canonical_value(key, value) {
                    return true;
                }
                walk(value)
            }),
            serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {
                false
            }
        }
    }
    if walk(&encoded) {
        return Err(SpikeError::new(
            SpikeErrorCode::Evidence,
            "evidence contains a private path, endpoint, URL, or credential",
        ));
    }
    Ok(())
}

fn is_sensitive_key(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "password"
            | "passwd"
            | "passphrase"
            | "token"
            | "secret"
            | "api_key"
            | "apikey"
            | "authorization"
            | "credential"
            | "credentials"
            | "username"
            | "user"
            | "login"
            | "email"
            | "cookie"
            | "session"
            | "bearer"
    )
}

fn is_safe_canonical_value(key: &str, value: &serde_json::Value) -> bool {
    match key.to_ascii_lowercase().as_str() {
        "token" => {
            value.is_number()
                || value.as_str().is_some_and(|token| {
                    let Some((session, sequence)) = token.rsplit_once('-') else {
                        return false;
                    };
                    matches!(session, "session-a" | "session-b")
                        && !sequence.is_empty()
                        && sequence.bytes().all(|byte| byte.is_ascii_digit())
                })
        }
        _ => false,
    }
}

fn contains_machine_detail(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    if ["http://", "https://", "ws://", "wss://", "file://"]
        .iter()
        .any(|prefix| lower.contains(prefix))
        || lower.contains("localhost")
        || lower.contains("--user-data-dir")
        || lower.contains("--remote-debugging-port")
        || lower.contains("/home/")
        || lower.contains("/users/")
        || lower.contains("/private/")
        || lower.contains("/tmp/")
        || lower.contains("/var/")
        || lower.contains("/root/")
        || lower.contains("/workspace/")
        || lower.contains("/build/")
        || lower.contains("\\\\")
        || lower.contains("c:\\")
        || lower.contains("c:/")
        || has_absolute_path(&lower)
        || has_ip_endpoint(&lower)
        || has_sensitive_assignment(&lower)
    {
        return true;
    }
    false
}

fn has_absolute_path(text: &str) -> bool {
    text.char_indices().any(|(index, character)| {
        character == '/'
            && (index == 0
                || text[..index].chars().next_back().is_some_and(|previous| {
                    previous.is_whitespace() || "([{'\"=:".contains(previous)
                }))
    })
}

fn has_ip_endpoint(text: &str) -> bool {
    text.split(|character: char| {
        character.is_whitespace()
            || matches!(
                character,
                '[' | ']' | '(' | ')' | '{' | '}' | ',' | ';' | '"' | '='
            )
    })
    .filter(|token| !token.is_empty())
    .any(is_ip_literal_or_endpoint)
}

fn is_ip_literal_or_endpoint(token: &str) -> bool {
    let token = token.trim_matches(|character: char| ".,:;".contains(character));
    if token.parse::<IpAddr>().is_ok() {
        return true;
    }
    if let Some(bracketed) = token.strip_prefix('[') {
        if let Some((host, port)) = bracketed.split_once("]:") {
            return port.parse::<u16>().is_ok() && host.parse::<IpAddr>().is_ok();
        }
    }
    token
        .rsplit_once(':')
        .is_some_and(|(host, port)| port.parse::<u16>().is_ok() && host.parse::<IpAddr>().is_ok())
}

fn has_sensitive_assignment(text: &str) -> bool {
    const SENSITIVE_KEYS: &[&str] = &[
        "password",
        "passwd",
        "passphrase",
        "token",
        "secret",
        "api_key",
        "apikey",
        "authorization",
        "credential",
        "credentials",
        "username",
        "user",
        "login",
        "email",
        "cookie",
        "session",
        "bearer",
    ];

    for key in SENSITIVE_KEYS {
        let mut rest = text;
        while let Some(index) = rest.find(key) {
            let before = &rest[..index];
            let after = &rest[index + key.len()..];
            let valid_boundary = before
                .chars()
                .next_back()
                .is_none_or(|character| !character.is_ascii_alphanumeric() && character != '_');
            let after_boundary = after
                .chars()
                .next()
                .is_none_or(|character| !character.is_ascii_alphanumeric() && character != '_');
            if valid_boundary && after_boundary {
                let assignment = after.trim_start();
                if assignment.starts_with('=') || assignment.starts_with(':') {
                    return true;
                }
            }
            rest = &rest[index + key.len()..];
        }
    }
    false
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
        "capture_elapsed_seconds",
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
        "handoff_elapsed_seconds",
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

/// Validate one platform report against the complete decisive schema-v2 contract.
///
/// This is intentionally shared by local Linux runs and the hosted macOS workflow so
/// platform qualification cannot silently diverge at the final validation boundary.
pub fn validate_decisive_report(
    report: &TransportEvidenceV1,
    expected_platform: &str,
) -> Result<(), SpikeError> {
    let root = resolve_repository_root(None)?;
    validate_decisive_report_at(report, expected_platform, &root)
}

pub fn validate_decisive_report_at(
    report: &TransportEvidenceV1,
    expected_platform: &str,
    repository_root: &Path,
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
    if !is_git_revision(&report.source.git_revision)
        || report.gate_provenance.implementation_revision != report.source.git_revision
        || report.gate_provenance.configuration_sha256
            != configuration_digest(&report.configuration)
    {
        return Err(SpikeError::new(
            SpikeErrorCode::Evidence,
            "decisive evidence lacks immutable gate implementation/configuration provenance",
        ));
    }
    let attestation = report
        .gate_provenance
        .source_attestation
        .as_ref()
        .ok_or_else(|| {
            SpikeError::new(
                SpikeErrorCode::Evidence,
                "decisive evidence lacks relevant-source attestation",
            )
        })?;
    let current_attestation =
        attest_relevant_source_at(repository_root, &report.source.git_revision)?;
    if attestation != &current_attestation {
        return Err(SpikeError::new(
            SpikeErrorCode::Evidence,
            "decisive evidence source attestation does not match the clean qualification tree",
        ));
    }
    let candidate_contract = report.candidate_contract.as_ref().ok_or_else(|| {
        SpikeError::new(
            SpikeErrorCode::Evidence,
            "decisive evidence lacks scripted candidate-contract trace and results",
        )
    })?;
    validate_candidate_contract(candidate_contract)?;
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
    // `decide` validates the full report, but this helper is also used by file and unit paths.
    // Recompute both candidate contracts here so no caller can bypass the canonical-material
    // check by supplying a precomputed report digest.
    for report in reports {
        let contract = report.candidate_contract.as_ref().ok_or_else(|| {
            SpikeError::new(
                SpikeErrorCode::Evidence,
                "decision report lacks canonical candidate-contract material",
            )
        })?;
        validate_candidate_contract(contract)?;
    }
    if reports[0].candidate != reports[1].candidate {
        return Err(SpikeError::new(
            SpikeErrorCode::Evidence,
            "platform reports use different candidates",
        ));
    }
    if reports[0].configuration != reports[1].configuration
        || reports[0].gate_provenance != reports[1].gate_provenance
        || reports[0].fixture != reports[1].fixture
    {
        return Err(SpikeError::new(
            SpikeErrorCode::Evidence,
            "platform reports use different immutable gate revision, configuration, or fixture",
        ));
    }
    let linux_contract = reports[0]
        .candidate_contract
        .as_ref()
        .expect("validated candidate contract");
    let macos_contract = reports[1]
        .candidate_contract
        .as_ref()
        .expect("validated candidate contract");
    if linux_contract.fixture_sha256 != macos_contract.fixture_sha256
        || linux_contract.trace_sha256 != macos_contract.trace_sha256
        || linux_contract.trace_observations != macos_contract.trace_observations
        || linux_contract.results != macos_contract.results
        || linux_contract.trace != macos_contract.trace
    {
        return Err(SpikeError::new(
            SpikeErrorCode::Evidence,
            "platform reports use different deterministic candidate-contract trace, fixture digest, or results",
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
    let mut platform_evidence = reports
        .iter()
        .zip(digests)
        .map(|(report, sha256)| PlatformEvidence {
            platform: report.environment.platform.clone(),
            sha256: sha256.clone(),
            gates: report.gates.clone(),
            candidate_contract: report
                .candidate_contract
                .clone()
                .expect("decisive report candidate contract was validated"),
        })
        .collect::<Vec<_>>();
    platform_evidence.sort_by(|left, right| left.platform.cmp(&right.platform));
    Ok(TransportDecisionV1 {
        schema_version: EVIDENCE_SCHEMA_VERSION,
        decision: TransportDecision::AdoptCdpkit,
        candidate: reports[0].candidate.clone(),
        evidence: platform_evidence,
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
    let root = resolve_repository_root(None)?;
    decide_from_files_at(linux_path, macos_path, &root)
}

pub fn decide_from_files_at(
    linux_path: &Path,
    macos_path: &Path,
    repository_root: &Path,
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
        validate_decisive_report_at(&report, platform, repository_root)?;
        reports.push(report);
        digests.push(sha256_digest(&bytes));
    }
    decide_with_digests(&reports, &digests)
}

pub fn write_json_schema(path: &Path) -> Result<(), SpikeError> {
    let schema = schemars::schema_for!(TransportEvidenceV2);
    let encoded = serde_json::to_vec_pretty(&schema)
        .map_err(|error| SpikeError::new(SpikeErrorCode::Evidence, error.to_string()))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| SpikeError::new(SpikeErrorCode::Io, error.to_string()))?;
    }
    std::fs::write(path, encoded)
        .map_err(|error| SpikeError::new(SpikeErrorCode::Io, error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spike::contract::{
        CandidateContractEvidence, CandidateContractTrace, CandidateRuntimeAssertions,
        CanonicalProtocolFixture, CanonicalWireObservation, CanonicalWireObservationKind,
    };

    fn candidate_contract() -> CandidateContractEvidence {
        let fixtures = committed_protocol_fixtures()
            .unwrap()
            .into_iter()
            .map(|fixture| CanonicalProtocolFixture {
                name: fixture.name,
                method: fixture.method,
                params: fixture.params,
            })
            .collect::<Vec<_>>();
        let mut observations = Vec::new();
        let mut sequence = 0_u64;
        let mut push = |connection, kind, request_id, method, session_id, params| {
            sequence += 1;
            observations.push(CanonicalWireObservation {
                sequence,
                connection,
                kind,
                request_id,
                method,
                session_id,
                params,
            });
        };
        for fixture in &fixtures {
            push(
                1,
                CanonicalWireObservationKind::Event,
                None,
                Some(fixture.method.clone()),
                Some("session-a".into()),
                fixture.params.clone(),
            );
        }
        for token in 0..200_u64 {
            push(
                1,
                CanonicalWireObservationKind::Command,
                Some(1000 + token),
                Some("Runtime.evaluate".into()),
                Some(
                    if token % 2 == 0 {
                        "session-a"
                    } else {
                        "session-b"
                    }
                    .into(),
                ),
                serde_json::json!({"token": token}),
            );
            push(
                1,
                CanonicalWireObservationKind::Event,
                None,
                Some("Runtime.consoleAPICalled".into()),
                Some(
                    if token % 2 == 0 {
                        "session-a"
                    } else {
                        "session-b"
                    }
                    .into(),
                ),
                serde_json::json!({"token": format!("{}-{token}", if token % 2 == 0 { "session-a" } else { "session-b" })}),
            );
        }
        push(
            1,
            CanonicalWireObservationKind::Command,
            Some(2001),
            Some("Runtime.evaluate".into()),
            Some("session-a".into()),
            serde_json::json!({"phase":"event-before-response"}),
        );
        push(
            1,
            CanonicalWireObservationKind::Event,
            None,
            Some("Runtime.consoleAPICalled".into()),
            Some("session-a".into()),
            serde_json::json!({"token":"session-a-999"}),
        );
        push(
            1,
            CanonicalWireObservationKind::Response,
            Some(2001),
            None,
            None,
            serde_json::json!({}),
        );
        push(
            1,
            CanonicalWireObservationKind::Command,
            Some(3001),
            Some("Runtime.evaluate".into()),
            Some("session-b".into()),
            serde_json::json!({"phase":"detach-during-pending"}),
        );
        push(
            1,
            CanonicalWireObservationKind::Event,
            None,
            Some("Target.detachedFromTarget".into()),
            Some("session-b".into()),
            serde_json::json!({"targetId":"target-b"}),
        );
        push(
            1,
            CanonicalWireObservationKind::ConnectionClosed,
            None,
            None,
            None,
            serde_json::json!({}),
        );
        for target in ["target-a", "target-b"] {
            push(
                2,
                CanonicalWireObservationKind::Command,
                Some(4000),
                Some("Target.attachToTarget".into()),
                None,
                serde_json::json!({"targetId":target}),
            );
        }
        CandidateContractEvidence::from_trace(CandidateContractTrace {
            fixtures,
            observations,
            runtime_assertions: CandidateRuntimeAssertions {
                pending_calls_closed: true,
                subscriptions_closed: true,
            },
        })
        .unwrap()
    }

    fn report(platform: &str, revision: &str) -> TransportEvidenceV2 {
        let configuration = GateConfiguration {
            minimum_seconds: 60.0,
            minimum_frames: 1_000,
            saturation_seconds: 10.0,
            saturation_attempts: 100,
            hard_stop_seconds: 120,
        };
        TransportEvidenceV2 {
            schema_version: EVIDENCE_SCHEMA_VERSION,
            candidate: CandidateIdentity {
                name: "candidate".into(),
                version: "1".into(),
                checksum: "checksum".into(),
            },
            source: SourceIdentity {
                git_revision: revision.into(),
                protocol_revision: "protocol".into(),
                rust_version: "rust".into(),
            },
            environment: SanitizedEnvironment {
                platform: platform.into(),
                architecture: "x86_64".into(),
            },
            browser: BrowserEvidence {
                product: "Chrome".into(),
                protocol: "1.3".into(),
                revision: "revision".into(),
            },
            fixture: FixtureEvidence {
                name: "fixture".into(),
                sha256: "fixture-sha".into(),
            },
            gate_provenance: GateProvenance {
                implementation_revision: revision.into(),
                configuration_sha256: configuration_digest(&configuration),
                source_attestation: None,
            },
            configuration,
            gates: Vec::new(),
            limitations: Vec::new(),
            candidate_contract: Some(candidate_contract()),
        }
    }

    #[test]
    fn decision_rejects_mixed_gate_implementation_revisions() {
        let linux = report("linux", "revision-a");
        let macos = report("macos", "revision-b");
        let error = decide_with_digests(&[linux, macos], &["sha256:a".into(), "sha256:b".into()])
            .expect_err("mixed revisions must not be selected");
        assert!(error.message.contains("immutable gate revision"));
    }

    #[test]
    fn decision_rejects_linux_only_rollups() {
        let linux_a = report("linux", "revision-a");
        let linux_b = report("linux", "revision-a");
        let error =
            decide_with_digests(&[linux_a, linux_b], &["sha256:a".into(), "sha256:b".into()])
                .expect_err("two Linux reports must not stand in for Linux and macOS");
        assert!(error.message.contains("one Linux and one macOS"));
    }

    #[test]
    fn decision_keeps_platform_labelled_gate_and_candidate_results() {
        let linux = report("linux", "revision-a");
        let macos = report("macos", "revision-a");
        let decision =
            decide_with_digests(&[linux, macos], &["sha256:a".into(), "sha256:b".into()])
                .expect("same revision reports should roll up");
        assert_eq!(decision.evidence.len(), 2);
        assert_eq!(decision.evidence[0].platform, "linux");
        assert_eq!(decision.evidence[1].platform, "macos");
        assert_eq!(decision.evidence[0].gates.len(), 0);
        assert_eq!(decision.evidence[1].gates.len(), 0);
        assert!(decision.evidence[0].candidate_contract.trace_observations > 0);
        assert_eq!(
            decision.evidence[0].candidate_contract.trace,
            decision.evidence[1].candidate_contract.trace
        );
    }

    #[test]
    fn decision_revalidates_canonical_material_before_platform_comparison() {
        let mut macos = report("macos", "revision-a");
        macos
            .candidate_contract
            .as_mut()
            .unwrap()
            .results
            .wire
            .routing_commands = 201;
        let error = decide_with_digests(
            &[report("linux", "revision-a"), macos],
            &["sha256:a".into(), "sha256:b".into()],
        )
        .expect_err("decision must recompute duplicated routing summaries");
        assert!(error.message.contains("candidate"));

        let mut macos = report("macos", "revision-a");
        macos.candidate_contract.as_mut().unwrap().trace.fixtures[0].params["kind"] =
            serde_json::json!("mutated");
        let error = decide_with_digests(
            &[report("linux", "revision-a"), macos],
            &["sha256:a".into(), "sha256:b".into()],
        )
        .expect_err("decision must reject mutated canonical fixture material");
        assert!(error.message.contains("fixture methods, params, or order"));
    }

    #[test]
    fn decision_rejects_each_candidate_contract_identity_mismatch() {
        let cases = ["fixture", "trace", "result"];
        for case in cases {
            let linux = report("linux", "revision-a");
            let mut macos = report("macos", "revision-a");
            let contract = macos.candidate_contract.as_mut().unwrap();
            match case {
                "fixture" => contract.fixture_sha256.push('x'),
                "trace" => contract.trace_sha256.replace_range(7..8, "b"),
                "result" => contract.results.wire.routing_commands += 1,
                _ => unreachable!(),
            }
            let error =
                decide_with_digests(&[linux, macos], &["sha256:a".into(), "sha256:b".into()])
                    .expect_err("cross-platform candidate contract drift must reject the decision");
            assert!(error.message.contains("candidate"));
        }
    }
}
