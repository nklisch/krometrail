#![cfg(feature = "cdpkit-transport")]

mod support;

use std::{
    collections::{HashMap, HashSet},
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
    AttachBrowser, BrowserConnectRequest, BrowserConnector, BrowserProduct, BrowserSessionEvent,
    BrowserSessionEvents, BrowserSessionPort, BrowserSessionState, BrowserStopOutcome, ByteOffset,
    CaptureGapReason, CaptureStreamState, EncodedFrame, ErrorCode, FrameAddress, IdSource, IdValue,
    ImageFormat, LaunchBrowser, ManagedProfile, MonotonicClock, ObservedTime, PortFuture,
    RecordingSink, SegmentId, SessionId, TargetCaptureStatus, TargetId,
};
use support::chrome::ChromeWrapperVariant;
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
    let (session, root) = connect_managed(
        "managed",
        fixture.url(),
        &sink,
        &clock,
        &ids,
        CaptureConfig::default(),
    )
    .await;

    let browser_status = session.status().await.expect("browser status");
    assert_eq!(browser_status.ownership, krometrail_core::BrowserOwnership::Managed);
    assert_eq!(browser_status.state, BrowserSessionState::Ready);
    let session_id = browser_status.session_id;
    let origin = session.session_origin().observed().as_nanos();
    let target_id = first_target(&session).await;
    assert_initial_capture_ready(&session, target_id).await;
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
    assert_frame_fidelity(&frames, origin, session_id, target_id);
    assert_strict_ordinals_by_target(&frames);

    let status = status_for(&session, target_id).await;
    assert_capture_diagnostics(&status, 30);
    let source_samples = frames
        .iter()
        .filter(|frame| frame.metadata().source_time().is_some())
        .count();
    let first_metadata = frames.first().expect("captured frame").metadata();
    eprintln!(
        "real capture diagnostics: frames={} source_samples={} jpeg_dimensions={:?} viewport={:?} scale={} ack_samples={} cadence_samples={} ack_p50={:?} ack_p95={:?} ack_p99={:?} ack_max={:?}",
        frames.len(),
        source_samples,
        first_metadata.image(),
        first_metadata.viewport(),
        first_metadata.device_scale_factor().get(),
        status.ack_latency().sample_count(),
        status.frame_cadence().sample_count(),
        status.ack_latency().p50_nanos(),
        status.ack_latency().p95_nanos(),
        status.ack_latency().p99_nanos(),
        status.ack_latency().max_nanos(),
    );

    let mut events = session
        .subscribe()
        .await
        .expect("capture event subscription");

    let outcome = tokio::time::timeout(STOP_TIMEOUT, session.stop())
        .await
        .expect("managed capture stop must be bounded")
        .expect("managed capture stop");
    assert_eq!(outcome, BrowserStopOutcome::ManagedBrowserClosed);
    assert_eq!(sink.flush_count(), 1);
    assert!(
        sink.gaps()
            .iter()
            .all(|gap| gap.session_id() == session_id)
    );

    let stopped = terminal_capture_status(&mut events, target_id, STOP_TIMEOUT)
        .await
        .expect("target-owned CaptureStateChanged must reach Stopped after stop");
    assert_eq!(stopped.state(), CaptureStreamState::Stopped);
    let stats = stopped.statistics();
    assert!(stats.received_frames() >= 30);
    assert_eq!(stats.received_frames(), stats.acknowledged_frames());
    assert_eq!(
        stats.acknowledged_frames(),
        stats.accepted_frames() + stats.dropped_frames()
    );
    assert_capture_diagnostics(&stopped, 30);
    drop(session);
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
    let chrome_wrapper = support::chrome::ChromeWrapper::for_product(
        BrowserProduct::Chrome,
        ChromeWrapperVariant::DefaultDpi,
    );
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
    let browser_status = session.status().await.expect("browser status");
    assert_eq!(browser_status.ownership, krometrail_core::BrowserOwnership::Attached);
    assert_eq!(browser_status.state, BrowserSessionState::Ready);
    let session_id = browser_status.session_id;
    let mut events = session
        .subscribe()
        .await
        .expect("capture event subscription");
    let control = factory
        .connect(launched.endpoint.browser_websocket_url().as_str())
        .await
        .expect("visibility control transport");
    let initial_target_id = first_target(&session).await;
    let _ = sink
        .wait_for_target_frames(initial_target_id, 5, CAPTURE_TIMEOUT)
        .await;
    let first_extra_key = create_target(control.as_ref(), fixture.url()).await;
    wait_for_session_target(&session, &first_extra_key).await;
    activate_target(control.as_ref(), &first_extra_key).await;
    let first_extra_id = target_id_for_key(&session, &first_extra_key).await;
    let _ = sink
        .wait_for_target_frames(first_extra_id, 5, CAPTURE_TIMEOUT)
        .await;
    let second_extra_key = create_target(control.as_ref(), fixture.url()).await;
    wait_for_session_target(&session, &second_extra_key).await;
    activate_target(control.as_ref(), &second_extra_key).await;
    let second_extra_id = target_id_for_key(&session, &second_extra_key).await;
    let _ = sink
        .wait_for_target_frames(second_extra_id, 5, CAPTURE_TIMEOUT)
        .await;
    let extra_keys = [first_extra_key, second_extra_key];

    let pages = session.status().await.expect("browser status").pages;
    let expected: HashMap<_, _> = pages
        .iter()
        .map(|page| &page.target)
        .filter(|target| {
            target.attachment_generation > 0
                && target.visibility != krometrail_core::TargetVisibility::Unknown
                && !matches!(
                    target.lifecycle,
                    krometrail_core::TargetLifecycle::Closed
                        | krometrail_core::TargetLifecycle::Failed
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
    for target_id in expected.values() {
        assert!(
            sink.frames()
                .iter()
                .any(|frame| frame.metadata().target_id() == *target_id),
            "target did not produce an owned frame"
        );
    }
    drop(control);

    let frames = sink.frames();
    let frame_counts = frames.iter().fold(HashMap::new(), |mut counts, frame| {
        *counts
            .entry(frame.metadata().target_id())
            .or_insert(0_usize) += 1;
        counts
    });
    eprintln!(
        "real two-target diagnostics: targets={} frames={} per_target={frame_counts:?} gaps={}",
        expected_ids.len(),
        frames.len(),
        sink.gaps().len(),
    );
    assert!(expected_ids.iter().all(|target_id| {
        frames
            .iter()
            .any(|frame| frame.metadata().target_id() == *target_id)
    }));
    assert_frame_fidelity_by_target(
        &frames,
        session.session_origin().observed().as_nanos(),
        session_id,
        &expected_ids,
    );
    assert_strict_ordinals_by_target(&frames);
    let statuses = session.status().await.expect("browser status").capture;
    for status in statuses
        .iter()
        .filter(|status| expected_ids.contains(&status.target_id()))
    {
        assert!(status.queue_depth() <= status.queue_capacity());
    }
    assert!(
        expected_ids.iter().all(|target_id| statuses
            .iter()
            .any(|status| status.target_id() == *target_id)),
        "every target must retain target-owned capture status"
    );
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
        assert!(
            visibility.visible_again,
            "visible visibility did not close the target hidden cycle"
        );
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
    let config = CaptureConfig {
        max_active_streams: NonZeroUsize::new(1).expect("one stream"),
        queue_capacity: NonZeroUsize::new(1).expect("one queue slot"),
        shutdown_timeout: Duration::from_secs(2),
        ..CaptureConfig::default()
    };

    // First release the blocked worker and prove that accepted work drains before a normal
    // managed shutdown. The second session intentionally stops while the sink remains blocked so
    // the incomplete-shutdown contract is exercised independently of the drain proof.
    let release_sink = Arc::new(TestSink::new(true));
    let release_clock = Arc::new(TestClock::new());
    let release_ids = Arc::new(TestIds::new());
    let (release_session, release_root) = connect_managed(
        "saturation-drain",
        fixture.url(),
        &release_sink,
        &release_clock,
        &release_ids,
        config.clone(),
    )
    .await;
    let release_target = first_target(&release_session).await;
    let mut release_events = release_session
        .subscribe()
        .await
        .expect("capture event subscription");
    let release_status = assert_saturation_evidence(
        &release_session,
        release_target,
        &release_sink,
        &release_clock,
        &mut release_events,
    )
    .await;
    let accepted_before_release = release_status.statistics().accepted_frames() as usize;
    release_sink.release();
    let _ = release_sink
        .wait_for_completed_frames(accepted_before_release.max(1), PROBE_TIMEOUT)
        .await;
    assert!(
        release_sink.completed_frames() >= accepted_before_release,
        "released sink did not drain accepted capture work"
    );
    let release_outcome = tokio::time::timeout(STOP_TIMEOUT, release_session.stop())
        .await
        .expect("drained capture stop must be bounded")
        .expect("drained capture stop");
    assert_eq!(release_outcome, BrowserStopOutcome::ManagedBrowserClosed);
    assert_eq!(release_sink.flush_count(), 1);
    drop(release_session);
    assert_profile_unreferenced(release_root.path());
    drop(release_root);

    let blocked_sink = Arc::new(TestSink::new(true));
    let blocked_clock = Arc::new(TestClock::new());
    let blocked_ids = Arc::new(TestIds::new());
    let (blocked_session, blocked_root) = connect_managed(
        "saturation-stop",
        fixture.url(),
        &blocked_sink,
        &blocked_clock,
        &blocked_ids,
        config,
    )
    .await;
    let target_id = first_target(&blocked_session).await;
    let mut events = blocked_session
        .subscribe()
        .await
        .expect("capture event subscription");
    let _ = assert_saturation_evidence(
        &blocked_session,
        target_id,
        &blocked_sink,
        &blocked_clock,
        &mut events,
    )
    .await;

    // The worker is intentionally still blocked. Stop must consume one aggregate budget, abandon
    // accepted work explicitly, emit CaptureStopped, and never wait forever for this sink.
    let stop_result = tokio::time::timeout(STOP_TIMEOUT, blocked_session.stop())
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
    // Incomplete shutdown intentionally makes no claim that the sink flush completed.
    drop(blocked_session);
    assert_profile_unreferenced(blocked_root.path());
    drop(blocked_root);
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
    let chrome_wrapper = support::chrome::ChromeWrapper::for_product(
        BrowserProduct::Chrome,
        ChromeWrapperVariant::DefaultDpi,
    );
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
    let session_id = session.status().await.expect("browser status").session_id;
    assert_eq!(
        session.status().await.expect("browser status").ownership,
        krometrail_core::BrowserOwnership::Attached
    );
    let target_id = first_target(&session).await;
    let mut events = session
        .subscribe()
        .await
        .expect("capture event subscription");
    let _ = sink.wait_for_completed_frames(20, CAPTURE_TIMEOUT).await;
    let old_frames = sink.frames();
    let old_target_frames: Vec<_> = old_frames
        .iter()
        .filter(|frame| frame.metadata().target_id() == target_id)
        .cloned()
        .collect();
    let old_status = status_for(&session, target_id).await;
    let old_generation = old_status.attachment_generation();
    assert_frame_fidelity(
        &old_target_frames,
        session.session_origin().observed().as_nanos(),
        session_id,
        target_id,
    );
    assert_strict_ordinals_by_target(&old_target_frames);
    let pre_sever_max = old_target_frames
        .iter()
        .map(|frame| frame.metadata().capture_ordinal().get())
        .max()
        .expect("pre-sever capture ordinal");

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
    let all_frames = sink
        .wait_for_target_frames_after_ordinal(target_id, 8, pre_sever_max, CAPTURE_TIMEOUT)
        .await;
    let restored_frames: Vec<_> = all_frames
        .iter()
        .filter(|frame| {
            frame.metadata().target_id() == target_id
                && frame.metadata().capture_ordinal().get() > pre_sever_max
        })
        .cloned()
        .collect();
    assert!(
        restored_frames.len() >= 8,
        "restored generation yielded too few frames above pre-sever ordinal"
    );
    assert!(
        restored_frames
            .iter()
            .all(|frame| { frame.metadata().capture_ordinal().get() > pre_sever_max })
    );
    let all_target_frames: Vec<_> = all_frames
        .iter()
        .filter(|frame| frame.metadata().target_id() == target_id)
        .cloned()
        .collect();
    assert_strict_ordinals_by_target(&all_target_frames);
    assert_frame_fidelity(
        &all_target_frames,
        session.session_origin().observed().as_nanos(),
        session_id,
        target_id,
    );
    eprintln!(
        "real reconnect diagnostics: old_generation={} restored_generation={} pre_sever_max={} old_frames={} restored_frames={}",
        old_generation,
        reconnect.generation,
        pre_sever_max,
        old_target_frames.len(),
        restored_frames.len(),
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
) {
    let root = support::chrome::temporary_profile_root(name);
    let launcher = SystemChromeLauncher::new(LauncherConfig {
        profile_root: root.path().to_owned(),
        startup_timeout: CAPTURE_TIMEOUT,
        shutdown_timeout: Duration::from_secs(4),
    });
    let chrome_wrapper = support::chrome::ChromeWrapper::for_product(
        BrowserProduct::Chrome,
        ChromeWrapperVariant::DefaultDpi,
    );
    let request = LaunchBrowser {
        executable: chrome_wrapper.as_ref().map(|wrapper| wrapper.path.clone()),
        profile: ManagedProfile::Temporary,
        initial_url: Some(fixture_url.to_owned()),
    };
    let factory = krometrail_cdp::transport::CdpkitTransportFactory::new()
        .with_command_timeout(Duration::from_secs(4));
    let connector = ProductionBrowserConnector::new(Arc::new(launcher), Arc::new(factory))
        .with_capture(clock.clone(), ids.clone(), sink.clone(), config);
    let session = connector
        .connect(BrowserConnectRequest::Launch(request))
        .await
        .expect("managed production capture session");
    (session, root)
}

async fn first_target(session: &Arc<dyn BrowserSessionPort>) -> TargetId {
    session
        .status()
        .await
        .expect("browser status")
        .pages
        .into_iter()
        .map(|page| page.target)
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
        .status()
        .await
        .expect("browser status")
        .capture
        .into_iter()
        .find(|status| status.target_id() == target_id)
        .expect("target capture status")
}

async fn assert_initial_capture_ready(session: &Arc<dyn BrowserSessionPort>, target_id: TargetId) {
    let target = session
        .status()
        .await
        .expect("browser status")
        .pages
        .into_iter()
        .map(|page| page.target)
        .find(|target| target.target.id() == target_id)
        .expect("initial capture target");
    assert_eq!(
        target.visibility,
        krometrail_core::TargetVisibility::Visible
    );
    assert_eq!(target.lifecycle, krometrail_core::TargetLifecycle::Attached);
    assert!(target.attachment_generation > 0);
    let status = status_for(session, target_id).await;
    assert_eq!(status.state(), CaptureStreamState::Capturing);
    assert_eq!(status.attachment_generation(), target.attachment_generation);
}

async fn assert_saturation_evidence(
    session: &Arc<dyn BrowserSessionPort>,
    target_id: TargetId,
    sink: &Arc<TestSink>,
    clock: &Arc<TestClock>,
    events: &mut Box<dyn BrowserSessionEvents>,
) -> TargetCaptureStatus {
    let baseline_calls = clock.calls();
    clock
        .wait_for_calls_at_least(baseline_calls + 12, CAPTURE_TIMEOUT)
        .await;

    let saturated_gap = wait_for_gap(
        events,
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

    let status = status_for(session, target_id).await;
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
    assert!(status.queue_depth() <= status.queue_capacity());
    assert_eq!(
        status.ack_latency().sample_count(),
        statistics.received_frames()
    );
    assert_capture_diagnostics(&status, statistics.received_frames());
    assert!(!sink.frames().is_empty());
    assert_eq!(
        sink.completed_frames(),
        0,
        "blocked sink must remain incomplete while acknowledgements continue"
    );
    eprintln!(
        "real saturation diagnostics: received={} acknowledged={} accepted={} dropped={} queue_depth={} ack_samples={} cadence_samples={}",
        statistics.received_frames(),
        statistics.acknowledged_frames(),
        statistics.accepted_frames(),
        statistics.dropped_frames(),
        status.queue_depth(),
        status.ack_latency().sample_count(),
        status.frame_cadence().sample_count(),
    );
    status
}

fn assert_capture_diagnostics(status: &TargetCaptureStatus, minimum_samples: u64) {
    assert!(status.queue_depth() <= status.queue_capacity());
    assert!(status.ack_latency().sample_count() >= minimum_samples);
    assert!(status.frame_cadence().sample_count() > 0);
    for summary in [status.ack_latency(), status.frame_cadence()] {
        if summary.sample_count() > 0 {
            assert!(summary.p50_nanos() <= summary.p95_nanos());
            assert!(summary.p95_nanos() <= summary.p99_nanos());
            // Percentiles are fixed-bucket upper bounds; the exact maximum can be below p99.
        }
    }
}

fn assert_frame_fidelity(
    frames: &[EncodedFrame],
    origin: u64,
    session_id: SessionId,
    target_id: TargetId,
) {
    assert!(!frames.is_empty(), "target must have captured frames");
    let mut frame_ids = HashSet::new();
    let mut observed = None;
    let mut session = None;
    for frame in frames {
        let metadata = frame.metadata();
        assert!(frame_ids.insert(metadata.id()), "duplicate FrameId");
        assert_eq!(metadata.session_id(), session_id);
        assert_eq!(metadata.target_id(), target_id);
        assert_eq!(metadata.format(), ImageFormat::Jpeg);
        assert!(!frame.bytes().is_empty(), "empty JPEG payload");
        assert_eq!(
            frame.byte_len().get(),
            u64::try_from(frame.bytes().len()).expect("frame bytes fit in u64")
        );
        let image = metadata.image();
        assert_eq!(
            jpeg_dimensions(frame.bytes()),
            Some((image.width(), image.height()))
        );
        assert!(image.width() > 0 && image.height() > 0);
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
        if let Some(source_time) = metadata.source_time() {
            assert!(
                source_time.as_nanos() >= 0,
                "Chrome source timestamps must be non-negative when supplied"
            );
        }
        observed = Some(metadata.observed_time());
        session = Some(metadata.session_time());
    }
}

fn assert_frame_fidelity_by_target(
    frames: &[EncodedFrame],
    origin: u64,
    session_id: SessionId,
    target_ids: &HashSet<TargetId>,
) {
    let mut frame_ids = HashSet::new();
    for frame in frames {
        assert!(
            target_ids.contains(&frame.metadata().target_id()),
            "frame crossed target identity"
        );
        assert!(frame_ids.insert(frame.metadata().id()), "duplicate FrameId");
    }
    for target_id in target_ids {
        let target_frames: Vec<_> = frames
            .iter()
            .filter(|frame| frame.metadata().target_id() == *target_id)
            .cloned()
            .collect();
        assert_frame_fidelity(&target_frames, origin, session_id, *target_id);
    }
}

fn assert_strict_ordinals_by_target(frames: &[EncodedFrame]) {
    let mut last_by_target = HashMap::new();
    for frame in frames {
        let metadata = frame.metadata();
        let ordinal = metadata.capture_ordinal().get();
        assert!(ordinal > 0, "capture ordinal must be non-zero");
        if let Some(previous) = last_by_target.insert(metadata.target_id(), ordinal) {
            assert!(
                ordinal > previous,
                "Krometrail capture ordinals must strictly increase per target"
            );
        }
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

async fn wait_for_session_target(session: &Arc<dyn BrowserSessionPort>, key: &str) {
    tokio::time::timeout(CAPTURE_TIMEOUT, async {
        loop {
            let ready = session
                .status()
                .await
                .expect("browser status")
                .pages
                .into_iter()
                .map(|page| page.target)
                .any(|target| {
                    target.target.browser_target_key() == key
                        && target.attachment_generation > 0
                        && target.visibility != krometrail_core::TargetVisibility::Unknown
                        && !matches!(
                            target.lifecycle,
                            krometrail_core::TargetLifecycle::Closed
                                | krometrail_core::TargetLifecycle::Failed
                        )
                });
            if ready {
                return;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("session target readiness deadline");
}

async fn target_id_for_key(session: &Arc<dyn BrowserSessionPort>, key: &str) -> TargetId {
    tokio::time::timeout(CAPTURE_TIMEOUT, async {
        loop {
            if let Some(target) = session
                .status()
                .await
                .expect("browser status")
                .pages
                .into_iter()
                .map(|page| page.target)
                .find(|target| target.target.browser_target_key() == key)
            {
                return target.target.id();
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("target identity deadline")
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
    let attached = transport
        .send_raw(
            &CommandScope::Browser,
            "Target.attachToTarget",
            serde_json::json!({"targetId": target_id, "flatten": true}),
        )
        .await
        .expect("attach visibility probe target");
    let session_id = attached
        .get("sessionId")
        .and_then(serde_json::Value::as_str)
        .expect("visibility probe session identity")
        .to_owned();
    let scope = CommandScope::session(session_id.clone()).expect("visibility probe session");
    transport
        .send_raw(&scope, "Page.bringToFront", serde_json::json!({}))
        .await
        .expect("bring target to front");
    transport
        .send_raw(
            &CommandScope::Browser,
            "Target.detachFromTarget",
            serde_json::json!({"sessionId": session_id}),
        )
        .await
        .expect("detach visibility probe target");
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

async fn terminal_capture_status(
    events: &mut Box<dyn BrowserSessionEvents>,
    target_id: TargetId,
    timeout: Duration,
) -> Option<TargetCaptureStatus> {
    tokio::time::timeout(timeout, async {
        loop {
            match events.next().await {
                Ok(Some(BrowserSessionEvent::CaptureStateChanged { status }))
                    if status.target_id() == target_id
                        && status.state() == CaptureStreamState::Stopped =>
                {
                    return Some(status);
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
    visible_again: bool,
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
            visible_again: false,
        };
    };
    activate_target(control.as_ref(), &extra_keys[0]).await;
    activate_target(control.as_ref(), &extra_keys[1]).await;
    activate_target(control.as_ref(), &extra_keys[0]).await;
    drop(control);

    tokio::time::timeout(PROBE_TIMEOUT, async {
        let mut hidden_target = None;
        let mut hidden_gap_targets = HashSet::new();
        let mut visible_again = false;
        loop {
            if let Some(target_id) = hidden_target {
                if hidden_gap_targets.contains(&target_id) && visible_again {
                    let visible_target_is_capturing = session
                        .status()
                        .await
                        .expect("browser status")
                        .capture
                        .iter()
                        .any(|status| {
                            status.target_id() == target_id
                                && status.state() == CaptureStreamState::Capturing
                        });
                    if visible_target_is_capturing {
                        return VisibilityEvidence {
                            hidden_event: true,
                            hidden_gap: true,
                            visible_again: true,
                        };
                    }
                }
            }
            match events.next().await {
                Ok(Some(BrowserSessionEvent::TargetChanged { target }))
                    if target_ids.contains(&target.target.id())
                        && target.visibility == krometrail_core::TargetVisibility::Hidden =>
                {
                    hidden_target = Some(target.target.id());
                }
                Ok(Some(BrowserSessionEvent::TargetChanged { target }))
                    if target_ids.contains(&target.target.id())
                        && target.visibility == krometrail_core::TargetVisibility::Visible
                        && hidden_target == Some(target.target.id()) =>
                {
                    visible_again = true;
                }
                Ok(Some(BrowserSessionEvent::CaptureGapDeclared { gap }))
                    if target_ids.contains(&gap.target_id())
                        && *gap.reason() == CaptureGapReason::TargetHidden =>
                {
                    hidden_gap_targets.insert(gap.target_id());
                }
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => break,
            }
        }
        let visible_target_is_capturing = if let Some(target_id) = hidden_target {
            session
                .status()
                .await
                .expect("browser status")
                .capture
                .iter()
                .any(|status| {
                    status.target_id() == target_id
                        && status.state() == CaptureStreamState::Capturing
                })
        } else {
            false
        };
        VisibilityEvidence {
            hidden_event: hidden_target.is_some(),
            hidden_gap: hidden_target.is_some_and(|target| hidden_gap_targets.contains(&target)),
            visible_again: visible_again && visible_target_is_capturing,
        }
    })
    .await
    .unwrap_or(VisibilityEvidence {
        hidden_event: false,
        hidden_gap: false,
        visible_again: false,
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
                .status()
                .await
                .expect("browser status")
                .capture
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

    fn completed_frames(&self) -> usize {
        self.state.lock().expect("sink lock").completed_frames
    }

    fn release(&self) {
        self.released.store(true, Ordering::Release);
        self.release.notify_waiters();
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
        .unwrap_or_else(|_| {
            let counts = self.frames().into_iter().fold(HashMap::new(), |mut counts, frame| {
                *counts.entry(frame.metadata().target_id()).or_insert(0_usize) += 1;
                counts
            });
            panic!(
                "real Chrome target frame deadline for {target_id:?}; observed target counts: {counts:?}"
            );
        })
    }

    async fn wait_for_target_frames_after_ordinal(
        &self,
        target_id: TargetId,
        minimum: usize,
        lower_exclusive: u64,
        timeout: Duration,
    ) -> Vec<EncodedFrame> {
        tokio::time::timeout(timeout, async {
            loop {
                let frames = self.frames();
                if frames
                    .iter()
                    .filter(|frame| {
                        frame.metadata().target_id() == target_id
                            && frame.metadata().capture_ordinal().get() > lower_exclusive
                    })
                    .count()
                    >= minimum
                {
                    return frames;
                }
                let notified = self.changed.notified();
                if self
                    .frames()
                    .iter()
                    .filter(|frame| {
                        frame.metadata().target_id() == target_id
                            && frame.metadata().capture_ordinal().get() > lower_exclusive
                    })
                    .count()
                    >= minimum
                {
                    continue;
                }
                notified.await;
            }
        })
        .await
        .expect("real Chrome restored frame deadline")
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
    fn append_frame(
        &self,
        frame: EncodedFrame,
    ) -> PortFuture<'_, krometrail_core::Result<FrameAddress>> {
        let should_block = self.blocked.load(Ordering::Acquire);
        let byte_offset = frame.metadata().capture_ordinal().get();
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
            Ok(FrameAddress::new(
                SegmentId::from_uuid(Uuid::from_u128(1)),
                ByteOffset::new(byte_offset),
            ))
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
