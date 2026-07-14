#![cfg(feature = "cdpkit-transport")]

mod support;

use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use krometrail_cdp::{
    BrowserEventConfig, CdpTransport, CdpTransportFactory, ProductionBrowserConnector,
    ReconnectPolicy, SupervisorConfig, TransportError, TransportFuture,
};
use krometrail_core::{
    AttachBrowser, BrowserConnectRequest, BrowserConnector, BrowserEvent, BrowserEventBatch,
    BrowserEventKind, BrowserEventSink, IdSource, IdValue, MonotonicClock, ObservedTime,
    PortFuture,
};
use serde_json::json;
use support::scripted_cdp::{ScriptedActivity, ScriptedCdp};
use uuid::Uuid;

#[derive(Clone)]
struct ScriptedFactory(ScriptedCdp);

impl CdpTransportFactory for ScriptedFactory {
    fn connect(
        &self,
        _browser_websocket_url: &str,
    ) -> TransportFuture<'_, Result<Arc<dyn CdpTransport>, TransportError>> {
        let transport = self.0.clone();
        Box::pin(async move { Ok(Arc::new(transport) as Arc<dyn CdpTransport>) })
    }
}

struct SequenceFactory(Mutex<VecDeque<ScriptedCdp>>);

impl SequenceFactory {
    fn new(transports: impl IntoIterator<Item = ScriptedCdp>) -> Self {
        Self(Mutex::new(transports.into_iter().collect()))
    }
}

impl CdpTransportFactory for SequenceFactory {
    fn connect(
        &self,
        _browser_websocket_url: &str,
    ) -> TransportFuture<'_, Result<Arc<dyn CdpTransport>, TransportError>> {
        let transport = self.0.lock().expect("transport sequence lock").pop_front();
        Box::pin(async move {
            transport
                .map(|transport| Arc::new(transport) as Arc<dyn CdpTransport>)
                .ok_or(TransportError::ConnectFailed)
        })
    }
}

#[derive(Default)]
struct TestClock(AtomicU64);

impl MonotonicClock for TestClock {
    fn now(&self) -> ObservedTime {
        ObservedTime::from_nanos(self.0.fetch_add(1, Ordering::AcqRel) + 1)
    }
}

#[derive(Default)]
struct TestIds(AtomicU64);

impl IdSource for TestIds {
    fn next(&self) -> IdValue {
        let next = self.0.fetch_add(1, Ordering::AcqRel) + 1;
        IdValue::from_uuid(Uuid::from_u128(u128::from(next)))
    }
}

#[derive(Default)]
struct RecordingEventSink {
    events: Mutex<Vec<BrowserEvent>>,
    changed: tokio::sync::Notify,
}

impl RecordingEventSink {
    fn events(&self) -> Vec<BrowserEvent> {
        self.events.lock().expect("event sink lock").clone()
    }

    async fn wait_for_kind_count(&self, kind: BrowserEventKind, count: usize) {
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                let notified = self.changed.notified();
                if self
                    .events
                    .lock()
                    .expect("event sink lock")
                    .iter()
                    .filter(|event| event.kind() == kind)
                    .count()
                    >= count
                {
                    return;
                }
                notified.await;
            }
        })
        .await
        .expect("browser events were not persisted");
    }
}

impl BrowserEventSink for RecordingEventSink {
    fn append_event_batch(
        &self,
        batch: BrowserEventBatch,
    ) -> PortFuture<'_, krometrail_core::Result<()>> {
        Box::pin(async move {
            self.events
                .lock()
                .expect("event sink lock")
                .extend(batch.events().iter().cloned());
            self.changed.notify_waiters();
            Ok(())
        })
    }
}

fn two_targets() -> serde_json::Value {
    json!({"targetInfos":[
        {"targetId":"target-a","type":"page","url":"http://a.test/","title":"a"},
        {"targetId":"target-b","type":"page","url":"http://b.test/","title":"b"}
    ]})
}

fn reconnect_transport(session: &str, message: &str) -> ScriptedCdp {
    let transport = ScriptedCdp::chrome();
    transport.hold_events_open();
    let targets = json!({"targetInfos":[
        {"targetId":"stable-target","type":"page","url":"http://stable.test/","title":"stable"}
    ]});
    transport.push_response("Target.getTargets", targets.clone());
    transport.push_response("Target.getTargets", targets);
    transport.push_response(
        "Target.attachToTarget",
        json!({"sessionId":"probe-session"}),
    );
    transport.push_response("Target.attachToTarget", json!({"sessionId":session}));
    transport.push_scoped_event(
        "Runtime.consoleAPICalled",
        Some(session),
        json!({"type":"log","args":[{"type":"string","value":message}],"timestamp":1.0}),
    );
    transport
}

#[tokio::test]
async fn one_authority_routes_same_named_streams_by_session_and_installs_before_restore() {
    let transport = ScriptedCdp::chrome();
    transport.hold_events_open();
    transport.push_response("Target.getTargets", two_targets());
    transport.push_response("Target.getTargets", two_targets());
    transport.push_response(
        "Target.attachToTarget",
        json!({"sessionId":"probe-session"}),
    );
    transport.push_response("Target.attachToTarget", json!({"sessionId":"session-a"}));
    transport.push_response("Target.attachToTarget", json!({"sessionId":"session-b"}));
    transport.push_scoped_event(
        "Runtime.consoleAPICalled",
        Some("session-a"),
        json!({"type":"log","args":[{"type":"string","value":"from-a"}],"timestamp":1.0}),
    );
    transport.push_scoped_event(
        "Runtime.consoleAPICalled",
        Some("session-b"),
        json!({"type":"warning","args":[{"type":"string","value":"from-b"}],"timestamp":2.0}),
    );

    let clock = Arc::new(TestClock::default());
    let ids = Arc::new(TestIds::default());
    let sink = Arc::new(RecordingEventSink::default());
    let session = ProductionBrowserConnector::new(
        Arc::new(krometrail_cdp::SystemChromeLauncher::new(
            krometrail_cdp::LauncherConfig::default(),
        )),
        Arc::new(ScriptedFactory(transport.clone())),
    )
    .with_config(SupervisorConfig::default())
    .with_browser_events(
        clock as Arc<dyn MonotonicClock>,
        ids as Arc<dyn IdSource>,
        sink.clone() as Arc<dyn BrowserEventSink>,
        BrowserEventConfig::default(),
    )
    .connect(BrowserConnectRequest::Attach(
        AttachBrowser::new("ws://127.0.0.1:9222/devtools/browser/event-routing").unwrap(),
    ))
    .await
    .unwrap();

    sink.wait_for_kind_count(BrowserEventKind::ConsoleMessage, 2)
        .await;
    let console = sink
        .events()
        .into_iter()
        .filter(|event| event.kind() == BrowserEventKind::ConsoleMessage)
        .collect::<Vec<_>>();
    assert_eq!(console.len(), 2);
    assert_ne!(console[0].target_id(), console[1].target_id());
    assert!(
        console
            .iter()
            .all(|event| event.attachment_generation() == 1)
    );

    let activity = transport.activity();
    for transport_session in ["session-a", "session-b"] {
        let page_enable = activity
            .iter()
            .position(|entry| {
                matches!(
                    entry,
                    ScriptedActivity::Command { method, session }
                        if method == "Page.enable" && session.as_deref() == Some(transport_session)
                )
            })
            .expect("authority Page.enable command");
        let subscriptions = activity
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                matches!(
                    entry,
                    ScriptedActivity::Subscription { session, .. }
                        if session.as_deref() == Some(transport_session)
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(subscriptions.len(), 12);
        assert!(subscriptions.iter().all(|(index, _)| *index < page_enable));

        let commands = activity
            .iter()
            .filter_map(|entry| match entry {
                ScriptedActivity::Command { method, session }
                    if session.as_deref() == Some(transport_session) =>
                {
                    Some(method.as_str())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            &commands[..6],
            &[
                "Page.enable",
                "Page.setLifecycleEventsEnabled",
                "Runtime.enable",
                "Log.enable",
                "Network.enable",
                "Accessibility.enable",
            ]
        );
    }

    session.stop().await.unwrap();
}

#[tokio::test]
async fn reconnect_rebinds_streams_without_crossing_or_accepting_the_old_generation() {
    let initial = reconnect_transport("old-session", "old-generation");
    let replacement = reconnect_transport("new-session", "new-generation");
    let clock = Arc::new(TestClock::default());
    let ids = Arc::new(TestIds::default());
    let sink = Arc::new(RecordingEventSink::default());
    let config = SupervisorConfig {
        reconnect: ReconnectPolicy {
            delays: vec![Duration::ZERO].into_boxed_slice(),
            attempt_timeout: Duration::from_secs(1),
        },
        ..SupervisorConfig::default()
    };
    let session = ProductionBrowserConnector::new(
        Arc::new(krometrail_cdp::SystemChromeLauncher::new(
            krometrail_cdp::LauncherConfig::default(),
        )),
        Arc::new(SequenceFactory::new([initial.clone(), replacement.clone()])),
    )
    .with_config(config)
    .with_browser_events(
        clock as Arc<dyn MonotonicClock>,
        ids as Arc<dyn IdSource>,
        sink.clone() as Arc<dyn BrowserEventSink>,
        BrowserEventConfig::default(),
    )
    .connect(BrowserConnectRequest::Attach(
        AttachBrowser::new("ws://127.0.0.1:9222/devtools/browser/event-reconnect").unwrap(),
    ))
    .await
    .unwrap();

    sink.wait_for_kind_count(BrowserEventKind::ConsoleMessage, 1)
        .await;
    initial.disconnect();
    sink.wait_for_kind_count(BrowserEventKind::ConsoleMessage, 2)
        .await;

    let console = sink
        .events()
        .into_iter()
        .filter(|event| event.kind() == BrowserEventKind::ConsoleMessage)
        .collect::<Vec<_>>();
    assert_eq!(console.len(), 2);
    assert_eq!(console[0].target_id(), console[1].target_id());
    assert_eq!(console[0].attachment_generation(), 1);
    assert_eq!(console[1].attachment_generation(), 2);
    assert!(console[0].ordinal() < console[1].ordinal());
    assert_eq!(
        initial
            .subscriptions()
            .iter()
            .filter(|(method, session)| {
                method == "Runtime.consoleAPICalled" && session.as_deref() == Some("old-session")
            })
            .count(),
        1
    );
    assert_eq!(
        replacement
            .subscriptions()
            .iter()
            .filter(|(method, session)| {
                method == "Runtime.consoleAPICalled" && session.as_deref() == Some("new-session")
            })
            .count(),
        1
    );

    session.stop().await.unwrap();
}
