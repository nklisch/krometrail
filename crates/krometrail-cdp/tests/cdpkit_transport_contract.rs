#![cfg(feature = "cdp-spike-cdpkit")]

use krometrail_cdp::spike::{cdpkit_adapter::CdpkitTransportFactory, run_candidate_wire_contract};

#[tokio::test]
async fn decisive_cdpkit_candidate_contract_binds_its_own_scripted_endpoint() {
    let evidence = run_candidate_wire_contract(|endpoint| {
        Box::new(CdpkitTransportFactory::with_scripted_endpoint(endpoint))
    })
    .await
    .expect("decisive candidate contract");

    assert!(evidence.trace_sha256.starts_with("sha256:"));
    assert!(evidence.trace_observations > 0);
    assert_eq!(evidence.results.drift_fixtures, 3);
    assert!(evidence.results.connection_survived);
    assert_eq!(evidence.results.routing_commands, 200);
    assert_eq!(evidence.results.routing_events, 200);
    assert_eq!(evidence.results.routing_cross_delivery, 0);
    assert!(evidence.results.event_before_response);
    assert!(evidence.results.detach_during_pending);
    assert!(evidence.results.pending_calls_closed);
    assert!(evidence.results.subscriptions_closed);
    assert!(evidence.results.socket_closed);
    assert_eq!(evidence.results.reconnect_connections, 2);
    assert_eq!(evidence.results.sessions_rebuilt, 2);
}
