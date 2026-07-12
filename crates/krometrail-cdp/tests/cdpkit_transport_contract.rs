#![cfg(feature = "cdp-spike-cdpkit")]

use krometrail_cdp::spike::{
    cdpkit_adapter::CdpkitTransportFactory, committed_protocol_fixtures,
    ordered_protocol_fixture_digest, run_candidate_wire_contract,
};

async fn decisive_candidate_contract() -> krometrail_cdp::spike::CandidateContractEvidence {
    let evidence = run_candidate_wire_contract(|endpoint| {
        Box::new(CdpkitTransportFactory::with_scripted_endpoint(endpoint))
    })
    .await
    .expect("decisive candidate contract");

    assert_eq!(evidence.fixture_sha256, ordered_protocol_fixture_digest());
    assert!(evidence.trace_sha256.starts_with("sha256:"));
    assert!(evidence.trace_observations > 0);
    let expected_methods = committed_protocol_fixtures()
        .unwrap()
        .into_iter()
        .map(|fixture| fixture.method)
        .collect::<Vec<_>>();
    assert_eq!(
        evidence.results.wire.drift_fixtures,
        expected_methods.len() as u64
    );
    assert_eq!(evidence.results.wire.drift_methods, expected_methods);
    assert!(evidence.results.wire.connection_survived);
    assert_eq!(evidence.results.wire.routing_commands, 200);
    assert_eq!(evidence.results.wire.routing_events, 200);
    assert_eq!(evidence.results.wire.routing_cross_delivery, 0);
    assert!(evidence.results.wire.event_before_response);
    assert!(evidence.results.wire.detach_during_pending);
    assert!(evidence.results.runtime.pending_calls_closed);
    assert!(evidence.results.runtime.subscriptions_closed);
    assert!(evidence.results.wire.socket_closed);
    assert_eq!(evidence.results.wire.reconnect_connections, 2);
    assert_eq!(evidence.results.wire.sessions_rebuilt, 2);
    evidence
}

#[tokio::test]
async fn decisive_candidate_contract_is_repeatable_in_one_process() {
    let first = decisive_candidate_contract().await;
    let second = decisive_candidate_contract().await;
    assert_eq!(first, second, "candidate contract must be deterministic");
}
