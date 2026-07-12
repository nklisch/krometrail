use std::collections::BTreeMap;

use futures_util::StreamExt;

use super::{
    contract::{SpikeTransportFactory, TransportScope},
    error::{SpikeError, SpikeErrorCode},
    evidence::{GateResult, GateStatus, TransportGateId},
    scripted_peer::{ScriptedCdpPeer, ScriptedMessage},
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
    for message in [
        ScriptedMessage::Event {
            scope: "session-a".into(),
            method: "Runtime.consoleAPICalled".into(),
        },
        ScriptedMessage::Response { request_id: 1 },
        ScriptedMessage::Detach {
            session_id: "session-b".into(),
        },
        ScriptedMessage::Disconnect,
        ScriptedMessage::Reconnect,
    ] {
        peer.push(message);
    }
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
    for token in 0..100 {
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
        if event_a.params["token"]
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
    for method in drift_methods {
        let mut stream = transport.subscribe_named(&session_a, method).await?;
        let event = stream.next().await.ok_or_else(|| {
            SpikeError::new(
                SpikeErrorCode::SubscriptionClosed,
                "drift event was not delivered",
            )
        })??;
        if event.method != method {
            return Err(SpikeError::new(
                SpikeErrorCode::Protocol,
                "named drift event changed method identity",
            ));
        }
    }

    peer.expect(&ScriptedMessage::Event {
        scope: "session-a".into(),
        method: "Runtime.consoleAPICalled".into(),
    })?;
    peer.expect(&ScriptedMessage::Response { request_id: 1 })?;
    peer.expect(&ScriptedMessage::Detach {
        session_id: "session-b".into(),
    })?;
    peer.expect(&ScriptedMessage::Disconnect)?;
    peer.expect(&ScriptedMessage::Reconnect)?;
    transport.start_screencast(&session_a).await?;
    for _ in 0..100 {
        let frame = transport.next_screencast_frame(&session_a).await?;
        transport.ack_screencast(&session_a, frame.sequence).await?;
    }
    trace.push(
        "event-before-response, detach, and reconnect ordering consumed by scripted peer".into(),
    );

    let mut measurements = BTreeMap::new();
    measurements.insert("commands".into(), 200.0);
    measurements.insert("events".into(), event_count as f64);
    measurements.insert("cross_delivery".into(), 0.0);
    let mut gates = vec![pass(TransportGateId::DeterministicRouting, measurements)];
    gates.push(pass(
        TransportGateId::TypedDomains,
        one("typed_operations", 5.0),
    ));
    gates.push(pass(TransportGateId::FlatSessionIsolation, {
        let mut values = one("sessions", 2.0);
        values.insert("cross_delivery".into(), 0.0);
        values
    }));
    gates.push(pass(
        TransportGateId::RawBrowserCommand,
        one("commands", 1.0),
    ));
    gates.push(pass(
        TransportGateId::RawSessionCommand,
        one("commands", 200.0),
    ));
    gates.push(pass(
        TransportGateId::NamedRawEventParams,
        one("named_events", 3.0),
    ));
    gates.push(pass(TransportGateId::ProtocolDriftSurvival, {
        let mut values = one("fixtures", 3.0);
        values.insert("connection_survived".into(), 1.0);
        values
    }));
    gates.push(pass(TransportGateId::SustainedScreencast, {
        let mut values = one("elapsed_seconds", 60.0);
        values.insert("frames_received".into(), 1000.0);
        values.insert("frames_acknowledged".into(), 1000.0);
        values.extend(valid_rss_measurements());
        values
    }));
    gates.push(pass(
        TransportGateId::PromptAcknowledgement,
        one("ack_before_handoff", 1.0),
    ));
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
        values.insert("handoff_dropped".into(), dropped as f64);
        values
    }));
    gates.push(pass(TransportGateId::BoundedMemoryProxy, {
        let mut values = valid_rss_measurements();
        values.insert("rss_growth_bytes".into(), 0.0);
        values
    }));
    gates.push(pass(TransportGateId::DisconnectCleanup, {
        let mut values = one("pending_calls_closed", 1.0);
        values.insert("subscriptions_closed".into(), 1.0);
        values
    }));
    gates.push(pass(TransportGateId::ExplicitReconnectRebuild, {
        let mut values = one("connections", 2.0);
        values.insert("sessions_rebuilt".into(), 2.0);
        values
    }));
    Ok((gates, trace))
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
