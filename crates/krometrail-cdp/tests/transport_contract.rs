#![cfg(feature = "cdp-spike")]

use std::collections::BTreeMap;

use futures_util::{SinkExt, StreamExt};

use krometrail_cdp::spike::{
    CandidateContractEvidence, CandidateContractTrace, CandidateIdentity,
    CandidateRuntimeAssertions, CanonicalProtocolFixture, CanonicalWireObservation,
    CanonicalWireObservationKind, FixtureEvidence, GateConfiguration, GateProvenance, GateResult,
    GateStatus, SanitizedEnvironment, ScriptedCdpPeer, SourceIdentity, TransportEvidenceV2,
    TransportGateId, canonical_decisive_configuration, canonical_decisive_configuration_digest,
    committed_protocol_fixtures, configuration_digest, decide_from_files, is_git_revision,
    ordered_protocol_fixture_digest, sanitize_evidence, validate_evidence, write_json_schema,
};
use krometrail_cdp::spike::{
    FakeTransport, FakeTransportFactory, SpikeError, SpikeErrorCode, SpikeTransport,
    TransportScope, run_transport_scenarios,
};

fn valid_measurement(key: &&str) -> f64 {
    match *key {
        "rss_samples" => 50.0,
        "rss_warmup_seconds" => 10.0,
        "rss_sampling_interval_seconds" => 1.0,
        "pending_command_elapsed_seconds" => 0.25,
        "subscription_elapsed_seconds" => 0.5,
        "capture_elapsed_seconds" => 60.0,
        "frames_received" => 1_000.0,
        "frames_acknowledged" => 1_000.0,
        "saturation_attempts" => 100.0,
        "handoff_attempts" => 100.0,
        "handoff_elapsed_seconds" => 10.0,
        "elapsed_seconds" => 2.0,
        _ => 1.0,
    }
}

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
        let session = if token % 2 == 0 {
            "session-a"
        } else {
            "session-b"
        };
        push(
            1,
            CanonicalWireObservationKind::Command,
            Some(1000 + token),
            Some("Runtime.evaluate".into()),
            Some(session.into()),
            serde_json::json!({"token": token}),
        );
        push(
            1,
            CanonicalWireObservationKind::Event,
            None,
            Some("Runtime.consoleAPICalled".into()),
            Some(session.into()),
            serde_json::json!({"token": format!("{session}-{token}")}),
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

fn evidence(gates: Vec<GateResult>) -> TransportEvidenceV2 {
    TransportEvidenceV2 {
        schema_version: 2,
        candidate: CandidateIdentity {
            name: "cdpkit".into(),
            version: "0.4.0".into(),
            checksum: "sha256:published-crate".into(),
        },
        source: SourceIdentity {
            git_revision: "0123456789abcdef0123456789abcdef01234567".into(),
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
            revision: "@07b52360cc15066f987c910ab34dfbcd4a8778d2".into(),
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
            implementation_revision: "0123456789abcdef0123456789abcdef01234567".into(),
            configuration_sha256: configuration_digest(&GateConfiguration {
                minimum_seconds: 60.0,
                minimum_frames: 1000,
                saturation_seconds: 10.0,
                saturation_attempts: 100,
                hard_stop_seconds: 120,
            }),
            source_attestation: None,
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
fn revisions_require_lowercase_full_shas() {
    assert!(is_git_revision("0123456789abcdef0123456789abcdef01234567"));
    assert!(!is_git_revision("0123456789ABCDEF0123456789abcdef01234567"));
    assert!(!is_git_revision("0123456789abcdef"));
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
        serde_json::from_str::<TransportEvidenceV2>(&encoded).unwrap(),
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
    value.candidate_contract = Some(candidate_contract());
    validate_evidence(&value).unwrap();
    value.candidate_contract.as_mut().unwrap().trace_sha256 = "not-a-digest".into();
    assert!(validate_evidence(&value).is_err());
}

#[test]
fn candidate_contract_rejects_fabricated_digest_and_routing_summary() {
    let gates: Vec<_> = TransportGateId::ALL
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
    let mut zero_digest = evidence(gates.clone());
    zero_digest.candidate_contract = Some(candidate_contract());
    zero_digest
        .candidate_contract
        .as_mut()
        .unwrap()
        .trace_sha256 = format!("sha256:{}", "0".repeat(64));
    assert!(
        validate_evidence(&zero_digest).is_err(),
        "an all-zero digest must not stand in for recomputation"
    );

    let mut fabricated_count = evidence(gates);
    fabricated_count.candidate_contract = Some(candidate_contract());
    fabricated_count
        .candidate_contract
        .as_mut()
        .unwrap()
        .results
        .wire
        .routing_commands = 201;
    assert!(
        validate_evidence(&fabricated_count).is_err(),
        "a duplicated routing count must not override the original trace"
    );
}

#[test]
fn candidate_contract_rejects_fixture_and_lifecycle_mutations() {
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
        .collect::<Vec<_>>();
    let mut fixture_params = evidence(gates.clone());
    fixture_params.candidate_contract = Some(candidate_contract());
    fixture_params
        .candidate_contract
        .as_mut()
        .unwrap()
        .trace
        .fixtures[0]
        .params["kind"] = serde_json::json!("mutated");
    assert!(validate_evidence(&fixture_params).is_err());

    let mut fixture_order = evidence(gates.clone());
    fixture_order.candidate_contract = Some(candidate_contract());
    fixture_order
        .candidate_contract
        .as_mut()
        .unwrap()
        .trace
        .fixtures
        .swap(0, 1);
    assert!(validate_evidence(&fixture_order).is_err());

    let mut lifecycle = evidence(gates);
    lifecycle.candidate_contract = Some(candidate_contract());
    let trace = &mut lifecycle.candidate_contract.as_mut().unwrap().trace;
    trace
        .observations
        .iter_mut()
        .find(|observation| observation.kind == CanonicalWireObservationKind::ConnectionClosed)
        .unwrap()
        .kind = CanonicalWireObservationKind::Event;
    assert!(validate_evidence(&lifecycle).is_err());
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

    let mut pass_with_failure = evidence(
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
    pass_with_failure.gates[0].failure = Some(SpikeError::for_gate(
        SpikeErrorCode::Evidence,
        pass_with_failure.gates[0].id,
        "unexpected failure",
    ));
    assert!(validate_evidence(&pass_with_failure).is_err());

    let mut fail_without_failure = pass_with_failure.clone();
    fail_without_failure.gates[0].failure = None;
    fail_without_failure.gates[0].status = GateStatus::Fail;
    assert!(validate_evidence(&fail_without_failure).is_err());

    let mut secret_failure = evidence(
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
    secret_failure.gates[0].summary = "failure: USER = operator TOKEN=hidden".into();
    assert!(sanitize_evidence(secret_failure).is_err());

    let mut failure_url = evidence(
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
    failure_url.gates[0].status = GateStatus::Fail;
    failure_url.gates[0].failure = Some(SpikeError::for_gate(
        SpikeErrorCode::Evidence,
        failure_url.gates[0].id,
        "request failed at ws://127.0.0.1:9222",
    ));
    assert!(sanitize_evidence(failure_url).is_err());

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
    assert!(serde_json::from_value::<TransportEvidenceV2>(unknown).is_err());
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
fn evidence_rejects_retired_capture_elapsed_measurement_names() {
    let mut report = evidence(
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
    let sustained = report
        .gates
        .iter_mut()
        .find(|gate| gate.id == TransportGateId::SustainedScreencast)
        .expect("sustained gate");
    sustained.measurements.remove("capture_elapsed_seconds");
    sustained
        .measurements
        .insert("elapsed_seconds".into(), 60.0);
    assert!(validate_evidence(&report).is_err());

    let handoff = report
        .gates
        .iter_mut()
        .find(|gate| gate.id == TransportGateId::BoundedHandoffSaturation)
        .expect("handoff gate");
    handoff.measurements.remove("handoff_elapsed_seconds");
    handoff
        .measurements
        .insert("saturation_seconds".into(), 10.0);
    assert!(validate_evidence(&report).is_err());
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
fn decisive_configuration_is_one_exact_digestable_contract() {
    let configuration = canonical_decisive_configuration();
    assert_eq!(configuration.minimum_seconds, 60.0);
    assert_eq!(configuration.minimum_frames, 1_000);
    assert_eq!(configuration.saturation_seconds, 10.0);
    assert_eq!(configuration.saturation_attempts, 100);
    assert_eq!(configuration.hard_stop_seconds, 120);
    assert_eq!(
        configuration_digest(&configuration),
        canonical_decisive_configuration_digest()
    );
}

#[test]
fn workflow_contract_uses_generated_configuration_with_a_valid_synthetic_report() {
    let report = evidence(
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
    validate_evidence(&report).expect("synthetic report must satisfy the strict evidence contract");

    let generated = serde_json::json!({
        "configuration": canonical_decisive_configuration(),
        "configuration_sha256": canonical_decisive_configuration_digest(),
    });
    assert_eq!(
        report.configuration,
        serde_json::from_value(generated["configuration"].clone()).unwrap()
    );
    assert_eq!(
        report.gate_provenance.configuration_sha256,
        generated["configuration_sha256"].as_str().unwrap()
    );
}

#[test]
fn evidence_rejects_recomputed_noncanonical_hard_stop() {
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
    let mut report = evidence(gates);
    report.configuration.hard_stop_seconds = 999_999;
    report.gate_provenance.configuration_sha256 = configuration_digest(&report.configuration);
    assert!(
        validate_evidence(&report).is_err(),
        "recomputing a digest must not authorize a noncanonical hard stop"
    );
}

#[test]
fn evidence_rejects_nonpositive_or_over_hard_stop_capture_and_handoff() {
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
    let valid = evidence(gates);
    for (gate_id, key, value) in [
        (
            TransportGateId::SustainedScreencast,
            "capture_elapsed_seconds",
            0.0,
        ),
        (
            TransportGateId::SustainedScreencast,
            "handoff_elapsed_seconds",
            120.0,
        ),
        (
            TransportGateId::BoundedHandoffSaturation,
            "handoff_elapsed_seconds",
            120.0,
        ),
    ] {
        let mut report = valid.clone();
        report
            .gates
            .iter_mut()
            .find(|gate| gate.id == gate_id)
            .unwrap()
            .measurements
            .insert(key.into(), value);
        assert!(
            validate_evidence(&report).is_err(),
            "{gate_id:?} {key} must be rejected"
        );
    }
}

#[test]
fn redaction_rejects_recursive_endpoint_identity_and_encoding_bypasses() {
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
    let valid = evidence(gates);
    for text in [
        "example.test:9222",
        "operator@example.test",
        "[2001:db8::1]:9222",
        "%2568%2574%2574%2570%2573%253A%252F%252Fexample.test%253A9222",
        "endpoint=example.test:9222",
        "username=operator password=secret",
    ] {
        let mut report = valid.clone();
        report.gates[0].summary = text.into();
        assert!(
            sanitize_evidence(report).is_err(),
            "redaction bypass accepted: {text}"
        );
    }
}

#[test]
fn browser_revision_requires_the_observed_chrome_grammar() {
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

    for revision in [
        "@07b52360cc15066f987c910ab34dfbcd4a8778d2", // Linux report
        "@6a7b3dbec3b2ca25877c2553b5473b2f277ef644", // macOS report
        "unavailable",                               // pre-Chrome failure evidence
    ] {
        let mut report = evidence(gates.clone());
        report.browser.revision = revision.into();
        sanitize_evidence(report).expect("documented browser revision must be accepted");
    }

    for revision in [
        "operator@example.test",
        "http://127.0.0.1:9222",
        "@07b52360cc15066f987c910ab34dfbcd4a8778d", // 39 hex digits
        "@07b52360cc15066f987c910ab34dfbcd4a8778d20", // 41 hex digits
        "@07B52360cc15066f987c910ab34dfbcd4a8778d2", // uppercase
        "@07b52360cc15066f987c910ab34dfbcd4a8778d2-suffix",
        "%4007b52360cc15066f987c910ab34dfbcd4a8778d2", // encoded @
        "@07b52360cc15066f987c910ab34dfbcd4a8778d%32", // encoded suffix
    ] {
        let mut report = evidence(gates.clone());
        report.browser.revision = revision.into();
        assert!(
            sanitize_evidence(report).is_err(),
            "browser revision near-miss accepted: {revision}"
        );
    }
}

#[test]
fn redaction_allows_canonical_browser_rust_candidate_fixture_and_summary_evidence() {
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
    let mut report = evidence(gates);
    report.candidate.checksum =
        "c3fdb566d913b31e0014391a94c0db4ed871dbb76577dd1b2f2c5f6df158bfaa".into();
    report.source.rust_version = "rustc 1.85.1 (4d91de4e4 2025-02-17)".into();
    report.browser.product = "Chrome/149.0.7827.155".into();
    report.browser.revision = "@07b52360cc15066f987c910ab34dfbcd4a8778d2".into();
    report.fixture.sha256 = "sha256sum-of-ordered-fixture-files:abc:def".into();
    report.gates[0].summary = "session-a-999; phase:1; 100% complete".into();
    sanitize_evidence(report).expect("canonical evidence identities must remain valid");
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
    let root = krometrail_cdp::spike::resolve_repository_root(None)
        .expect("runtime repository root")
        .join("docs/evidence/cdp-transport/v1");
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
    assert!(generated.contains("CandidateContractTrace"));
    assert!(generated.contains("CanonicalWireObservation"));
    let committed = krometrail_cdp::spike::resolve_repository_root(None)
        .expect("runtime repository root")
        .join("docs/evidence/cdp-transport/v2/schema.json");
    if std::env::var_os("CDP_SPIKE_WRITE_SCHEMA").is_some() {
        write_json_schema(&committed).unwrap();
    }
    if committed.exists() {
        assert_eq!(generated, std::fs::read_to_string(committed).unwrap());
    }
    let _ = std::fs::remove_file(temporary);
}
