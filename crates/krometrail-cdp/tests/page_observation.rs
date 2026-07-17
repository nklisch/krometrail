#![cfg(feature = "cdpkit-transport")]

mod support;

use std::sync::Arc;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use krometrail_cdp::{
    CdpTransport, CdpTransportFactory, ProductionBrowserConnector, TransportError, TransportFuture,
};
use krometrail_core::{
    BrowserConnectRequest, BrowserConnector, BrowserOperationRequest, BrowserOperationResult,
    CoordinateSpace, CssPoint, CssRect, CssSize, ElementLocator, ImageFormat, InspectPageRequest,
    LiveObservationRequest, ObservationPart, ReadOnlyEvaluationRequest, ScreenshotRequest,
    ScreenshotTarget, SnapshotPageRequest,
};
use serde_json::{Value, json};
use support::scripted_cdp::ScriptedCdp;

#[derive(Clone)]
struct ScriptedFactory(ScriptedCdp);
impl CdpTransportFactory for ScriptedFactory {
    fn connect(
        &self,
        _url: &str,
    ) -> TransportFuture<'_, Result<Arc<dyn CdpTransport>, TransportError>> {
        let transport = self.0.clone();
        Box::pin(async move { Ok(Arc::new(transport) as Arc<dyn CdpTransport>) })
    }
}

fn layout() -> Value {
    json!({
        "cssLayoutViewport":{"pageX":10.0,"pageY":20.0,"clientWidth":800.0,"clientHeight":600.0},
        "cssVisualViewport":{"pageX":10.0,"pageY":20.0,"clientWidth":800.0,"clientHeight":600.0,"scale":1.0},
        "cssContentSize":{"x":0.0,"y":0.0,"width":1000.0,"height":1800.0}
    })
}
fn identity() -> Value {
    json!({"result":{"type":"object","value":{"url":"http://fixture/","title":"Observation fixture","readiness":"complete","deviceScaleFactor":1.0}}})
}
fn history() -> Value {
    json!({"currentIndex":1,"entries":[{"id":1},{"id":2},{"id":3}]})
}
fn frame_tree() -> Value {
    frame_tree_with_loader("loader-1")
}

fn frame_tree_with_loader(loader_id: &str) -> Value {
    json!({"frameTree":{"frame":{"id":"main","loaderId":loader_id}}})
}
fn ax_tree() -> Value {
    json!({"nodes":[
        {"nodeId":"root","ignored":false,"role":{"value":"document"},"name":{"value":"Observation fixture"},"childIds":["button"],"additive":"ignored"},
        {"nodeId":"button","ignored":false,"role":{"value":"button"},"name":{"value":"Replace me"},"backendDOMNodeId":42,"properties":[{"name":"focusable","value":{"value":true}},{"name":"future","value":{"value":"ignored"}}]}
    ]})
}
fn png_base64(width: u32, height: u32) -> String {
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    bytes.extend_from_slice(&13_u32.to_be_bytes());
    bytes.extend_from_slice(b"IHDR");
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&height.to_be_bytes());
    STANDARD.encode(bytes)
}

fn script_page_observation(transport: &ScriptedCdp) {
    transport.hold_events_open();
    // Compatibility and initial visibility consume the first two Runtime responses.
    transport.push_response(
        "Runtime.evaluate",
        json!({"result":{"type":"number","value":2}}),
    );
    transport.push_response(
        "Runtime.evaluate",
        json!({"result":{"type":"string","value":"visible"}}),
    );
    transport.push_response("Runtime.evaluate", identity());
    transport.push_response(
        "Runtime.evaluate",
        json!({"result":{"type":"number","value":42}}),
    );
    // Current reference screenshot scale probe.
    transport.push_response(
        "Runtime.evaluate",
        json!({"result":{"type":"number","value":1.0}}),
    );

    for _ in 0..4 {
        transport.push_response("Page.getLayoutMetrics", layout());
    }
    transport.push_response("Page.getLayoutMetrics", layout());
    transport.push_response("Page.getNavigationHistory", history());
    transport.push_response("Page.getNavigationHistory", history());

    // Compatibility consumes the first accessibility response.
    transport.push_response("Accessibility.getFullAXTree", json!({}));
    for _ in 0..3 {
        transport.push_response("Accessibility.getFullAXTree", ax_tree());
    }
    for _ in 0..2 {
        transport.push_response("Page.getFrameTree", frame_tree());
    }
    transport.push_response(
        "Page.captureScreenshot",
        json!({"data":png_base64(100, 50)}),
    );
}

#[tokio::test]
async fn production_operation_port_routes_snapshot_screenshot_and_partial_live_evidence() {
    let transport = ScriptedCdp::chrome();
    script_page_observation(&transport);
    let connector = ProductionBrowserConnector::new(
        Arc::new(krometrail_cdp::SystemChromeLauncher::new(
            krometrail_cdp::LauncherConfig::default(),
        )),
        Arc::new(ScriptedFactory(transport.clone())),
    );
    let session = connector
        .connect(BrowserConnectRequest::Attach(
            krometrail_core::AttachBrowser::new("ws://127.0.0.1:9222/devtools/browser/scripted")
                .unwrap(),
        ))
        .await
        .unwrap();
    let target_id = session.status().await.unwrap().pages[0].target.target.id();

    let inspected = session
        .execute(
            BrowserOperationRequest::InspectPage(InspectPageRequest::new(target_id)),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::InspectPage(inspected) = inspected else {
        panic!("inspect result")
    };
    assert_eq!(inspected.url, "http://fixture/");
    assert!(inspected.navigation.can_go_back && inspected.navigation.can_go_forward);
    assert_eq!(inspected.viewport.layout_viewport.origin.x, 10.0);

    let evaluated = session
        .execute(
            BrowserOperationRequest::EvaluatePage(
                ReadOnlyEvaluationRequest::new(target_id, "21 * 2", false).unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::EvaluatePage(evaluated) = evaluated else {
        panic!("evaluation result")
    };
    assert_eq!(
        evaluated.value,
        krometrail_core::EvaluationValue::Json(json!(42))
    );

    let first = session
        .execute(
            BrowserOperationRequest::SnapshotPage(SnapshotPageRequest::new(target_id)),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::SnapshotPage(first) = first else {
        panic!("snapshot result")
    };
    assert_eq!(first.nodes.len(), 2);
    let old_reference = first.nodes[1].reference.unwrap();
    let second = session
        .execute(
            BrowserOperationRequest::SnapshotPage(SnapshotPageRequest::new(target_id)),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::SnapshotPage(second) = second else {
        panic!("snapshot result")
    };
    let current_reference = second.nodes[1].reference.unwrap();
    assert_eq!(first.generation, second.generation);
    assert_eq!(old_reference, current_reference);

    // The scripted resolver verifies the exact backend node, live state, and geometry.
    transport.push_response("Page.getFrameTree", frame_tree());
    transport.push_response("DOM.describeNode", json!({"node":{"backendNodeId":42}}));
    transport.push_response(
        "DOM.resolveNode",
        json!({"object":{"objectId":"private-object"}}),
    );
    transport.push_response(
        "Runtime.callFunctionOn",
        json!({"result":{"type":"object","value":{"connected":true,"visuallyHidden":false,"interactionBlocked":true}}}),
    );
    transport.push_response(
        "DOM.getBoxModel",
        json!({"model":{"border":[100.0,200.0,200.0,200.0,200.0,250.0,100.0,250.0]}}),
    );
    let screenshot = session
        .execute(
            BrowserOperationRequest::TakeScreenshot(
                ScreenshotRequest::new(
                    target_id,
                    ScreenshotTarget::Element(ElementLocator::Reference(old_reference)),
                    ImageFormat::Png,
                    None,
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::TakeScreenshot(screenshot) = screenshot else {
        panic!("screenshot result")
    };
    assert_eq!(
        screenshot.metadata().resolved_document_rect.origin,
        CssPoint::new(100.0, 200.0).unwrap()
    );
    assert_eq!(
        (
            screenshot.metadata().image.width(),
            screenshot.metadata().image.height()
        ),
        (100, 50)
    );

    // Viewport CSS clips are translated exactly by the fresh layout origin.
    transport.push_response(
        "Runtime.evaluate",
        json!({"result":{"type":"number","value":1.0}}),
    );
    transport.push_response("Page.captureScreenshot", json!({"data":png_base64(20, 30)}));
    let viewport_region = CssRect::new(
        CssPoint::new(5.0, 7.0).unwrap(),
        CssSize::new(20.0, 30.0).unwrap(),
    )
    .unwrap();
    let region = session
        .execute(
            BrowserOperationRequest::TakeScreenshot(
                ScreenshotRequest::new(
                    target_id,
                    ScreenshotTarget::Region {
                        rect: viewport_region,
                        space: CoordinateSpace::ViewportCss,
                    },
                    ImageFormat::Png,
                    None,
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::TakeScreenshot(region) = region else {
        panic!("region screenshot")
    };
    assert_eq!(
        region.metadata().resolved_document_rect.origin,
        CssPoint::new(15.0, 27.0).unwrap()
    );

    // Disabled and inert state is interaction-only: a visible selector remains screenshotable.
    transport.push_response(
        "Runtime.evaluate",
        json!({"result":{"type":"number","value":1.0}}),
    );
    transport.push_response("DOM.getDocument", json!({"root":{"nodeId":1}}));
    transport.push_response("DOM.querySelector", json!({"nodeId":2}));
    transport.push_response("DOM.describeNode", json!({"node":{"backendNodeId":43}}));
    transport.push_response("DOM.describeNode", json!({"node":{"backendNodeId":43}}));
    transport.push_response(
        "DOM.resolveNode",
        json!({"object":{"objectId":"disabled-visible"}}),
    );
    transport.push_response(
        "Runtime.callFunctionOn",
        json!({"result":{"type":"object","value":{"connected":true,"visuallyHidden":false,"interactionBlocked":true}}}),
    );
    transport.push_response(
        "DOM.getBoxModel",
        json!({"model":{"border":[300.0,200.0,380.0,200.0,380.0,240.0,300.0,240.0]}}),
    );
    transport.push_response("Page.captureScreenshot", json!({"data":png_base64(80, 40)}));
    let disabled = session
        .execute(
            BrowserOperationRequest::TakeScreenshot(
                ScreenshotRequest::new(
                    target_id,
                    ScreenshotTarget::Element(ElementLocator::CssSelector(
                        krometrail_core::NonEmptyText::new("#disabled-action").unwrap(),
                    )),
                    ImageFormat::Png,
                    None,
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("disabled visible selector screenshot");
    let BrowserOperationResult::TakeScreenshot(disabled) = disabled else {
        panic!("disabled selector screenshot")
    };
    assert_eq!(disabled.metadata().image.width(), 80);

    // A loader change invalidates an otherwise current generation before backing-node lookup.
    transport.push_response("Page.getLayoutMetrics", layout());
    transport.push_response(
        "Runtime.evaluate",
        json!({"result":{"type":"number","value":1.0}}),
    );
    transport.push_response("Page.getFrameTree", frame_tree_with_loader("loader-2"));
    let navigated = session
        .execute(
            BrowserOperationRequest::TakeScreenshot(
                ScreenshotRequest::new(
                    target_id,
                    ScreenshotTarget::Element(ElementLocator::Reference(current_reference)),
                    ImageFormat::Png,
                    None,
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(navigated.code, krometrail_core::ErrorCode::StaleReference);

    // Refresh against the new document, then prove live actionability is rechecked.
    transport.push_response("Page.getFrameTree", frame_tree_with_loader("loader-2"));
    let refreshed = session
        .execute(
            BrowserOperationRequest::SnapshotPage(SnapshotPageRequest::new(target_id)),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::SnapshotPage(refreshed) = refreshed else {
        panic!("refreshed snapshot")
    };
    let refreshed_reference = refreshed.nodes[1].reference.unwrap();
    transport.push_response("Page.getLayoutMetrics", layout());
    transport.push_response(
        "Runtime.evaluate",
        json!({"result":{"type":"number","value":1.0}}),
    );
    transport.push_response("Page.getFrameTree", frame_tree_with_loader("loader-2"));
    transport.push_response("DOM.describeNode", json!({"node":{"backendNodeId":42}}));
    transport.push_response(
        "DOM.resolveNode",
        json!({"object":{"objectId":"private-object-2"}}),
    );
    transport.push_response(
        "Runtime.callFunctionOn",
        json!({"result":{"type":"object","value":{"connected":true,"visuallyHidden":true,"interactionBlocked":false}}}),
    );
    let hidden = session
        .execute(
            BrowserOperationRequest::TakeScreenshot(
                ScreenshotRequest::new(
                    target_id,
                    ScreenshotTarget::Element(ElementLocator::Reference(refreshed_reference)),
                    ImageFormat::Png,
                    None,
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(
        hidden.code,
        krometrail_core::ErrorCode::ReferenceNotActionable
    );

    // Encoded output is independently checked after Chrome reports success.
    for _ in 0..3 {
        transport.push_response("Page.getLayoutMetrics", layout());
    }
    transport.push_response(
        "Runtime.evaluate",
        json!({"result":{"type":"number","value":1.0}}),
    );
    transport.push_response(
        "Page.captureScreenshot",
        json!({"data":STANDARD.encode(b"not an image")}),
    );
    let malformed_image = session
        .execute(
            BrowserOperationRequest::TakeScreenshot(
                ScreenshotRequest::new(
                    target_id,
                    ScreenshotTarget::Viewport,
                    ImageFormat::Png,
                    None,
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(
        malformed_image.code,
        krometrail_core::ErrorCode::ScreenshotFailed
    );

    // The final screenshot failure must not discard successful live page/snapshot evidence.
    transport.push_response("Runtime.evaluate", identity());
    transport.push_response(
        "Runtime.evaluate",
        json!({"result":{"type":"number","value":1.0}}),
    );
    transport.push_response("Page.getFrameTree", frame_tree_with_loader("loader-2"));
    transport.push_response("Accessibility.getFullAXTree", ax_tree());
    transport.push_failure("Page.captureScreenshot", TransportError::CommandFailed);
    let live = session
        .execute(
            BrowserOperationRequest::ObserveLive(LiveObservationRequest::new(target_id)),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::ObserveLive(live) = live else {
        panic!("live result")
    };
    assert!(matches!(live.page, ObservationPart::Available(_)));
    assert!(matches!(live.snapshot, ObservationPart::Available(_)));
    let ObservationPart::Unavailable(error) = live.screenshot else {
        panic!("screenshot must fail honestly")
    };
    assert_eq!(error.code, krometrail_core::ErrorCode::ScreenshotFailed);

    transport.push_response("Page.getFrameTree", frame_tree_with_loader("loader-2"));
    transport.push_response("Accessibility.getFullAXTree", json!({"nodes":"malformed"}));
    let malformed_ax = session
        .execute(
            BrowserOperationRequest::SnapshotPage(SnapshotPageRequest::new(target_id)),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(
        malformed_ax.code,
        krometrail_core::ErrorCode::PageObservationFailed
    );

    let calls = transport.command_calls();
    let screenshot_call = calls
        .iter()
        .find(|call| call.method == "Page.captureScreenshot")
        .unwrap();
    assert_eq!(screenshot_call.session.as_deref(), Some("session-a"));
    assert_eq!(
        screenshot_call
            .params
            .pointer("/clip/x")
            .and_then(Value::as_f64),
        Some(100.0)
    );
    assert_eq!(
        screenshot_call
            .params
            .pointer("/clip/scale")
            .and_then(Value::as_f64),
        Some(1.0)
    );
    let region_call = calls
        .iter()
        .find(|call| {
            call.method == "Page.captureScreenshot"
                && call.params.pointer("/clip/x").and_then(Value::as_f64) == Some(15.0)
        })
        .expect("viewport region command");
    assert_eq!(
        region_call
            .params
            .pointer("/clip/y")
            .and_then(Value::as_f64),
        Some(27.0)
    );
    let evaluation_call = calls
        .iter()
        .find(|call| {
            call.method == "Runtime.evaluate"
                && call.params.get("expression").and_then(Value::as_str) == Some("21 * 2")
        })
        .unwrap();
    assert_eq!(
        evaluation_call.params.get("returnByValue"),
        Some(&json!(true))
    );
    assert_eq!(
        evaluation_call.params.get("throwOnSideEffect"),
        Some(&json!(true))
    );
    assert!(evaluation_call.params.get("timeout").is_some());
    session.stop().await.unwrap();
    let terminal = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        session.execute(
            BrowserOperationRequest::InspectPage(InspectPageRequest::new(target_id)),
            krometrail_core::BrowserOperationContext::default(),
        ),
    )
    .await
    .expect("terminal actor must answer without hanging")
    .unwrap_err();
    assert_eq!(terminal.code, krometrail_core::ErrorCode::Cancelled);
}

#[test]
fn declared_region_contract_preserves_coordinate_space() {
    let rect = CssRect::new(
        CssPoint::new(5.0, 7.0).unwrap(),
        CssSize::new(20.0, 30.0).unwrap(),
    )
    .unwrap();
    let target = ScreenshotTarget::Region {
        rect,
        space: CoordinateSpace::ViewportCss,
    };
    let encoded = serde_json::to_string(&target).unwrap();
    assert!(encoded.contains("viewport_css"));
    assert_eq!(
        serde_json::from_str::<ScreenshotTarget>(&encoded).unwrap(),
        target
    );
}

#[tokio::test]
async fn opt_in_real_chrome_reports_forced_scale_without_fabricating_high_dpi() {
    if !support::chrome::real_browser_tests_enabled() {
        eprintln!("skipping forced-scale observation test; set KROMETRAIL_REAL_CHROME_TESTS=1");
        return;
    }
    let _lock = support::chrome::real_browser_lock().await;
    let Some(wrapper) = support::chrome::ChromeWrapper::for_product(
        krometrail_core::BrowserProduct::Chrome,
        support::chrome::ChromeWrapperVariant::HighDpi,
    )
    .or_else(|| {
        support::chrome::ChromeWrapper::for_product(
            krometrail_core::BrowserProduct::Chromium,
            support::chrome::ChromeWrapperVariant::HighDpi,
        )
    }) else {
        eprintln!("forced-scale observation unavailable: no Chrome/Chromium installation");
        return;
    };
    let root = support::chrome::temporary_profile_root("page-observation-scale");
    let connector = ProductionBrowserConnector::new(
        Arc::new(krometrail_cdp::SystemChromeLauncher::new(
            krometrail_cdp::LauncherConfig {
                profile_root: root.path().to_path_buf(),
                startup_timeout: std::time::Duration::from_secs(45),
                shutdown_timeout: std::time::Duration::from_secs(3),
            },
        )),
        Arc::new(
            krometrail_cdp::transport::CdpkitTransportFactory::new()
                .with_command_timeout(std::time::Duration::from_secs(15)),
        ),
    );
    let session = connector
        .connect(BrowserConnectRequest::Launch(
            krometrail_core::LaunchBrowser {
                executable: Some(wrapper.path.clone()),
                profile: krometrail_core::ManagedProfile::Temporary,
                initial_url: Some(support::chrome::page_observation_fixture_url()),
                every_nth_frame: krometrail_core::EveryNthFrame::default(),
            },
        ))
        .await
        .expect("forced-scale Chrome observation session");
    let target_id = session.status().await.unwrap().pages[0].target.target.id();
    let result = session
        .execute(
            BrowserOperationRequest::InspectPage(InspectPageRequest::new(target_id)),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("forced-scale inspection");
    let BrowserOperationResult::InspectPage(page) = result else {
        panic!("inspection")
    };
    let measured = page.viewport.device_scale_factor.get();
    eprintln!(
        "forced high-DPI request measured device scale {measured}; requested scale was {}",
        wrapper.variant.force_device_scale_factor()
    );
    assert!(measured.is_finite() && measured > 0.0);
    session.stop().await.unwrap();
}

#[tokio::test]
async fn opt_in_real_chrome_observes_fixture_and_all_screenshot_target_families() {
    if !support::chrome::real_browser_tests_enabled() {
        eprintln!("skipping real Chrome page-observation test; set KROMETRAIL_REAL_CHROME_TESTS=1");
        return;
    }
    let _lock = support::chrome::real_browser_lock().await;
    let root = support::chrome::temporary_profile_root("page-observation");
    let connector = ProductionBrowserConnector::new(
        Arc::new(krometrail_cdp::SystemChromeLauncher::new(
            krometrail_cdp::LauncherConfig {
                profile_root: root.path().to_path_buf(),
                startup_timeout: std::time::Duration::from_secs(45),
                shutdown_timeout: std::time::Duration::from_secs(3),
            },
        )),
        Arc::new(
            krometrail_cdp::transport::CdpkitTransportFactory::new()
                .with_command_timeout(std::time::Duration::from_secs(15)),
        ),
    );
    let session = connector
        .connect(BrowserConnectRequest::Launch(
            krometrail_core::LaunchBrowser {
                executable: None,
                profile: krometrail_core::ManagedProfile::Temporary,
                initial_url: Some(support::chrome::page_observation_fixture_url()),
                every_nth_frame: krometrail_core::EveryNthFrame::default(),
            },
        ))
        .await
        .expect("real observation fixture");
    let target_id = session.status().await.unwrap().pages[0].target.target.id();
    let inspected = session
        .execute(
            BrowserOperationRequest::InspectPage(InspectPageRequest::new(target_id)),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("real inspection");
    let BrowserOperationResult::InspectPage(inspected) = inspected else {
        panic!("inspection")
    };
    assert_eq!(inspected.title, "Observation fixture");
    assert!(inspected.viewport.device_scale_factor.get() > 0.0);
    let evaluated = session
        .execute(
            BrowserOperationRequest::EvaluatePage(
                ReadOnlyEvaluationRequest::new(target_id, "document.title", false).unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("real read-only evaluation");
    let BrowserOperationResult::EvaluatePage(evaluated) = evaluated else {
        panic!("evaluation")
    };
    assert_eq!(
        evaluated.value,
        krometrail_core::EvaluationValue::Json(json!("Observation fixture"))
    );
    let snapshot = session
        .execute(
            BrowserOperationRequest::SnapshotPage(SnapshotPageRequest::new(target_id)),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::SnapshotPage(snapshot) = snapshot else {
        panic!("snapshot")
    };
    assert!(
        snapshot
            .nodes
            .iter()
            .any(|node| node.name.as_deref() == Some("Replace me"))
    );
    assert!(
        snapshot
            .nodes
            .iter()
            .any(|node| node.name.as_deref() == Some("Shadow action"))
    );
    assert!(
        snapshot
            .nodes
            .iter()
            .any(|node| node.name.as_deref() == Some("Same-origin frame"))
    );
    let reference = snapshot
        .nodes
        .iter()
        .find_map(|node| {
            (node.name.as_deref() == Some("Replace me"))
                .then_some(node.reference)
                .flatten()
        })
        .expect("button reference");
    let targets = [
        ScreenshotTarget::Element(ElementLocator::Reference(reference)),
        ScreenshotTarget::Element(ElementLocator::CssSelector(
            krometrail_core::NonEmptyText::new("#replace-me").unwrap(),
        )),
        ScreenshotTarget::Element(ElementLocator::CssSelector(
            krometrail_core::NonEmptyText::new("#disabled-action").unwrap(),
        )),
        ScreenshotTarget::Element(ElementLocator::CssSelector(
            krometrail_core::NonEmptyText::new("#inert-action").unwrap(),
        )),
        ScreenshotTarget::Region {
            rect: CssRect::new(
                CssPoint::new(0.0, 0.0).unwrap(),
                CssSize::new(100.0, 100.0).unwrap(),
            )
            .unwrap(),
            space: CoordinateSpace::ViewportCss,
        },
        ScreenshotTarget::Region {
            rect: CssRect::new(
                CssPoint::new(0.0, 0.0).unwrap(),
                CssSize::new(100.0, 100.0).unwrap(),
            )
            .unwrap(),
            space: CoordinateSpace::DocumentCss,
        },
        ScreenshotTarget::Viewport,
        ScreenshotTarget::FullPage,
    ];
    for target in targets {
        let requested = target.clone();
        let request = || {
            BrowserOperationRequest::TakeScreenshot(
                ScreenshotRequest::new(target_id, requested.clone(), ImageFormat::Png, None)
                    .unwrap(),
            )
        };
        // Chrome occasionally fails one capture while changing surface size between variants.
        // The public contract marks screenshot failure as safe to retry; one bounded retry tests
        // that recovery without weakening the required successful payload/provenance assertions.
        let result = match session
            .execute(
                request(),
                krometrail_core::BrowserOperationContext::default(),
            )
            .await
        {
            Err(error) if error.code == krometrail_core::ErrorCode::ScreenshotFailed => {
                session
                    .execute(
                        request(),
                        krometrail_core::BrowserOperationContext::default(),
                    )
                    .await
            }
            result => result,
        }
        .unwrap_or_else(|error| panic!("real screenshot {requested:?}: {error:?}"));
        let BrowserOperationResult::TakeScreenshot(image) = result else {
            panic!("screenshot")
        };
        assert!(!image.bytes().is_empty());
        assert!(image.metadata().device_scale_factor.get() > 0.0);
    }

    let replaced = session
        .execute(
            BrowserOperationRequest::TakeScreenshot(
                ScreenshotRequest::new(
                    target_id,
                    ScreenshotTarget::Element(ElementLocator::Reference(reference)),
                    ImageFormat::Png,
                    None,
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(replaced.code, krometrail_core::ErrorCode::StaleReference);
    let refreshed = session
        .execute(
            BrowserOperationRequest::SnapshotPage(SnapshotPageRequest::new(target_id)),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("snapshot after backing-node replacement");
    let BrowserOperationResult::SnapshotPage(refreshed) = refreshed else {
        panic!("refreshed snapshot")
    };
    assert!(refreshed.generation > snapshot.generation);

    let live = session
        .execute(
            BrowserOperationRequest::ObserveLive(LiveObservationRequest::new(target_id)),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::ObserveLive(live) = live else {
        panic!("live")
    };
    assert!(matches!(live.page, ObservationPart::Available(_)));
    assert!(matches!(live.snapshot, ObservationPart::Available(_)));
    assert!(matches!(live.screenshot, ObservationPart::Available(_)));
    session.stop().await.unwrap();
}
