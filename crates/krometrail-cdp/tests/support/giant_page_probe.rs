//! Opt-in diagnostic, not a passing regression for the historical session-kill report.
//! Fixture growth is bounded; only command names, byte counts, and local failure metadata print.
use super::*;
use krometrail_cdp::{TransportError, TransportFuture, TransportEvents, TransportClose};
use krometrail_core::{BrowserOperationRequest, LiveObservationRequest, PageSelection};
use serde_json::Value;

struct ProbeFactory;
struct ProbeTransport(Arc<dyn CdpTransport>);
impl CdpTransportFactory for ProbeFactory {
    fn connect(&self, url: &str) -> TransportFuture<'_, Result<Arc<dyn CdpTransport>, TransportError>> {
        let url = url.to_owned();
        Box::pin(async move {
            eprintln!("GIANT connect");
            // Independent browser-level connection distinguishes reader death from Chrome death.
            use cdpkit::Sender as _;
            let health = cdpkit::CDP::connect_ws(&url).await;
            if let Ok(health) = health {
                health.set_command_timeout(Duration::from_secs(3));
                eprintln!("GIANT independent_browser_alive={}", health.send_raw("Browser.getVersion", serde_json::json!({})).await.is_ok());
            } else {
                eprintln!("GIANT independent_browser_alive=false");
            }
            let inner = krometrail_cdp::transport::CdpkitTransportFactory::new()
                .with_command_timeout(Duration::from_secs(10)).connect(&url).await?;
            Ok(Arc::new(ProbeTransport(inner)) as Arc<dyn CdpTransport>)
        })
    }
}
impl CdpTransport for ProbeTransport {
    fn send_raw(&self, scope: &CommandScope, method: &str, params: Value) -> TransportFuture<'_, Result<Value, TransportError>> {
        let scope = scope.clone();
        let method = method.to_owned();
        Box::pin(async move {
            let started = Instant::now();
            let result = self.0.send_raw(&scope, &method, params).await;
            // Never serialize values to logs. Serialized size is diagnostic only.
            eprintln!("GIANT command={method} elapsed_ms={} bytes={} error={:?} closed={}", started.elapsed().as_millis(), result.as_ref().map(|v| serde_json::to_vec(v).unwrap().len()).unwrap_or(0), result.as_ref().err(), self.0.is_closed());
            if method == "Browser.getVersion" && let Ok(value) = &result {
                eprintln!("GIANT browser={} protocol={}", value["product"], value["protocolVersion"]);
            }
            result
        })
    }
    fn subscribe_named(&self, scope: &CommandScope, method: &str) -> TransportFuture<'_, Result<Box<dyn TransportEvents>, TransportError>> { self.0.subscribe_named(scope, method) }
    fn close_reason(&self) -> Option<TransportClose> { self.0.close_reason() }
    fn is_closed(&self) -> bool { self.0.is_closed() }
}

#[tokio::test]
#[ignore = "local giant-page diagnosis; run explicitly with real-browser opt-in"]
async fn bounded_local_giant_page_diagnosis() {
    assert!(support::chrome::real_browser_tests_enabled(), "requires KROMETRAIL_REAL_CHROME_TESTS=1");
    let _lock = support::chrome::real_browser_lock().await;
    let _subscriber = tracing::subscriber::set_default(ProbeSubscriber);
    let root = tempfile::tempdir().unwrap();
    let fixture = support::static_fixture::FixtureServer::start().unwrap();
    let wrapper = support::chrome::ChromeWrapper::for_product(BrowserProduct::Chrome, ChromeWrapperVariant::DefaultDpi).expect("Chrome required for explicit diagnostic");
    let launcher = SystemChromeLauncher::new(LauncherConfig { profile_root: root.path().join("profiles"), startup_timeout: CAPTURE_TIMEOUT, shutdown_timeout: Duration::from_secs(4) });
    let sink = Arc::new(TestSink::new(false));
    let connector = ProductionBrowserConnector::new(Arc::new(launcher), Arc::new(ProbeFactory)).with_capture(Arc::new(TestClock::new()), Arc::new(TestIds::new()), sink.clone(), Arc::new(support::retention::AlwaysAvailableRetention), CaptureConfig::default());
    let session = connector.connect(BrowserConnectRequest::Launch(LaunchBrowser { executable: Some(wrapper.path.clone()), profile: ManagedProfile::Temporary, initial_url: Some(fixture.url()), every_nth_frame: Default::default(), focus: Default::default() })).await.unwrap();
    let result = tokio::time::timeout(Duration::from_secs(180), async {
        let target = first_target(&session).await;
        let mut events = session.subscribe().await.unwrap();
        // Keep DOM cardinality modest while independently increasing AX name payload.
        // 4,096 nodes × 16 KiB is the hard maximum synthetic text allocation.
        for text_bytes in [64, 1024, 4096, 8192, 16384] {
            eprintln!("GIANT fixture nodes=4096 text_bytes={text_bytes}");
            let expression = format!("(() => {{ const f=document.createDocumentFragment(); for(let i=0;i<4096;i++) {{const b=document.createElement('button'); b.textContent='x'.repeat({text_bytes}); f.append(b);}} document.body.replaceChildren(f); return document.querySelectorAll('button').length; }})()");
            let mutation = session.execute(BrowserOperationRequest::Evaluate(krometrail_core::ReadOnlyEvaluationRequest::new(target, expression, false).unwrap())).await;
            eprintln!("GIANT mutation_ok={}", mutation.is_ok());
            let observation = session.execute(BrowserOperationRequest::ObserveLive(LiveObservationRequest { target: PageSelection::Target(target) })).await;
            eprintln!("GIANT observation_ok={} error={:?}", observation.is_ok(), observation.as_ref().err().map(|e| e.code));
            let status = session.status().await.unwrap();
            eprintln!("GIANT state={:?} capture={:?}", status.state, status.capture.iter().map(|s| s.state()).collect::<Vec<_>>());
            if status.state != BrowserSessionState::Ready || mutation.is_err() {
                let _ = tokio::time::timeout(Duration::from_secs(90), async {
                    while let Ok(Some(event)) = events.next().await {
                        if let BrowserSessionEvent::SessionStateChanged { state } = event.event {
                            eprintln!("GIANT lifecycle={state:?}");
                            if matches!(state, BrowserSessionState::Ended | BrowserSessionState::Ready) { break; }
                        }
                    }
                }).await;
                break;
            }
        }
    }).await;
    eprintln!("GIANT deadline_expired={}", result.is_err());
    let stop = session.stop().await;
    eprintln!("GIANT stop={stop:?}");
    drop(session);
    assert!(support::chrome::process_references(root.path()).is_empty(), "owned browser must be reaped");
    assert!(result.is_ok(), "diagnostic deadline expired");
}
