#![cfg(feature = "cdpkit-transport")]

mod support;

use krometrail_cdp::{
    CdpTransport, CommandScope, LocalCdpEndpoint, TransportClose, TransportError,
    probe_compatibility,
};
use serde_json::json;
use support::scripted_cdp::ScriptedCdp;

#[test]
fn malformed_and_non_loopback_endpoints_fail_before_side_effects() {
    for input in [
        "https://127.0.0.1:9222",
        "wss://127.0.0.1:9222/devtools/browser/id",
        "ws://user:secret@127.0.0.1:9222/devtools/browser/id",
        "ws://127.0.0.1:9222/devtools/browser/id#fragment",
        "ws://8.8.8.8:9222/devtools/browser/id",
        "not a URL",
    ] {
        assert!(
            LocalCdpEndpoint::from_websocket_url(input).is_err(),
            "accepted {input}"
        );
    }
    let endpoint =
        LocalCdpEndpoint::from_websocket_url("ws://localhost:9222/devtools/browser/id").unwrap();
    assert_eq!(endpoint.redacted_label(), "localhost:9222");
}

#[tokio::test]
async fn scripted_transport_preserves_named_events_and_flat_session_scope() {
    let transport = ScriptedCdp::chrome();
    transport.push_event("Runtime.consoleAPICalled", json!({"additive": 7}));
    let browser = CommandScope::Browser;
    let session = CommandScope::session("session-a").unwrap();
    transport
        .send_raw(&browser, "Browser.getVersion", json!({}))
        .await
        .unwrap();
    let mut events = transport
        .subscribe_named(&session, "Runtime.consoleAPICalled")
        .await
        .unwrap();
    let event = events.next().await.unwrap().unwrap();
    assert_eq!(event.method, "Runtime.consoleAPICalled");
    assert_eq!(event.params, json!({"additive": 7}));
    assert!(events.next().await.unwrap().is_none());
    assert_eq!(transport.commands()[0].1, None);
    assert_eq!(transport.subscriptions()[0].1.as_deref(), Some("session-a"));
    let session_b = CommandScope::session("session-b").unwrap();
    transport
        .send_raw(&session, "Runtime.enable", json!({"token":"a"}))
        .await
        .unwrap();
    transport
        .send_raw(&session_b, "Runtime.enable", json!({"token":"b"}))
        .await
        .unwrap();
    let sessions: Vec<_> = transport
        .commands()
        .into_iter()
        .filter_map(|(_, session)| session)
        .collect();
    assert_eq!(sessions, vec!["session-a", "session-b"]);
}

#[tokio::test]
async fn malformed_responses_are_rejected_without_source_text() {
    let transport = ScriptedCdp::chrome();
    transport.malformed("Browser.getVersion");
    let error = probe_compatibility(&transport).await.unwrap_err();
    assert!(matches!(
        error,
        krometrail_cdp::CompatibilityProbeError::InvalidIdentity
    ));
    assert!(!error.to_string().contains("malformed-response"));
}

#[tokio::test]
async fn compatibility_requires_every_registry_capability_and_does_not_start_capture() {
    let transport = ScriptedCdp::chrome();
    let compatibility = probe_compatibility(&transport).await.unwrap();
    assert!(
        compatibility
            .capabilities
            .iter()
            .all(|capability| capability.available)
    );
    assert!(
        !transport
            .commands()
            .iter()
            .any(|(method, _)| method == "Page.startScreencast")
    );
}

#[tokio::test]
async fn missing_capability_is_a_stable_failure() {
    let transport = ScriptedCdp::chrome();
    transport.missing("Input.dispatchMouseEvent");
    let error = probe_compatibility(&transport).await.unwrap_err();
    assert!(
        error
            .missing()
            .contains(&krometrail_core::RendererCapability::Input)
    );
    assert_eq!(
        error.to_core_error().code,
        krometrail_core::ErrorCode::BrowserCompatibilityFailed
    );
}

#[test]
fn no_transport_error_contains_a_source_or_endpoint() {
    let error = TransportError::Disconnected;
    assert_eq!(error.to_string(), "transport disconnected");
    assert!(!error.to_string().contains("ws://"));
    let close = TransportClose {
        reason: krometrail_core::NonEmptyText::new("remote").unwrap(),
    };
    assert_eq!(close.reason.as_str(), "remote");
}
