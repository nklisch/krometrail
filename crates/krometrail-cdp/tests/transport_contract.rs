#![cfg(feature = "cdp-spike")]

use std::collections::BTreeMap;

use futures_util::{SinkExt, StreamExt};

use krometrail_cdp::spike::{
    CandidateContractEvidence, CandidateContractResults, CandidateIdentity,
    CandidateRuntimeAssertions, CandidateWireResults, FixtureEvidence, GateConfiguration,
    GateProvenance, GateResult, GateStatus, SanitizedEnvironment, ScriptedCdpPeer, SourceIdentity,
    TransportEvidenceV1, TransportEvidenceV2, TransportGateId, committed_protocol_fixtures,
    configuration_digest, decide_from_files, ordered_protocol_fixture_digest, validate_evidence,
    write_json_schema,
};
use krometrail_cdp::spike::{
    FakeTransport, FakeTransportFactory, SpikeTransport, TransportScope, run_transport_scenarios,
};

fn valid_measurement(key: &&str) -> f64 {
    match *key {
        "rss_samples" => 50.0,
        "rss_warmup_seconds" => 10.0,
        "rss_sampling_interval_seconds" => 1.0,
        "pending_command_elapsed_seconds" => 0.25,
        "subscription_elapsed_seconds" => 0.5,
        "elapsed_seconds" => 2.0,
        _ => 1.0,
    }
}

fn evidence(gates: Vec<GateResult>) -> TransportEvidenceV2 {
    TransportEvidenceV2 {
        schema_version: 2,
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
            hard_stop_seconds: 120,
        },
        gate_provenance: GateProvenance {
            implementation_revision: "0123456789abcdef".into(),
            configuration_sha256: configuration_digest(&GateConfiguration {
                minimum_seconds: 60.0,
                minimum_frames: 1000,
                saturation_seconds: 10.0,
                saturation_attempts: 100,
                hard_stop_seconds: 120,
            }),
        },
        gates,
        limitations: vec!["named event parameters are not wildcard envelopes".into()],
        candidate_contract: None,
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
    let fixtures = committed_protocol_fixtures().unwrap();
    assert_eq!(
        fixtures
            .iter()
            .map(|fixture| fixture.method.as_str())
            .collect::<Vec<_>>(),
        vec![
            "Protocol.unknownEvent",
            "Runtime.additiveField",
            "Runtime.unknownEnum",
        ]
    );
    assert_eq!(fixtures[0].params, serde_json::json!({"kind":"unknown"}));
    assert_eq!(
        fixtures[1].params,
        serde_json::json!({"known":true,"new_field":7})
    );
    assert_eq!(
        fixtures[2].params,
        serde_json::json!({"value":"future-value"})
    );
    assert!(ordered_protocol_fixture_digest().starts_with("sha256:"));
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
fn candidate_wire_contract_is_separate_and_trace_bound() {
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
                .collect(),
            failure: None,
        })
        .collect();
    let mut value = evidence(gates);
    value.candidate_contract = Some(CandidateContractEvidence {
        fixture_sha256: krometrail_cdp::spike::ordered_protocol_fixture_digest(),
        trace_sha256: format!("sha256:{}", "a".repeat(64)),
        trace_observations: 10,
        results: CandidateContractResults {
            wire: CandidateWireResults {
                drift_fixtures: 3,
                drift_methods: vec![
                    "Protocol.unknownEvent".into(),
                    "Runtime.additiveField".into(),
                    "Runtime.unknownEnum".into(),
                ],
                connection_survived: true,
                routing_commands: 200,
                routing_events: 200,
                routing_cross_delivery: 0,
                event_before_response: true,
                detach_during_pending: true,
                socket_closed: true,
                reconnect_connections: 2,
                sessions_rebuilt: 2,
            },
            runtime: CandidateRuntimeAssertions {
                pending_calls_closed: true,
                subscriptions_closed: true,
            },
        },
    });
    validate_evidence(&value).unwrap();
    value.candidate_contract.as_mut().unwrap().trace_sha256 = "not-a-digest".into();
    assert!(validate_evidence(&value).is_err());
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
fn evidence_rejects_nominal_missing_and_over_threshold_deadline_values() {
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

    let mut nominal = evidence(gates.clone());
    let disconnect = nominal
        .gates
        .iter_mut()
        .find(|gate| gate.id == TransportGateId::DisconnectCleanup)
        .unwrap();
    disconnect
        .measurements
        .insert("deadline_seconds".into(), 1.0);
    assert!(validate_evidence(&nominal).is_err());

    let mut missing = evidence(gates.clone());
    missing
        .gates
        .iter_mut()
        .find(|gate| gate.id == TransportGateId::ExplicitReconnectRebuild)
        .unwrap()
        .measurements
        .remove("elapsed_seconds");
    assert!(validate_evidence(&missing).is_err());

    let mut over_threshold = evidence(gates);
    over_threshold
        .gates
        .iter_mut()
        .find(|gate| gate.id == TransportGateId::ExplicitReconnectRebuild)
        .unwrap()
        .measurements
        .insert("elapsed_seconds".into(), 5.1);
    assert!(validate_evidence(&over_threshold).is_err());
}

#[test]
fn evidence_rejects_legacy_rss_alias_and_omitted_cadence_or_warmup() {
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
    let mut aliased = evidence(gates.clone());
    for id in [
        TransportGateId::SustainedScreencast,
        TransportGateId::BoundedMemoryProxy,
    ] {
        let gate = aliased.gates.iter_mut().find(|gate| gate.id == id).unwrap();
        gate.measurements.remove("rss_samples");
        gate.measurements.insert("rss_sample_count".into(), 50.0);
    }
    assert!(
        validate_evidence(&aliased).is_err(),
        "legacy RSS alias must be rejected"
    );

    let mut missing_cadence = evidence(gates.clone());
    missing_cadence.gates.iter_mut().for_each(|gate| {
        if matches!(
            gate.id,
            TransportGateId::SustainedScreencast | TransportGateId::BoundedMemoryProxy
        ) {
            gate.measurements.remove("rss_sampling_interval_seconds");
        }
    });
    assert!(validate_evidence(&missing_cadence).is_err());

    let mut missing_warmup = evidence(gates);
    missing_warmup.gates.iter_mut().for_each(|gate| {
        if matches!(
            gate.id,
            TransportGateId::SustainedScreencast | TransportGateId::BoundedMemoryProxy
        ) {
            gate.measurements.remove("rss_warmup_seconds");
        }
    });
    assert!(validate_evidence(&missing_warmup).is_err());
}

#[test]
fn evidence_rejects_stale_schema_and_configuration_provenance() {
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
    let mut stale = evidence(gates);
    stale.schema_version = 1;
    assert!(validate_evidence(&stale).is_err());
    let mut mixed_config = evidence(
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
    );
    mixed_config.gate_provenance.configuration_sha256 = format!("sha256:{}", "b".repeat(64));
    assert!(validate_evidence(&mixed_config).is_err());
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
fn historical_reports_are_rejected_until_requalified_with_observed_deadlines() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/evidence/cdp-transport/v1");
    let result = decide_from_files(
        &root.join("cdpkit-linux.json"),
        &root.join("cdpkit-macos.json"),
    );
    assert!(
        result.is_err(),
        "nominal-only historical reports must be obsolete"
    );
}

#[test]
fn checked_schema_is_generated_by_the_rust_evidence_types() {
    let temporary = std::env::temp_dir().join("krometrail-cdp-transport-schema.json");
    write_json_schema(&temporary).unwrap();
    let generated = std::fs::read_to_string(&temporary).unwrap();
    assert!(generated.contains("TransportEvidenceV2"));
    let committed = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/evidence/cdp-transport/v2/schema.json");
    if std::env::var_os("CDP_SPIKE_WRITE_SCHEMA").is_some() {
        write_json_schema(&committed).unwrap();
    }
    if committed.exists() {
        assert_eq!(generated, std::fs::read_to_string(committed).unwrap());
    }
    let _ = std::fs::remove_file(temporary);
}
