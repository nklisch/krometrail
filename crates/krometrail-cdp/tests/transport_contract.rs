#![cfg(feature = "cdp-spike")]

use std::collections::BTreeMap;

use futures_util::{SinkExt, StreamExt};

use krometrail_cdp::spike::{
    CandidateIdentity, FixtureEvidence, GateConfiguration, GateResult, GateStatus,
    SanitizedEnvironment, ScriptedCdpPeer, SourceIdentity, TransportEvidenceV1, TransportGateId,
    validate_evidence, write_json_schema,
};
use krometrail_cdp::spike::{
    FakeTransport, FakeTransportFactory, SpikeTransport, TransportScope, run_transport_scenarios,
};

fn valid_measurement(key: &&str) -> f64 {
    match *key {
        "rss_samples" => 50.0,
        "rss_warmup_seconds" => 10.0,
        "rss_sampling_interval_seconds" => 1.0,
        _ => 1.0,
    }
}

fn evidence(gates: Vec<GateResult>) -> TransportEvidenceV1 {
    TransportEvidenceV1 {
        schema_version: 1,
        candidate: CandidateIdentity {
            name: "cdpkit".into(),
            version: "0.4.0".into(),
            checksum: "sha256:published-crate".into(),
        },
        source: SourceIdentity {
            git_revision: "0123456789abcdef".into(),
            protocol_revision: "0123456789abcdef".into(),
            rust_version: "1.85".into(),
        },
        environment: SanitizedEnvironment {
            platform: "linux".into(),
            architecture: "x86_64".into(),
        },
        browser: krometrail_cdp::spike::BrowserEvidence {
            product: "Chrome/1".into(),
            protocol: "1.3".into(),
            revision: "123".into(),
        },
        fixture: FixtureEvidence {
            name: "protocol-fixtures".into(),
            sha256: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into(),
        },
        configuration: GateConfiguration {
            minimum_seconds: 60.0,
            minimum_frames: 1000,
            saturation_seconds: 10.0,
            saturation_attempts: 100,
        },
        gates,
        limitations: vec!["named event parameters are not wildcard envelopes".into()],
    }
}

#[tokio::test]
async fn fake_transport_runs_the_single_deterministic_scenario_registry() {
    let factory = FakeTransportFactory;
    let first = run_transport_scenarios(&factory, &mut ScriptedCdpPeer::empty()).await;
    let second = run_transport_scenarios(&factory, &mut ScriptedCdpPeer::empty()).await;
    assert!(first.passed(), "first run: {first:?}");
    assert_eq!(first, second);
    assert_eq!(first.gates.len(), TransportGateId::ALL.len());
}

#[tokio::test]
async fn scripted_peer_exposes_a_local_websocket_without_a_machine_port() {
    let (mut client, mut server) = ScriptedCdpPeer::websocket_pair(4096).await;
    client
        .send(tokio_tungstenite::tungstenite::Message::Text(
            "scripted".into(),
        ))
        .await
        .unwrap();
    assert_eq!(
        server.next().await.unwrap().unwrap().into_text().unwrap(),
        "scripted"
    );
}

#[tokio::test]
async fn fake_disconnect_and_rebuild_are_explicit_and_deterministic() {
    let fake = FakeTransport::new();
    let session = fake.attach_flat_page("target-a").await.unwrap();
    let _subscription = fake
        .subscribe_named(&session, "Runtime.consoleAPICalled")
        .await
        .unwrap();
    fake.disconnect("scripted disconnect");
    let reason = fake.close_reason().unwrap();
    assert!(reason.pending_calls_closed);
    assert!(reason.subscriptions_closed);
    assert!(
        fake.send_raw(&session, "Runtime.evaluate", serde_json::json!({}))
            .await
            .is_err()
    );
    assert!(
        fake.subscribe_named(&session, "Runtime.consoleAPICalled")
            .await
            .is_err()
    );
    fake.rebuild();
    assert_eq!(
        fake.attach_flat_page("target-a").await.unwrap(),
        TransportScope::session("session-a")
    );
    assert_eq!(
        fake.attach_flat_page("target-b").await.unwrap(),
        TransportScope::session("session-b")
    );
}

#[test]
fn protocol_fixtures_are_named_inputs_to_the_drift_scenarios() {
    for fixture in [
        include_str!("fixtures/protocol/unknown-event.json"),
        include_str!("fixtures/protocol/additive-field.json"),
        include_str!("fixtures/protocol/unknown-enum.json"),
    ] {
        let value: serde_json::Value = serde_json::from_str(fixture).unwrap();
        assert!(value["method"].as_str().is_some());
        assert!(value["params"].is_object());
    }
}

#[test]
fn evidence_round_trips_and_requires_every_registered_gate() {
    let gates = TransportGateId::ALL
        .into_iter()
        .map(|id| GateResult {
            id,
            status: GateStatus::Pass,
            summary: "fixture passed".into(),
            measurements: id
                .measurement_keys()
                .iter()
                .map(|key| ((*key).into(), valid_measurement(key)))
                .collect::<BTreeMap<_, _>>(),
            failure: None,
        })
        .collect();
    let value = evidence(gates);
    validate_evidence(&value).unwrap();
    let encoded = serde_json::to_string_pretty(&value).unwrap();
    assert_eq!(
        serde_json::from_str::<TransportEvidenceV1>(&encoded).unwrap(),
        value
    );
}

#[test]
fn evidence_rejects_duplicate_or_missing_gates_non_finite_values_and_leaks() {
    let mut gates = TransportGateId::ALL
        .into_iter()
        .map(|id| GateResult {
            id,
            status: GateStatus::Pass,
            summary: "fixture passed".into(),
            measurements: id
                .measurement_keys()
                .iter()
                .map(|key| ((*key).into(), valid_measurement(key)))
                .collect(),
            failure: None,
        })
        .collect::<Vec<_>>();
    let valid = evidence(gates.clone());
    validate_evidence(&valid).unwrap();
    gates.pop();
    assert!(validate_evidence(&evidence(gates)).is_err());
    let mut duplicate = valid.clone();
    duplicate.gates[1].id = duplicate.gates[0].id;
    assert!(validate_evidence(&duplicate).is_err());
    let mut non_finite = valid.clone();
    non_finite.gates[0]
        .measurements
        .insert("bad".into(), f64::NAN);
    assert!(validate_evidence(&non_finite).is_err());
    let mut leaked = valid;
    leaked.source.git_revision = "/home/operator/project".into();
    assert!(validate_evidence(&leaked).is_err());
    let mut unknown = serde_json::to_value(evidence(
        TransportGateId::ALL
            .into_iter()
            .map(|id| GateResult {
                id,
                status: GateStatus::Pass,
                summary: "fixture passed".into(),
                measurements: id
                    .measurement_keys()
                    .iter()
                    .map(|key| ((*key).into(), valid_measurement(key)))
                    .collect(),
                failure: None,
            })
            .collect(),
    ))
    .unwrap();
    unknown["unexpected"] = serde_json::json!(true);
    assert!(serde_json::from_value::<TransportEvidenceV1>(unknown).is_err());
}

#[test]
fn evidence_rejects_zero_rss_samples_and_window_values() {
    let gates = TransportGateId::ALL
        .into_iter()
        .map(|id| GateResult {
            id,
            status: GateStatus::Pass,
            summary: "fixture passed".into(),
            measurements: id
                .measurement_keys()
                .iter()
                .map(|key| ((*key).into(), valid_measurement(key)))
                .collect::<BTreeMap<_, _>>(),
            failure: None,
        })
        .collect::<Vec<_>>();
    let mut value = evidence(gates);
    let memory = value
        .gates
        .iter_mut()
        .find(|gate| gate.id == TransportGateId::BoundedMemoryProxy)
        .unwrap();
    memory.measurements.insert("rss_samples".into(), 0.0);
    memory
        .measurements
        .insert("rss_first_window_median_bytes".into(), 0.0);
    memory
        .measurements
        .insert("rss_last_window_median_bytes".into(), 0.0);
    memory.measurements.insert("rss_peak_bytes".into(), 0.0);

    assert!(validate_evidence(&value).is_err());
}

#[test]
fn checked_schema_is_generated_by_the_rust_evidence_types() {
    let temporary = std::env::temp_dir().join("krometrail-cdp-transport-schema.json");
    write_json_schema(&temporary).unwrap();
    let generated = std::fs::read_to_string(&temporary).unwrap();
    assert!(generated.contains("TransportEvidenceV1"));
    let committed = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/evidence/cdp-transport/v1/schema.json");
    if std::env::var_os("CDP_SPIKE_WRITE_SCHEMA").is_some() {
        write_json_schema(&committed).unwrap();
    }
    if committed.exists() {
        assert_eq!(generated, std::fs::read_to_string(committed).unwrap());
    }
    let _ = std::fs::remove_file(temporary);
}
