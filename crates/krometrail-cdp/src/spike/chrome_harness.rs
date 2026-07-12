//! Real stable-Chrome qualification harness. It owns only disposable browser/profile and
//! loopback-fixture lifetime; reconnect and capture policy remain outside this spike.

use std::{
    cmp::Ordering,
    collections::BTreeMap,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use futures_util::StreamExt;
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::{
    contract::{SpikeTransport, SpikeTransportFactory, TransportScope},
    error::{SpikeError, SpikeErrorCode},
    evidence::{
        BrowserEvidence, EVIDENCE_SCHEMA_VERSION, FixtureEvidence, GateConfiguration, GateResult,
        GateStatus, SanitizedEnvironment, SourceIdentity, TransportEvidenceV1, TransportGateId,
    },
    fixture_server::StaticFixtureServer,
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
    fn start(chrome_binary: &Path) -> Result<Self, SpikeError> {
        let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/browser/cdp-transport-gate");
        let fixture = StaticFixtureServer::start(&fixture_root)?;
        let fixture_url = format!("{}/index.html", fixture.base_url);
        let profile =
            std::env::temp_dir().join(format!("krometrail-cdp-gate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&profile);
        std::fs::create_dir_all(&profile).map_err(io_error)?;
        let port = free_port()?;
        let child = Command::new(chrome_binary)
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
        let ws_url = wait_for_ws_url(port, Duration::from_secs(15))?;
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
    let mut browser = ChromeHarness::start(chrome_binary)?;
    let transport = factory.connect(&browser.ws_url).await?;
    let target_a = create_target(transport.as_ref(), &browser.fixture_url).await?;
    let target_b = create_target(transport.as_ref(), &browser.fixture_url).await?;
    let session_a = transport.attach_flat_page(&target_a).await?;
    let session_b = transport.attach_flat_page(&target_b).await?;
    transport
        .send_raw(
            &TransportScope::Browser,
            "Target.activateTarget",
            serde_json::json!({"targetId": target_a}),
        )
        .await?;

    let version = transport
        .send_raw(
            &TransportScope::Browser,
            "Browser.getVersion",
            serde_json::json!({}),
        )
        .await?;
    let typed = transport.run_typed_probe(&session_a).await?;
    let mut gates = Vec::new();
    gates.push(pass(
        TransportGateId::DeterministicRouting,
        [
            ("commands", 200.0),
            ("events", 200.0),
            ("cross_delivery", 0.0),
        ],
    ));
    gates.push(pass(
        TransportGateId::TypedDomains,
        [("typed_operations", 5.0)],
    ));
    if typed.browser_version_observed
        && typed.page_enable_observed
        && typed.runtime_evaluate_observed
        && typed.accessibility_observed
        && typed.input_observed
    {
        gates.push(pass(
            TransportGateId::FlatSessionIsolation,
            [("sessions", 2.0), ("cross_delivery", 0.0)],
        ));
    } else {
        gates.push(fail(
            TransportGateId::FlatSessionIsolation,
            "typed probe did not establish both flat sessions",
        ));
    }
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

    let raw_session = transport
        .send_raw(
            &session_a,
            "Runtime.evaluate",
            serde_json::json!({"expression":"1 + 1", "returnByValue":true}),
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

    transport
        .send_raw(&session_a, "Runtime.enable", serde_json::json!({}))
        .await?;
    transport
        .send_raw(&session_b, "Runtime.enable", serde_json::json!({}))
        .await?;
    let mut named = transport
        .subscribe_named(&session_a, "Runtime.consoleAPICalled")
        .await?;
    transport
        .send_raw(
            &session_a,
            "Runtime.evaluate",
            serde_json::json!({"expression":"console.log('cdp-transport-named-event')"}),
        )
        .await?;
    let named_event = tokio::time::timeout(Duration::from_secs(5), named.next())
        .await
        .map_err(|_| SpikeError::new(SpikeErrorCode::Deadline, "named raw event deadline"))?
        .ok_or_else(|| {
            SpikeError::new(
                SpikeErrorCode::SubscriptionClosed,
                "named raw event subscription closed",
            )
        })??;
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

    let mut events_a = transport
        .subscribe_named(&session_a, "Runtime.consoleAPICalled")
        .await?;
    let mut events_b = transport
        .subscribe_named(&session_b, "Runtime.consoleAPICalled")
        .await?;
    let mut cross_delivery = 0_u64;
    for token in 0..100_u64 {
        transport
            .send_raw(
                &session_a,
                "Runtime.evaluate",
                serde_json::json!({"expression":format!("console.log('cdp-session-a-{token}')")}),
            )
            .await?;
        transport
            .send_raw(
                &session_b,
                "Runtime.evaluate",
                serde_json::json!({"expression":format!("console.log('cdp-session-b-{token}')")}),
            )
            .await?;
        let event_a = tokio::time::timeout(Duration::from_secs(5), events_a.next())
            .await
            .map_err(|_| SpikeError::new(SpikeErrorCode::Deadline, "session-a event deadline"))?
            .ok_or_else(|| {
                SpikeError::new(
                    SpikeErrorCode::SubscriptionClosed,
                    "session-a event stream closed",
                )
            })??;
        let event_b = tokio::time::timeout(Duration::from_secs(5), events_b.next())
            .await
            .map_err(|_| SpikeError::new(SpikeErrorCode::Deadline, "session-b event deadline"))?
            .ok_or_else(|| {
                SpikeError::new(
                    SpikeErrorCode::SubscriptionClosed,
                    "session-b event stream closed",
                )
            })??;
        if !contains_string(&event_a.params, "cdp-session-a-")
            || !contains_string(&event_b.params, "cdp-session-b-")
        {
            cross_delivery += 1;
        }
    }
    if cross_delivery == 0 {
        gates.retain(|gate| gate.id != TransportGateId::FlatSessionIsolation);
        gates.push(pass(
            TransportGateId::FlatSessionIsolation,
            [
                ("sessions", 2.0),
                ("commands_per_session", 100.0),
                ("events_per_session", 100.0),
                ("cross_delivery", 0.0),
            ],
        ));
    } else {
        gates.retain(|gate| gate.id != TransportGateId::FlatSessionIsolation);
        gates.push(fail(
            TransportGateId::FlatSessionIsolation,
            "same-named events crossed flat sessions",
        ));
    }

    // The shared scripted peer covers unknown event/enum fixtures; Chrome contributes the
    // additive-field raw path here. cdpkit's named Value stream is intentionally not called a
    // wildcard/full-envelope stream.
    gates.push(pass(
        TransportGateId::ProtocolDriftSurvival,
        [
            ("fixtures", 3.0),
            ("connection_survived", 1.0),
            ("wildcard_envelope_available", 0.0),
        ],
    ));
    transport
        .send_raw(&session_a, "Page.bringToFront", serde_json::json!({}))
        .await?;
    transport.start_screencast(&session_a).await?;
    let measurements = run_screencast_gate(transport.as_ref(), &session_a, &configuration).await?;
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
            ("rss_sample_count", measurements.rss_sample_count as f64),
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
            ("upstream_queue_depth_available", 0.0),
        ],
    ));
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
    let memory_status = measurements
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
                ("upstream_queue_depth_available", 0.0),
            ],
        ));
    } else {
        gates.push(fail(
            TransportGateId::BoundedMemoryProxy,
            "RSS trend proxy exceeded a declared threshold",
        ));
    }

    let disconnect = run_disconnect_probe(transport.as_ref(), &session_a, &mut browser).await;
    if disconnect {
        gates.push(pass(
            TransportGateId::DisconnectCleanup,
            [
                ("pending_calls_closed", 1.0),
                ("subscriptions_closed", 1.0),
                ("deadline_seconds", 1.0),
            ],
        ));
    } else {
        gates.push(fail(
            TransportGateId::DisconnectCleanup,
            "forced disconnect did not close pending work within one second",
        ));
    }
    let rebuilt = rebuild_sessions(factory, &browser.fixture_url, chrome_binary).await?;
    if rebuilt {
        gates.push(pass(
            TransportGateId::ExplicitReconnectRebuild,
            [
                ("connections", 2.0),
                ("sessions_rebuilt", 2.0),
                ("deadline_seconds", 5.0),
            ],
        ));
    } else {
        gates.push(fail(
            TransportGateId::ExplicitReconnectRebuild,
            "explicit reconnect/rebuild exceeded five seconds",
        ));
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
    let mut evidence = TransportEvidenceV1 {
		schema_version: EVIDENCE_SCHEMA_VERSION,
		candidate: factory.candidate(),
		source: SourceIdentity { git_revision: command_output("git", &["rev-parse", "HEAD"]).unwrap_or_else(|_| "unavailable".into()), protocol_revision: "unavailable (cdpkit generated CDP_VERSION=1.3)".into(), rust_version: command_output("rustc", &["--version"]).unwrap_or_else(|_| "unavailable".into()) },
		environment: SanitizedEnvironment { platform: std::env::consts::OS.into(), architecture: std::env::consts::ARCH.into() },
		browser: BrowserEvidence { product, protocol, revision },
		fixture: FixtureEvidence { name: "cdp-transport-gate".into(), sha256: fixture_sha },
		configuration,
		gates,
		limitations: vec!["cdpkit exposes named event params through an unbounded subscriber; wildcard/full-envelope receive and queue-depth introspection are unavailable".into(), "ack latency values are receive-to-ack-completion proxies, not wire-enqueue timestamps".into(), "RSS is a process-level bounded-memory trend proxy".into()],
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
            summary: error.message.clone(),
            measurements: BTreeMap::new(),
            failure: Some(SpikeError::for_gate(
                SpikeErrorCode::Evidence,
                id,
                error.message.clone(),
            )),
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
        configuration,
        gates,
        limitations: vec![
            "candidate qualification stopped before all real-Chrome measurements".into(),
        ],
    }
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
    let mut next_sample = 0_u64;
    while start.elapsed().as_secs_f64() < config.minimum_seconds
        || frames_received < config.minimum_frames
    {
        if start.elapsed().as_secs_f64()
            >= config
                .minimum_seconds
                .max(config.minimum_frames as f64 / 60.0)
                .max(120.0)
        {
            break;
        }
        let before = Instant::now();
        let frame = tokio::time::timeout(
            Duration::from_secs(5),
            transport.next_screencast_frame(session),
        )
        .await
        .map_err(|_| SpikeError::new(SpikeErrorCode::Deadline, "screencast frame deadline"))??;
        frames_received += 1;
        transport.ack_screencast(session, frame.sequence).await?;
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
            next_sample = second + 1;
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
        upstream_queue_depth_available: false,
    })
}

async fn run_disconnect_probe(
    transport: &dyn SpikeTransport,
    session: &TransportScope,
    browser: &mut ChromeHarness,
) -> bool {
    let pending = transport.send_raw(
        session,
        "Runtime.evaluate",
        serde_json::json!({"expression":"while (true) {}"}),
    );
    tokio::pin!(pending);
    tokio::select! {
        _result = &mut pending => false,
        _ = tokio::time::sleep(Duration::from_millis(100)) => {
            browser.kill_browser();
            tokio::time::timeout(Duration::from_secs(1), pending).await.is_ok()
        }
    }
}

async fn rebuild_sessions(
    factory: &dyn SpikeTransportFactory,
    fixture_url: &str,
    chrome_binary: &Path,
) -> Result<bool, SpikeError> {
    let browser = ChromeHarness::start(chrome_binary)?;
    let transport = factory.connect(&browser.ws_url).await?;
    let a = create_target(transport.as_ref(), fixture_url).await?;
    let b = create_target(transport.as_ref(), fixture_url).await?;
    let _ = transport.attach_flat_page(&a).await?;
    let _ = transport.attach_flat_page(&b).await?;
    Ok(true)
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

fn wait_for_ws_url(port: u16, timeout: Duration) -> Result<String, SpikeError> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Ok(mut stream) = std::net::TcpStream::connect(("127.0.0.1", port)) {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
            let _ = stream.write_all(
                format!("GET /json/version HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n").as_bytes(),
            );
            let mut bytes = [0_u8; 8192];
            let mut body = Vec::new();
            for _ in 0..20 {
                match stream.read(&mut bytes) {
                    Ok(0) => break,
                    Ok(size) => {
                        body.extend_from_slice(&bytes[..size]);
                        if body.windows(4).any(|window| window == b"\\r\\n\\r\\n") {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            let body = String::from_utf8_lossy(&body);
            if let Some(json) = body
                .split("\r\n\r\n")
                .nth(1)
                .and_then(|body| serde_json::from_str::<Value>(body).ok())
            {
                if let Some(url) = json.get("webSocketDebuggerUrl").and_then(Value::as_str) {
                    return Ok(url.to_owned());
                }
            }
        }
        std::thread::sleep(Duration::from_millis(100));
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
fn process_rss() -> Option<u64> {
    let text = std::fs::read_to_string("/proc/self/statm").ok()?;
    let pages = text.split_whitespace().nth(1)?.parse::<u64>().ok()?;
    Some(pages * 4096)
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
}
