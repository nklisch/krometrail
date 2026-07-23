#![cfg(feature = "cdpkit-transport")]

mod support;

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, AtomicUsize, Ordering},
};
use std::time::{Duration, SystemTime};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use krometrail_cdp::{
    CaptureConfig, CdpTransport, CdpTransportFactory, CommandScope, NamedEvent,
    ProductionBrowserConnector, ReconnectPolicy, SupervisorConfig, TransportError, TransportEvents,
    TransportFuture,
};
use krometrail_core::{
    AttachBrowser, BrowserConnectRequest, BrowserConnector, BrowserSessionEvent,
    BrowserSessionState, ByteOffset, CapabilityId, CaptureGapReason, DiskBudgetBytes, EncodedFrame,
    EveryNthFrame, FrameAddress, IdSource, IdValue, ImageFormat, MonotonicClock, ObservedTime,
    PageTarget, PortFuture, RecordingCatalog, RecordingSession, RecordingSink, SegmentId,
    SessionId, TargetId, TargetLifecycle, WallClock,
};
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Debug, Default)]
struct CaptureTestSink;

impl RecordingSink for CaptureTestSink {
    fn append_frame(
        &self,
        _frame: EncodedFrame,
    ) -> PortFuture<'_, krometrail_core::Result<FrameAddress>> {
        Box::pin(std::future::ready(Ok(FrameAddress::new(
            SegmentId::from_uuid(Uuid::from_u128(1)),
            ByteOffset::new(1),
        ))))
    }

    fn append_gap(
        &self,
        _gap: krometrail_core::CaptureGap,
    ) -> PortFuture<'_, krometrail_core::Result<()>> {
        Box::pin(std::future::ready(Ok(())))
    }

    fn flush(&self, _session_id: SessionId) -> PortFuture<'_, krometrail_core::Result<()>> {
        Box::pin(std::future::ready(Ok(())))
    }
}

#[derive(Debug, Default)]
struct CaptureTestClock;

impl MonotonicClock for CaptureTestClock {
    fn now(&self) -> ObservedTime {
        ObservedTime::from_nanos(0)
    }
}

#[derive(Debug, Default)]
struct CaptureTestIds {
    next: AtomicU64,
}

#[derive(Clone, Debug)]
struct SessionCatalogProbe {
    records: Arc<Mutex<Vec<RecordingSession>>>,
    fail_put: bool,
    fail_terminal_put: bool,
}

impl SessionCatalogProbe {
    fn new(fail_put: bool) -> Self {
        Self {
            records: Arc::new(Mutex::new(Vec::new())),
            fail_put,
            fail_terminal_put: false,
        }
    }

    fn failing_terminal_write() -> Self {
        Self {
            records: Arc::new(Mutex::new(Vec::new())),
            fail_put: false,
            fail_terminal_put: true,
        }
    }
}

impl RecordingCatalog for SessionCatalogProbe {
    fn put_session(
        &self,
        session: RecordingSession,
    ) -> PortFuture<'_, krometrail_core::Result<()>> {
        if self.fail_put
            || (self.fail_terminal_put
                && session.lifecycle() == krometrail_core::SessionLifecycle::Ended)
        {
            return Box::pin(std::future::ready(Err(
                krometrail_core::KrometrailError::new(
                    krometrail_core::ErrorCode::PersistenceFailed,
                    krometrail_core::NonEmptyText::new("catalog write failed").unwrap(),
                ),
            )));
        }
        self.records.lock().unwrap().push(session);
        Box::pin(std::future::ready(Ok(())))
    }

    fn note_terminal_session(&self, session: RecordingSession) {
        let mut records = self.records.lock().unwrap();
        if let Some(existing) = records
            .iter_mut()
            .find(|record| record.id() == session.id())
        {
            *existing = session;
        } else {
            records.push(session);
        }
    }

    fn put_target(
        &self,
        _session_id: SessionId,
        _target: PageTarget,
    ) -> PortFuture<'_, krometrail_core::Result<()>> {
        Box::pin(std::future::ready(Ok(())))
    }

    fn session(
        &self,
        session_id: SessionId,
    ) -> PortFuture<'_, krometrail_core::Result<Option<RecordingSession>>> {
        let record = self
            .records
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|record| record.id() == session_id)
            .cloned();
        Box::pin(std::future::ready(Ok(record)))
    }

    fn target(
        &self,
        _session_id: SessionId,
        _target_id: TargetId,
    ) -> PortFuture<'_, krometrail_core::Result<Option<PageTarget>>> {
        Box::pin(std::future::ready(Ok(None)))
    }
}

#[derive(Debug)]
struct FixedWallClock(SystemTime);

impl WallClock for FixedWallClock {
    fn now(&self) -> SystemTime {
        self.0
    }
}

impl IdSource for CaptureTestIds {
    fn next(&self) -> IdValue {
        IdValue::from_uuid(Uuid::from_u128(
            self.next.fetch_add(1, Ordering::Relaxed).saturating_add(1) as u128,
        ))
    }
}

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
async fn connect_persists_recording_session_and_shutdown_ends_it() {
    let initial = Arc::new(support::scripted_cdp::ScriptedCdp::chrome());
    initial.hold_events_open();
    initial.push_response(
        "Target.attachToTarget",
        json!({"sessionId": "probe-session"}),
    );
    initial.push_response("Target.attachToTarget", json!({"sessionId": "session-a"}));
    let factory = Arc::new(support::scripted_cdp::ScriptedCdpFactory::new([
        Arc::clone(&initial),
    ]));
    let catalog = SessionCatalogProbe::new(false);
    let stride = EveryNthFrame::new(3).unwrap();
    let connector = ProductionBrowserConnector::new(
        Arc::new(krometrail_cdp::SystemChromeLauncher::new(
            krometrail_cdp::LauncherConfig::default(),
        )),
        factory,
    )
    .with_capture(
        Arc::new(CaptureTestClock),
        Arc::new(CaptureTestIds::default()),
        Arc::new(CaptureTestSink),
        Arc::new(support::retention::AlwaysAvailableRetention),
        CaptureConfig::default(),
    )
    .with_session_catalog(
        Arc::new(catalog.clone()),
        Arc::new(FixedWallClock(
            SystemTime::UNIX_EPOCH + Duration::from_secs(1),
        )),
        DiskBudgetBytes::new(1024).unwrap(),
        vec![CapabilityId::Control],
    )
    .with_config(SupervisorConfig {
        reconnect: ReconnectPolicy {
            delays: vec![Duration::ZERO].into_boxed_slice(),
            attempt_timeout: Duration::from_secs(1),
        },
        subscriber_capacity: 64,
        reconnect_target_limit: 8,
        reconnect_attach_concurrency: 2,
    });
    let session = connector
        .connect(BrowserConnectRequest::Attach(
            AttachBrowser::new("ws://127.0.0.1:9222/devtools/browser/fake")
                .unwrap()
                .with_every_nth_frame(stride),
        ))
        .await
        .unwrap();
    let records = catalog.records.lock().unwrap().clone();
    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0].lifecycle(),
        krometrail_core::SessionLifecycle::Recording
    );
    assert_eq!(records[0].origin(), ObservedTime::from_nanos(0));
    assert_eq!(records[0].every_nth_frame(), stride);
    assert!(records[0].ended_at().is_none());

    session.stop().await.unwrap();
    let records = catalog.records.lock().unwrap().clone();
    assert_eq!(records.len(), 2);
    let ended = records.last().unwrap();
    assert_eq!(ended.id(), records[0].id());
    assert_eq!(ended.lifecycle(), krometrail_core::SessionLifecycle::Ended);
    assert!(ended.ended_at().unwrap() >= ended.started_at());
}

#[tokio::test]
async fn terminal_catalog_write_failure_keeps_readers_fail_closed() {
    let initial = Arc::new(support::scripted_cdp::ScriptedCdp::chrome());
    initial.hold_events_open();
    initial.push_response(
        "Target.attachToTarget",
        json!({"sessionId": "probe-session"}),
    );
    initial.push_response("Target.attachToTarget", json!({"sessionId": "session-a"}));
    let factory = Arc::new(support::scripted_cdp::ScriptedCdpFactory::new([
        Arc::clone(&initial),
    ]));
    let catalog = SessionCatalogProbe::failing_terminal_write();
    let connector = ProductionBrowserConnector::new(
        Arc::new(krometrail_cdp::SystemChromeLauncher::new(
            krometrail_cdp::LauncherConfig::default(),
        )),
        factory,
    )
    .with_session_catalog(
        Arc::new(catalog.clone()),
        Arc::new(FixedWallClock(
            SystemTime::UNIX_EPOCH + Duration::from_secs(1),
        )),
        DiskBudgetBytes::new(1024).unwrap(),
        vec![CapabilityId::Control],
    );
    let session = connector
        .connect(BrowserConnectRequest::Attach(
            AttachBrowser::new("ws://127.0.0.1:9222/devtools/browser/fake").unwrap(),
        ))
        .await
        .unwrap();

    session.stop().await.unwrap();
    let records = catalog.records.lock().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0].lifecycle(),
        krometrail_core::SessionLifecycle::Ended
    );
}

#[tokio::test]
async fn connect_session_catalog_failure_fails_connect() {
    let initial = Arc::new(support::scripted_cdp::ScriptedCdp::chrome());
    initial.hold_events_open();
    initial.push_response(
        "Target.attachToTarget",
        json!({"sessionId": "probe-session"}),
    );
    initial.push_response("Target.attachToTarget", json!({"sessionId": "session-a"}));
    let factory = Arc::new(support::scripted_cdp::ScriptedCdpFactory::new([
        Arc::clone(&initial),
    ]));
    let connector = ProductionBrowserConnector::new(
        Arc::new(krometrail_cdp::SystemChromeLauncher::new(
            krometrail_cdp::LauncherConfig::default(),
        )),
        factory,
    )
    .with_session_catalog(
        Arc::new(SessionCatalogProbe::new(true)),
        Arc::new(FixedWallClock(SystemTime::UNIX_EPOCH)),
        DiskBudgetBytes::new(1024).unwrap(),
        vec![CapabilityId::Control],
    );
    let error = match connector
        .connect(BrowserConnectRequest::Attach(
            AttachBrowser::new("ws://127.0.0.1:9222/devtools/browser/fake").unwrap(),
        ))
        .await
    {
        Ok(_) => panic!("a failed catalog write must fail browser connect"),
        Err(error) => error,
    };
    assert_eq!(error.code, krometrail_core::ErrorCode::PersistenceFailed);
}

#[tokio::test]
async fn closed_capture_frame_stream_reconnects_and_restores_capture_generation() {
    let initial = Arc::new(support::scripted_cdp::ScriptedCdp::chrome());
    initial.hold_events_open();
    initial.push_response(
        "Target.attachToTarget",
        json!({"sessionId": "probe-session"}),
    );
    initial.push_response("Target.attachToTarget", json!({"sessionId": "session-a"}));

    let reconnected = Arc::new(support::scripted_cdp::ScriptedCdp::chrome());
    reconnected.hold_events_open();
    let targets = json!({"targetInfos": [{
        "targetId": "target-a",
        "type": "page",
        "url": "http://fixture/",
        "title": "fixture"
    }]});
    reconnected.push_response("Target.getTargets", targets.clone());
    reconnected.push_response("Target.getTargets", targets);
    reconnected.push_response(
        "Target.attachToTarget",
        json!({"sessionId": "probe-session-reconnected"}),
    );
    reconnected.push_response(
        "Target.attachToTarget",
        json!({"sessionId": "session-a-reconnected"}),
    );

    let factory = Arc::new(support::scripted_cdp::ScriptedCdpFactory::new([
        Arc::clone(&initial),
        Arc::clone(&reconnected),
    ]));
    let connector = ProductionBrowserConnector::new(
        Arc::new(krometrail_cdp::SystemChromeLauncher::new(
            krometrail_cdp::LauncherConfig::default(),
        )),
        factory,
    )
    .with_capture(
        Arc::new(CaptureTestClock),
        Arc::new(CaptureTestIds::default()),
        Arc::new(CaptureTestSink),
        Arc::new(support::retention::AlwaysAvailableRetention),
        CaptureConfig::default(),
    )
    .with_config(SupervisorConfig {
        reconnect: ReconnectPolicy {
            delays: vec![Duration::ZERO].into_boxed_slice(),
            attempt_timeout: Duration::from_secs(1),
        },
        subscriber_capacity: 64,
        reconnect_target_limit: 8,
        reconnect_attach_concurrency: 2,
    });
    let session = connector
        .connect(BrowserConnectRequest::Attach(
            AttachBrowser::new("ws://127.0.0.1:9222/devtools/browser/fake").unwrap(),
        ))
        .await
        .unwrap();
    initial
        .wait_for_command_count("Page.startScreencast", 1)
        .await;

    initial.close_event_stream("Page.screencastFrame", Some("session-a"));

    let status = tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let status = session.status().await.unwrap();
            if status.capture.len() == 1
                && status.capture[0].attachment_generation() == 2
                && status.capture[0].state() == krometrail_core::CaptureStreamState::Capturing
            {
                break status;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("capture stream closure must restore capture on a new generation");
    assert_eq!(status.capture.len(), 1);
    assert_eq!(status.capture[0].attachment_generation(), 2);
    assert_eq!(
        status.capture[0].state(),
        krometrail_core::CaptureStreamState::Capturing
    );
    let outcome = session.stop().await.unwrap();
    assert_eq!(outcome.closure(), krometrail_core::BrowserClosure::Detached);
    assert_eq!(outcome.quality(), krometrail_core::ShutdownQuality::Clean);
}

#[tokio::test]
async fn failed_capture_acknowledgement_reconnects_without_retrying_the_token() {
    let initial = Arc::new(support::scripted_cdp::ScriptedCdp::chrome());
    initial.hold_events_open();
    initial.push_response(
        "Target.attachToTarget",
        json!({"sessionId": "probe-session"}),
    );
    initial.push_response("Target.attachToTarget", json!({"sessionId": "session-a"}));
    initial.push_failure("Page.screencastFrameAck", TransportError::CommandFailed);

    let reconnected = Arc::new(support::scripted_cdp::ScriptedCdp::chrome());
    reconnected.hold_events_open();
    let targets = json!({"targetInfos": [{
        "targetId": "target-a",
        "type": "page",
        "url": "http://fixture/",
        "title": "fixture"
    }]});
    reconnected.push_response("Target.getTargets", targets.clone());
    reconnected.push_response("Target.getTargets", targets);
    reconnected.push_response(
        "Target.attachToTarget",
        json!({"sessionId": "probe-session-reconnected"}),
    );
    reconnected.push_response(
        "Target.attachToTarget",
        json!({"sessionId": "session-a-reconnected"}),
    );

    let factory = Arc::new(support::scripted_cdp::ScriptedCdpFactory::new([
        Arc::clone(&initial),
        Arc::clone(&reconnected),
    ]));
    let connector = ProductionBrowserConnector::new(
        Arc::new(krometrail_cdp::SystemChromeLauncher::new(
            krometrail_cdp::LauncherConfig::default(),
        )),
        factory,
    )
    .with_capture(
        Arc::new(CaptureTestClock),
        Arc::new(CaptureTestIds::default()),
        Arc::new(CaptureTestSink),
        Arc::new(support::retention::AlwaysAvailableRetention),
        CaptureConfig::default(),
    )
    .with_config(SupervisorConfig {
        reconnect: ReconnectPolicy {
            delays: vec![Duration::ZERO].into_boxed_slice(),
            attempt_timeout: Duration::from_secs(1),
        },
        subscriber_capacity: 64,
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
    initial
        .wait_for_command_count("Page.startScreencast", 1)
        .await;

    initial.push_scoped_event(
        "Page.screencastFrame",
        Some("session-a"),
        json!({"sessionId": 7, "data": "unused", "metadata": {}}),
    );
    reconnected
        .wait_for_command_count("Page.startScreencast", 1)
        .await;
    assert_eq!(
        initial
            .command_calls()
            .iter()
            .filter(|call| call.method == "Page.screencastFrameAck")
            .count(),
        1
    );
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if let Some(BrowserSessionEvent::CaptureGapDeclared { gap }) =
                events.next().await.unwrap()
                && gap.reason() == &CaptureGapReason::AcknowledgementFailed
            {
                break;
            }
        }
    })
    .await
    .expect("the original acknowledgement gap remains observable during recovery");

    let jpeg = [0xff, 0xd8, 0xff, 0xc0, 0, 8, 8, 0, 2, 0, 2, 1, 0xff, 0xd9];
    reconnected.push_scoped_event(
        "Page.screencastFrame",
        Some("session-a-reconnected"),
        json!({
            "sessionId": 8,
            "data": STANDARD.encode(jpeg),
            "metadata": {
                "deviceWidth": 800,
                "deviceHeight": 600,
                "pageScaleFactor": 1.0,
                "timestamp": 1.0
            }
        }),
    );
    let status = tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let status = session.status().await.unwrap();
            if status.capture.len() == 1
                && status.capture[0].attachment_generation() == 2
                && status.capture[0].statistics().persisted_frames() == 1
            {
                break status;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("replacement capture generation persists a later frame");
    assert_eq!(
        status.capture[0].state(),
        krometrail_core::CaptureStreamState::Capturing
    );
    assert_eq!(
        reconnected
            .command_calls()
            .iter()
            .filter(|call| call.method == "Page.screencastFrameAck")
            .count(),
        1
    );
    let outcome = session.stop().await.unwrap();
    assert_eq!(outcome.closure(), krometrail_core::BrowserClosure::Detached);
}

#[tokio::test]
async fn scripted_capture_preserves_stride_for_jpeg_png_dynamic_and_reconnect_generations() {
    for (format, jpeg_quality) in [(ImageFormat::Jpeg, Some(80)), (ImageFormat::Png, None)] {
        let stride = EveryNthFrame::new(7).unwrap();
        let initial = Arc::new(support::scripted_cdp::ScriptedCdp::chrome());
        initial.hold_events_open();
        initial.push_response(
            "Target.attachToTarget",
            json!({"sessionId": "probe-session"}),
        );
        initial.push_response("Target.attachToTarget", json!({"sessionId": "session-a"}));
        initial.push_response("Target.attachToTarget", json!({"sessionId": "session-b"}));

        let reconnected = Arc::new(support::scripted_cdp::ScriptedCdp::chrome());
        reconnected.hold_events_open();
        let reconnected_targets = json!({"targetInfos": [
            {"targetId": "target-a", "type": "page", "url": "http://fixture/", "title": "fixture"},
            {"targetId": "target-b", "type": "page", "url": "http://dynamic/", "title": "dynamic"}
        ]});
        // Compatibility probing and connection setup each take a target snapshot.
        reconnected.push_response("Target.getTargets", reconnected_targets.clone());
        reconnected.push_response("Target.getTargets", reconnected_targets);
        reconnected.push_response(
            "Target.attachToTarget",
            json!({"sessionId": "probe-session-reconnected"}),
        );
        reconnected.push_response(
            "Target.attachToTarget",
            json!({"sessionId": "session-a-reconnected"}),
        );
        reconnected.push_response(
            "Target.attachToTarget",
            json!({"sessionId": "session-b-reconnected"}),
        );
        let factory = Arc::new(support::scripted_cdp::ScriptedCdpFactory::new([
            Arc::clone(&initial),
            Arc::clone(&reconnected),
        ]));
        let connector = ProductionBrowserConnector::new(
            Arc::new(krometrail_cdp::SystemChromeLauncher::new(
                krometrail_cdp::LauncherConfig::default(),
            )),
            factory,
        )
        .with_capture(
            Arc::new(CaptureTestClock),
            Arc::new(CaptureTestIds::default()),
            Arc::new(CaptureTestSink),
            Arc::new(support::retention::AlwaysAvailableRetention),
            CaptureConfig {
                format,
                jpeg_quality,
                ..CaptureConfig::default()
            },
        )
        .with_config(SupervisorConfig {
            reconnect: ReconnectPolicy {
                delays: vec![Duration::ZERO].into_boxed_slice(),
                attempt_timeout: Duration::from_secs(1),
            },
            subscriber_capacity: 64,
            reconnect_target_limit: 8,
            reconnect_attach_concurrency: 2,
        });
        let session = connector
            .connect(BrowserConnectRequest::Attach(
                AttachBrowser::new("ws://127.0.0.1:9222/devtools/browser/fake")
                    .unwrap()
                    .with_every_nth_frame(stride),
            ))
            .await
            .unwrap();
        let mut events = session.subscribe().await.unwrap();
        initial
            .wait_for_command_count("Page.startScreencast", 1)
            .await;
        let initial_status = session.status().await.unwrap();
        assert_eq!(initial_status.every_nth_frame, stride);
        assert_eq!(initial_status.capture.len(), 1);
        assert_eq!(initial_status.capture[0].every_nth_frame(), stride);
        assert_start_commands_are_subscribed(&initial);
        let expected_start = if format == ImageFormat::Jpeg {
            json!({"format": "jpeg", "quality": 80, "everyNthFrame": 7})
        } else {
            json!({"format": "png", "everyNthFrame": 7})
        };
        assert_eq!(start_params(&initial), vec![expected_start]);

        initial.push_event(
            "Target.targetCreated",
            json!({"targetInfo": {
                "targetId": "target-b", "type": "page", "url": "http://dynamic/", "title": "dynamic"
            }}),
        );
        initial
            .wait_for_command_count("Page.startScreencast", 2)
            .await;
        let dynamic_status = session.status().await.unwrap();
        assert_eq!(dynamic_status.capture.len(), 2);
        assert!(
            dynamic_status
                .capture
                .iter()
                .all(|status| status.every_nth_frame() == stride)
        );
        assert_start_commands_are_subscribed(&initial);

        let initial_target_id = dynamic_status
            .pages
            .iter()
            .find(|page| page.target.target.browser_target_key() == "target-a")
            .expect("initial target")
            .target
            .target
            .id();
        initial.disconnect();
        reconnected
            .wait_for_command_count("Page.startScreencast", 2)
            .await;
        let restored_status = session.status().await.unwrap();
        assert_eq!(restored_status.every_nth_frame, stride);
        assert_eq!(restored_status.capture.len(), 2);
        assert!(
            restored_status
                .capture
                .iter()
                .all(|status| status.every_nth_frame() == stride)
        );
        let restored_target = restored_status
            .pages
            .iter()
            .find(|page| page.target.target.browser_target_key() == "target-a")
            .expect("restored initial target");
        assert_eq!(restored_target.target.target.id(), initial_target_id);
        assert_eq!(restored_target.target.attachment_generation, 2);
        assert_start_commands_are_subscribed(&reconnected);
        assert!(
            start_params(&reconnected)
                .iter()
                .all(|params| params["everyNthFrame"] == 7)
        );

        let mut saw_capture_status = false;
        let mut saw_disconnect_gap = false;
        tokio::time::timeout(Duration::from_secs(1), async {
            while !(saw_capture_status && saw_disconnect_gap) {
                match events.next().await.unwrap() {
                    Some(BrowserSessionEvent::CaptureStateChanged { status }) => {
                        saw_capture_status |= status.every_nth_frame() == stride;
                    }
                    Some(BrowserSessionEvent::CaptureGapDeclared { gap }) => {
                        saw_disconnect_gap |= gap.reason()
                            == &CaptureGapReason::BrowserDisconnected
                            && gap.estimated_missing_frames().is_none();
                    }
                    Some(_) => {}
                    None => break,
                }
            }
        })
        .await
        .expect("capture status and disconnect gap events are scripted");
        assert!(saw_capture_status);
        assert!(saw_disconnect_gap);
        let outcome = session.stop().await.unwrap();
        assert_eq!(outcome.closure(), krometrail_core::BrowserClosure::Detached);
        assert_eq!(outcome.quality(), krometrail_core::ShutdownQuality::Clean);
    }
}

fn start_params(script: &support::scripted_cdp::ScriptedCdp) -> Vec<Value> {
    script
        .command_calls()
        .into_iter()
        .filter(|call| call.method == "Page.startScreencast")
        .map(|call| call.params)
        .collect()
}

fn assert_start_commands_are_subscribed(script: &support::scripted_cdp::ScriptedCdp) {
    let activity = script.activity();
    let mut starts = 0;
    for (index, item) in activity.iter().enumerate() {
        if matches!(
            item,
            support::scripted_cdp::ScriptedActivity::Command { method, .. }
                if method == "Page.startScreencast"
        ) {
            starts += 1;
            let session = match item {
                support::scripted_cdp::ScriptedActivity::Command { session, .. } => session,
                _ => unreachable!("matched command"),
            };
            for method in [
                "Page.screencastFrame",
                "Page.frameResized",
                "Page.frameNavigated",
                "Page.navigatedWithinDocument",
            ] {
                let subscribed = activity[..index].iter().any(|item| {
                    matches!(
                        item,
                        support::scripted_cdp::ScriptedActivity::Subscription {
                            method: subscribed_method,
                            session: subscribed_session,
                        } if subscribed_method == method && subscribed_session == session
                    )
                });
                assert!(
                    subscribed,
                    "start screencast must follow generation-scoped {method} subscription"
                );
            }
            let observed_geometry = activity[..index].iter().any(|item| {
                matches!(
                    item,
                    support::scripted_cdp::ScriptedActivity::Command {
                        method,
                        session: observed_session,
                    } if method == "Page.getLayoutMetrics" && observed_session == session
                )
            });
            assert!(
                observed_geometry,
                "start screencast must follow authoritative geometry observation"
            );
        }
    }
    assert!(starts > 0);
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
    let stride = EveryNthFrame::new(23).unwrap();
    let session = connector
        .connect(BrowserConnectRequest::Attach(
            AttachBrowser::new("ws://127.0.0.1:9222/devtools/browser/fake")
                .unwrap()
                .with_every_nth_frame(stride),
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
    assert_eq!(session.status().await.unwrap().pages.len(), 1);
    assert_eq!(session.status().await.unwrap().every_nth_frame, stride);
    let outcome = session.stop().await.unwrap();
    assert_eq!(outcome.closure(), krometrail_core::BrowserClosure::Detached);
    assert_eq!(outcome.quality(), krometrail_core::ShutdownQuality::Clean);

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
        every_nth_frame: krometrail_core::EveryNthFrame::default(),
        focus: krometrail_core::BrowserFocusPolicy::default(),
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
        .status()
        .await
        .unwrap()
        .pages
        .into_iter()
        .map(|page| page.target)
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
        .status()
        .await
        .unwrap()
        .pages
        .into_iter()
        .map(|page| page.target)
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
            {
                if target.target.browser_target_key() == created_key
                    && target.lifecycle == TargetLifecycle::Attached
                {
                    break target;
                }
            }
        }
    })
    .await
    .expect("production target discovery should be bounded");
    assert_eq!(created.target.browser_target_key(), created_key);
    // A late event from the severed generation must not undo the restored exact-key state while
    // the new connection is processing this target event.
    let restored_after_post_event = session
        .status()
        .await
        .unwrap()
        .pages
        .into_iter()
        .map(|page| page.target)
        .find(|target| target.target.browser_target_key() == initial_key)
        .expect("restored target should survive post-rebuild events");
    assert_eq!(restored_after_post_event.target.id(), initial_target_id);
    assert_eq!(
        restored_after_post_event.attachment_generation,
        restored_generation
    );

    drop(created_events);
    drop(post_rebuild);
    let outcome = session.stop().await.unwrap();
    assert_eq!(outcome.closure(), krometrail_core::BrowserClosure::Detached);
    assert_eq!(outcome.quality(), krometrail_core::ShutdownQuality::Clean);
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
