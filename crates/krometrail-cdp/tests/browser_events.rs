#![cfg(feature = "cdpkit-transport")]

mod support;

use std::{
    collections::VecDeque,
    path::PathBuf,
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
    BrowserEventClass, BrowserEventFilter, BrowserEventKind, BrowserEventSelection,
    BrowserEventSelector, BrowserEventSeverity, BrowserEventSink, BrowserEventSource,
    CaptureGapPolicy, CaptureOrdinal, CapturedFrame, DeviceScaleFactor, EncodedFrame,
    EventPageLimit, FrameId, IdSource, IdValue, ImageFormat, MonotonicClock, ObservedTime,
    PixelDimensions, PortFuture, RangeResolutionOptions, RecordingSink, ResolvedRange,
    RetentionPolicy, SessionId, SessionRange, SessionTime, TargetId, TemporalContextQuery,
    TemporalContextRequest, TemporalRangeAnchorKind,
};
use krometrail_store::{
    IndexStoreConfig, RecordingStore, RotationConfig, SegmentStoreConfig, SegmentWriter,
    SqliteIndex,
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

struct PersistentEventStore {
    root: PathBuf,
    store: Arc<RecordingStore>,
}

impl PersistentEventStore {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("krometrail-cdp-browser-events-{}", Uuid::new_v4()));
        let segments = root.join("segments");
        let index = Arc::new(
            SqliteIndex::open(IndexStoreConfig {
                database_path: root.join("index.sqlite3"),
                segments_directory: segments.clone(),
                busy_timeout: Duration::from_secs(1),
            })
            .unwrap(),
        );
        let writer = Arc::new(
            SegmentWriter::open(SegmentStoreConfig {
                directory: segments,
                rotation: RotationConfig::suggested(),
            })
            .unwrap(),
        );
        let store = Arc::new(RecordingStore::new(writer, index).unwrap());
        Self { root, store }
    }

    fn cleanup(self) {
        let root = self.root.clone();
        drop(self);
        std::fs::remove_dir_all(root).unwrap();
    }
}

fn event_selector(session_id: SessionId, target_id: TargetId) -> BrowserEventSelector {
    BrowserEventSelector::new(
        session_id,
        target_id,
        SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(u64::MAX)).unwrap(),
        Vec::<BrowserEventClass>::new(),
        BrowserEventSeverity::Debug,
    )
    .unwrap()
}

async fn persisted_events(
    store: &RecordingStore,
    session_id: SessionId,
    target_ids: &[TargetId],
) -> Vec<BrowserEvent> {
    let mut events = Vec::new();
    for target_id in target_ids {
        events.extend(
            store
                .chronological_events(
                    event_selector(session_id, *target_id),
                    None,
                    EventPageLimit::new(1_000).unwrap(),
                )
                .await
                .unwrap(),
        );
    }
    events
}

async fn wait_for_persisted_kind_count(
    store: &RecordingStore,
    session_id: SessionId,
    target_ids: &[TargetId],
    kind: BrowserEventKind,
    count: usize,
) -> Vec<BrowserEvent> {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let events = persisted_events(store, session_id, target_ids).await;
            if events.iter().filter(|event| event.kind() == kind).count() >= count {
                return events;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("browser events were not committed to the v5 store")
}

async fn append_frame(
    store: &RecordingStore,
    session_id: SessionId,
    target_id: TargetId,
    id: u128,
    ordinal: u64,
    time: SessionTime,
) -> FrameId {
    let frame_id = FrameId::from_uuid(Uuid::from_u128(id));
    let frame = EncodedFrame::new(
        CapturedFrame::new(
            frame_id,
            session_id,
            target_id,
            CaptureOrdinal::new(ordinal).unwrap(),
            None,
            ObservedTime::from_nanos(time.as_nanos()),
            time,
            ImageFormat::Jpeg,
            PixelDimensions::new(1, 1).unwrap(),
            PixelDimensions::new(1, 1).unwrap(),
            DeviceScaleFactor::new(1.0).unwrap(),
            vec![],
        )
        .unwrap(),
        vec![ordinal as u8],
    )
    .unwrap();
    store.append_frame(frame).await.unwrap();
    frame_id
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

fn two_target_generation_transport(
    session_a: &str,
    session_b: &str,
    generation: &str,
    include_private_corpus: bool,
) -> ScriptedCdp {
    let transport = ScriptedCdp::chrome();
    transport.hold_events_open();
    let targets = two_targets();
    transport.push_response("Target.getTargets", targets.clone());
    transport.push_response("Target.getTargets", targets);
    transport.push_response(
        "Target.attachToTarget",
        json!({"sessionId":"probe-session"}),
    );
    transport.push_response("Target.attachToTarget", json!({"sessionId":session_a}));
    transport.push_response("Target.attachToTarget", json!({"sessionId":session_b}));
    for (session, target) in [(session_a, "a"), (session_b, "b")] {
        transport.push_scoped_event(
            "Runtime.consoleAPICalled",
            Some(session),
            json!({
                "type":"error",
                "args":[{"type":"string","value":format!("{generation}-{target}")}],
                "timestamp":1.0
            }),
        );
    }
    if include_private_corpus {
        transport.push_scoped_event(
            "Runtime.exceptionThrown",
            Some(session_a),
            json!({
                "timestamp":2.0,
                "exceptionDetails":{
                    "text":"token=console-secret /home/operator/private.js",
                    "exception":{"className":"authorization=exception-secret"},
                    "stackTrace":{"callFrames":[{
                        "functionName":"password=stack-secret",
                        "url":"file:///Users/operator/private/source.js",
                        "lineNumber":7,
                        "columnNumber":9
                    }]}
                },
                "fillValue":"fill-value-sentinel",
                "uploadFiles":["/private/upload-path-sentinel.txt"]
            }),
        );
        transport.push_scoped_event(
            "Network.requestWillBeSent",
            Some(session_a),
            json!({
                "requestId":"raw-request-sentinel",
                "timestamp":3.0,
                "type":"Document",
                "request":{
                    "method":"POST",
                    "url":"https://user:url-secret@example.test/private/path?query-secret=yes#fragment-secret",
                    "headers":{"Authorization":"Bearer auth-sentinel","Cookie":"cookie-sentinel"},
                    "postData":"body-sentinel"
                },
                "responseBody":"response-body-sentinel"
            }),
        );
        transport.push_scoped_event(
            "Page.javascriptDialogOpening",
            Some(session_a),
            json!({
                "type":"prompt",
                "message":"dialog-message-sentinel",
                "defaultPrompt":"dialog-default-sentinel"
            }),
        );
        transport.push_scoped_event(
            "Page.javascriptDialogClosed",
            Some(session_a),
            json!({"result":true,"userInput":"dialog-input-sentinel"}),
        );
    }
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
async fn explicitly_disabled_events_add_no_recording_streams_or_domain_enables() {
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
        json!({"type":"error","args":[{"type":"string","value":"must-not-persist"}]}),
    );

    let sink = Arc::new(RecordingEventSink::default());
    let session = ProductionBrowserConnector::new(
        Arc::new(krometrail_cdp::SystemChromeLauncher::new(
            krometrail_cdp::LauncherConfig::default(),
        )),
        Arc::new(ScriptedFactory(transport.clone())),
    )
    .with_browser_events(
        Arc::new(TestClock::default()) as Arc<dyn MonotonicClock>,
        Arc::new(TestIds::default()) as Arc<dyn IdSource>,
        sink.clone() as Arc<dyn BrowserEventSink>,
        BrowserEventConfig::disabled(),
    )
    .connect(BrowserConnectRequest::Attach(
        AttachBrowser::new("ws://127.0.0.1:9222/devtools/browser/events-disabled").unwrap(),
    ))
    .await
    .unwrap();

    for method in [
        "Runtime.consoleAPICalled",
        "Runtime.exceptionThrown",
        "Log.entryAdded",
        "Network.requestWillBeSent",
        "Network.responseReceived",
        "Network.loadingFinished",
        "Network.loadingFailed",
        "Page.frameNavigated",
        "Page.navigatedWithinDocument",
        "Page.javascriptDialogClosed",
    ] {
        assert!(
            transport
                .subscriptions()
                .iter()
                .all(|(subscribed, _)| subscribed != method),
            "disabled browser events installed {method}",
        );
    }
    for method in ["Log.enable", "Network.enable"] {
        assert!(
            transport
                .commands()
                .iter()
                .all(|(called, transport_session)| {
                    called != method || transport_session.as_deref() == Some("probe-session")
                }),
            "disabled browser events enabled {method} for a recording target",
        );
    }
    // Page lifecycle and dialog-opening signals remain authority-owned for explicit control
    // operations; unlike semantic collection, they neither persist nor enable optional domains.
    assert!(sink.events().is_empty());
    assert!(
        transport
            .commands()
            .iter()
            .any(|(method, _)| method == "Page.enable")
    );
    assert!(
        transport
            .commands()
            .iter()
            .any(|(method, _)| method == "Runtime.enable")
    );
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

#[tokio::test]
async fn two_target_reconnect_persists_sanitized_events_into_same_range_context() {
    let initial = two_target_generation_transport("old-a", "old-b", "old-generation", true);
    let replacement = two_target_generation_transport("new-a", "new-b", "new-generation", false);
    let fixture = PersistentEventStore::new();
    let clock = Arc::new(TestClock::default());
    let ids = Arc::new(TestIds::default());
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
        Arc::clone(&fixture.store) as Arc<dyn BrowserEventSink>,
        BrowserEventConfig::default(),
    )
    .connect(BrowserConnectRequest::Attach(
        AttachBrowser::new("ws://127.0.0.1:9222/devtools/browser/event-store-context").unwrap(),
    ))
    .await
    .unwrap();

    let status = session.status().await.unwrap();
    let mut target_ids = status
        .pages
        .iter()
        .map(|page| page.target.target.id())
        .collect::<Vec<_>>();
    target_ids.sort();
    assert_eq!(target_ids.len(), 2);
    wait_for_persisted_kind_count(
        fixture.store.as_ref(),
        status.session_id,
        &target_ids,
        BrowserEventKind::ConsoleMessage,
        2,
    )
    .await;
    for kind in [
        BrowserEventKind::JavascriptException,
        BrowserEventKind::NetworkRequestStarted,
        BrowserEventKind::DialogOpened,
        BrowserEventKind::DialogClosed,
    ] {
        wait_for_persisted_kind_count(
            fixture.store.as_ref(),
            status.session_id,
            &target_ids,
            kind,
            1,
        )
        .await;
    }

    initial.disconnect();
    let persisted = wait_for_persisted_kind_count(
        fixture.store.as_ref(),
        status.session_id,
        &target_ids,
        BrowserEventKind::ConsoleMessage,
        4,
    )
    .await;

    for target_id in &target_ids {
        let console = persisted
            .iter()
            .filter(|event| {
                event.target_id() == *target_id && event.kind() == BrowserEventKind::ConsoleMessage
            })
            .collect::<Vec<_>>();
        assert_eq!(console.len(), 2);
        assert_eq!(console[0].attachment_generation(), 1);
        assert_eq!(console[1].attachment_generation(), 2);
        assert!(console[0].ordinal() < console[1].ordinal());
        assert!(
            console
                .iter()
                .all(|event| event.session_id() == status.session_id)
        );
    }

    let serialized_rows = serde_json::to_string(&persisted).unwrap();
    for forbidden in [
        "fill-value-sentinel",
        "upload-path-sentinel",
        "console-secret",
        "/home/operator",
        "exception-secret",
        "stack-secret",
        "/Users/operator",
        "url-secret",
        "/private/path",
        "query-secret",
        "fragment-secret",
        "raw-request-sentinel",
        "auth-sentinel",
        "cookie-sentinel",
        "body-sentinel",
        "response-body-sentinel",
        "dialog-message-sentinel",
        "dialog-default-sentinel",
        "dialog-input-sentinel",
    ] {
        assert!(
            !serialized_rows.contains(forbidden),
            "persisted browser-event row leaked {forbidden}",
        );
    }

    let target_id = target_ids[0];
    let target_events = persisted
        .iter()
        .filter(|event| event.target_id() == target_id)
        .collect::<Vec<_>>();
    let end = target_events
        .iter()
        .map(|event| event.session_time())
        .max()
        .unwrap();
    let first_frame = append_frame(
        fixture.store.as_ref(),
        status.session_id,
        target_id,
        50_001,
        1,
        SessionTime::ZERO,
    )
    .await;
    let last_frame = append_frame(
        fixture.store.as_ref(),
        status.session_id,
        target_id,
        50_002,
        2,
        end,
    )
    .await;
    let range = SessionRange::new(SessionTime::ZERO, end).unwrap();
    let resolved = ResolvedRange::new(
        status.session_id,
        target_id,
        TemporalRangeAnchorKind::SessionTime,
        range,
        range,
        vec![first_frame, last_frame],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        RangeResolutionOptions {
            retention: RetentionPolicy::RequireComplete,
            capture_gaps: CaptureGapPolicy::Include,
            ..RangeResolutionOptions::DEFAULT
        },
    )
    .unwrap();
    let context = fixture
        .store
        .context(
            TemporalContextRequest::new(
                resolved,
                None,
                BrowserEventFilter::new(vec![], BrowserEventSeverity::Debug).unwrap(),
                BrowserEventSelection::chronological(100, None).unwrap(),
                vec![],
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let stored_console = target_events
        .iter()
        .filter(|event| event.kind() == BrowserEventKind::ConsoleMessage)
        .map(|event| (event.id(), event.session_time(), event.ordinal()))
        .collect::<Vec<_>>();
    let context_console = context
        .browser_events
        .events
        .iter()
        .filter(|selected| selected.event.kind() == BrowserEventKind::ConsoleMessage)
        .map(|selected| {
            (
                selected.event.id(),
                selected.event.session_time(),
                selected.event.ordinal(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(context_console, stored_console);
    assert_eq!(context.capture_quality.frame_count, 2);
    assert_eq!(context.browser_events.effective_range, range);

    session.stop().await.unwrap();
    drop(session);
    fixture.cleanup();
}
