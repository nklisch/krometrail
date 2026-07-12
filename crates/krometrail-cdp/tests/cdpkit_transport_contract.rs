#![cfg(feature = "cdp-spike-cdpkit")]

use krometrail_cdp::spike::{
    cdpkit_adapter::CdpkitTransportFactory, fixture_server::ScriptedCdpServer,
    run_transport_scenarios,
};

#[tokio::test]
async fn exact_cdpkit_uses_the_shared_scripted_peer_scenarios() {
    let server = ScriptedCdpServer::start()
        .await
        .expect("scripted CDP server");
    let factory = CdpkitTransportFactory::with_scripted_endpoint(server.ws_url.clone());
    let mut peer = server.controller();
    let evidence = run_transport_scenarios(&factory, &mut peer).await;
    assert!(
        evidence.passed(),
        "candidate scenario failure: {evidence:?}"
    );
    assert!(peer.connection_count() >= 2);
    assert!(peer.event_before_response("Runtime.evaluate"));
    assert_eq!(peer.drift_methods_observed().len(), 3);
    assert_eq!(evidence.gates.len(), 13);
}
