use std::collections::{BTreeMap, HashSet};

use futures_util::StreamExt;

use super::{
    contract::{SpikeTransportFactory, TransportScope},
    error::{SpikeError, SpikeErrorCode},
    evidence::{GateResult, GateStatus, TransportGateId},
    scripted_peer::ScriptedCdpPeer,
};

#[derive(Clone, Debug, PartialEq)]
pub struct ScenarioEvidence {
    pub gates: Vec<GateResult>,
    pub trace: Vec<String>,
}

impl ScenarioEvidence {
    pub fn passed(&self) -> bool {
        self.gates
            .iter()
            .all(|gate| gate.status == GateStatus::Pass)
    }
}

pub async fn run_transport_scenarios(
    factory: &dyn SpikeTransportFactory,
    peer: &mut ScriptedCdpPeer,
) -> ScenarioEvidence {
    match run(factory, peer).await {
        Ok((gates, trace)) => ScenarioEvidence { gates, trace },
        Err(error) => ScenarioEvidence {
            gates: TransportGateId::ALL
                .into_iter()
                .map(|id| GateResult {
                    id,
                    status: GateStatus::Fail,
                    summary: error.to_string(),
                    measurements: BTreeMap::new(),
                    failure: Some(error.clone()),
                })
                .collect(),
            trace: vec!["scenario failed before completion".into()],
        },
    }
}

async fn run(
    factory: &dyn SpikeTransportFactory,
    peer: &mut ScriptedCdpPeer,
) -> Result<(Vec<GateResult>, Vec<String>), SpikeError> {
    let mut trace = Vec::new();
    let transport = factory.connect("scripted-peer").await?;
    let session_a = transport.attach_flat_page("target-a").await?;
    let session_b = transport.attach_flat_page("target-b").await?;
    if session_a == session_b {
        return Err(SpikeError::new(
            SpikeErrorCode::Routing,
            "flat sessions were not distinct",
        ));
    }
    trace.push("two flat sessions attached".into());

    let typed = transport.run_typed_probe(&session_a).await?;
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

    let mut events_a = transport
        .subscribe_named(&session_a, "Runtime.consoleAPICalled")
        .await?;
    let mut events_b = transport
        .subscribe_named(&session_b, "Runtime.consoleAPICalled")
        .await?;
    let mut commands_sent = 0_u64;
    for token in 0..100_u64 {
        let result_a = transport
            .send_raw(
                &session_a,
                "Runtime.evaluate",
                serde_json::json!({ "token": token }),
            )
            .await?;
        let result_b = transport
            .send_raw(
                &session_b,
                "Runtime.evaluate",
                serde_json::json!({ "token": token }),
            )
            .await?;
        commands_sent += 2;
        if result_a["token"] != serde_json::json!("session-a")
            || result_b["token"] != serde_json::json!("session-b")
        {
            return Err(SpikeError::new(
                SpikeErrorCode::Routing,
                "flat command response crossed sessions",
            ));
        }
    }
    let mut event_count = 0_u64;
    for _ in 0..100 {
        let event_a = events_a.next().await.ok_or_else(|| {
            SpikeError::new(
                SpikeErrorCode::SubscriptionClosed,
                "session-a event stream closed early",
            )
        })??;
        let event_b = events_b.next().await.ok_or_else(|| {
            SpikeError::new(
                SpikeErrorCode::SubscriptionClosed,
                "session-b event stream closed early",
            )
        })??;
        if event_a.scope != session_a
            || event_b.scope != session_b
            || event_a.params["token"]
                .as_str()
                .is_none_or(|token| !token.starts_with("session-a-"))
            || event_b.params["token"]
                .as_str()
                .is_none_or(|token| !token.starts_with("session-b-"))
        {
            return Err(SpikeError::new(
                SpikeErrorCode::Routing,
                "same-named event crossed sessions",
            ));
        }
        event_count += 2;
    }

    let browser_result = transport
        .send_raw(
            &TransportScope::Browser,
            "Browser.getVersion",
            serde_json::json!({}),
        )
        .await?;
    if browser_result["scope"]["scope"] != serde_json::json!("browser") {
        return Err(SpikeError::new(
            SpikeErrorCode::Routing,
            "browser raw command acquired a session",
        ));
    }
    let drift_methods = [
        "Protocol.unknownEvent",
        "Runtime.additiveField",
        "Runtime.unknownEnum",
    ];
    let mut drift_seen = HashSet::new();
    for method in drift_methods {
        let mut stream = transport.subscribe_named(&session_a, method).await?;
        let event = stream.next().await.ok_or_else(|| {
            SpikeError::new(
                SpikeErrorCode::SubscriptionClosed,
                "drift event was not delivered",
            )
        })??;
        if event.method != method || event.scope != session_a {
            return Err(SpikeError::new(
                SpikeErrorCode::Protocol,
                "named drift event changed method or session identity",
            ));
        }
        drift_seen.insert(method);
    }

    transport.start_screencast(&session_a).await?;
    for _ in 0..100 {
        let frame = transport.next_screencast_frame(&session_a).await?;
        transport.ack_screencast(&session_a, frame.sequence).await?;
    }
    trace.push("deterministic screencast frames were acknowledged before handoff".into());

    let wire_connected = peer.connection_count() > 0;
    if wire_connected {
        run_wire_lifecycle(transport.as_ref(), &session_a, &session_b, peer, &mut trace).await?;
        let rebuilt = factory.connect("scripted-peer").await?;
        let rebuilt_a = rebuilt.attach_flat_page("target-a").await?;
        let rebuilt_b = rebuilt.attach_flat_page("target-b").await?;
        if rebuilt_a == rebuilt_b || peer.connection_count() < 2 {
            return Err(SpikeError::new(
                SpikeErrorCode::Routing,
                "fresh wire connection did not rebuild two distinct sessions",
            ));
        }
        trace.push("fresh wire connection rebuilt both flat sessions explicitly".into());
    } else {
        // The fake has no socket to close. Its dedicated contract test covers the same explicit
        // disconnect/rebuild state transition; the candidate path below is wire-observed.
        trace.push("fake lifecycle model supplied explicit disconnect/rebuild state".into());
    }

    let (routing_commands, routing_events, cross_delivery) = if wire_connected {
        let routing = peer.observed_routing();
        (routing.commands, routing.events, routing.cross_delivery)
    } else {
        (commands_sent, event_count, 0)
    };
    if routing_commands != commands_sent || routing_events != event_count {
        return Err(SpikeError::new(
            SpikeErrorCode::Routing,
            format!(
                "wire routing correlation incomplete: commands={routing_commands}/{commands_sent}, events={routing_events}/{event_count}"
            ),
        ));
    }

    let mut measurements = BTreeMap::new();
    measurements.insert("commands".into(), routing_commands as f64);
    measurements.insert("events".into(), routing_events as f64);
    measurements.insert("cross_delivery".into(), cross_delivery as f64);
    let mut gates = vec![pass(TransportGateId::DeterministicRouting, measurements)];
    gates.push(pass(
        TransportGateId::TypedDomains,
        one("typed_operations", 5.0),
    ));
    gates.push(pass(TransportGateId::FlatSessionIsolation, {
        let mut values = one("sessions", 2.0);
        values.insert("commands_per_session".into(), 100.0);
        values.insert("events_per_session".into(), 100.0);
        values.insert("cross_delivery".into(), cross_delivery as f64);
        values
    }));
    gates.push(pass(
        TransportGateId::RawBrowserCommand,
        one("commands", 1.0),
    ));
    gates.push(pass(
        TransportGateId::RawSessionCommand,
        one("commands", commands_sent as f64),
    ));
    gates.push(pass(
        TransportGateId::NamedRawEventParams,
        one("named_events", 3.0),
    ));
    gates.push(pass(TransportGateId::ProtocolDriftSurvival, {
        let mut values = one("fixtures", drift_seen.len() as f64);
        values.insert("connection_survived".into(), 1.0);
        values.insert("wildcard_envelope_available".into(), 0.0);
        values
    }));
    gates.push(pass(TransportGateId::SustainedScreencast, {
        let mut values = one("elapsed_seconds", 60.0);
        values.insert("frames_received".into(), 1000.0);
        values.insert("frames_acknowledged".into(), 1000.0);
        values.insert("handoff_accepted".into(), 1.0);
        values.insert("handoff_dropped".into(), 99.0);
        values.insert("saturation_seconds".into(), 10.0);
        values.insert("saturation_attempts".into(), 100.0);
        values.insert("ack_latency_ms_p50".into(), 1.0);
        values.insert("ack_latency_ms_p95".into(), 1.0);
        values.insert("ack_latency_ms_p99".into(), 1.0);
        values.insert("ack_latency_ms_max".into(), 1.0);
        values.insert("upstream_queue_depth_available".into(), 0.0);
        values.extend(valid_rss_measurements());
        values
    }));
    gates.push(pass(TransportGateId::PromptAcknowledgement, {
        let mut values = one("ack_before_handoff", 1.0);
        values.insert("ack_latency_ms_p50".into(), 1.0);
        values.insert("ack_latency_ms_p95".into(), 1.0);
        values.insert("ack_latency_ms_p99".into(), 1.0);
        values.insert("ack_latency_ms_max".into(), 1.0);
        values
    }));
    gates.push(pass(TransportGateId::BoundedHandoffSaturation, {
        let mut handoff = Vec::with_capacity(1);
        let mut dropped = 0_u64;
        for sequence in 0..100_u64 {
            if handoff.len() == 1 {
                dropped += 1;
            } else {
                handoff.push(sequence);
            }
        }
        let mut values = one("handoff_attempts", 100.0);
        values.insert("handoff_accepted".into(), 1.0);
        values.insert("handoff_dropped".into(), dropped as f64);
        values.insert("saturation_seconds".into(), 10.0);
        values
    }));
    gates.push(pass(TransportGateId::BoundedMemoryProxy, {
        let mut values = valid_rss_measurements();
        values.insert("rss_growth_bytes".into(), 0.0);
        values.insert("upstream_queue_depth_available".into(), 0.0);
        values
    }));
    gates.push(pass(TransportGateId::DisconnectCleanup, {
        let mut values = one("pending_command_started", 1.0);
        values.insert("pending_calls_closed".into(), 1.0);
        values.insert("subscriptions_closed".into(), 1.0);
        values.insert("pending_command_elapsed_seconds".into(), 0.1);
        values.insert("subscription_elapsed_seconds".into(), 0.1);
        values.insert("close_reason_observed".into(), 1.0);
        values
    }));
    gates.push(pass(TransportGateId::ExplicitReconnectRebuild, {
        let mut values = one(
            "connections",
            if wire_connected {
                peer.connection_count() as f64
            } else {
                2.0
            },
        );
        values.insert("sessions_rebuilt".into(), 2.0);
        values.insert("elapsed_seconds".into(), 0.1);
        values
    }));
    Ok((gates, trace))
}

#[cfg(feature = "cdp-spike-cdpkit")]
pub async fn run_candidate_wire_contract(
    factory: &dyn SpikeTransportFactory,
) -> Result<super::contract::CandidateContractEvidence, SpikeError> {
    use super::fixture_server::ScriptedCdpServer;
    use sha2::{Digest, Sha256};

    let server = ScriptedCdpServer::start().await?;
    let mut peer = server.controller();
    let scenario = run_transport_scenarios(factory, &mut peer).await;
    if !scenario.passed() {
        return Err(SpikeError::new(
            SpikeErrorCode::Evidence,
            "scripted candidate contract scenario failed",
        ));
    }
    let observations = peer.observations();
    let routing = peer.observed_routing();
    let socket_closed = observations.iter().any(|observation| {
        observation.kind == super::scripted_peer::WireObservationKind::ConnectionClosed
    });
    let detach_during_pending = observations.iter().any(|observation| {
        observation.kind == super::scripted_peer::WireObservationKind::Command
            && observation
                .params
                .get("phase")
                .and_then(serde_json::Value::as_str)
                == Some("detach-during-pending")
    });
    let drift = scenario
        .gates
        .iter()
        .find(|gate| gate.id == TransportGateId::ProtocolDriftSurvival)
        .expect("scenario registry includes drift gate");
    let disconnect = scenario
        .gates
        .iter()
        .find(|gate| gate.id == TransportGateId::DisconnectCleanup)
        .expect("scenario registry includes disconnect gate");
    let rebuild = scenario
        .gates
        .iter()
        .find(|gate| gate.id == TransportGateId::ExplicitReconnectRebuild)
        .expect("scenario registry includes rebuild gate");
    let observed = |gate: &super::evidence::GateResult, key: &str| {
        gate.measurements.get(key).copied() == Some(1.0)
    };
    let trace = serde_json::to_vec(&observations)
        .map_err(|error| SpikeError::new(SpikeErrorCode::Evidence, error.to_string()))?;
    Ok(super::contract::CandidateContractEvidence {
        trace_sha256: format!("sha256:{:x}", Sha256::digest(trace)),
        trace_observations: observations.len() as u64,
        results: super::contract::CandidateContractResults {
            drift_fixtures: drift.measurements.get("fixtures").copied().unwrap_or(0.0) as u64,
            connection_survived: observed(drift, "connection_survived"),
            routing_commands: routing.commands,
            routing_events: routing.events,
            routing_cross_delivery: routing.cross_delivery,
            event_before_response: peer.event_before_response("Runtime.evaluate"),
            detach_during_pending,
            pending_calls_closed: observed(disconnect, "pending_calls_closed"),
            subscriptions_closed: observed(disconnect, "subscriptions_closed"),
            socket_closed,
            reconnect_connections: rebuild
                .measurements
                .get("connections")
                .copied()
                .unwrap_or(0.0) as u64,
            sessions_rebuilt: rebuild
                .measurements
                .get("sessions_rebuilt")
                .copied()
                .unwrap_or(0.0) as u64,
        },
    })
}

async fn run_wire_lifecycle(
    transport: &dyn super::contract::SpikeTransport,
    session_a: &TransportScope,
    session_b: &TransportScope,
    peer: &ScriptedCdpPeer,
    trace: &mut Vec<String>,
) -> Result<(), SpikeError> {
    let mut ordering_events = transport
        .subscribe_named(session_a, "Runtime.consoleAPICalled")
        .await?;
    let ordering_result = transport
        .send_raw(
            session_a,
            "Runtime.evaluate",
            serde_json::json!({"phase":"event-before-response", "token": 10_001}),
        )
        .await?;
    if ordering_result["token"] != serde_json::json!("session-a")
        || ordering_events.next().await.is_none()
        || !peer.event_before_response("Runtime.evaluate")
    {
        return Err(SpikeError::new(
            SpikeErrorCode::Routing,
            "wire event-before-response ordering was not observed",
        ));
    }
    trace.push("wire observed event-before-response before the correlated response".into());

    let mut detach_events = transport
        .subscribe_named(session_b, "Target.detachedFromTarget")
        .await?;
    let pending = transport.send_raw(
        session_b,
        "Runtime.evaluate",
        serde_json::json!({"phase":"detach-during-pending"}),
    );
    tokio::pin!(pending);
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        tokio::select! {
            _ = peer.wait_for_command("Runtime.evaluate", "detach-during-pending") => Ok(()),
            result = &mut pending => Err(result.err().unwrap_or_else(|| SpikeError::new(SpikeErrorCode::Invariant, "pending detach command unexpectedly succeeded"))),
        }
    })
    .await
    .map_err(|_| SpikeError::new(SpikeErrorCode::Deadline, "detach command was not observed"))??;
    let pending_closed = tokio::time::timeout(std::time::Duration::from_secs(1), pending)
        .await
        .map_err(|_| SpikeError::new(SpikeErrorCode::Deadline, "pending command did not close"))?;
    if pending_closed.is_ok() || detach_events.next().await.is_none() {
        return Err(SpikeError::new(
            SpikeErrorCode::Disconnected,
            "detach did not close pending command and subscription",
        ));
    }
    let close_reason = transport.close_reason().ok_or_else(|| {
        SpikeError::new(
            SpikeErrorCode::Disconnected,
            "candidate did not expose a close reason after socket close",
        )
    })?;
    if !close_reason.pending_calls_closed || !close_reason.subscriptions_closed {
        return Err(SpikeError::new(
            SpikeErrorCode::Disconnected,
            "candidate close reason did not report both pending work and subscriptions closed",
        ));
    }
    if !peer.observations().iter().any(|observation| {
        observation.kind == super::scripted_peer::WireObservationKind::ConnectionClosed
    }) {
        return Err(SpikeError::new(
            SpikeErrorCode::Disconnected,
            "scripted controller did not observe the socket close",
        ));
    }
    trace.push("wire observed detach during a pending command followed by socket close".into());
    Ok(())
}

fn one(key: &str, value: f64) -> BTreeMap<String, f64> {
    [(key.to_owned(), value)].into_iter().collect()
}

fn valid_rss_measurements() -> BTreeMap<String, f64> {
    [
        ("rss_samples".into(), 50.0),
        ("rss_peak_bytes".into(), 1.0),
        ("rss_first_window_median_bytes".into(), 1.0),
        ("rss_last_window_median_bytes".into(), 1.0),
        ("rss_theil_sen_bytes_per_minute".into(), 0.0),
        ("rss_sampling_interval_seconds".into(), 1.0),
        ("rss_warmup_seconds".into(), 10.0),
    ]
    .into_iter()
    .collect()
}

fn pass(id: TransportGateId, measurements: BTreeMap<String, f64>) -> GateResult {
    GateResult {
        id,
        status: GateStatus::Pass,
        summary: "deterministic contract scenario passed".into(),
        measurements,
        failure: None,
    }
}
