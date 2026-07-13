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
    BrowserSessionState, BrowserStopOutcome, TargetLifecycle,
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
        reconnect_target_limit: 8,
        reconnect_attach_concurrency: 2,
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
        reconnect_target_limit: 8,
        reconnect_attach_concurrency: 2,
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

    let terminal_count = tokio::time::timeout(Duration::from_secs(1), async {
        let mut terminal_count = 0;
        loop {
            match events.next().await.unwrap() {
                Some(BrowserSessionEvent::SessionStateChanged {
                    state: BrowserSessionState::Ended,
                }) => terminal_count += 1,
                Some(_) => {}
                None => break terminal_count,
            }
        }
    })
    .await
    .expect("stop must close the session event stream");
    assert_eq!(terminal_count, 1, "stop publishes exactly one Ended event");
}

#[tokio::test]
async fn opt_in_real_chrome_reconnects_through_a_new_physical_proxy_connection() {
    if !support::chrome::real_browser_tests_enabled() {
        eprintln!("skipping real Chrome test; set KROMETRAIL_REAL_CHROME_TESTS=1");
        return;
    }
    let _browser_lock = support::chrome::real_browser_lock().await;
    let root_guard = support::chrome::temporary_profile_root("reconnect");
    let root = root_guard.path().to_path_buf();
    let launcher = krometrail_cdp::SystemChromeLauncher::new(krometrail_cdp::LauncherConfig {
        profile_root: root.clone(),
        startup_timeout: Duration::from_secs(45),
        shutdown_timeout: Duration::from_secs(3),
    });
    let request = krometrail_core::LaunchBrowser {
        executable: None,
        profile: krometrail_core::ManagedProfile::Temporary,
        initial_url: Some(support::chrome::fixture_url()),
    };
    // The browser is deliberately launched outside ProductionBrowserConnector. The connector
    // therefore exercises the attached ownership path while this test retains an independent
    // owner capable of proving Chrome survives the proxy fault and detached stop.
    let mut launched = launcher
        .launch_owned(&request)
        .await
        .expect("real Chrome should launch for reconnect supervision");
    let mut proxy = support::cdp_proxy::CdpFaultProxy::start(&launched.endpoint)
        .await
        .expect("loopback CDP fault proxy should bind");
    let factory = krometrail_cdp::transport::CdpkitTransportFactory::new()
        .with_command_timeout(Duration::from_secs(3));
    let connector = ProductionBrowserConnector::new(Arc::new(launcher), Arc::new(factory.clone()))
        .with_config(SupervisorConfig {
            reconnect: ReconnectPolicy {
                delays: vec![Duration::from_millis(1), Duration::from_millis(5)].into_boxed_slice(),
                attempt_timeout: Duration::from_secs(5),
            },
            subscriber_capacity: 32,
            reconnect_target_limit: 64,
            reconnect_attach_concurrency: 4,
        });
    let session = connector
        .connect(BrowserConnectRequest::Attach(
            AttachBrowser::new(proxy.http_endpoint()).unwrap(),
        ))
        .await
        .expect("production connector should attach through the proxy");
    assert!(proxy.version_request_count() >= 1);
    assert!(proxy.connection_count() >= 1);
    let initial_proxy_path = proxy.websocket_path();

    let initial = session
        .targets()
        .await
        .unwrap()
        .into_iter()
        .find(|target| target.lifecycle == TargetLifecycle::Attached)
        .expect("real Chrome should expose an attached page target");
    let initial_key = initial.target.browser_target_key().to_owned();
    let initial_target_id = initial.target.id();
    let initial_generation = initial.attachment_generation;
    let mut events = session.subscribe().await.unwrap();

    assert!(
        proxy.sever_active_transport(),
        "proxy must have an active production WebSocket to sever"
    );
    assert!(
        launched.process.is_alive(),
        "severing the transport must not terminate externally owned Chrome"
    );

    let mut restored_generation = tokio::time::timeout(Duration::from_secs(10), async {
        let mut saw_reconnecting = false;
        let mut saw_suspended = false;
        let mut restored = None;
        loop {
            let event = events
                .next()
                .await
                .expect("session event stream should remain open")
                .expect("session event stream should not end during reconnect");
            match event {
                BrowserSessionEvent::SessionStateChanged {
                    state: BrowserSessionState::Reconnecting,
                } => saw_reconnecting = true,
                BrowserSessionEvent::TargetChanged { target }
                    if target.target.browser_target_key() == initial_key
                        && target.lifecycle == TargetLifecycle::Suspended =>
                {
                    saw_suspended = true;
                }
                BrowserSessionEvent::TargetChanged { target }
                    if target.target.browser_target_key() == initial_key
                        && target.lifecycle == TargetLifecycle::Attached
                        && target.attachment_generation > initial_generation =>
                {
                    assert_eq!(target.target.id(), initial_target_id);
                    restored = Some(target.attachment_generation);
                }
                BrowserSessionEvent::SessionStateChanged {
                    state: BrowserSessionState::Ready,
                } if saw_reconnecting && saw_suspended && restored.is_some() => {
                    break restored.expect("restored target generation");
                }
                _ => {}
            }
        }
    })
    .await
    .expect("real Chrome reconnect should be bounded");
    assert!(restored_generation > initial_generation);
    assert!(
        proxy.wait_for_connections(2, Duration::from_secs(2)).await,
        "supervision must establish a second physical proxy-to-Chrome connection"
    );
    assert!(proxy.version_request_count() >= 2);
    assert_ne!(
        proxy.websocket_path(),
        initial_proxy_path,
        "HTTP reconnect must use the rotated WebSocket path"
    );
    assert!(launched.process.is_alive());

    let restored = session
        .targets()
        .await
        .unwrap()
        .into_iter()
        .find(|target| target.target.browser_target_key() == initial_key)
        .expect("reconnected target should remain discoverable");
    assert_eq!(restored.target.id(), initial_target_id);
    assert_eq!(restored.attachment_generation, restored_generation);
    assert_eq!(restored.lifecycle, TargetLifecycle::Attached);

    // Repeat the same fault through several rotating HTTP discovery paths. Each cycle must rebuild
    // the exact target key without leaking a transport or publishing a half-restored generation.
    for _ in 0..2 {
        let previous_path = proxy.websocket_path();
        assert!(proxy.sever_active_transport());
        let previous_generation = restored_generation;
        restored_generation = tokio::time::timeout(Duration::from_secs(10), async {
            let mut candidate = None;
            loop {
                let event = events
                    .next()
                    .await
                    .expect("session event stream should remain open during repeated reconnect")
                    .expect("session event stream should not end during repeated reconnect");
                match event {
                    BrowserSessionEvent::TargetChanged { target }
                        if target.target.browser_target_key() == initial_key
                            && target.lifecycle == TargetLifecycle::Attached
                            && target.attachment_generation > previous_generation =>
                    {
                        assert_eq!(target.target.id(), initial_target_id);
                        candidate = Some(target.attachment_generation);
                    }
                    BrowserSessionEvent::SessionStateChanged {
                        state: BrowserSessionState::Ready,
                    } if candidate.is_some() => break candidate.unwrap(),
                    _ => {}
                }
            }
        })
        .await
        .expect("repeated real Chrome reconnect should be bounded");
        assert!(restored_generation > previous_generation);
        assert_ne!(proxy.websocket_path(), previous_path);
        assert!(launched.process.is_alive());
    }
    assert!(proxy.wait_for_connections(4, Duration::from_secs(3)).await);
    assert!(proxy.version_request_count() >= 4);

    // A fresh real cdpkit client exercises the rebuilt endpoint's post-reconnect browser command
    // and event path. The production supervisor is already subscribed before this target is made.
    let post_rebuild_url = proxy.websocket_url();
    let post_rebuild = factory
        .connect(&post_rebuild_url)
        .await
        .expect("post-rebuild cdpkit connection");
    assert!(
        proxy.wait_for_connections(5, Duration::from_secs(1)).await,
        "post-rebuild command client must use a new physical connection"
    );
    let browser = CommandScope::Browser;
    let mut created_events = post_rebuild
        .subscribe_named(&browser, "Target.targetCreated")
        .await
        .expect("post-rebuild target event subscription");
    post_rebuild
        .send_raw(
            &browser,
            "Target.setDiscoverTargets",
            json!({"discover": true}),
        )
        .await
        .expect("post-rebuild target discovery command");
    let created_key = post_rebuild
        .send_raw(
            &browser,
            "Target.createTarget",
            json!({"url": support::chrome::fixture_url()}),
        )
        .await
        .expect("post-rebuild target creation command")
        .get("targetId")
        .and_then(Value::as_str)
        .expect("Chrome should return a target key")
        .to_owned();
    let created_event = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let event = created_events
                .next()
                .await
                .expect("post-rebuild target event stream should stay open")
                .expect("Chrome should emit targetCreated");
            if event
                .params
                .pointer("/targetInfo/targetId")
                .and_then(Value::as_str)
                == Some(created_key.as_str())
            {
                break event.params;
            }
        }
    })
    .await
    .expect("post-rebuild target event should be bounded");
    assert_eq!(
        created_event
            .pointer("/targetInfo/targetId")
            .and_then(Value::as_str),
        Some(created_key.as_str())
    );
    let targets_after_create = post_rebuild
        .send_raw(&browser, "Target.getTargets", json!({}))
        .await
        .expect("post-rebuild target snapshot command");
    assert!(
        targets_after_create
            .get("targetInfos")
            .and_then(Value::as_array)
            .is_some_and(|targets| {
                targets.iter().any(|target| {
                    target.get("targetId").and_then(Value::as_str) == Some(created_key.as_str())
                })
            })
    );

    let created = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let event = events
                .next()
                .await
                .expect("production event stream should remain open after rebuild")
                .expect("production event stream should not end after rebuild");
            if let BrowserSessionEvent::TargetChanged { target }
            | BrowserSessionEvent::TargetDiscovered { target } = event
                && target.target.browser_target_key() == created_key
                && target.lifecycle == TargetLifecycle::Attached
            {
                break target;
            }
        }
    })
    .await
    .expect("production target discovery should be bounded");
    assert_eq!(created.target.browser_target_key(), created_key);
    // A late event from the severed generation must not undo the restored exact-key state while
    // the new connection is processing this target event.
    let restored_after_post_event = session
        .targets()
        .await
        .unwrap()
        .into_iter()
        .find(|target| target.target.browser_target_key() == initial_key)
        .expect("restored target should survive post-rebuild events");
    assert_eq!(restored_after_post_event.target.id(), initial_target_id);
    assert_eq!(
        restored_after_post_event.attachment_generation,
        restored_generation
    );

    drop(created_events);
    drop(post_rebuild);
    assert_eq!(session.stop().await.unwrap(), BrowserStopOutcome::Detached);
    assert!(
        launched.process.is_alive(),
        "attached stop must leave externally owned Chrome alive"
    );
    let direct = factory
        .connect(launched.endpoint.browser_websocket_url().as_str())
        .await
        .expect("Chrome should accept a direct connection after detached stop");
    direct
        .send_raw(&browser, "Browser.getVersion", json!({}))
        .await
        .expect("Chrome should answer after detached stop");
    drop(direct);

    proxy.shutdown().await;
    drop(proxy);
    launched
        .shutdown()
        .await
        .expect("test-owned Chrome should shut down cleanly");
    drop(launched);
    drop(root_guard);
    assert!(
        support::chrome::process_references(&root).is_empty(),
        "test Chrome must not retain the unique profile root"
    );
    assert!(!root.exists(), "test profile root must be removed");
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
