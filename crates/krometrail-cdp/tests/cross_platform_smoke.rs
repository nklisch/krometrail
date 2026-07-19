#![cfg(feature = "cdpkit-transport")]

mod support;

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    env, fs,
    io::{Read, Write},
    net::TcpListener,
    num::NonZeroUsize,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use krometrail_cdp::{
    CaptureConfig, LauncherConfig, ProductionBrowserConnector, SystemChromeLauncher,
};
use krometrail_core::{
    BrowserConnectRequest, BrowserConnector, BrowserInstallation, BrowserProduct,
    BrowserSessionEvent, BrowserSessionEvents, BrowserSessionPort, BrowserSessionState,
    BrowserStopOutcome, ByteOffset, CaptureGap, CaptureGapReason, CaptureTimingSummary,
    EncodedFrame, FrameAddress, IdSource, IdValue, ImageFormat, LaunchBrowser, ManagedProfile,
    MonotonicClock, ObservedTime, PortFuture, RecordingSink, SegmentId, SessionId,
    TargetCaptureStatus, TargetId,
};
use uuid::Uuid;

use support::{
    chrome::{ChromeWrapper, ChromeWrapperVariant},
    smoke_evidence::{
        BrowserInstallationEvidence, CDPKIT_VERSION, CaptureConfigSnapshot,
        CrossPlatformSmokeEvidence, DeclaredGap, Dimensions, FIXTURE_NAME, FIXTURE_RELATIVE_PATH,
        FixtureEvidence, KIND, Launch, OrdinalRange, Provenance, RuntimeVersion, SCHEMA_VERSION,
        Session, Shutdown, TimingSummary, load_schema, sample_path, schema_path,
        validate_against_schema,
    },
};

const CAPTURE_TIMEOUT: Duration = Duration::from_secs(45);
const STOP_TIMEOUT: Duration = Duration::from_secs(12);
const PROBE_TIMEOUT: Duration = Duration::from_secs(8);
const MIN_FIDELITY_FRAMES: usize = 30;
const INDEX_HTML_SHA256: &str =
    "sha256:9b42ae730d12a95772a946bf55e4838a5443b6cb4c536424570219041b6e2a68";
const ANIMATION_JS_SHA256: &str =
    "sha256:84ba666539a996012a781637c1a894d8c7a4789cfca84661bd7cf8b79efa2e13";
const NON_CLAIMS: &[&str] = &[
    "no transport requalification (final5 owns cdpkit selection)",
    "no host-speed percentile threshold (ack/cadence are diagnostics)",
    "no product-thesis capture-probability threshold",
    "no duration sweep, defect corpus, artifact comparison, or storage validation",
    "no chrome-acknowledgement-token continuity claim",
];
const CONFIGURATION_NAMES: &[&str] = &[
    "linux-chrome",
    "linux-chromium",
    "macos-chrome-default-dpi",
    "macos-chrome-high-dpi",
];

#[derive(Clone, Debug)]
struct Configuration {
    name: &'static str,
    variant: ChromeWrapperVariant,
    product: BrowserProduct,
}

fn configurations_for_this_platform() -> Vec<Configuration> {
    #[cfg(target_os = "linux")]
    {
        vec![
            Configuration {
                name: "linux-chrome",
                variant: ChromeWrapperVariant::DefaultDpi,
                product: BrowserProduct::Chrome,
            },
            Configuration {
                name: "linux-chromium",
                variant: ChromeWrapperVariant::DefaultDpi,
                product: BrowserProduct::Chromium,
            },
        ]
    }
    #[cfg(target_os = "macos")]
    {
        vec![
            Configuration {
                name: "macos-chrome-default-dpi",
                variant: ChromeWrapperVariant::DefaultDpi,
                product: BrowserProduct::Chrome,
            },
            Configuration {
                name: "macos-chrome-high-dpi",
                variant: ChromeWrapperVariant::HighDpi,
                product: BrowserProduct::Chrome,
            },
        ]
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Vec::new()
    }
}

#[test]
fn deterministic_wrapper_script_bytes_contain_forced_scale_flags() {
    let executable = PathBuf::from("/tmp/sentinel-chrome");
    let default = ChromeWrapper::script_bytes(&executable, ChromeWrapperVariant::DefaultDpi);
    let default = String::from_utf8(default).unwrap();
    assert!(default.contains("--headless=new"));
    assert!(default.contains("--disable-gpu"));
    assert!(default.contains("--no-sandbox"));
    assert!(default.contains("--force-device-scale-factor=1"));

    let high = ChromeWrapper::script_bytes(&executable, ChromeWrapperVariant::HighDpi);
    let high = String::from_utf8(high).unwrap();
    assert!(high.contains("--headless=new"));
    assert!(high.contains("--disable-gpu"));
    assert!(high.contains("--no-sandbox"));
    assert!(high.contains("--high-dpi-support=1"));
    assert!(high.contains("--force-device-scale-factor=2"));
}

#[test]
fn deterministic_product_filter_returns_matching_installation_or_none() {
    let chrome = installation_for(BrowserProduct::Chrome);
    let chromium = installation_for(BrowserProduct::Chromium);

    if chrome.is_none() && chromium.is_none() {
        eprintln!("not_installed: no Chrome or Chromium installation discovered");
    }
    if let Some(installation) = chrome {
        assert_eq!(installation.product, BrowserProduct::Chrome);
    }
    if let Some(installation) = chromium {
        assert_eq!(installation.product, BrowserProduct::Chromium);
    }
}

#[test]
fn deterministic_configurations_match_platform() {
    let configurations = configurations_for_this_platform();
    #[cfg(target_os = "linux")]
    assert_eq!(
        configurations
            .iter()
            .map(|configuration| configuration.name)
            .collect::<Vec<_>>(),
        ["linux-chrome", "linux-chromium"]
    );
    #[cfg(target_os = "macos")]
    assert_eq!(
        configurations
            .iter()
            .map(|configuration| configuration.name)
            .collect::<Vec<_>>(),
        ["macos-chrome-default-dpi", "macos-chrome-high-dpi"]
    );
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    assert!(configurations.is_empty());

    for configuration in configurations {
        assert!(CONFIGURATION_NAMES.contains(&configuration.name));
        let expected_product = match configuration.name {
            "linux-chrome" | "macos-chrome-default-dpi" | "macos-chrome-high-dpi" => {
                BrowserProduct::Chrome
            }
            "linux-chromium" => BrowserProduct::Chromium,
            _ => unreachable!(),
        };
        assert_eq!(configuration.product, expected_product);
        assert_eq!(
            configuration.variant.force_device_scale_factor(),
            match configuration.variant {
                ChromeWrapperVariant::DefaultDpi => 1.0,
                ChromeWrapperVariant::HighDpi => 2.0,
            }
        );
    }
}

#[test]
fn deterministic_canonical_content_matches_committed_sample() {
    let expected = fs::read(sample_path()).expect("committed sample.json exists");
    let expected = expected.strip_suffix(b"\n").unwrap_or(&expected);
    let actual = CrossPlatformSmokeEvidence::sample()
        .to_canonical_bytes()
        .expect("sample serializes");
    assert_eq!(
        expected, actual,
        "committed sample.json must match the serializer's canonical content"
    );
}

#[test]
fn deterministic_schema_validates_sample_and_serializer_output() {
    let schema = load_schema();
    let sample_value = serde_json::to_value(CrossPlatformSmokeEvidence::sample()).unwrap();
    validate_against_schema(&sample_value, &schema)
        .expect("serializer output validates against schema");

    let committed_sample = serde_json::from_slice::<serde_json::Value>(
        &fs::read(sample_path()).expect("sample.json exists"),
    )
    .expect("sample.json is valid JSON");
    validate_against_schema(&committed_sample, &schema)
        .expect("committed sample.json validates against schema");
}

#[test]
fn deterministic_committed_runtime_evidence_is_canonical_and_schema_valid() {
    let directory = schema_path().parent().expect("schema directory").to_owned();
    let mut paths = fs::read_dir(directory)
        .expect("evidence directory")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().and_then(|extension| extension.to_str()) == Some("json")
                && !matches!(
                    path.file_name().and_then(|name| name.to_str()),
                    Some("schema.json" | "sample.json")
                )
        })
        .collect::<Vec<_>>();
    paths.sort();
    let schema = load_schema();
    for path in paths {
        let bytes = fs::read(&path).expect("committed runtime evidence");
        let document: CrossPlatformSmokeEvidence =
            serde_json::from_slice(&bytes).expect("runtime evidence shape");
        document.validate().expect("runtime evidence invariants");
        validate_against_schema(
            &serde_json::to_value(&document).expect("runtime evidence encodes"),
            &schema,
        )
        .expect("runtime evidence schema");
        assert_eq!(
            document.to_canonical_bytes().expect("canonical evidence"),
            bytes,
            "{} is not canonical",
            path.display()
        );
    }
}

#[test]
fn deterministic_evidence_invariants_reject_invalid_variants_and_private_data() {
    let mut evidence = CrossPlatformSmokeEvidence::sample();
    evidence.validate().expect("canonical sample validates");

    evidence.non_claims.clear();
    assert!(evidence.validate().is_err());

    let mut evidence = CrossPlatformSmokeEvidence::sample();
    evidence.provenance.launch.force_device_scale_factor = 2.0;
    assert!(evidence.validate().is_err());

    let mut evidence = CrossPlatformSmokeEvidence::sample();
    evidence.provenance.launch.wrapper_variant = "unknown".into();
    assert!(evidence.validate().is_err());

    let mut evidence = CrossPlatformSmokeEvidence::sample();
    evidence.provenance.runtime_version.user_agent = "ws://127.0.0.1:9222/private".into();
    assert!(evidence.validate().is_err());
}

#[tokio::test]
async fn deterministic_browser_version_accessors_on_scripted_session() {
    let transport = support::scripted_cdp::ScriptedCdp::chrome();
    let compatibility = krometrail_cdp::probe_compatibility(&transport)
        .await
        .expect("scripted Chrome passes compatibility probe");
    let version = &compatibility.version;
    assert_eq!(version.product(), BrowserProduct::Chrome);
    assert!(!version.product_version().as_str().is_empty());
    assert!(!version.revision().is_empty());
    assert!(!version.protocol_version().is_empty());
    assert!(!version.user_agent().is_empty());
    assert!(!version.js_version().is_empty());
}

#[test]
fn deterministic_committed_schema_and_sample_exist() {
    assert!(schema_path().is_file(), "schema.json must be committed");
    assert!(sample_path().is_file(), "sample.json must be committed");
}

#[tokio::test]
async fn opt_in_cross_platform_smoke_records_fidelity_loss_and_cleanup_per_configuration() {
    if !support::chrome::real_browser_tests_enabled() {
        eprintln!("gate_unavailable: set KROMETRAIL_REAL_CHROME_TESTS=1");
        return;
    }
    let configurations = configurations_for_this_platform();
    if configurations.is_empty() {
        eprintln!("wrong_platform: cross-platform smoke supports only Linux and macOS");
        return;
    }

    let _browser_lock = support::chrome::real_browser_lock().await;
    let mut evidence = Vec::new();
    for configuration in configurations {
        let Some(installation) = installation_for(configuration.product) else {
            eprintln!(
                "not_installed: {} requires {}",
                configuration.name,
                configuration.product.as_str()
            );
            continue;
        };
        let document = run_configuration(&configuration, &installation).await;
        // Preserve a completed configuration's evidence even if a later decisive configuration
        // fails. This keeps a high-DPI failure honest without discarding a valid default-DPI run.
        write_evidence(&document);
        evidence.push(document);
    }

    #[cfg(target_os = "macos")]
    assert_macos_scale_pair(&evidence);
}

fn installation_for(product: BrowserProduct) -> Option<BrowserInstallation> {
    krometrail_cdp::discover_installations(None)
        .into_iter()
        .find(|installation| installation.product == product)
}

async fn run_configuration(
    configuration: &Configuration,
    installation: &BrowserInstallation,
) -> CrossPlatformSmokeEvidence {
    let fixture = FixtureServer::start();
    let fidelity_config = CaptureConfig::default();
    let fidelity = run_fidelity_session(
        configuration,
        installation,
        fixture.url(),
        fidelity_config.clone(),
    )
    .await;
    let loss_config = CaptureConfig {
        queue_capacity: NonZeroUsize::new(1).expect("one queue slot"),
        ..CaptureConfig::default()
    };
    let loss = run_loss_session(configuration, installation, fixture.url(), loss_config).await;

    assert_eq!(
        fidelity.runtime_version.product,
        installation.product.as_str()
    );
    let document = CrossPlatformSmokeEvidence {
        schema_version: SCHEMA_VERSION,
        kind: KIND.into(),
        provenance: Provenance {
            krometrail_revision: git_revision(),
            rust_version: rust_version(),
            cdpkit_version: CDPKIT_VERSION.into(),
            platform: platform_name().into(),
            architecture: architecture_name().into(),
            configuration_name: configuration.name.into(),
            browser_installation: BrowserInstallationEvidence {
                executable_source: installation.source.as_str().into(),
                product: installation.product.as_str().into(),
                discovered_version: installation.version.as_str().into(),
            },
            runtime_version: fidelity.runtime_version,
            launch: Launch {
                ownership: "managed".into(),
                profile_kind: "temporary".into(),
                endpoint: "loopback".into(),
                wrapper_variant: configuration.variant.as_str().into(),
                force_device_scale_factor: configuration.variant.force_device_scale_factor(),
            },
            capture_config: CaptureConfigSnapshot::from(&fidelity_config),
            fixture: FixtureEvidence {
                name: FIXTURE_NAME.into(),
                path: FIXTURE_RELATIVE_PATH.into(),
                index_html_sha256: INDEX_HTML_SHA256.into(),
                animation_js_sha256: ANIMATION_JS_SHA256.into(),
            },
        },
        sessions: vec![fidelity.session, loss.session],
        shutdown: Shutdown {
            outcome: "managed_browser_closed".into(),
            flush_count: 1,
            process_references_after: Vec::new(),
            profile_references_after: Vec::new(),
        },
        non_claims: NON_CLAIMS.iter().map(|claim| (*claim).into()).collect(),
    };
    document.validate().expect("runtime evidence invariants");
    validate_against_schema(
        &serde_json::to_value(&document).expect("runtime evidence encodes"),
        &load_schema(),
    )
    .expect("runtime evidence validates against committed schema");
    document
}

struct FidelityResult {
    session: Session,
    runtime_version: RuntimeVersion,
}

async fn run_fidelity_session(
    configuration: &Configuration,
    installation: &BrowserInstallation,
    fixture_url: &str,
    config: CaptureConfig,
) -> FidelityResult {
    let sink = Arc::new(TestSink::new(false));
    let clock = Arc::new(TestClock::new());
    let ids = Arc::new(TestIds::new());
    let (session, root) = connect_managed(
        configuration,
        installation,
        fixture_url,
        &sink,
        &clock,
        &ids,
        config.clone(),
    )
    .await;

    let browser_status = session.status().await.expect("browser status");
    assert_eq!(browser_status.state, BrowserSessionState::Ready);
    assert_eq!(browser_status.ownership.as_str(), "managed");
    let session_id = browser_status.session_id;
    let version = &browser_status.compatibility.version;
    assert_eq!(version.product(), installation.product);
    let runtime_version = RuntimeVersion {
        product: version.product().as_str().into(),
        product_version: version.product_version().as_str().into(),
        revision: version.revision().into(),
        protocol_version: version.protocol_version().into(),
        user_agent: version.user_agent().into(),
        js_version: version.js_version().into(),
    };
    let origin = session.session_origin().observed().as_nanos();
    let target_id = first_target(&session).await;
    let frames = sink
        .wait_for_completed_frames(MIN_FIDELITY_FRAMES, CAPTURE_TIMEOUT)
        .await;
    let target_frames: Vec<_> = frames
        .into_iter()
        .filter(|frame| frame.metadata().target_id() == target_id)
        .collect();
    assert!(target_frames.len() >= MIN_FIDELITY_FRAMES);
    assert_frame_fidelity(&target_frames, origin, session_id, target_id);
    assert_strict_ordinals_by_target(&target_frames);

    let status = status_for(&session, target_id).await;
    assert_capture_diagnostics(&status, MIN_FIDELITY_FRAMES as u64);
    let scale = target_frames[0].metadata().device_scale_factor().get();
    match configuration.variant {
        ChromeWrapperVariant::DefaultDpi => {
            assert!(scale <= 1.5, "default-DPI scale {scale} exceeds its band")
        }
        ChromeWrapperVariant::HighDpi => {
            assert!(scale >= 1.5, "high-DPI scale {scale} is below its band")
        }
    }
    assert!(target_frames.iter().all(|frame| {
        (frame.metadata().device_scale_factor().get() - scale).abs() <= f64::EPSILON
    }));

    let evidence_session =
        session_evidence("fidelity", &config, &target_frames, &status, sink.gaps());
    let outcome = tokio::time::timeout(STOP_TIMEOUT, session.stop())
        .await
        .expect("fidelity stop must be bounded")
        .expect("fidelity managed stop");
    assert_eq!(outcome, BrowserStopOutcome::ManagedBrowserClosed);
    assert_eq!(sink.flush_count(), 1);
    drop(session);
    assert_no_profile_references(root.path());
    drop(root);

    FidelityResult {
        session: evidence_session,
        runtime_version,
    }
}

struct LossResult {
    session: Session,
}

async fn run_loss_session(
    configuration: &Configuration,
    installation: &BrowserInstallation,
    fixture_url: &str,
    config: CaptureConfig,
) -> LossResult {
    let sink = Arc::new(TestSink::new(true));
    let clock = Arc::new(TestClock::new());
    let ids = Arc::new(TestIds::new());
    let (session, root) = connect_managed(
        configuration,
        installation,
        fixture_url,
        &sink,
        &clock,
        &ids,
        config.clone(),
    )
    .await;
    let target_id = first_target(&session).await;
    let mut events = session
        .subscribe()
        .await
        .expect("loss capture event subscription");
    let baseline_calls = clock.calls();
    clock
        .wait_for_calls_at_least(baseline_calls + 12, CAPTURE_TIMEOUT)
        .await;
    let gap = wait_for_gap_event(
        &mut events,
        target_id,
        CaptureGapReason::IngestionQueueSaturated,
        PROBE_TIMEOUT,
    )
    .await
    .expect("saturation gap event");
    assert!(gap.estimated_missing_frames().is_some());

    let status = status_for(&session, target_id).await;
    let statistics = status.statistics();
    assert!(statistics.received_frames() >= statistics.acknowledged_frames());
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

    let accepted = statistics.accepted_frames() as usize;
    sink.release();
    let frames = sink
        .wait_for_completed_frames(accepted.max(1), PROBE_TIMEOUT)
        .await;
    assert!(sink.completed_frames() >= accepted);
    let target_frames: Vec<_> = frames
        .into_iter()
        .filter(|frame| frame.metadata().target_id() == target_id)
        .collect();
    assert!(!target_frames.is_empty());
    assert_frame_fidelity(
        &target_frames,
        session.session_origin().observed().as_nanos(),
        session.status().await.expect("browser status").session_id,
        target_id,
    );
    assert_strict_ordinals_by_target(&target_frames);
    let mut gaps = sink.gaps();
    if !gaps.iter().any(|candidate| candidate.id() == gap.id()) {
        gaps.push(gap);
    }
    let evidence_session =
        session_evidence("loss_reporting", &config, &target_frames, &status, gaps);

    let outcome = tokio::time::timeout(STOP_TIMEOUT, session.stop())
        .await
        .expect("loss stop must be bounded")
        .expect("loss managed stop");
    assert_eq!(outcome, BrowserStopOutcome::ManagedBrowserClosed);
    assert_eq!(sink.flush_count(), 1);
    drop(session);
    assert_no_profile_references(root.path());
    drop(root);

    LossResult {
        session: evidence_session,
    }
}

async fn connect_managed(
    configuration: &Configuration,
    installation: &BrowserInstallation,
    fixture_url: &str,
    sink: &Arc<TestSink>,
    clock: &Arc<TestClock>,
    ids: &Arc<TestIds>,
    config: CaptureConfig,
) -> (
    Arc<dyn BrowserSessionPort>,
    support::chrome::TemporaryRootGuard,
) {
    let root = support::chrome::temporary_profile_root("smoke");
    let launcher = SystemChromeLauncher::new(LauncherConfig {
        profile_root: root.path().to_owned(),
        startup_timeout: CAPTURE_TIMEOUT,
        shutdown_timeout: Duration::from_secs(4),
    });
    let wrapper = ChromeWrapper::new(
        installation.executable.clone(),
        installation.product,
        configuration.variant,
    );
    let request = LaunchBrowser {
        executable: Some(wrapper.path.clone()),
        profile: ManagedProfile::Temporary,
        initial_url: Some(fixture_url.to_owned()),
        every_nth_frame: krometrail_core::EveryNthFrame::default(),
        focus: krometrail_core::BrowserFocusPolicy::default(),
    };
    let factory = krometrail_cdp::transport::CdpkitTransportFactory::new()
        .with_command_timeout(Duration::from_secs(4));
    let connector = ProductionBrowserConnector::new(Arc::new(launcher), Arc::new(factory))
        .with_capture(
            clock.clone(),
            ids.clone(),
            sink.clone(),
            Arc::new(support::retention::AlwaysAvailableRetention),
            config,
        );
    let session = connector
        .connect(BrowserConnectRequest::Launch(request))
        .await
        .expect("managed production capture session");
    (session, root)
}

async fn wait_for_gap_event(
    events: &mut Box<dyn BrowserSessionEvents>,
    target_id: TargetId,
    reason: CaptureGapReason,
    timeout: Duration,
) -> Option<CaptureGap> {
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
        .expect("attached fixture target")
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

fn session_evidence(
    name: &str,
    config: &CaptureConfig,
    frames: &[EncodedFrame],
    status: &TargetCaptureStatus,
    gaps: Vec<CaptureGap>,
) -> Session {
    let first = frames.first().expect("captured frame").metadata();
    let last = frames.last().expect("captured frame").metadata();
    let mut gap_counts = BTreeMap::<String, u64>::new();
    for gap in &gaps {
        assert_eq!(gap.target_id(), status.target_id());
        *gap_counts.entry(gap.reason().as_str().into()).or_default() += 1;
    }
    let recorded_gap_count = gap_counts.values().sum::<u64>();
    if recorded_gap_count < status.statistics().gap_count() && gap_counts.len() == 1 {
        // The bounded event subscriber may be observed before every repeated saturation event is
        // drained. The status counter is authoritative for the total when all observed gaps share
        // the same reason, as they do in this capacity-one loss session.
        *gap_counts.values_mut().next().expect("one gap reason") = status.statistics().gap_count();
    }
    Session {
        name: name.into(),
        capture_config: CaptureConfigSnapshot::from(config),
        frame_count: frames.len() as u64,
        source_time_samples: frames
            .iter()
            .filter(|frame| frame.metadata().source_time().is_some())
            .count() as u64,
        image_dimensions: Dimensions {
            width: first.image().width(),
            height: first.image().height(),
        },
        viewport: Dimensions {
            width: first.viewport().width(),
            height: first.viewport().height(),
        },
        device_scale_factor: first.device_scale_factor().get(),
        capture_ordinal_range: OrdinalRange {
            min: first.capture_ordinal().get(),
            max: last.capture_ordinal().get(),
        },
        observed_clock_span_nanos: last
            .observed_time()
            .as_nanos()
            .saturating_sub(first.observed_time().as_nanos()),
        session_clock_span_nanos: last
            .session_time()
            .as_nanos()
            .saturating_sub(first.session_time().as_nanos()),
        ack_latency_nanos: timing_summary(status.ack_latency()),
        frame_cadence_nanos: timing_summary(status.frame_cadence()),
        declared_gaps: gap_counts
            .into_iter()
            .map(|(reason, count)| DeclaredGap { reason, count })
            .collect(),
        visibility_events: gaps
            .iter()
            .filter(|gap| *gap.reason() == CaptureGapReason::TargetHidden)
            .count() as u64,
    }
}

fn timing_summary(summary: &CaptureTimingSummary) -> TimingSummary {
    TimingSummary {
        samples: summary.sample_count(),
        p50: summary.p50_nanos(),
        p95: summary.p95_nanos(),
        p99: summary.p99_nanos(),
        max: summary.max_nanos(),
    }
}

fn assert_capture_diagnostics(status: &TargetCaptureStatus, minimum_samples: u64) {
    assert!(status.queue_depth() <= status.queue_capacity());
    assert!(status.ack_latency().sample_count() >= minimum_samples);
    assert!(status.frame_cadence().sample_count() > 0);
    for summary in [status.ack_latency(), status.frame_cadence()] {
        assert!(summary.p50_nanos() <= summary.p95_nanos());
        assert!(summary.p95_nanos() <= summary.p99_nanos());
    }
}

fn assert_frame_fidelity(
    frames: &[EncodedFrame],
    origin: u64,
    session_id: SessionId,
    target_id: TargetId,
) {
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
        observed = Some(metadata.observed_time());
        session = Some(metadata.session_time());
    }
}

fn assert_strict_ordinals_by_target(frames: &[EncodedFrame]) {
    let mut last_by_target = HashMap::new();
    for frame in frames {
        let metadata = frame.metadata();
        let ordinal = metadata.capture_ordinal().get();
        assert!(ordinal > 0);
        if let Some(previous) = last_by_target.insert(metadata.target_id(), ordinal) {
            assert!(
                ordinal > previous,
                "capture ordinals must strictly increase"
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
        let length = u16::from_be_bytes([*bytes.get(index)?, *bytes.get(index + 1)?]) as usize;
        if length < 2 || index + length > limit {
            return None;
        }
        if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) {
            let height = u16::from_be_bytes([*bytes.get(index + 3)?, *bytes.get(index + 4)?]);
            let width = u16::from_be_bytes([*bytes.get(index + 5)?, *bytes.get(index + 6)?]);
            return Some((width.into(), height.into()));
        }
        index += length;
    }
    None
}

fn assert_no_profile_references(path: &Path) {
    let references = support::chrome::process_references(path);
    assert!(
        references.is_empty(),
        "managed browser/profile references remain after stop: {references:?}"
    );
}

#[cfg(target_os = "macos")]
fn assert_macos_scale_pair(evidence: &[CrossPlatformSmokeEvidence]) {
    let scale = |name: &str| {
        evidence
            .iter()
            .find(|document| document.provenance.configuration_name == name)
            .and_then(|document| document.sessions.first())
            .map(|session| session.device_scale_factor)
            .unwrap_or_else(|| panic!("missing decisive macOS evidence {name}"))
    };
    let default = scale("macos-chrome-default-dpi");
    let high = scale("macos-chrome-high-dpi");
    assert!(default <= 1.5);
    assert!(high >= 1.5);
    assert!(
        high > default,
        "high-DPI scale must exceed default-DPI scale"
    );
}

fn write_evidence(evidence: &CrossPlatformSmokeEvidence) {
    let root = env::var_os("KROMETRAIL_SMOKE_EVIDENCE_DIR")
        .map(PathBuf::from)
        .map(|path| {
            if path.is_absolute() {
                path
            } else {
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../..")
                    .join(path)
            }
        })
        .unwrap_or_else(|| {
            env::temp_dir().join(format!(
                "krometrail-smoke-{}-{}",
                evidence.provenance.configuration_name,
                std::process::id()
            ))
        });
    fs::create_dir_all(&root).expect("create smoke evidence directory");
    let path = root.join(format!("{}.json", evidence.provenance.configuration_name));
    let bytes = evidence.to_canonical_bytes().expect("canonical evidence");
    fs::write(&path, bytes).expect("write smoke evidence");
    eprintln!("wrote smoke evidence: {}", path.display());
}

fn git_revision() -> String {
    command_output("git", &["rev-parse", "HEAD"])
        .filter(|revision| {
            revision.len() == 40 && revision.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
        .expect("git revision must be available for evidence provenance")
}

fn rust_version() -> String {
    command_output("rustc", &["--version"]).expect("rustc version must be available")
}

fn command_output(program: &str, arguments: &[&str]) -> Option<String> {
    let output = Command::new(program).args(arguments).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

#[cfg(target_os = "linux")]
fn platform_name() -> &'static str {
    "linux"
}
#[cfg(target_os = "macos")]
fn platform_name() -> &'static str {
    "macos"
}
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn platform_name() -> &'static str {
    "unsupported"
}

#[cfg(target_arch = "x86_64")]
fn architecture_name() -> &'static str {
    "x86_64"
}
#[cfg(target_arch = "aarch64")]
fn architecture_name() -> &'static str {
    "aarch64"
}
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
fn architecture_name() -> &'static str {
    "unsupported"
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
    gaps: Vec<CaptureGap>,
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

    fn gaps(&self) -> Vec<CaptureGap> {
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
                if self.completed_frames() >= minimum {
                    return self.frames();
                }
                let notified = self.changed.notified();
                if self.completed_frames() >= minimum {
                    continue;
                }
                notified.await;
            }
        })
        .await
        .expect("real Chrome frame capture deadline")
    }

    async fn await_release(&self) {
        loop {
            if self.released.load(Ordering::Acquire) {
                return;
            }
            let notified = self.release.notified();
            if self.released.load(Ordering::Acquire) {
                continue;
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
        self.state.lock().expect("sink lock").frames.push(frame);
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

    fn append_gap(&self, gap: CaptureGap) -> PortFuture<'_, krometrail_core::Result<()>> {
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
        ObservedTime::from_nanos(
            u64::try_from(self.origin.elapsed().as_nanos()).unwrap_or(u64::MAX),
        )
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
