#![cfg(feature = "cdp-spike-cdpkit")]

use krometrail_cdp::spike::{
    ScriptedCdpPeer, cdpkit_adapter::CdpkitTransportFactory, fixture_server::ScriptedCdpServer,
    run_transport_scenarios,
};

#[tokio::test]
async fn exact_cdpkit_uses_the_shared_scripted_peer_scenarios() {
    let server = ScriptedCdpServer::start()
        .await
        .expect("scripted CDP server");
    let factory = CdpkitTransportFactory::with_scripted_endpoint(server.ws_url.clone());
    let mut peer = ScriptedCdpPeer::empty();
    let evidence = run_transport_scenarios(&factory, &mut peer).await;
    assert!(
        evidence.passed(),
        "candidate scenario failure: {evidence:?}"
    );
    assert!(peer.is_exhausted());
    assert_eq!(evidence.gates.len(), 13);
}
