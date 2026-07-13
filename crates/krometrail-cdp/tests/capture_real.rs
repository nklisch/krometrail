#![cfg(feature = "cdpkit-transport")]

mod support;

use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{Read, Write},
    net::TcpListener,
    num::NonZeroUsize,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use krometrail_cdp::{
    CaptureConfig, CdpTransport, CdpTransportFactory, CommandScope, LauncherConfig,
    ProductionBrowserConnector, SystemChromeLauncher,
};
use krometrail_core::{
    AttachBrowser, BrowserConnectRequest, BrowserConnector, BrowserSessionEvent,
    BrowserSessionEvents, BrowserSessionPort, BrowserSessionState, BrowserStopOutcome,
    CaptureGapReason, CaptureStreamState, EncodedFrame, ErrorCode, IdSource, IdValue, ImageFormat,
    LaunchBrowser, ManagedProfile, MonotonicClock, ObservedTime, PortFuture, RecordingSink,
    SessionId, TargetCaptureStatus, TargetId,
};
use uuid::Uuid;

const CAPTURE_TIMEOUT: Duration = Duration::from_secs(45);
const STOP_TIMEOUT: Duration = Duration::from_secs(12);
const PROBE_TIMEOUT: Duration = Duration::from_secs(8);

#[tokio::test]
async fn opt_in_real_chrome_capture_records_fidelity_and_managed_cleanup() {
    if !real_chrome_test_available() {
        return;
    }
    let _browser_lock = support::chrome::real_browser_lock().await;
    let fixture = FixtureServer::start();
    let sink = Arc::new(TestSink::new(false));
    let clock = Arc::new(TestClock::new());
    let ids = Arc::new(TestIds::new());
    let (session, root, mut launched) = connect_managed(
        "managed",
        fixture.url(),
        &sink,
        &clock,
        &ids,
        CaptureConfig::default(),
    )
    .await;

    assert_eq!(session.state(), BrowserSessionState::Ready);
    let origin = session.session_origin().observed().as_nanos();
    let target_id = first_target(&session).await;
    let _ = sink.wait_for_completed_frames(30, CAPTURE_TIMEOUT).await;

    let frames = sink.frames();
    assert!(
        frames.len() >= 30,
        "real Chrome yielded fewer than 30 frames"
    );
    assert!(
        frames
            .iter()
            .all(|frame| frame.metadata().target_id() == target_id)
    );
    assert_frame_fidelity(&frames, origin, session.session_id());
    assert_strict_sequence(&frames);

    let status = status_for(&session, target_id).await;
    assert_capture_diagnostics(&status, 30);
    eprintln!(
        "real capture diagnostics: frames={} ack_samples={} cadence_samples={} ack_p50={:?} ack_p95={:?} ack_p99={:?} ack_max={:?}",
        frames.len(),
        status.ack_latency().sample_count(),
        status.frame_cadence().sample_count(),
        status.ack_latency().p50_nanos(),
        status.ack_latency().p95_nanos(),
        status.ack_latency().p99_nanos(),
        status.ack_latency().max_nanos(),
    );

    let outcome = tokio::time::timeout(STOP_TIMEOUT, session.stop())
        .await
        .expect("managed capture stop must be bounded")
        .expect("managed capture stop");
    assert_eq!(outcome, BrowserStopOutcome::Detached);
    assert_eq!(sink.flush_count(), 1);
    assert!(
        sink.gaps()
            .iter()
            .all(|gap| gap.session_id() == session.session_id())
    );

    let stopped = status_for(&session, target_id).await;
    assert_eq!(stopped.state(), CaptureStreamState::Stopped);
    drop(session);
    launched.shutdown().await.expect("owned browser cleanup");
    assert_profile_unreferenced(root.path());
    drop(root);
}

#[tokio::test]
async fn opt_in_real_chrome_capture_isolates_two_targets_and_records_visibility_when_available() {
    if !real_chrome_test_available() {
        return;
    }
    let _browser_lock = support::chrome::real_browser_lock().await;
    let fixture = FixtureServer::start();
    let root = support::chrome::temporary_profile_root("targets");
    let root_path = root.path().to_path_buf();
    let launcher = SystemChromeLauncher::new(LauncherConfig {
        profile_root: root_path.clone(),
        startup_timeout: CAPTURE_TIMEOUT,
        shutdown_timeout: Duration::from_secs(4),
    });
    let chrome_wrapper = ChromeWrapper::new();
    let request = LaunchBrowser {
        executable: chrome_wrapper.as_ref().map(|wrapper| wrapper.path.clone()),
        profile: ManagedProfile::Temporary,
        initial_url: Some(fixture.url().to_owned()),
    };
    let mut launched = launcher
        .launch_owned(&request)
        .await
        .expect("managed Chrome launch");
    let factory = krometrail_cdp::transport::CdpkitTransportFactory::new()
        .with_command_timeout(Duration::from_secs(4));
    let sink = Arc::new(TestSink::new(false));
    let clock = Arc::new(TestClock::new());
    let ids = Arc::new(TestIds::new());
    let connector = ProductionBrowserConnector::new(Arc::new(launcher), Arc::new(factory.clone()))
        .with_capture(
            clock,
            ids,
            sink.clone(),
            capture_config(4, Duration::from_secs(5)),
        );
    let session = connector
        .connect(BrowserConnectRequest::Attach(
            AttachBrowser::new(launched.endpoint.browser_websocket_url().as_str())
                .expect("validated browser endpoint"),
        ))
        .await
        .expect("attached capture session");
    assert_eq!(session.state(), BrowserSessionState::Ready);
    let mut events = session
        .subscribe()
        .await
        .expect("capture event subscription");
    let control = factory
        .connect(launched.endpoint.browser_websocket_url().as_str())
        .await
        .expect("visibility control transport");
    let extra_keys = [
        create_target(control.as_ref(), fixture.url()).await,
        create_target(control.as_ref(), fixture.url()).await,
    ];
    wait_for_session_targets(&session, &extra_keys).await;

    let targets = session.targets().await.expect("target snapshot");
    let expected: HashMap<_, _> = targets
        .iter()
        .filter(|target| {
            !matches!(
                target.lifecycle,
                krometrail_core::TargetLifecycle::Closed | krometrail_core::TargetLifecycle::Failed
            )
        })
        .map(|target| {
            (
                target.target.browser_target_key().to_owned(),
                target.target.id(),
            )
        })
        .collect();
    assert!(extra_keys.iter().all(|key| expected.contains_key(key)));
    assert!(
        expected.len() >= 3,
        "expected initial and two extra page targets"
    );
    let expected_ids: HashSet<_> = expected.values().copied().collect();

    // Activate each page through the browser scope. This both gives hidden pages a chance to
    // render and lets the test observe whether Chrome emits its optional visibility event.
    for key in expected.keys() {
        activate_target(control.as_ref(), key).await;
        let target_id = expected[key];
        let _ = sink
            .wait_for_target_frames(target_id, 5, CAPTURE_TIMEOUT)
            .await;
    }
    drop(control);

    let frames = sink.frames();
    assert!(expected_ids.iter().all(|target_id| {
        frames
            .iter()
            .any(|frame| frame.metadata().target_id() == *target_id)
    }));
    assert_frame_fidelity(
        &frames,
        session.session_origin().observed().as_nanos(),
        session.session_id(),
    );
    for target_id in &expected_ids {
        let target_frames: Vec<_> = frames
            .iter()
            .filter(|frame| frame.metadata().target_id() == *target_id)
            .cloned()
            .collect();
        assert_strict_sequence(&target_frames);
    }
    for gap in sink.gaps() {
        assert!(
            expected_ids.contains(&gap.target_id()),
            "gap crossed target identity"
        );
    }

    let visibility = wait_for_visibility_cycle(
        &session,
        &mut events,
        &expected_ids,
        &extra_keys,
        &factory,
        &launched,
    )
    .await;
    if visibility.hidden_event {
        assert!(
            visibility.hidden_gap,
            "hidden visibility did not open a TargetHidden gap"
        );
        let statuses = session.capture_statuses().await.expect("capture statuses");
        assert!(statuses.iter().any(|status| {
            expected_ids.contains(&status.target_id())
                && status.state() == CaptureStreamState::Capturing
        }));
    } else {
        eprintln!(
            "skipping visibility assertion; Chrome emitted no visibility event for this target setup"
        );
    }

    let outcome = tokio::time::timeout(STOP_TIMEOUT, session.stop())
        .await
        .expect("attached capture stop must be bounded")
        .expect("attached capture stop");
    assert_eq!(outcome, BrowserStopOutcome::Detached);

    let probe = factory
        .connect(launched.endpoint.browser_websocket_url().as_str())
        .await
        .expect("attached stop must leave external Chrome alive");
    probe
        .send_raw(
            &CommandScope::Browser,
            "Browser.getVersion",
            serde_json::json!({}),
        )
        .await
        .expect("external Chrome remains responsive");
    drop(probe);
    launched.shutdown().await.expect("owned browser cleanup");
    assert_profile_unreferenced(&root_path);
    drop(root);
}

#[tokio::test]
async fn opt_in_real_chrome_capture_bounds_saturation_and_reports_incomplete_blocked_stop() {
    if !real_chrome_test_available() {
        return;
    }
    let _browser_lock = support::chrome::real_browser_lock().await;
    let fixture = FixtureServer::start();
    let sink = Arc::new(TestSink::new(true));
    let clock = Arc::new(TestClock::new());
    let ids = Arc::new(TestIds::new());
    let config = CaptureConfig {
        max_active_streams: NonZeroUsize::new(1).expect("one stream"),
        queue_capacity: NonZeroUsize::new(1).expect("one queue slot"),
        shutdown_timeout: Duration::from_secs(2),
        ..CaptureConfig::default()
    };
    let (session, root, mut launched) =
        connect_managed("saturation", fixture.url(), &sink, &clock, &ids, config).await;
    let target_id = first_target(&session).await;
    let mut events = session
        .subscribe()
        .await
        .expect("capture event subscription");
    let baseline_calls = clock.calls();
    clock
        .wait_for_calls_at_least(baseline_calls + 12, CAPTURE_TIMEOUT)
        .await;

    let saturated_gap = wait_for_gap(
        &mut events,
        target_id,
        CaptureGapReason::IngestionQueueSaturated,
        PROBE_TIMEOUT,
    )
    .await
    .expect("saturation gap event");
    assert!(
        saturated_gap
            .estimated_missing_frames()
            .is_some_and(|count| count.get() > 0)
    );

    let status = status_for(&session, target_id).await;
    let statistics = status.statistics();
    assert!(statistics.received_frames() >= 3);
    assert_eq!(
        statistics.received_frames(),
        statistics.acknowledged_frames()
    );
    assert_eq!(
        statistics.acknowledged_frames(),
        statistics.accepted_frames() + statistics.dropped_frames()
    );
    assert!(statistics.dropped_frames() > 0);
    assert!(status.queue_depth() <= 1);
    assert_eq!(
        status.ack_latency().sample_count(),
        statistics.received_frames()
    );
    assert!(!sink.frames().is_empty());

    // The worker is intentionally still blocked. Stop must consume one aggregate budget, abandon
    // accepted work explicitly, emit CaptureStopped, and never wait forever for this sink.
    let stop_result = tokio::time::timeout(STOP_TIMEOUT, session.stop())
        .await
        .expect("blocked capture stop must be bounded");
    let error = stop_result.expect_err("blocked sink must make shutdown incomplete");
    assert_eq!(error.code, ErrorCode::ShutdownIncomplete);
    let stopped_gap = wait_for_gap(
        &mut events,
        target_id,
        CaptureGapReason::CaptureStopped,
        PROBE_TIMEOUT,
    )
    .await;
    assert!(
        stopped_gap.is_some(),
        "incomplete stop must publish CaptureStopped"
    );
    assert_eq!(sink.flush_count(), 1);

    drop(session);
    launched.shutdown().await.expect("owned browser cleanup");
    assert_profile_unreferenced(root.path());
    drop(root);
}

#[tokio::test]
async fn opt_in_real_chrome_capture_fences_one_disconnect_and_resets_generation_identity() {
    if !real_chrome_test_available() {
        return;
    }
    let _browser_lock = support::chrome::real_browser_lock().await;
    let fixture = FixtureServer::start();
    let root = support::chrome::temporary_profile_root("reconnect");
    let root_path = root.path().to_path_buf();
    let launcher = SystemChromeLauncher::new(LauncherConfig {
        profile_root: root_path.clone(),
        startup_timeout: CAPTURE_TIMEOUT,
        shutdown_timeout: Duration::from_secs(4),
    });
    let chrome_wrapper = ChromeWrapper::new();
    let request = LaunchBrowser {
        executable: chrome_wrapper.as_ref().map(|wrapper| wrapper.path.clone()),
        profile: ManagedProfile::Temporary,
        initial_url: Some(fixture.url().to_owned()),
    };
    let mut launched = launcher
        .launch_owned(&request)
        .await
        .expect("managed Chrome launch");
    let factory = krometrail_cdp::transport::CdpkitTransportFactory::new()
        .with_command_timeout(Duration::from_secs(4));
    let raw = factory
        .connect(launched.endpoint.browser_websocket_url().as_str())
        .await
        .expect("real Chrome activation transport");
    let page_key = first_browser_target(raw.as_ref()).await;
    activate_target(raw.as_ref(), &page_key).await;
    drop(raw);
    let proxy = support::cdp_proxy::CdpFaultProxy::start(&launched.endpoint)
        .await
        .expect("fault proxy");
    let sink = Arc::new(TestSink::new(false));
    let clock = Arc::new(TestClock::new());
    let ids = Arc::new(TestIds::new());
    let connector = ProductionBrowserConnector::new(Arc::new(launcher), Arc::new(factory))
        .with_capture(
            clock,
            ids,
            sink.clone(),
            capture_config(4, Duration::from_secs(5)),
        );
    let session = connector
        .connect(BrowserConnectRequest::Attach(
            AttachBrowser::new(proxy.http_endpoint()).expect("validated proxy endpoint"),
        ))
        .await
        .expect("proxy-backed capture session");
    let target_id = first_target(&session).await;
    let mut events = session
        .subscribe()
        .await
        .expect("capture event subscription");
    let _ = sink.wait_for_completed_frames(20, CAPTURE_TIMEOUT).await;
    let old_frames = sink.frames();
    let old_status = status_for(&session, target_id).await;
    let old_generation = old_status.attachment_generation();
    assert_strict_sequence(
        &old_frames
            .iter()
            .filter(|frame| frame.metadata().target_id() == target_id)
            .cloned()
            .collect::<Vec<_>>(),
    );

    assert!(
        proxy.sever_active_transport(),
        "fault proxy had no active transport"
    );
    let reconnect = wait_for_reconnect(
        &session,
        &mut events,
        target_id,
        old_generation,
        CAPTURE_TIMEOUT,
    )
    .await
    .expect("reconnect generation and browser-disconnected gap");
    assert!(reconnect.generation > old_generation);
    assert!(reconnect.browser_disconnected);
    let restored_count = sink.frames().len();
    let _ = sink
        .wait_for_completed_frames(restored_count + 8, CAPTURE_TIMEOUT)
        .await;
    let all_frames = sink.frames();
    let restored_frames: Vec<_> = all_frames
        .iter()
        .skip(restored_count)
        .filter(|frame| frame.metadata().target_id() == target_id)
        .cloned()
        .collect();
    assert!(
        restored_frames.len() >= 8,
        "restored generation yielded too few frames"
    );
    assert_strict_sequence(&restored_frames);
    assert!(
        all_frames
            .iter()
            .all(|frame| frame.metadata().target_id() == target_id)
    );

    let outcome = tokio::time::timeout(STOP_TIMEOUT, session.stop())
        .await
        .expect("reconnected capture stop must be bounded")
        .expect("reconnected capture stop");
    assert_eq!(outcome, BrowserStopOutcome::Detached);
    drop(session);
    let mut proxy = proxy;
    proxy.shutdown().await;
    launched.shutdown().await.expect("owned browser cleanup");
    assert_profile_unreferenced(&root_path);
    drop(root);
}

fn real_chrome_test_available() -> bool {
    if !support::chrome::real_browser_tests_enabled() {
        eprintln!("skipping real Chrome test; set KROMETRAIL_REAL_CHROME_TESTS=1");
        return false;
    }
    if krometrail_cdp::discover_installations(None).is_empty() {
        eprintln!("skipping real Chrome test; no supported Chrome or Chromium installation found");
        return false;
    }
    true
}

fn capture_config(queue_capacity: usize, shutdown_timeout: Duration) -> CaptureConfig {
    CaptureConfig {
        queue_capacity: NonZeroUsize::new(queue_capacity).expect("capture queue is non-zero"),
        shutdown_timeout,
        ..CaptureConfig::default()
    }
}

async fn connect_managed(
    name: &str,
    fixture_url: &str,
    sink: &Arc<TestSink>,
    clock: &Arc<TestClock>,
    ids: &Arc<TestIds>,
    config: CaptureConfig,
) -> (
    Arc<dyn BrowserSessionPort>,
    support::chrome::TemporaryRootGuard,
    krometrail_cdp::LaunchedChrome,
) {
    let root = support::chrome::temporary_profile_root(name);
    let launcher = SystemChromeLauncher::new(LauncherConfig {
        profile_root: root.path().to_owned(),
        startup_timeout: CAPTURE_TIMEOUT,
        shutdown_timeout: Duration::from_secs(4),
    });
    let chrome_wrapper = ChromeWrapper::new();
    let request = LaunchBrowser {
        executable: chrome_wrapper.as_ref().map(|wrapper| wrapper.path.clone()),
        profile: ManagedProfile::Temporary,
        initial_url: Some(fixture_url.to_owned()),
    };
    let launched = launcher
        .launch_owned(&request)
        .await
        .expect("managed Chrome launch");
    let factory = krometrail_cdp::transport::CdpkitTransportFactory::new()
        .with_command_timeout(Duration::from_secs(4));
    let raw = factory
        .connect(launched.endpoint.browser_websocket_url().as_str())
        .await
        .expect("real Chrome activation transport");
    let page_key = first_browser_target(raw.as_ref()).await;
    activate_target(raw.as_ref(), &page_key).await;
    drop(raw);

    // The production connector is still exercised end-to-end; the separate pre-activation keeps
    // the managed browser's first page visible in headless CI where an unactivated window only
    // produces its initial paint.
    let connector = ProductionBrowserConnector::new(Arc::new(launcher), Arc::new(factory))
        .with_capture(clock.clone(), ids.clone(), sink.clone(), config);
    let session = connector
        .connect(BrowserConnectRequest::Attach(
            AttachBrowser::new(launched.endpoint.browser_websocket_url().as_str())
                .expect("validated browser endpoint"),
        ))
        .await
        .expect("managed production capture session");
    (session, root, launched)
}

async fn first_target(session: &Arc<dyn BrowserSessionPort>) -> TargetId {
    session
        .targets()
        .await
        .expect("target snapshot")
        .into_iter()
        .find(|target| {
            target.lifecycle == krometrail_core::TargetLifecycle::Attached
                && target.target.url() != "about:blank"
        })
        .map(|target| target.target.id())
        .expect("attached page target")
}

async fn status_for(
    session: &Arc<dyn BrowserSessionPort>,
    target_id: TargetId,
) -> TargetCaptureStatus {
    session
        .capture_statuses()
        .await
        .expect("capture statuses")
        .into_iter()
        .find(|status| status.target_id() == target_id)
        .expect("target capture status")
}

fn assert_capture_diagnostics(status: &TargetCaptureStatus, minimum_samples: u64) {
    assert!(status.queue_depth() <= status.queue_capacity());
    assert!(status.ack_latency().sample_count() >= minimum_samples);
    assert!(status.frame_cadence().sample_count() > 0);
    for summary in [status.ack_latency(), status.frame_cadence()] {
        if summary.sample_count() > 0 {
            assert!(summary.p50_nanos() <= summary.p95_nanos());
            assert!(summary.p95_nanos() <= summary.p99_nanos());
            assert!(summary.p99_nanos() <= summary.max_nanos());
        }
    }
}

fn assert_frame_fidelity(frames: &[EncodedFrame], origin: u64, session_id: SessionId) {
    let mut frame_ids = HashSet::new();
    let mut observed = None;
    let mut session = None;
    let mut source: Option<krometrail_core::SourceTime> = None;
    for frame in frames {
        let metadata = frame.metadata();
        assert!(frame_ids.insert(metadata.id()), "duplicate FrameId");
        assert_eq!(metadata.session_id(), session_id);
        assert_eq!(metadata.format(), ImageFormat::Jpeg);
        assert!(!frame.bytes().is_empty(), "empty JPEG payload");
        let image = metadata.image();
        assert_eq!(
            jpeg_dimensions(frame.bytes()),
            Some((image.width(), image.height()))
        );
        assert!(metadata.viewport().width() > 0 && metadata.viewport().height() > 0);
        assert!(metadata.device_scale_factor().get().is_finite());
        assert!(metadata.device_scale_factor().get() > 0.0);
        assert!(metadata.session_time().as_nanos() <= metadata.observed_time().as_nanos());
        assert!(metadata.observed_time().as_nanos() >= origin);
        if let Some(previous) = observed {
            assert!(metadata.observed_time() >= previous);
        }
        if let Some(previous) = session {
            assert!(metadata.session_time() >= previous);
        }
        if let Some(current) = metadata.source_time() {
            if let Some(previous) = source {
                assert!(
                    current.as_nanos() >= previous.as_nanos(),
                    "Chrome source timestamps went backwards"
                );
            }
            source = Some(current);
        }
        observed = Some(metadata.observed_time());
        session = Some(metadata.session_time());
    }
}

fn assert_strict_sequence(frames: &[EncodedFrame]) {
    for pair in frames.windows(2) {
        assert!(
            pair[1].metadata().capture_ordinal() > pair[0].metadata().capture_ordinal(),
            "Krometrail capture ordinals must strictly increase within one target stream"
        );
    }
}

fn jpeg_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.get(..2) != Some(&[0xff, 0xd8]) {
        return None;
    }
    let limit = bytes.len().min(64 * 1024);
    let mut index = 2;
    while index + 1 < limit {
        if bytes[index] != 0xff {
            return None;
        }
        while index < limit && bytes[index] == 0xff {
            index += 1;
        }
        let marker = *bytes.get(index)?;
        index += 1;
        if marker == 0xd9 || marker == 0xda {
            return None;
        }
        if marker == 0x01 || (0xd0..=0xd7).contains(&marker) {
            continue;
        }
        let segment_length =
            u16::from_be_bytes([*bytes.get(index)?, *bytes.get(index + 1)?]) as usize;
        if segment_length < 2 || index + segment_length > limit {
            return None;
        }
        if is_jpeg_sof(marker) {
            let height = u16::from_be_bytes([*bytes.get(index + 3)?, *bytes.get(index + 4)?]);
            let width = u16::from_be_bytes([*bytes.get(index + 5)?, *bytes.get(index + 6)?]);
            return (width > 0 && height > 0).then_some((width as u32, height as u32));
        }
        index += segment_length;
    }
    None
}

fn is_jpeg_sof(marker: u8) -> bool {
    matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf)
}

async fn wait_for_session_targets(session: &Arc<dyn BrowserSessionPort>, keys: &[String; 2]) {
    tokio::time::timeout(CAPTURE_TIMEOUT, async {
        loop {
            let targets = session.targets().await.expect("target snapshot");
            if keys.iter().all(|key| {
                targets.iter().any(|target| {
                    target.target.browser_target_key() == key
                        && !matches!(
                            target.lifecycle,
                            krometrail_core::TargetLifecycle::Closed
                                | krometrail_core::TargetLifecycle::Failed
                        )
                })
            }) {
                return;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("session target readiness deadline");
}

async fn first_browser_target(transport: &dyn CdpTransport) -> String {
    tokio::time::timeout(CAPTURE_TIMEOUT, async {
        loop {
            let value = transport
                .send_raw(
                    &CommandScope::Browser,
                    "Target.getTargets",
                    serde_json::json!({}),
                )
                .await
                .expect("browser target snapshot");
            let target = value
                .get("targetInfos")
                .and_then(serde_json::Value::as_array)
                .and_then(|targets| {
                    targets.iter().find_map(|target| {
                        (target.get("type").and_then(serde_json::Value::as_str) == Some("page")
                            && target
                                .get("url")
                                .and_then(serde_json::Value::as_str)
                                .is_some_and(|url| url != "about:blank"))
                        .then(|| target.get("targetId"))
                        .flatten()
                        .and_then(serde_json::Value::as_str)
                    })
                });
            if let Some(target) = target {
                return target.to_owned();
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("browser page target identity deadline")
}

async fn create_target(transport: &dyn CdpTransport, fixture_url: &str) -> String {
    transport
        .send_raw(
            &CommandScope::Browser,
            "Target.createTarget",
            serde_json::json!({"url": fixture_url}),
        )
        .await
        .expect("create target")
        .get("targetId")
        .and_then(serde_json::Value::as_str)
        .expect("created target identity")
        .to_owned()
}

async fn activate_target(transport: &dyn CdpTransport, target_id: &str) {
    transport
        .send_raw(
            &CommandScope::Browser,
            "Target.activateTarget",
            serde_json::json!({"targetId": target_id}),
        )
        .await
        .expect("activate target");
}

async fn wait_for_gap(
    events: &mut Box<dyn BrowserSessionEvents>,
    target_id: TargetId,
    reason: CaptureGapReason,
    timeout: Duration,
) -> Option<krometrail_core::CaptureGap> {
    tokio::time::timeout(timeout, async {
        loop {
            match events.next().await {
                Ok(Some(BrowserSessionEvent::CaptureGapDeclared { gap }))
                    if gap.target_id() == target_id && *gap.reason() == reason =>
                {
                    return Some(gap);
                }
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => return None,
            }
        }
    })
    .await
    .ok()
    .flatten()
}

struct VisibilityEvidence {
    hidden_event: bool,
    hidden_gap: bool,
}

async fn wait_for_visibility_cycle(
    session: &Arc<dyn BrowserSessionPort>,
    events: &mut Box<dyn BrowserSessionEvents>,
    target_ids: &HashSet<TargetId>,
    extra_keys: &[String; 2],
    factory: &krometrail_cdp::transport::CdpkitTransportFactory,
    launched: &krometrail_cdp::LaunchedChrome,
) -> VisibilityEvidence {
    let Ok(control) = factory
        .connect(launched.endpoint.browser_websocket_url().as_str())
        .await
    else {
        return VisibilityEvidence {
            hidden_event: false,
            hidden_gap: false,
        };
    };
    for key in extra_keys {
        activate_target(control.as_ref(), key).await;
    }
    drop(control);

    tokio::time::timeout(PROBE_TIMEOUT, async {
        let mut hidden_event = false;
        let mut hidden_gap = false;
        loop {
            let statuses = session.capture_statuses().await.expect("capture statuses");
            let visible_again = statuses.iter().any(|status| {
                target_ids.contains(&status.target_id())
                    && status.state() == CaptureStreamState::Capturing
            });
            if hidden_event && hidden_gap && visible_again {
                return VisibilityEvidence {
                    hidden_event,
                    hidden_gap,
                };
            }
            match events.next().await {
                Ok(Some(BrowserSessionEvent::TargetChanged { target }))
                    if target_ids.contains(&target.target.id())
                        && target.visibility == krometrail_core::TargetVisibility::Hidden =>
                {
                    hidden_event = true;
                }
                Ok(Some(BrowserSessionEvent::CaptureGapDeclared { gap }))
                    if target_ids.contains(&gap.target_id())
                        && *gap.reason() == CaptureGapReason::TargetHidden =>
                {
                    hidden_gap = true;
                }
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => break,
            }
        }
        VisibilityEvidence {
            hidden_event,
            hidden_gap,
        }
    })
    .await
    .unwrap_or(VisibilityEvidence {
        hidden_event: false,
        hidden_gap: false,
    })
}

struct ReconnectEvidence {
    generation: u64,
    browser_disconnected: bool,
}

async fn wait_for_reconnect(
    session: &Arc<dyn BrowserSessionPort>,
    events: &mut Box<dyn BrowserSessionEvents>,
    target_id: TargetId,
    old_generation: u64,
    timeout: Duration,
) -> Option<ReconnectEvidence> {
    tokio::time::timeout(timeout, async {
        let mut browser_disconnected = false;
        loop {
            let restored = session
                .capture_statuses()
                .await
                .expect("capture statuses")
                .into_iter()
                .find(|status| {
                    status.target_id() == target_id
                        && status.attachment_generation() > old_generation
                        && status.state() == CaptureStreamState::Capturing
                });
            if let Some(status) = restored {
                if browser_disconnected {
                    return Some(ReconnectEvidence {
                        generation: status.attachment_generation(),
                        browser_disconnected,
                    });
                }
            }
            match events.next().await {
                Ok(Some(BrowserSessionEvent::CaptureGapDeclared { gap }))
                    if gap.target_id() == target_id
                        && *gap.reason() == CaptureGapReason::BrowserDisconnected =>
                {
                    browser_disconnected = true;
                }
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => return None,
            }
        }
    })
    .await
    .ok()
    .flatten()
}

fn assert_profile_unreferenced(path: &Path) {
    let references = support::chrome::process_references(path);
    assert!(
        references.is_empty(),
        "managed Chrome still references its test profile"
    );
}

static WRAPPER_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct ChromeWrapper {
    path: std::path::PathBuf,
}

impl ChromeWrapper {
    #[cfg(unix)]
    fn new() -> Option<Self> {
        use std::os::unix::fs::PermissionsExt;

        let executable = krometrail_cdp::discover_installations(None)
            .first()?
            .executable
            .clone();
        let sequence = WRAPPER_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "krometrail-real-chrome-wrapper-{}-{sequence}",
            std::process::id()
        ));
        let script = format!(
            "#!/bin/sh\nexec {} --headless=new --disable-gpu --no-sandbox \"$@\"\n",
            shell_quote(&executable)
        );
        fs::write(&path, script).expect("Chrome wrapper");
        let mut permissions = fs::metadata(&path)
            .expect("Chrome wrapper metadata")
            .permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&path, permissions).expect("Chrome wrapper permissions");
        Some(Self { path })
    }

    #[cfg(not(unix))]
    fn new() -> Option<Self> {
        None
    }
}

#[cfg(unix)]
fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}

impl Drop for ChromeWrapper {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

const FIXTURE_INDEX: &[u8] =
    include_bytes!("../../../tests/fixtures/browser/cdp-transport-gate/index.html");
const FIXTURE_ANIMATION: &[u8] =
    include_bytes!("../../../tests/fixtures/browser/cdp-transport-gate/animation.js");

struct FixtureServer {
    url: String,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl FixtureServer {
    fn start() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("fixture listener");
        listener
            .set_nonblocking(true)
            .expect("fixture listener nonblocking");
        let address = listener.local_addr().expect("fixture listener address");
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let thread = thread::spawn(move || {
            while !thread_stop.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => serve_fixture(stream),
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::yield_now();
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            url: format!("http://127.0.0.1:{}/index.html", address.port()),
            stop,
            thread: Some(thread),
        }
    }

    fn url(&self) -> &str {
        &self.url
    }
}

impl Drop for FixtureServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn serve_fixture(mut stream: std::net::TcpStream) {
    let mut request = [0_u8; 2048];
    let Ok(size) = stream.read(&mut request) else {
        return;
    };
    let request = String::from_utf8_lossy(&request[..size]);
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");
    let (status, content_type, body) = match path {
        "/" | "/index.html" => ("200 OK", "text/html; charset=utf-8", FIXTURE_INDEX),
        "/animation.js" => (
            "200 OK",
            "text/javascript; charset=utf-8",
            FIXTURE_ANIMATION,
        ),
        _ => (
            "404 Not Found",
            "text/plain; charset=utf-8",
            b"not found" as &[u8],
        ),
    };
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: {content_type}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(body);
}

struct TestSink {
    state: Mutex<SinkState>,
    changed: tokio::sync::Notify,
    release: tokio::sync::Notify,
    blocked: AtomicBool,
    released: AtomicBool,
}

struct SinkState {
    frames: Vec<EncodedFrame>,
    gaps: Vec<krometrail_core::CaptureGap>,
    completed_frames: usize,
    flush_count: usize,
}

impl TestSink {
    fn new(blocked: bool) -> Self {
        Self {
            state: Mutex::new(SinkState {
                frames: Vec::new(),
                gaps: Vec::new(),
                completed_frames: 0,
                flush_count: 0,
            }),
            changed: tokio::sync::Notify::new(),
            release: tokio::sync::Notify::new(),
            blocked: AtomicBool::new(blocked),
            released: AtomicBool::new(!blocked),
        }
    }

    fn frames(&self) -> Vec<EncodedFrame> {
        self.state.lock().expect("sink lock").frames.clone()
    }

    fn gaps(&self) -> Vec<krometrail_core::CaptureGap> {
        self.state.lock().expect("sink lock").gaps.clone()
    }

    fn flush_count(&self) -> usize {
        self.state.lock().expect("sink lock").flush_count
    }

    async fn wait_for_completed_frames(
        &self,
        minimum: usize,
        timeout: Duration,
    ) -> Vec<EncodedFrame> {
        tokio::time::timeout(timeout, async {
            loop {
                let frames = self.frames();
                let completed = self.state.lock().expect("sink lock").completed_frames;
                if completed >= minimum {
                    return frames;
                }
                let notified = self.changed.notified();
                let completed = self.state.lock().expect("sink lock").completed_frames;
                if completed >= minimum {
                    continue;
                }
                notified.await;
            }
        })
        .await
        .expect("real Chrome frame capture deadline")
    }

    async fn wait_for_target_frames(
        &self,
        target_id: TargetId,
        minimum: usize,
        timeout: Duration,
    ) -> Vec<EncodedFrame> {
        tokio::time::timeout(timeout, async {
            loop {
                let frames = self.frames();
                if frames
                    .iter()
                    .filter(|frame| frame.metadata().target_id() == target_id)
                    .count()
                    >= minimum
                {
                    return frames;
                }
                let notified = self.changed.notified();
                if self
                    .frames()
                    .iter()
                    .filter(|frame| frame.metadata().target_id() == target_id)
                    .count()
                    >= minimum
                {
                    continue;
                }
                notified.await;
            }
        })
        .await
        .expect("real Chrome target frame deadline")
    }

    async fn await_release(&self) {
        loop {
            if self.released.load(Ordering::Acquire) {
                return;
            }
            let notified = self.release.notified();
            if self.released.load(Ordering::Acquire) {
                return;
            }
            notified.await;
        }
    }
}

impl RecordingSink for TestSink {
    fn append_frame(&self, frame: EncodedFrame) -> PortFuture<'_, krometrail_core::Result<()>> {
        let should_block = self.blocked.load(Ordering::Acquire);
        {
            self.state.lock().expect("sink lock").frames.push(frame);
        }
        self.changed.notify_waiters();
        Box::pin(async move {
            if should_block {
                self.await_release().await;
            }
            self.state.lock().expect("sink lock").completed_frames += 1;
            self.changed.notify_waiters();
            Ok(())
        })
    }

    fn append_gap(
        &self,
        gap: krometrail_core::CaptureGap,
    ) -> PortFuture<'_, krometrail_core::Result<()>> {
        self.state.lock().expect("sink lock").gaps.push(gap);
        self.changed.notify_waiters();
        Box::pin(std::future::ready(Ok(())))
    }

    fn flush(&self, _session_id: SessionId) -> PortFuture<'_, krometrail_core::Result<()>> {
        self.state.lock().expect("sink lock").flush_count += 1;
        Box::pin(std::future::ready(Ok(())))
    }
}

struct TestClock {
    origin: Instant,
    calls: AtomicU64,
    changed: tokio::sync::Notify,
}

impl TestClock {
    fn new() -> Self {
        Self {
            origin: Instant::now(),
            calls: AtomicU64::new(0),
            changed: tokio::sync::Notify::new(),
        }
    }

    fn calls(&self) -> u64 {
        self.calls.load(Ordering::Acquire)
    }

    async fn wait_for_calls_at_least(&self, minimum: u64, timeout: Duration) {
        tokio::time::timeout(timeout, async {
            loop {
                if self.calls() >= minimum {
                    return;
                }
                let notified = self.changed.notified();
                if self.calls() >= minimum {
                    continue;
                }
                notified.await;
            }
        })
        .await
        .expect("real Chrome acknowledgement liveness deadline");
    }
}

impl MonotonicClock for TestClock {
    fn now(&self) -> ObservedTime {
        self.calls.fetch_add(1, Ordering::AcqRel);
        self.changed.notify_waiters();
        let nanos = self.origin.elapsed().as_nanos();
        ObservedTime::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX))
    }
}

struct TestIds {
    next: AtomicU64,
}

impl TestIds {
    fn new() -> Self {
        Self {
            next: AtomicU64::new(1),
        }
    }
}

impl IdSource for TestIds {
    fn next(&self) -> IdValue {
        let value = self.next.fetch_add(1, Ordering::Relaxed) as u128;
        IdValue::from_uuid(Uuid::from_u128(
            0x1234_5678_0000_0000_0000_0000_0000_0000 | value,
        ))
    }
}
