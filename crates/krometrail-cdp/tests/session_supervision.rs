#![cfg(feature = "cdpkit-transport")]

mod support;

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

use krometrail_cdp::{
    CdpTransport, CdpTransportFactory, CommandScope, NamedEvent, ProductionBrowserConnector,
    ReconnectPolicy, SupervisorConfig, TransportError, TransportEvents, TransportFuture,
};
use krometrail_core::{
    AttachBrowser, BrowserConnectRequest, BrowserConnector, BrowserSessionEvent,
    BrowserSessionState, BrowserStopOutcome,
};
use serde_json::{Value, json};

#[test]
fn reconnect_policy_is_finite_and_fixture_is_static() {
    assert!(!support::chrome::fixture_url().is_empty());
    assert!(support::static_fixture::contains_stable_fixture_markers());
    let policy = ReconnectPolicy {
        delays: vec![Duration::from_millis(1), Duration::from_millis(2)].into_boxed_slice(),
        attempt_timeout: Duration::from_millis(10),
    };
    assert_eq!(policy.delays.len(), 2);
    assert!(policy.delays.iter().all(|delay| *delay > Duration::ZERO));
    let config = SupervisorConfig {
        reconnect: policy,
        subscriber_capacity: 2,
    };
    assert_eq!(config.subscriber_capacity, 2);
}

#[tokio::test]
async fn production_supervisor_rebuilds_after_a_transport_event_stream_closes() {
    let factory = Arc::new(ScriptedReconnectFactory::default());
    let connector = ProductionBrowserConnector::new(
        Arc::new(krometrail_cdp::SystemChromeLauncher::new(
            krometrail_cdp::LauncherConfig::default(),
        )),
        factory,
    )
    .with_config(SupervisorConfig {
        reconnect: ReconnectPolicy {
            delays: vec![Duration::from_millis(1)].into_boxed_slice(),
            attempt_timeout: Duration::from_millis(100),
        },
        subscriber_capacity: 16,
    });
    let session = connector
        .connect(BrowserConnectRequest::Attach(
            AttachBrowser::new("ws://127.0.0.1:9222/devtools/browser/fake").unwrap(),
        ))
        .await
        .unwrap();
    let mut events = session.subscribe().await.unwrap();

    let reconnecting = tokio::time::timeout(Duration::from_secs(1), events.next())
        .await
        .expect("transport closure should enter reconnect")
        .unwrap()
        .unwrap();
    assert!(matches!(
        reconnecting,
        BrowserSessionEvent::SessionStateChanged {
            state: BrowserSessionState::Reconnecting
        }
    ));

    let mut saw_ready = false;
    for _ in 0..8 {
        let event = tokio::time::timeout(Duration::from_secs(1), events.next())
            .await
            .expect("reconnect should be bounded")
            .unwrap()
            .unwrap();
        if matches!(
            event,
            BrowserSessionEvent::SessionStateChanged {
                state: BrowserSessionState::Ready
            }
        ) {
            saw_ready = true;
            break;
        }
    }
    assert!(saw_ready, "reconnect did not publish Ready");
    assert_eq!(session.targets().await.unwrap().len(), 1);
    assert_eq!(session.stop().await.unwrap(), BrowserStopOutcome::Detached);
}

#[test]
fn cancellation_input_is_typed_at_the_supervision_boundary() {
    let input = krometrail_cdp::SupervisorInput::ConnectionLost(krometrail_cdp::TransportClose {
        reason: krometrail_core::NonEmptyText::new("remote").unwrap(),
    });
    assert!(matches!(
        input,
        krometrail_cdp::SupervisorInput::ConnectionLost(_)
    ));
    let _ = BrowserSessionState::Reconnecting;
}

#[derive(Default)]
struct ScriptedReconnectFactory {
    connections: AtomicUsize,
}

impl CdpTransportFactory for ScriptedReconnectFactory {
    fn connect(
        &self,
        _browser_websocket_url: &str,
    ) -> TransportFuture<'_, Result<Arc<dyn CdpTransport>, TransportError>> {
        let connection = self.connections.fetch_add(1, Ordering::AcqRel);
        Box::pin(async move {
            Ok(Arc::new(ScriptedReconnectTransport {
                close_events_once: connection == 0,
                next_session: AtomicUsize::new(0),
                commands: Mutex::new(Vec::new()),
            }) as Arc<dyn CdpTransport>)
        })
    }
}

struct ScriptedReconnectTransport {
    close_events_once: bool,
    next_session: AtomicUsize,
    commands: Mutex<Vec<(String, Option<String>)>>,
}

impl CdpTransport for ScriptedReconnectTransport {
    fn send_raw(
        &self,
        scope: &CommandScope,
        method: &str,
        _params: Value,
    ) -> TransportFuture<'_, Result<Value, TransportError>> {
        let session = match scope {
            CommandScope::Browser => None,
            CommandScope::Session(session) => Some(session.as_str().to_owned()),
        };
        self.commands
            .lock()
            .expect("scripted command lock")
            .push((method.to_owned(), session));
        let value = match method {
            "Browser.getVersion" => json!({
                "protocolVersion": "1.3",
                "product": "Chrome/149",
                "revision": "fixture",
                "userAgent": "Chrome/149",
                "jsVersion": "12",
            }),
            "Target.getTargets" => json!({
                "targetInfos": [{
                    "targetId": "page-a",
                    "type": "page",
                    "url": "http://fixture/",
                    "title": "fixture",
                    "attached": false,
                }],
            }),
            "Target.attachToTarget" => {
                let id = self.next_session.fetch_add(1, Ordering::Relaxed);
                json!({"sessionId": format!("session-{id}")})
            }
            "Schema.getDomains" => {
                json!({"domains": [{"name": "Page", "commands": [{"name": "startScreencast"}]}]})
            }
            "Runtime.evaluate" => json!({
                "result": {"result": {"type": "string", "value": "visible"}},
            }),
            _ => json!({}),
        };
        Box::pin(async move { Ok(value) })
    }

    fn subscribe_named(
        &self,
        _scope: &CommandScope,
        _method: &str,
    ) -> TransportFuture<'_, Result<Box<dyn TransportEvents>, TransportError>> {
        Box::pin(async move {
            Ok(Box::new(ScriptedReconnectEvents {
                close_once: self.close_events_once,
            }) as Box<dyn TransportEvents>)
        })
    }

    fn close_reason(&self) -> Option<krometrail_cdp::TransportClose> {
        None
    }

    fn is_closed(&self) -> bool {
        false
    }
}

struct ScriptedReconnectEvents {
    close_once: bool,
}

impl TransportEvents for ScriptedReconnectEvents {
    fn next(&mut self) -> TransportFuture<'_, Result<Option<NamedEvent>, TransportError>> {
        let close_once = std::mem::replace(&mut self.close_once, false);
        Box::pin(async move {
            if close_once {
                Err(TransportError::SubscriptionClosed)
            } else {
                std::future::pending().await
            }
        })
    }
}
