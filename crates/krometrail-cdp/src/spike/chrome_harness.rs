//! Real stable-Chrome qualification harness. It owns only disposable browser/profile and
//! loopback-fixture lifetime; reconnect and capture policy remain outside this spike.

use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    future::Future,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use futures_util::StreamExt;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const DISCONNECT_DEADLINE_SECONDS: f64 = 1.0;
const REBUILD_DEADLINE_SECONDS: f64 = 5.0;

#[derive(Clone, Debug)]
struct DisconnectMeasurements {
    pending_command_started: bool,
    pending_command_elapsed_seconds: f64,
    subscription_elapsed_seconds: f64,
    pending_calls_closed: bool,
    subscriptions_closed: bool,
    close_reason_observed: bool,
}

#[derive(Clone, Debug)]
struct RebuildMeasurements {
    connections: u64,
    sessions_rebuilt: u64,
    elapsed_seconds: f64,
}

pub async fn run_with_hard_stop<T, F>(hard_stop_seconds: u64, operation: F) -> Result<T, SpikeError>
where
    F: std::future::Future<Output = Result<T, SpikeError>>,
{
    run_with_hard_stop_stage(
        hard_stop_seconds,
        StageTracker::new(QualificationStage::Initializing),
        operation,
    )
    .await
}

pub async fn run_with_hard_stop_stage<T, F>(
    hard_stop_seconds: u64,
    stage: StageTracker,
    operation: F,
) -> Result<T, SpikeError>
where
    F: std::future::Future<Output = Result<T, SpikeError>>,
{
    if hard_stop_seconds == 0 {
        return Err(SpikeError::new(
            SpikeErrorCode::Evidence,
            "hard-stop seconds must be positive",
        ));
    }
    tokio::time::timeout(Duration::from_secs(hard_stop_seconds), operation)
        .await
        .map_err(|_| {
            let active = stage.current();
            SpikeError::new(
                SpikeErrorCode::Deadline,
                format!(
                    "complete real-Chrome gate exceeded {hard_stop_seconds} seconds at stage {active}"
                ),
            )
            .at_stage(active)
        })?
        .map_err(|error| error.at_stage(stage.current()))
}

use super::{
    cdpkit_adapter::CdpkitTransportFactory,
    contract::{SpikeTransport, SpikeTransportFactory, TransportScope},
    error::{QualificationStage, SpikeError, SpikeErrorCode, StageTracker},
    evidence::{
        BrowserEvidence, EVIDENCE_SCHEMA_VERSION, FixtureEvidence, GateConfiguration,
        GateProvenance, GateResult, GateStatus, RSS_SAMPLE_INTERVAL_SECONDS, RSS_WARMUP_SECONDS,
        SanitizedEnvironment, SourceIdentity, TransportEvidenceV1, TransportGateId,
        configuration_digest, rss_measurements_are_valid,
    },
    fixture_server::StaticFixtureServer,
    scenarios::run_candidate_wire_contract_with_stage,
};

#[derive(Clone, Debug)]
pub struct ScreencastMeasurements {
    pub elapsed_seconds: f64,
    pub frames_received: u64,
    pub frames_acknowledged: u64,
    pub handoff_accepted: u64,
    pub handoff_dropped: u64,
    pub saturation_seconds: f64,
    pub saturation_attempts: u64,
    pub ack_latency_ms_p50: f64,
    pub ack_latency_ms_p95: f64,
    pub ack_latency_ms_p99: f64,
    pub ack_latency_ms_max: f64,
    pub rss_sample_count: u64,
    pub rss_peak_bytes: u64,
    pub rss_first_window_median_bytes: u64,
    pub rss_last_window_median_bytes: u64,
    pub rss_theil_sen_bytes_per_minute: f64,
    pub rss_sampling_interval_seconds: f64,
    pub upstream_queue_depth_available: bool,
}

struct ChromeHarness {
    child: Child,
    profile: PathBuf,
    _fixture: StaticFixtureServer,
    fixture_url: String,
    ws_url: String,
}

impl ChromeHarness {
    async fn start(chrome_binary: &Path) -> Result<Self, SpikeError> {
        let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/browser/cdp-transport-gate");
        let fixture = StaticFixtureServer::start(&fixture_root)?;
        let fixture_url = format!("{}/index.html", fixture.base_url);
        let profile =
            std::env::temp_dir().join(format!("krometrail-cdp-gate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&profile);
        std::fs::create_dir_all(&profile).map_err(io_error)?;
        let port = free_port()?;
        let mut child = Command::new(chrome_binary)
            .args([
                "--headless=new",
                "--no-sandbox",
                "--disable-gpu",
                "--disable-dev-shm-usage",
                "--no-first-run",
                "--no-default-browser-check",
                "--remote-debugging-address=127.0.0.1",
            ])
            .arg(format!("--remote-debugging-port={port}"))
            .arg(format!("--user-data-dir={}", profile.display()))
            .arg(&fixture_url)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(io_error)?;
        let ws_url = match wait_for_ws_url(port, Duration::from_secs(15)).await {
            Ok(url) => url,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = std::fs::remove_dir_all(&profile);
                return Err(error);
            }
        };
        Ok(Self {
            child,
            profile,
            _fixture: fixture,
            fixture_url,
            ws_url,
        })
    }

    fn kill_browser(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for ChromeHarness {
    fn drop(&mut self) {
        self.kill_browser();
        let _ = std::fs::remove_dir_all(&self.profile);
    }
}

pub async fn run_real_chrome_gate(
    factory: &dyn SpikeTransportFactory,
    configuration: GateConfiguration,
    chrome_binary: &Path,
) -> Result<TransportEvidenceV1, SpikeError> {
    let hard_stop_seconds = configuration.hard_stop_seconds;
    let stage = StageTracker::new(QualificationStage::Initializing);
    run_with_hard_stop_stage(
        hard_stop_seconds,
        stage.clone(),
        run_real_chrome_gate_inner(factory, configuration, chrome_binary, stage),
    )
    .await
}

async fn run_real_chrome_gate_inner(
    factory: &dyn SpikeTransportFactory,
    configuration: GateConfiguration,
    chrome_binary: &Path,
    stage: StageTracker,
) -> Result<TransportEvidenceV1, SpikeError> {
    // Unknown future events cannot be made to occur in real Chrome. Run the exact candidate
    // contract against the wire-connected scripted controller and bind its trace digest to this
    // report instead of presenting those fixtures as a Chrome measurement.
    stage.set(QualificationStage::CandidateContract);
    let candidate_contract = run_candidate_wire_contract_with_stage(
        |endpoint| Box::new(CdpkitTransportFactory::with_scripted_endpoint(endpoint)),
        stage.clone(),
    )
    .await?;
    stage.set(QualificationStage::ChromeStartup);
    let mut browser = ChromeHarness::start(chrome_binary).await?;
    stage.set(QualificationStage::BrowserConnect);
    let transport = bounded(
        &stage,
        QualificationStage::BrowserConnect,
        factory.connect(&browser.ws_url),
    )
    .await?;
    stage.set(QualificationStage::TargetSetup);
    let target_a = bounded(
        &stage,
        QualificationStage::TargetSetup,
        create_target(transport.as_ref(), &browser.fixture_url),
    )
    .await?;
    let target_b = bounded(
        &stage,
        QualificationStage::TargetSetup,
        create_target(transport.as_ref(), &browser.fixture_url),
    )
    .await?;
    let session_a = bounded(
        &stage,
        QualificationStage::TargetSetup,
        transport.attach_flat_page(&target_a),
    )
    .await?;
    let session_b = bounded(
        &stage,
        QualificationStage::TargetSetup,
        transport.attach_flat_page(&target_b),
    )
    .await?;
    bounded(
        &stage,
        QualificationStage::TargetSetup,
        transport.send_raw(
            &TransportScope::Browser,
            "Target.activateTarget",
            serde_json::json!({"targetId": target_a}),
        ),
    )
    .await?;

    stage.set(QualificationStage::TypedProbe);
    let version = bounded(
        &stage,
        QualificationStage::TypedProbe,
        transport.send_raw(
            &TransportScope::Browser,
            "Browser.getVersion",
            serde_json::json!({}),
        ),
    )
    .await?;
    let typed = bounded(
        &stage,
        QualificationStage::TypedProbe,
        transport.run_typed_probe(&session_a),
    )
    .await?;
    if !typed.browser_version_observed
        || !typed.page_enable_observed
        || !typed.runtime_evaluate_observed
        || !typed.accessibility_observed
        || !typed.input_observed
    {
        return Err(SpikeError::new(
            SpikeErrorCode::Protocol,
            "typed domain probe was incomplete",
        ));
    }
    let mut gates = Vec::new();
    gates.push(pass(
        TransportGateId::TypedDomains,
        [("typed_operations", 5.0)],
    ));
    let mut observed_commands = BTreeSet::new();
    let mut observed_events = BTreeSet::new();
    let mut cross_delivery = 0_u64;
    if version.get("product").and_then(Value::as_str).is_some() {
        gates.push(pass(
            TransportGateId::RawBrowserCommand,
            [("commands", 1.0)],
        ));
    } else {
        gates.push(fail(
            TransportGateId::RawBrowserCommand,
            "Browser.getVersion raw result lacked product",
        ));
    }

    let raw_session = bounded(
        &stage,
        QualificationStage::RoutingSubscriptions,
        transport.send_raw(
            &session_a,
            "Runtime.evaluate",
            serde_json::json!({"expression":"1 + 1", "returnByValue":true}),
        ),
    )
    .await?;
    if raw_session.is_object() {
        gates.push(pass(
            TransportGateId::RawSessionCommand,
            [("commands", 1.0)],
        ));
    } else {
        gates.push(fail(
            TransportGateId::RawSessionCommand,
            "session raw command returned a non-object",
        ));
    }

    stage.set(QualificationStage::RoutingSubscriptions);
    bounded(
        &stage,
        QualificationStage::RoutingSubscriptions,
        transport.send_raw(&session_a, "Runtime.enable", serde_json::json!({})),
    )
    .await?;
    bounded(
        &stage,
        QualificationStage::RoutingSubscriptions,
        transport.send_raw(&session_b, "Runtime.enable", serde_json::json!({})),
    )
    .await?;
    let mut named = bounded(
        &stage,
        QualificationStage::RoutingSubscriptions,
        transport.subscribe_named(&session_a, "Runtime.consoleAPICalled"),
    )
    .await?;
    bounded(
        &stage,
        QualificationStage::RoutingSubscriptions,
        transport.send_raw(
            &session_a,
            "Runtime.evaluate",
            serde_json::json!({"expression":"console.log('cdp-transport-named-event')"}),
        ),
    )
    .await?;
    let named_event = bounded_stream(
        &stage,
        QualificationStage::RoutingSubscriptions,
        &mut named,
        "named raw event subscription closed",
    )
    .await?;
    if named_event.method == "Runtime.consoleAPICalled" {
        gates.push(pass(
            TransportGateId::NamedRawEventParams,
            [("named_events", 1.0)],
        ));
    } else {
        gates.push(fail(
            TransportGateId::NamedRawEventParams,
            "named event method identity changed",
        ));
    }

    let mut events_a = bounded(
        &stage,
        QualificationStage::RoutingSubscriptions,
        transport.subscribe_named(&session_a, "Runtime.consoleAPICalled"),
    )
    .await?;
    let mut events_b = bounded(
        &stage,
        QualificationStage::RoutingSubscriptions,
        transport.subscribe_named(&session_b, "Runtime.consoleAPICalled"),
    )
    .await?;
    let session_a_id = session_a.session_id().unwrap_or("session-a");
    let session_b_id = session_b.session_id().unwrap_or("session-b");
    for token in 0..100_u64 {
        let token_a = format!("cdp-session-a-{token}");
        let token_b = format!("cdp-session-b-{token}");
        observed_commands.insert((session_a_id.to_owned(), token_a.clone()));
        observed_commands.insert((session_b_id.to_owned(), token_b.clone()));
        bounded(
            &stage,
            QualificationStage::RoutingSubscriptions,
            transport.send_raw(
                &session_a,
                "Runtime.evaluate",
                serde_json::json!({"expression":format!("console.log('{token_a}')")}),
            ),
        )
        .await?;
        bounded(
            &stage,
            QualificationStage::RoutingSubscriptions,
            transport.send_raw(
                &session_b,
                "Runtime.evaluate",
                serde_json::json!({"expression":format!("console.log('{token_b}')")}),
            ),
        )
        .await?;
        let event_a = bounded_stream(
            &stage,
            QualificationStage::RoutingSubscriptions,
            &mut events_a,
            "session-a event stream closed",
        )
        .await?;
        let event_b = bounded_stream(
            &stage,
            QualificationStage::RoutingSubscriptions,
            &mut events_b,
            "session-b event stream closed",
        )
        .await?;
        let event_a_matches =
            event_a.scope == session_a && contains_string(&event_a.params, &token_a);
        let event_b_matches =
            event_b.scope == session_b && contains_string(&event_b.params, &token_b);
        if event_a_matches {
            observed_events.insert((session_a_id.to_owned(), token_a));
        } else {
            cross_delivery += 1;
        }
        if event_b_matches {
            observed_events.insert((session_b_id.to_owned(), token_b));
        } else {
            cross_delivery += 1;
        }
    }
    let correlated_pairs: BTreeSet<_> = observed_commands
        .intersection(&observed_events)
        .cloned()
        .collect();
    let correlated_commands = correlated_pairs.len() as f64;
    let correlated_events = correlated_pairs.len() as f64;
    let commands_per_session = observed_commands
        .iter()
        .filter(|(session, _)| session == session_a_id || session == session_b_id)
        .fold(
            BTreeMap::<String, u64>::new(),
            |mut counts, (session, _)| {
                *counts.entry(session.clone()).or_default() += 1;
                counts
            },
        );
    let events_per_session = observed_events
        .iter()
        .filter(|(session, _)| session == session_a_id || session == session_b_id)
        .fold(
            BTreeMap::<String, u64>::new(),
            |mut counts, (session, _)| {
                *counts.entry(session.clone()).or_default() += 1;
                counts
            },
        );
    gates.push(pass(
        TransportGateId::DeterministicRouting,
        [
            ("commands", correlated_commands),
            ("events", correlated_events),
            ("cross_delivery", cross_delivery as f64),
        ],
    ));
    if cross_delivery == 0 {
        gates.push(pass(
            TransportGateId::FlatSessionIsolation,
            [
                ("sessions", 2.0),
                (
                    "commands_per_session",
                    commands_per_session.values().copied().min().unwrap_or(0) as f64,
                ),
                (
                    "events_per_session",
                    events_per_session.values().copied().min().unwrap_or(0) as f64,
                ),
                ("cross_delivery", cross_delivery as f64),
            ],
        ));
    } else {
        gates.push(fail(
            TransportGateId::FlatSessionIsolation,
            "same-named events crossed flat sessions",
        ));
    }

    // Unknown event/enum fixtures are candidate-contract evidence, not real-Chrome
    // measurements. The attached trace digest makes that boundary auditable.
    let mut drift_gate = pass(
        TransportGateId::ProtocolDriftSurvival,
        [
            ("fixtures", candidate_contract.results.drift_fixtures as f64),
            (
                "connection_survived",
                f64::from(candidate_contract.results.connection_survived),
            ),
            ("wildcard_envelope_available", 0.0),
        ],
    );
    drift_gate.summary =
        "candidate wire-contract drift trace attached; not a real-Chrome fixture measurement"
            .into();
    gates.push(drift_gate);
    stage.set(QualificationStage::ScreencastStart);
    bounded(
        &stage,
        QualificationStage::ScreencastStart,
        transport.send_raw(&session_a, "Page.bringToFront", serde_json::json!({})),
    )
    .await?;
    bounded(
        &stage,
        QualificationStage::ScreencastStart,
        transport.start_screencast(&session_a),
    )
    .await?;
    stage.set(QualificationStage::ScreencastFrameReceive);
    let measurements =
        run_screencast_gate(transport.as_ref(), &session_a, &configuration, &stage).await?;
    let rss_values = rss_measurement_map(&measurements);
    let rss_valid = rss_measurements_are_valid(&rss_values, configuration.minimum_seconds);
    if rss_valid {
        gates.push(pass(
            TransportGateId::SustainedScreencast,
            [
                ("elapsed_seconds", measurements.elapsed_seconds),
                ("frames_received", measurements.frames_received as f64),
                (
                    "frames_acknowledged",
                    measurements.frames_acknowledged as f64,
                ),
                ("handoff_accepted", measurements.handoff_accepted as f64),
                ("handoff_dropped", measurements.handoff_dropped as f64),
                ("saturation_seconds", measurements.saturation_seconds),
                (
                    "saturation_attempts",
                    measurements.saturation_attempts as f64,
                ),
                ("ack_latency_ms_p50", measurements.ack_latency_ms_p50),
                ("ack_latency_ms_p95", measurements.ack_latency_ms_p95),
                ("ack_latency_ms_p99", measurements.ack_latency_ms_p99),
                ("ack_latency_ms_max", measurements.ack_latency_ms_max),
                ("rss_samples", measurements.rss_sample_count as f64),
                ("rss_peak_bytes", measurements.rss_peak_bytes as f64),
                (
                    "rss_first_window_median_bytes",
                    measurements.rss_first_window_median_bytes as f64,
                ),
                (
                    "rss_last_window_median_bytes",
                    measurements.rss_last_window_median_bytes as f64,
                ),
                (
                    "rss_theil_sen_bytes_per_minute",
                    measurements.rss_theil_sen_bytes_per_minute,
                ),
                (
                    "rss_sampling_interval_seconds",
                    measurements.rss_sampling_interval_seconds,
                ),
                ("rss_warmup_seconds", RSS_WARMUP_SECONDS as f64),
                ("upstream_queue_depth_available", 0.0),
            ],
        ));
    } else {
        gates.push(fail(
            TransportGateId::SustainedScreencast,
            "required RSS samples or windows were absent or zero",
        ));
    }
    let ack_status =
        measurements.ack_latency_ms_p99 <= 250.0 && measurements.ack_latency_ms_max <= 1000.0;
    if ack_status {
        gates.push(pass(
            TransportGateId::PromptAcknowledgement,
            [
                ("ack_before_handoff", 1.0),
                ("ack_latency_ms_p50", measurements.ack_latency_ms_p50),
                ("ack_latency_ms_p95", measurements.ack_latency_ms_p95),
                ("ack_latency_ms_p99", measurements.ack_latency_ms_p99),
                ("ack_latency_ms_max", measurements.ack_latency_ms_max),
            ],
        ));
    } else {
        gates.push(fail(
            TransportGateId::PromptAcknowledgement,
            "ack latency proxy exceeded p99 or max threshold",
        ));
    }
    gates.push(pass(
        TransportGateId::BoundedHandoffSaturation,
        [
            ("handoff_attempts", measurements.saturation_attempts as f64),
            ("handoff_accepted", measurements.handoff_accepted as f64),
            ("handoff_dropped", measurements.handoff_dropped as f64),
            ("saturation_seconds", measurements.saturation_seconds),
        ],
    ));
    let memory_status = rss_valid
        && measurements
            .rss_last_window_median_bytes
            .saturating_sub(measurements.rss_first_window_median_bytes)
            <= 32 * 1024 * 1024
        && measurements.rss_theil_sen_bytes_per_minute <= 8.0 * 1024.0 * 1024.0;
    if memory_status {
        gates.push(pass(
            TransportGateId::BoundedMemoryProxy,
            [
                ("rss_samples", measurements.rss_sample_count as f64),
                (
                    "rss_growth_bytes",
                    measurements
                        .rss_last_window_median_bytes
                        .saturating_sub(measurements.rss_first_window_median_bytes)
                        as f64,
                ),
                ("rss_peak_bytes", measurements.rss_peak_bytes as f64),
                (
                    "rss_theil_sen_bytes_per_minute",
                    measurements.rss_theil_sen_bytes_per_minute,
                ),
                (
                    "rss_first_window_median_bytes",
                    measurements.rss_first_window_median_bytes as f64,
                ),
                (
                    "rss_last_window_median_bytes",
                    measurements.rss_last_window_median_bytes as f64,
                ),
                (
                    "rss_sampling_interval_seconds",
                    measurements.rss_sampling_interval_seconds,
                ),
                ("rss_warmup_seconds", RSS_WARMUP_SECONDS as f64),
                ("upstream_queue_depth_available", 0.0),
            ],
        ));
    } else {
        gates.push(fail(
            TransportGateId::BoundedMemoryProxy,
            "RSS trend proxy exceeded a declared threshold",
        ));
    }

    stage.set(QualificationStage::Disconnect);
    match run_disconnect_probe(transport.as_ref(), &session_a, &mut browser, &stage).await {
        Ok(measurements) => {
            gates.push(pass(
                TransportGateId::DisconnectCleanup,
                [
                    (
                        "pending_command_started",
                        bool_measurement(measurements.pending_command_started),
                    ),
                    (
                        "pending_calls_closed",
                        bool_measurement(measurements.pending_calls_closed),
                    ),
                    (
                        "subscriptions_closed",
                        bool_measurement(measurements.subscriptions_closed),
                    ),
                    (
                        "pending_command_elapsed_seconds",
                        measurements.pending_command_elapsed_seconds,
                    ),
                    (
                        "subscription_elapsed_seconds",
                        measurements.subscription_elapsed_seconds,
                    ),
                    (
                        "close_reason_observed",
                        bool_measurement(measurements.close_reason_observed),
                    ),
                ],
            ));
        }
        Err(error) => gates.push(fail(
            TransportGateId::DisconnectCleanup,
            &format!("disconnect cleanup failed: {error}"),
        )),
    }
    stage.set(QualificationStage::Rebuild);
    match run_with_hard_stop_stage(
        REBUILD_DEADLINE_SECONDS as u64,
        stage.clone(),
        rebuild_sessions(factory, &browser.fixture_url, chrome_binary, stage.clone()),
    )
    .await
    {
        Ok(measurements) => gates.push(pass(
            TransportGateId::ExplicitReconnectRebuild,
            [
                ("connections", measurements.connections as f64),
                ("sessions_rebuilt", measurements.sessions_rebuilt as f64),
                ("elapsed_seconds", measurements.elapsed_seconds),
            ],
        )),
        Err(error) => gates.push(fail(
            TransportGateId::ExplicitReconnectRebuild,
            &format!("explicit reconnect/rebuild failed: {error}"),
        )),
    }

    let (product, revision, protocol) = (
        version
            .get("product")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_owned(),
        version
            .get("revision")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_owned(),
        version
            .get("protocolVersion")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_owned(),
    );
    let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/browser/cdp-transport-gate");
    let fixture_sha = sha256_directory(&fixture_path)?;
    let implementation_revision =
        command_output("git", &["rev-parse", "HEAD"]).unwrap_or_else(|_| "unavailable".into());
    let rust_version =
        command_output("rustc", &["--version"]).unwrap_or_else(|_| "unavailable".into());
    let gate_provenance = GateProvenance {
        implementation_revision: implementation_revision.clone(),
        configuration_sha256: configuration_digest(&configuration),
    };
    let mut evidence = TransportEvidenceV1 {
        schema_version: EVIDENCE_SCHEMA_VERSION,
        candidate: factory.candidate(),
        source: SourceIdentity {
            git_revision: implementation_revision,
            protocol_revision: "unavailable (cdpkit generated CDP_VERSION=1.3)".into(),
            rust_version,
        },
        environment: SanitizedEnvironment {
            platform: std::env::consts::OS.into(),
            architecture: std::env::consts::ARCH.into(),
        },
        browser: BrowserEvidence { product, protocol, revision },
        fixture: FixtureEvidence { name: "cdp-transport-gate".into(), sha256: fixture_sha },
        configuration,
        gate_provenance,
        gates,
        limitations: vec![
            "cdpkit exposes named event params through an unbounded subscriber; wildcard/full-envelope receive and queue-depth introspection are unavailable".into(),
            "ack latency values are receive-to-ack-completion proxies, not wire-enqueue timestamps".into(),
            "RSS is a process-level proxy from a continuously drained reader; it does not prove the hidden cdpkit subscriber queue is bounded".into(),
            format!("candidate-contract trace digest: {}", candidate_contract.trace_sha256),
        ],
        candidate_contract: Some(candidate_contract),
    };
    // Keep output deterministic even when gate construction order changes.
    evidence.gates.sort_by_key(|gate| gate.id);
    Ok(evidence)
}

pub fn failure_evidence(
    factory: &dyn SpikeTransportFactory,
    configuration: GateConfiguration,
    error: &SpikeError,
) -> TransportEvidenceV1 {
    let gates = TransportGateId::ALL
        .into_iter()
        .map(|id| GateResult {
            id,
            status: GateStatus::Fail,
            summary: error.to_string(),
            measurements: BTreeMap::new(),
            failure: Some(
                SpikeError::for_gate(SpikeErrorCode::Evidence, id, error.message.clone())
                    .at_stage(error.stage.unwrap_or(QualificationStage::Evidence)),
            ),
        })
        .collect();
    TransportEvidenceV1 {
        schema_version: EVIDENCE_SCHEMA_VERSION,
        candidate: factory.candidate(),
        source: SourceIdentity {
            git_revision: "unavailable".into(),
            protocol_revision: "unavailable".into(),
            rust_version: "unavailable".into(),
        },
        environment: SanitizedEnvironment {
            platform: std::env::consts::OS.into(),
            architecture: std::env::consts::ARCH.into(),
        },
        browser: BrowserEvidence {
            product: "unavailable".into(),
            protocol: "unavailable".into(),
            revision: "unavailable".into(),
        },
        fixture: FixtureEvidence {
            name: "cdp-transport-gate".into(),
            sha256: "unavailable".into(),
        },
        gate_provenance: GateProvenance {
            implementation_revision: "unavailable".into(),
            configuration_sha256: configuration_digest(&configuration),
        },
        configuration,
        gates,
        limitations: vec![
            "candidate qualification stopped before all real-Chrome measurements".into(),
        ],
        candidate_contract: None,
    }
}

async fn bounded<T, F>(
    stage: &StageTracker,
    phase: QualificationStage,
    operation: F,
) -> Result<T, SpikeError>
where
    F: Future<Output = Result<T, SpikeError>>,
{
    stage.set(phase);
    tokio::time::timeout(Duration::from_secs(5), operation)
        .await
        .map_err(|_| {
            SpikeError::new(
                SpikeErrorCode::Deadline,
                "qualification operation exceeded five-second phase deadline",
            )
            .at_stage(phase)
        })?
        .map_err(|error| error.at_stage(phase))
}

async fn bounded_stream<T, S>(
    stage: &StageTracker,
    phase: QualificationStage,
    stream: &mut S,
    closed_message: &str,
) -> Result<T, SpikeError>
where
    S: futures_util::Stream<Item = Result<T, SpikeError>> + Unpin,
{
    stage.set(phase);
    tokio::time::timeout(Duration::from_secs(5), stream.next())
        .await
        .map_err(|_| {
            SpikeError::new(
                SpikeErrorCode::Deadline,
                "qualification subscription receive exceeded five-second phase deadline",
            )
            .at_stage(phase)
        })?
        .ok_or_else(|| {
            SpikeError::new(SpikeErrorCode::SubscriptionClosed, closed_message).at_stage(phase)
        })?
        .map_err(|error| error.at_stage(phase))
}

async fn create_target(
    transport: &dyn SpikeTransport,
    fixture_url: &str,
) -> Result<String, SpikeError> {
    let result = transport
        .send_raw(
            &TransportScope::Browser,
            "Target.createTarget",
            serde_json::json!({"url": fixture_url}),
        )
        .await?;
    result
        .get("targetId")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| {
            SpikeError::new(
                SpikeErrorCode::Routing,
                "Target.createTarget lacked targetId",
            )
        })
}

async fn run_screencast_gate(
    transport: &dyn SpikeTransport,
    session: &TransportScope,
    config: &GateConfiguration,
    stage: &StageTracker,
) -> Result<ScreencastMeasurements, SpikeError> {
    let start = Instant::now();
    let mut frames_received = 0_u64;
    let mut frames_acknowledged = 0_u64;
    let mut accepted = 0_u64;
    let mut dropped = 0_u64;
    let mut attempts = 0_u64;
    let (handoff, _saturated_consumer) = tokio::sync::mpsc::channel::<i64>(1);
    let mut latencies = Vec::new();
    let mut rss = Vec::new();
    let mut next_sample = RSS_WARMUP_SECONDS;
    while start.elapsed().as_secs_f64() < config.minimum_seconds
        || frames_received < config.minimum_frames
    {
        if start.elapsed().as_secs_f64()
            >= config
                .minimum_seconds
                .max(config.minimum_frames as f64 / 60.0)
        {
            break;
        }
        let before = Instant::now();
        stage.set(QualificationStage::ScreencastFrameReceive);
        let frame = bounded(
            stage,
            QualificationStage::ScreencastFrameReceive,
            transport.next_screencast_frame(session),
        )
        .await?;
        frames_received += 1;
        bounded(
            stage,
            QualificationStage::ScreencastAck,
            transport.ack_screencast(session, frame.sequence),
        )
        .await?;
        frames_acknowledged += 1;
        latencies.push(before.elapsed().as_secs_f64() * 1000.0);
        attempts += 1;
        if handoff.try_send(frame.sequence).is_ok() {
            accepted += 1;
        } else {
            dropped += 1;
        }
        let second = start.elapsed().as_secs();
        if second >= next_sample {
            next_sample = second + RSS_SAMPLE_INTERVAL_SECONDS;
            if let Some(value) = process_rss() {
                rss.push((second, value));
            }
        }
    }
    let elapsed = start.elapsed().as_secs_f64();
    if elapsed < config.minimum_seconds
        || frames_received < config.minimum_frames
        || frames_received != frames_acknowledged
        || attempts < config.saturation_attempts
        || elapsed < config.saturation_seconds
        || dropped == 0
    {
        return Err(SpikeError::new(
            SpikeErrorCode::Invariant,
            format!(
                "sustained gate incomplete: elapsed={elapsed:.3}s frames={frames_received} acks={frames_acknowledged} attempts={attempts} drops={dropped}"
            ),
        ));
    }
    let values = latencies.clone();
    let median = percentile(values.clone(), 0.50);
    let p95 = percentile(values.clone(), 0.95);
    let p99 = percentile(values.clone(), 0.99);
    let max = values.into_iter().fold(0.0, f64::max);
    let first = median_window(&rss, 10, 30);
    let last = median_window(
        &rss,
        rss.last()
            .map(|(second, _)| second.saturating_sub(20))
            .unwrap_or(0),
        rss.last().map(|(second, _)| *second).unwrap_or(0),
    );
    let slope = theil_sen(&rss);
    Ok(ScreencastMeasurements {
        elapsed_seconds: elapsed,
        frames_received,
        frames_acknowledged,
        handoff_accepted: accepted,
        handoff_dropped: dropped,
        saturation_seconds: elapsed,
        saturation_attempts: attempts,
        ack_latency_ms_p50: median,
        ack_latency_ms_p95: p95,
        ack_latency_ms_p99: p99,
        ack_latency_ms_max: max,
        rss_sample_count: rss.len() as u64,
        rss_peak_bytes: rss.iter().map(|(_, value)| *value).max().unwrap_or(0),
        rss_first_window_median_bytes: first,
        rss_last_window_median_bytes: last,
        rss_theil_sen_bytes_per_minute: slope,
        rss_sampling_interval_seconds: median_sample_interval(&rss),
        upstream_queue_depth_available: false,
    })
}

async fn run_disconnect_probe(
    transport: &dyn SpikeTransport,
    session: &TransportScope,
    browser: &mut ChromeHarness,
    stage: &StageTracker,
) -> Result<DisconnectMeasurements, SpikeError> {
    let mut subscription = bounded(
        stage,
        QualificationStage::Disconnect,
        transport.subscribe_named(session, "Runtime.consoleAPICalled"),
    )
    .await?;
    let pending = transport.send_raw(
        session,
        "Runtime.evaluate",
        serde_json::json!({
            "expression": "console.log('cdp-transport-pending'); while (true) {}",
            "returnByValue": true
        }),
    );
    tokio::pin!(pending);

    // The console event is the wire-level barrier that proves Chrome started the command before
    // the browser is killed. It supplies deterministic readiness without a timing sleep.
    let started = tokio::time::timeout(Duration::from_secs(1), async {
        tokio::select! {
            result = &mut pending => {
                let _ = result;
                Err(SpikeError::new(
                    SpikeErrorCode::Disconnected,
                    "pending disconnect command completed before its readiness event",
                ))
            }
            event = subscription.next() => event
                .ok_or_else(|| SpikeError::new(
                    SpikeErrorCode::SubscriptionClosed,
                    "disconnect subscription closed before the pending command started",
                ).at_stage(QualificationStage::Disconnect))?,
        }
    })
    .await
    .map_err(|_| {
        SpikeError::new(
            SpikeErrorCode::Deadline,
            "pending disconnect command did not start within one second",
        )
    })??;
    if !contains_string(&started.params, "cdp-transport-pending") {
        return Err(SpikeError::new(
            SpikeErrorCode::Protocol,
            "pending disconnect command did not produce its readiness event",
        ));
    }

    let disconnect_started = Instant::now();
    browser.kill_browser();
    let (pending_result, subscription_result) = tokio::join!(
        async {
            let result = tokio::time::timeout(DISCONNECT_DURATION, &mut pending).await;
            (result, disconnect_started.elapsed().as_secs_f64())
        },
        async {
            let result = tokio::time::timeout(DISCONNECT_DURATION, subscription.next()).await;
            (result, disconnect_started.elapsed().as_secs_f64())
        }
    );
    let (pending_result, pending_command_elapsed_seconds) = pending_result;
    let (subscription_result, subscription_elapsed_seconds) = subscription_result;
    let pending_calls_closed = matches!(pending_result, Ok(Err(_)));
    let subscriptions_closed = matches!(subscription_result, Ok(None) | Ok(Some(Err(_))));
    let close_reason = transport.close_reason();
    let close_reason_observed = close_reason.is_some();
    let close_reason = close_reason.ok_or_else(|| {
        SpikeError::new(
            SpikeErrorCode::Disconnected,
            "candidate did not expose a close reason after socket close",
        )
    })?;

    if !pending_calls_closed
        || !subscriptions_closed
        || !close_reason.pending_calls_closed
        || !close_reason.subscriptions_closed
        || pending_command_elapsed_seconds >= DISCONNECT_DEADLINE_SECONDS
        || subscription_elapsed_seconds >= DISCONNECT_DEADLINE_SECONDS
    {
        return Err(SpikeError::new(
            SpikeErrorCode::Disconnected,
            format!(
                "disconnect outcomes were not both observed within one second: pending={pending_calls_closed}, subscription={subscriptions_closed}, reason={:?}",
                close_reason.reason
            ),
        ));
    }
    Ok(DisconnectMeasurements {
        pending_command_started: true,
        pending_command_elapsed_seconds,
        subscription_elapsed_seconds,
        pending_calls_closed,
        subscriptions_closed,
        close_reason_observed,
    })
}

const DISCONNECT_DURATION: Duration = Duration::from_secs(1);

async fn rebuild_sessions(
    factory: &dyn SpikeTransportFactory,
    fixture_url: &str,
    chrome_binary: &Path,
    stage: StageTracker,
) -> Result<RebuildMeasurements, SpikeError> {
    let started = Instant::now();
    stage.set(QualificationStage::Rebuild);
    let browser = ChromeHarness::start(chrome_binary).await?;
    let transport = bounded(
        &stage,
        QualificationStage::Rebuild,
        factory.connect(&browser.ws_url),
    )
    .await?;
    let a = bounded(
        &stage,
        QualificationStage::Rebuild,
        create_target(transport.as_ref(), fixture_url),
    )
    .await?;
    let b = bounded(
        &stage,
        QualificationStage::Rebuild,
        create_target(transport.as_ref(), fixture_url),
    )
    .await?;
    let session_a = bounded(
        &stage,
        QualificationStage::Rebuild,
        transport.attach_flat_page(&a),
    )
    .await?;
    let session_b = bounded(
        &stage,
        QualificationStage::Rebuild,
        transport.attach_flat_page(&b),
    )
    .await?;
    if session_a == session_b {
        return Err(SpikeError::new(
            SpikeErrorCode::Routing,
            "rebuild produced duplicate flat sessions",
        ));
    }
    let elapsed_seconds = started.elapsed().as_secs_f64();
    if elapsed_seconds >= REBUILD_DEADLINE_SECONDS {
        return Err(SpikeError::new(
            SpikeErrorCode::Deadline,
            "reconnect/session rebuild exceeded five seconds",
        ));
    }
    Ok(RebuildMeasurements {
        // The original gate connection plus this newly established connection.
        connections: 2,
        sessions_rebuilt: 2,
        elapsed_seconds,
    })
}

fn bool_measurement(value: bool) -> f64 {
    if value { 1.0 } else { 0.0 }
}

fn pass<const N: usize>(id: TransportGateId, values: [(&str, f64); N]) -> GateResult {
    GateResult {
        id,
        status: GateStatus::Pass,
        summary: "real-Chrome gate passed".into(),
        measurements: values
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
        failure: None,
    }
}
fn fail(id: TransportGateId, summary: &str) -> GateResult {
    GateResult {
        id,
        status: GateStatus::Fail,
        summary: summary.into(),
        measurements: BTreeMap::new(),
        failure: Some(SpikeError::for_gate(SpikeErrorCode::Evidence, id, summary)),
    }
}
fn io_error(error: std::io::Error) -> SpikeError {
    SpikeError::new(SpikeErrorCode::Io, error.to_string())
}
fn free_port() -> Result<u16, SpikeError> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).map_err(io_error)?;
    Ok(listener.local_addr().map_err(io_error)?.port())
}

async fn wait_for_ws_url(port: u16, timeout: Duration) -> Result<String, SpikeError> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        // A debugging endpoint can accept the HTTP request and leave the TCP stream open. Bound
        // both connect and body receive so startup cannot consume the global hard stop.
        let attempt = tokio::time::timeout(Duration::from_millis(500), async {
            let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .ok()?;
            let request = format!(
                "GET /json/version HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
            );
            stream.write_all(request.as_bytes()).await.ok()?;
            let mut body = [0_u8; 8192];
            let size = stream.read(&mut body).await.ok()?;
            let body = String::from_utf8_lossy(&body[..size]);
            let json = body
                .split("\r\n\r\n")
                .nth(1)
                .and_then(|body| serde_json::from_str::<Value>(body).ok())?;
            json.get("webSocketDebuggerUrl")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .await;
        if let Ok(Some(url)) = attempt {
            return Ok(url);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(SpikeError::new(
        SpikeErrorCode::Connect,
        "Chrome debugging endpoint did not become ready",
    ))
}

fn contains_string(value: &Value, prefix: &str) -> bool {
    match value {
        Value::String(text) => text.starts_with(prefix),
        Value::Array(values) => values.iter().any(|value| contains_string(value, prefix)),
        Value::Object(values) => values.values().any(|value| contains_string(value, prefix)),
        _ => false,
    }
}
fn percentile(mut values: Vec<f64>, percentile: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    values[((values.len() - 1) as f64 * percentile).round() as usize]
}
fn median_window(samples: &[(u64, u64)], start: u64, end: u64) -> u64 {
    let mut values: Vec<u64> = samples
        .iter()
        .filter(|(second, _)| *second >= start && *second <= end)
        .map(|(_, value)| *value)
        .collect();
    values.sort_unstable();
    values.get(values.len() / 2).copied().unwrap_or(0)
}
fn rss_measurement_map(measurements: &ScreencastMeasurements) -> BTreeMap<String, f64> {
    [
        ("rss_samples", measurements.rss_sample_count as f64),
        ("rss_peak_bytes", measurements.rss_peak_bytes as f64),
        (
            "rss_first_window_median_bytes",
            measurements.rss_first_window_median_bytes as f64,
        ),
        (
            "rss_last_window_median_bytes",
            measurements.rss_last_window_median_bytes as f64,
        ),
        (
            "rss_theil_sen_bytes_per_minute",
            measurements.rss_theil_sen_bytes_per_minute,
        ),
        (
            "rss_sampling_interval_seconds",
            measurements.rss_sampling_interval_seconds,
        ),
        ("rss_warmup_seconds", RSS_WARMUP_SECONDS as f64),
    ]
    .into_iter()
    .map(|(key, value)| (key.into(), value))
    .collect()
}

fn median_sample_interval(samples: &[(u64, u64)]) -> f64 {
    let intervals = samples
        .windows(2)
        .map(|window| window[1].0.saturating_sub(window[0].0) as f64)
        .collect();
    percentile(intervals, 0.5)
}

/// Parse the decimal KiB output emitted by macOS `ps` and normalize it to bytes.
///
/// This stays outside the platform-specific sampler so Linux tests compile and exercise
/// the exact fallible parsing path used by macOS without needing a macOS toolchain.
#[cfg(any(test, target_os = "macos"))]
fn parse_rss_kibibytes_to_bytes(value: &str) -> Option<u64> {
    value.trim().parse::<u64>().ok()?.checked_mul(1024)
}

#[cfg(target_os = "linux")]
fn process_rss() -> Option<u64> {
    let text = std::fs::read_to_string("/proc/self/statm").ok()?;
    let pages = text.split_whitespace().nth(1)?.parse::<u64>().ok()?;
    // Linux statm reports resident pages; Linux's base page size is 4096 bytes here.
    pages.checked_mul(4096)
}

#[cfg(target_os = "macos")]
fn process_rss() -> Option<u64> {
    let pid = std::process::id().to_string();
    let output = Command::new("ps")
        .args(["-o", "rss=", "-p", &pid])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let output = String::from_utf8(output.stdout).ok()?;
    parse_rss_kibibytes_to_bytes(&output)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn process_rss() -> Option<u64> {
    None
}
fn theil_sen(samples: &[(u64, u64)]) -> f64 {
    let mut slopes = Vec::new();
    for (index, (x1, y1)) in samples.iter().enumerate() {
        for (x2, y2) in samples.iter().skip(index + 1) {
            if x2 > x1 {
                slopes.push((*y2 as f64 - *y1 as f64) / (*x2 as f64 - *x1 as f64) * 60.0);
            }
        }
    }
    percentile(slopes, 0.5)
}
fn sha256_directory(path: &Path) -> Result<String, SpikeError> {
    let hashes = ["index.html", "animation.js"]
        .into_iter()
        .map(|file| {
            let bytes = std::fs::read(path.join(file)).map_err(io_error)?;
            Ok::<String, SpikeError>(format!("{:x}", Sha256::digest(bytes)))
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(format!(
        "sha256sum-of-ordered-fixture-files:{}:{}",
        hashes[0], hashes[1]
    ))
}
fn command_output(command: &str, args: &[&str]) -> Result<String, SpikeError> {
    let output = Command::new(command)
        .args(args)
        .output()
        .map_err(io_error)?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        Err(SpikeError::new(SpikeErrorCode::Evidence, "command failed"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_hashing_is_deterministic_and_does_not_require_external_hashing() {
        let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/browser/cdp-transport-gate");
        let first = sha256_directory(&fixture_path).expect("fixture digest");
        let second = sha256_directory(&fixture_path).expect("fixture digest");
        let expected = "sha256sum-of-ordered-fixture-files:9b42ae730d12a95772a946bf55e4838a5443b6cb4c536424570219041b6e2a68:84ba666539a996012a781637c1a894d8c7a4789cfca84661bd7cf8b79efa2e13";

        assert_eq!(first, expected);
        assert_eq!(first, second);

        // PATH manipulation is process-global and unsafe to simulate in parallel tests;
        // the source check is the safe cross-platform reproduction of the old failure.
        let source = include_str!("chrome_harness.rs");
        assert!(!source.contains("Command::new(\"sha256sum\")"));
    }

    #[test]
    fn parses_macos_rss_kibibytes_as_bytes_without_external_process_state() {
        assert_eq!(parse_rss_kibibytes_to_bytes(" 42\n"), Some(42 * 1024));
        assert_eq!(parse_rss_kibibytes_to_bytes("not-a-number"), None);
        assert_eq!(parse_rss_kibibytes_to_bytes("18446744073709551615"), None);
    }

    #[test]
    fn macos_sampler_uses_target_neutral_option_parser() {
        let source = include_str!("chrome_harness.rs");
        let macos_sampler = source
            .split("#[cfg(target_os = \"macos\")]\nfn process_rss")
            .nth(1)
            .and_then(|source| {
                source
                    .split("\n\n#[cfg(not(any(target_os = \"linux\", target_os = \"macos\")))]")
                    .next()
            })
            .expect("macOS process sampler");

        assert!(macos_sampler.contains("parse_rss_kibibytes_to_bytes"));
        assert!(!macos_sampler.contains(".parse::<u64>()?"));
    }

    #[tokio::test(start_paused = true)]
    async fn hard_stop_timeout_reports_the_active_stage_without_sleeping() {
        let stage = StageTracker::new(QualificationStage::ScreencastFrameReceive);
        let task = tokio::spawn(run_with_hard_stop_stage(
            5,
            stage,
            std::future::pending::<Result<(), SpikeError>>(),
        ));
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(5)).await;
        let error = task
            .await
            .expect("hard-stop task")
            .expect_err("must time out");
        assert_eq!(error.code, SpikeErrorCode::Deadline);
        assert_eq!(
            error.stage,
            Some(QualificationStage::ScreencastFrameReceive)
        );
        assert!(error.message.contains("ScreencastFrameReceive"));
    }

    #[tokio::test]
    async fn short_real_chrome_gate_is_bounded_when_chrome_is_available() {
        let chrome = Path::new("/usr/bin/google-chrome");
        if !chrome.exists() {
            return;
        }
        let configuration = GateConfiguration {
            minimum_seconds: 2.0,
            minimum_frames: 20,
            saturation_seconds: 2.0,
            saturation_attempts: 20,
            hard_stop_seconds: 30,
        };
        for _ in 0..2 {
            let evidence = run_real_chrome_gate(
                &CdpkitTransportFactory::new(),
                configuration.clone(),
                chrome,
            )
            .await
            .expect("short real-Chrome gate should complete");
            assert_eq!(evidence.gates.len(), TransportGateId::ALL.len());
            assert!(
                evidence
                    .gates
                    .iter()
                    .any(|gate| gate.id == TransportGateId::SustainedScreencast)
            );
        }
    }

    #[tokio::test]
    async fn hard_stop_rejects_zero_without_starting_the_operation() {
        let error = run_with_hard_stop(0, async {
            panic!("operation must not start");
            #[allow(unreachable_code)]
            Ok::<(), SpikeError>(())
        })
        .await
        .expect_err("zero hard stop must fail");
        assert_eq!(error.code, SpikeErrorCode::Evidence);
    }
}
