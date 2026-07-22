#![cfg(feature = "cdpkit-transport")]

mod support;

use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use krometrail_cdp::{
    CaptureConfig, CdpTransport, CdpTransportFactory, ProductionBrowserConnector, TransportError,
    TransportFuture,
};
use krometrail_core::{
    BrowserActionRequest, BrowserConnectRequest, BrowserConnector, BrowserOperationRequest,
    BrowserOperationResult, ByteOffset, CaptureGap, CaptureStreamState, ClickRequest,
    CoordinateSpace, CreatePageRequest, CssPoint, DialogAction, DragRequest, ElementLocator,
    EncodedFrame, ErrorCode, FillMode, FillRequest, FrameAccess, FrameAddress, HandleDialogRequest,
    HoverRequest, IdSource, IdValue, ImageFormat, InteractionLocator, InteractionOutcome, KeyChord,
    ListFramesRequest, ListPageAssetsRequest, Modifiers, MonotonicClock, MouseButton,
    NavigatePageRequest, ObservationPart, ObservedTime, PageSelection, PageSequence, PortFuture,
    PressKeysRequest, QueryPageRequest, QueryPageResult, ReadClipboardRequest,
    ReadOnlyEvaluationRequest, RecordingSink, ScreenshotRequest, ScreenshotTarget, ScrollDelta,
    ScrollRequest, SegmentId, SelectOptionRequest, SelectPageRequest, SelectValue,
    SemanticDocumentScope, SemanticQuery, SemanticQueryOutcome, SemanticTextMatch,
    SemanticTextMatchMode, SessionId, SetViewportRequest, SnapshotPageRequest, UploadFilesRequest,
    ValidatedFilePath, ViewportGuidanceCode, ViewportIntent, ViewportOverride, ViewportPreset,
    WaitCondition, WaitOutcome, WaitRequest, WriteClipboardRequest,
};
use serde_json::{Value, json};
use support::scripted_cdp::ScriptedCdp;
use uuid::Uuid;

#[derive(Default)]
struct QualificationRecordingSink {
    next_offset: AtomicU64,
}

impl RecordingSink for QualificationRecordingSink {
    fn append_frame(
        &self,
        _frame: EncodedFrame,
    ) -> PortFuture<'_, krometrail_core::Result<FrameAddress>> {
        let offset = self.next_offset.fetch_add(1, Ordering::Relaxed) + 1;
        Box::pin(std::future::ready(Ok(FrameAddress::new(
            SegmentId::from_uuid(Uuid::from_u128(1)),
            ByteOffset::new(offset),
        ))))
    }

    fn append_gap(&self, _gap: CaptureGap) -> PortFuture<'_, krometrail_core::Result<()>> {
        Box::pin(std::future::ready(Ok(())))
    }

    fn flush(&self, _session_id: SessionId) -> PortFuture<'_, krometrail_core::Result<()>> {
        Box::pin(std::future::ready(Ok(())))
    }
}

struct QualificationClock(Instant);

impl MonotonicClock for QualificationClock {
    fn now(&self) -> ObservedTime {
        ObservedTime::from_nanos(u64::try_from(self.0.elapsed().as_nanos()).unwrap_or(u64::MAX))
    }
}

struct QualificationIds;

impl IdSource for QualificationIds {
    fn next(&self) -> IdValue {
        IdValue::from_uuid(Uuid::new_v4())
    }
}

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
    layout_at(10.0, 20.0)
}
fn layout_at(page_x: f64, page_y: f64) -> Value {
    json!({
        "cssLayoutViewport":{"pageX":page_x,"pageY":page_y,"clientWidth":800.0,"clientHeight":600.0},
        "cssVisualViewport":{"pageX":page_x,"pageY":page_y,"clientWidth":800.0,"clientHeight":600.0,"scale":1.0},
        "cssContentSize":{"x":0.0,"y":0.0,"width":1600.0,"height":2400.0}
    })
}
fn identity() -> Value {
    json!({"result":{"value":{"url":"http://fixture/","title":"Interaction fixture","readiness":"complete","deviceScaleFactor":1.0}}})
}
fn history() -> Value {
    json!({"currentIndex":0,"entries":[{"id":1}]})
}
fn frame_tree() -> Value {
    json!({"frameTree":{"frame":{"id":"main","loaderId":"loader-1"}}})
}
fn ax_tree() -> Value {
    json!({"nodes":[{"nodeId":"root","ignored":false,"role":{"value":"document"}}]})
}
fn png_base64() -> String {
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    bytes.extend_from_slice(&13_u32.to_be_bytes());
    bytes.extend_from_slice(b"IHDR");
    bytes.extend_from_slice(&800_u32.to_be_bytes());
    bytes.extend_from_slice(&600_u32.to_be_bytes());
    STANDARD.encode(bytes)
}
fn startup_script(transport: &ScriptedCdp) {
    transport.hold_events_open();
    transport.push_response("Runtime.evaluate", json!({"result":{"value":2}}));
    transport.push_response("Runtime.evaluate", json!({"result":{"value":"visible"}}));
    transport.push_response("Accessibility.getFullAXTree", json!({}));
}
fn observation_script(transport: &ScriptedCdp) {
    transport.push_response("Runtime.evaluate", json!({"result":{"value":true}}));
    transport.push_response("Runtime.evaluate", json!({"result":{"value":true}}));
    transport.push_response("Runtime.evaluate", identity());
    transport.push_response("Runtime.evaluate", json!({"result":{"value":1.0}}));
    transport.push_response("Page.getLayoutMetrics", layout());
    transport.push_response("Page.getLayoutMetrics", layout());
    transport.push_response("Page.getNavigationHistory", history());
    transport.push_response("Page.getFrameTree", frame_tree());
    transport.push_response("Accessibility.getFullAXTree", ax_tree());
    transport.push_response("Page.captureScreenshot", json!({"data":png_base64()}));
}
async fn scripted_session(transport: ScriptedCdp) -> Arc<dyn krometrail_core::BrowserSessionPort> {
    scripted_session_with_evidence(transport, support::evidence_sink()).await
}

async fn scripted_session_with_evidence(
    transport: ScriptedCdp,
    evidence: Arc<dyn krometrail_core::InteractionEvidenceSink>,
) -> Arc<dyn krometrail_core::BrowserSessionPort> {
    let connector = ProductionBrowserConnector::new(
        Arc::new(krometrail_cdp::SystemChromeLauncher::new(
            krometrail_cdp::LauncherConfig::default(),
        )),
        Arc::new(ScriptedFactory(transport)),
    )
    .with_interaction_evidence(evidence);
    connector
        .connect(BrowserConnectRequest::Attach(
            krometrail_core::AttachBrowser::new("ws://127.0.0.1:9222/devtools/browser/scripted")
                .unwrap(),
        ))
        .await
        .unwrap()
}

async fn page_cursor(session: &Arc<dyn krometrail_core::BrowserSessionPort>) -> PageSequence {
    let result = session
        .execute(
            BrowserOperationRequest::ListPageContexts(
                krometrail_core::ListPageContextsRequest::default(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::ListPageContexts(result) = result else {
        panic!("page context inventory")
    };
    result.cursor
}

#[tokio::test]
async fn production_port_rejects_empty_coordinate_hits_and_returns_anchored_live_evidence() {
    let transport = ScriptedCdp::chrome();
    startup_script(&transport);
    transport.push_response("Page.getLayoutMetrics", layout());
    transport.push_response(
        "Runtime.evaluate",
        json!({"result":{"value":{"tagName":"DIV","x":0,"y":0,"width":20,"height":20}}}),
    );
    observation_script(&transport);
    let session = scripted_session(transport.clone()).await;
    let target = session.status().await.unwrap().pages[0].target.target.id();
    let calls = transport.command_calls();
    // The compatibility probe has its own temporary attach. The final attach is the production
    // session and must restore domains once before its visibility probe.
    let attach_index = calls
        .iter()
        .rposition(|call| call.method == "Target.attachToTarget")
        .expect("initial target attach");
    assert_eq!(
        calls[attach_index..attach_index + 5]
            .iter()
            .map(|call| call.method.as_str())
            .collect::<Vec<_>>(),
        vec![
            "Target.attachToTarget",
            "Page.enable",
            "Page.setLifecycleEventsEnabled",
            "Runtime.enable",
            "Accessibility.enable"
        ]
    );
    assert_eq!(
        calls[attach_index..]
            .iter()
            .filter(|call| {
                call.session.as_deref() == Some("session-a")
                    && matches!(
                        call.method.as_str(),
                        "Page.enable" | "Runtime.enable" | "Accessibility.enable"
                    )
            })
            .count(),
        3,
        "initial attach restores each session domain exactly once"
    );
    let request = ClickRequest::new(
        PageSelection::Target(target),
        InteractionLocator::coordinate(
            CssPoint::new(20.0, 30.0).unwrap(),
            CoordinateSpace::ViewportCss,
        )
        .unwrap(),
        MouseButton::Left,
        Modifiers {
            control: true,
            ..Modifiers::default()
        },
        2,
        false,
    )
    .unwrap();
    let result = session
        .execute(
            BrowserOperationRequest::Click(request),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::Click(result) = result else {
        panic!("click result")
    };
    assert_eq!(result.record.outcome, InteractionOutcome::Dispatched);
    assert_eq!(result.record.context.target_id, target);
    assert!(result.record.context.started_at <= result.record.dispatch_time);
    assert!(result.record.dispatch_time <= result.record.live_observation_time);
    assert!(result.record.parent_batch.is_none());
    let calls = transport.command_calls();
    let compositor = calls
        .iter()
        .position(|call| {
            call.method == "Runtime.evaluate"
                && call
                    .params
                    .get("expression")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|expression| expression.contains("requestAnimationFrame"))
        })
        .expect("automatic interaction observation waits for compositor readiness");
    let screenshot = calls
        .iter()
        .position(|call| call.method == "Page.captureScreenshot")
        .expect("interaction returns a post-action screenshot");
    assert!(compositor < screenshot);
    let mouse = calls
        .iter()
        .filter(|call| {
            call.method == "Input.dispatchMouseEvent"
                && call.params.get("clickCount") == Some(&json!(2))
        })
        .collect::<Vec<_>>();
    assert_eq!(mouse.len(), 3);
    assert_eq!(mouse[1].params["modifiers"], json!(2));
    assert_eq!(mouse[1].params["clickCount"], json!(2));
    assert!(
        mouse
            .iter()
            .all(|call| call.session.as_deref() == Some("session-a"))
    );
    session.stop().await.unwrap();

    let transport = ScriptedCdp::chrome();
    startup_script(&transport);
    transport.push_response("Page.getLayoutMetrics", layout());
    transport.push_response("Runtime.evaluate", json!({"result":{"value":null}}));
    let session = scripted_session(transport).await;
    let target = session.status().await.unwrap().pages[0].target.target.id();
    let request = ClickRequest::new(
        PageSelection::Target(target),
        InteractionLocator::coordinate(
            CssPoint::new(-10.0, -10.0).unwrap(),
            CoordinateSpace::ViewportCss,
        )
        .unwrap(),
        MouseButton::Left,
        Modifiers::default(),
        1,
        false,
    )
    .unwrap();
    let error = session
        .execute(
            BrowserOperationRequest::Click(request),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, krometrail_core::ErrorCode::InteractionFailed);
    assert!(error.message.as_str().contains("no_hit_target"));
    session.stop().await.unwrap();
}

#[tokio::test]
async fn dispatched_click_degrades_when_post_action_observation_command_fails() {
    let transport = ScriptedCdp::chrome();
    startup_script(&transport);
    transport.push_response("Page.getLayoutMetrics", layout());
    transport.push_response(
        "Runtime.evaluate",
        json!({"result":{"value":{"tagName":"BUTTON","x":0,"y":0,"width":20,"height":20}}}),
    );
    transport.push_failure("Page.getNavigationHistory", TransportError::CommandFailed);
    observation_script(&transport);
    let session = scripted_session(transport).await;
    let target = session.status().await.unwrap().pages[0].target.target.id();
    let request = ClickRequest::new(
        PageSelection::Target(target),
        InteractionLocator::coordinate(
            CssPoint::new(20.0, 30.0).unwrap(),
            CoordinateSpace::ViewportCss,
        )
        .unwrap(),
        MouseButton::Left,
        Modifiers::default(),
        1,
        false,
    )
    .unwrap();

    let result = session
        .execute(
            BrowserOperationRequest::Click(request),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("a dispatched click keeps its interaction record");
    let BrowserOperationResult::Click(result) = result else {
        panic!("click result")
    };
    assert_eq!(result.record.outcome, InteractionOutcome::Dispatched);
    match result.observation.page {
        ObservationPart::Unavailable(error) => {
            assert_eq!(error.code, ErrorCode::PageObservationFailed);
            assert!(error.message.as_str().contains("observation command"));
        }
        ObservationPart::Available(_) => panic!("failed page inspection was reported as available"),
    }
    session.stop().await.unwrap();
}

fn actionability_state(extra: Value) -> Value {
    let mut state = json!({
        "connected": true,
        "visuallyHidden": false,
        "interactionBlocked": false,
        "tagName": "BUTTON",
        "inputType": null,
        "isEditable": false,
        "isSelect": false,
        "isFileInput": false,
    });
    state
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    json!({"result": {"value": state}})
}

/// One selector resolution: document, selector, backend identity (twice),
/// live object, actionability/state probe, and geometry.
fn script_selector_resolution(transport: &ScriptedCdp, state: Value) {
    transport.push_response("DOM.getDocument", json!({"root":{"nodeId":1}}));
    transport.push_response("DOM.querySelector", json!({"nodeId":2}));
    transport.push_response("DOM.describeNode", json!({"node":{"backendNodeId":42}}));
    transport.push_response("DOM.describeNode", json!({"node":{"backendNodeId":42}}));
    transport.push_response(
        "DOM.resolveNode",
        json!({"object":{"objectId":"private-object"}}),
    );
    transport.push_response("Runtime.callFunctionOn", state);
    transport.push_response(
        "DOM.getBoxModel",
        json!({"model":{"border":[120.0,80.0,220.0,80.0,220.0,120.0,120.0,120.0]}}),
    );
}

/// The post-action state re-probe of the pre-resolved backing node.
fn script_post_probe(transport: &ScriptedCdp, state: Value) {
    transport.push_response("DOM.describeNode", json!({"node":{"backendNodeId":42}}));
    transport.push_response(
        "DOM.resolveNode",
        json!({"object":{"objectId":"private-object"}}),
    );
    transport.push_response("Runtime.callFunctionOn", state);
}

fn selector_click(target: krometrail_core::TargetId, css: &str) -> ClickRequest {
    ClickRequest::new(
        PageSelection::Target(target),
        selector(css),
        MouseButton::Left,
        Modifiers::default(),
        1,
        false,
    )
    .unwrap()
}

#[tokio::test]
async fn checkbox_click_postcondition_reports_the_checked_delta() {
    let transport = ScriptedCdp::chrome();
    startup_script(&transport);
    let unchecked =
        actionability_state(json!({"checked": false, "tagName": "INPUT", "inputType": "checkbox"}));
    // Pointer preparation resolves twice (before and after scroll).
    script_selector_resolution(&transport, unchecked.clone());
    script_selector_resolution(&transport, unchecked);
    script_post_probe(
        &transport,
        actionability_state(json!({"checked": true, "tagName": "INPUT", "inputType": "checkbox"})),
    );
    observation_script(&transport);
    let session = scripted_session(transport.clone()).await;
    let target = session.status().await.unwrap().pages[0].target.target.id();

    let result = session
        .execute(
            BrowserOperationRequest::Click(selector_click(target, "#checkbox")),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::Click(result) = result else {
        panic!("click result")
    };
    let postcondition = result.record.postcondition;
    assert_eq!(
        postcondition.target.node,
        krometrail_core::TargetNodeOutcome::Present
    );
    assert_eq!(postcondition.target.checked.before, Some(false));
    assert_eq!(postcondition.target.checked.after, Some(true));
    assert_eq!(postcondition.target.checked.changed, Some(true));
    assert_eq!(postcondition.target.value_length_changed, None);
    assert_eq!(postcondition.page.url_changed, Some(false));
    assert!(!postcondition.page.navigation_lifecycle_observed);
    // No growth after reconciliation: the honest "no new page observed" fact
    // keeps the cursor so a caller can still chain wait_for_page.
    let new_pages = postcondition
        .new_pages
        .as_ref()
        .expect("reconciliation ran for the state-changing click");
    assert!(new_pages.pages.is_empty());
    assert_eq!(new_pages.omitted, 0);
    session.stop().await.unwrap();
}

#[tokio::test]
async fn same_route_click_postcondition_reports_no_url_change_or_lifecycle_signal() {
    let transport = ScriptedCdp::chrome();
    startup_script(&transport);
    transport.push_response("Page.getLayoutMetrics", layout());
    transport.push_response(
        "Runtime.evaluate",
        json!({"result":{"value":{"tagName":"A","x":0,"y":0,"width":20,"height":20}}}),
    );
    observation_script(&transport);
    let session = scripted_session(transport.clone()).await;
    let target = session.status().await.unwrap().pages[0].target.target.id();

    let result = session
        .execute(
            BrowserOperationRequest::Click(coordinate_click(target)),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::Click(result) = result else {
        panic!("click result")
    };
    let postcondition = result.record.postcondition;
    // The link click stayed on the same route: an observed non-change, not
    // an absence of observation.
    assert_eq!(postcondition.page.url_changed, Some(false));
    assert!(!postcondition.page.navigation_lifecycle_observed);
    assert_eq!(
        postcondition.target.node,
        krometrail_core::TargetNodeOutcome::NotEvaluated
    );
    session.stop().await.unwrap();
}

#[tokio::test]
async fn fill_postcondition_reports_a_value_length_change() {
    let transport = ScriptedCdp::chrome();
    startup_script(&transport);
    script_selector_resolution(
        &transport,
        actionability_state(json!({
            "tagName": "INPUT",
            "inputType": "text",
            "isEditable": true,
            "valueLength": 0,
        })),
    );
    // Replace-mode clearing resolves the editable object and verifies
    // emptiness before inserting text.
    transport.push_response(
        "DOM.resolveNode",
        json!({"object":{"objectId":"private-object"}}),
    );
    transport.push_response("Runtime.callFunctionOn", json!({"result":{"value":true}}));
    transport.push_response("Runtime.callFunctionOn", json!({"result":{"value":0}}));
    script_post_probe(
        &transport,
        actionability_state(json!({
            "tagName": "INPUT",
            "inputType": "text",
            "isEditable": true,
            "valueLength": 8,
        })),
    );
    observation_script(&transport);
    let session = scripted_session(transport.clone()).await;
    let target = session.status().await.unwrap().pages[0].target.target.id();

    let result = session
        .execute(
            BrowserOperationRequest::Fill(
                FillRequest::new(
                    PageSelection::Target(target),
                    selector("#text-input"),
                    "replaced",
                    FillMode::Replace,
                    false,
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::Fill(result) = result else {
        panic!("fill result")
    };
    let postcondition = result.record.postcondition;
    assert_eq!(
        postcondition.target.node,
        krometrail_core::TargetNodeOutcome::Present
    );
    assert_eq!(postcondition.target.value_length_changed, Some(true));
    assert_eq!(
        postcondition.target.checked,
        krometrail_core::FlagObservation::unobserved()
    );
    session.stop().await.unwrap();
}

#[tokio::test]
async fn degraded_post_probe_is_unobserved_and_keeps_the_dispatched_action_successful() {
    let transport = ScriptedCdp::chrome();
    startup_script(&transport);
    let button = actionability_state(json!({"checked": false}));
    script_selector_resolution(&transport, button.clone());
    script_selector_resolution(&transport, button);
    // The post-action re-probe loses its transport; the proven dispatch and
    // its observation must be unaffected. The probe never ran to completion,
    // so the target outcome is unobserved — a probe failure is not evidence
    // that the node detached.
    transport.push_failure("DOM.describeNode", TransportError::CommandFailed);
    observation_script(&transport);
    let session = scripted_session(transport.clone()).await;
    let target = session.status().await.unwrap().pages[0].target.target.id();

    let result = session
        .execute(
            BrowserOperationRequest::Click(selector_click(target, "#button")),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("post-probe degradation never fails a dispatched interaction");
    let BrowserOperationResult::Click(result) = result else {
        panic!("click result")
    };
    assert_eq!(result.record.outcome, InteractionOutcome::Dispatched);
    let postcondition = result.record.postcondition;
    assert_eq!(
        postcondition.target.node,
        krometrail_core::TargetNodeOutcome::Unobserved
    );
    assert_eq!(postcondition.target.checked.before, Some(false));
    assert_eq!(postcondition.target.checked.after, None);
    assert_eq!(postcondition.target.checked.changed, None);
    assert!(matches!(
        result.observation.page,
        ObservationPart::Available(_)
    ));
    session.stop().await.unwrap();
}

#[tokio::test]
async fn malformed_post_probe_payload_is_unobserved_not_a_detachment_claim() {
    let transport = ScriptedCdp::chrome();
    startup_script(&transport);
    let button = actionability_state(json!({"checked": false}));
    script_selector_resolution(&transport, button.clone());
    script_selector_resolution(&transport, button);
    // The probe response lacks the boolean `connected` fact; defaulting it to
    // false would fabricate a detachment observation.
    script_post_probe(&transport, json!({"result": {"value": {"checked": true}}}));
    observation_script(&transport);
    let session = scripted_session(transport.clone()).await;
    let target = session.status().await.unwrap().pages[0].target.target.id();

    let result = session
        .execute(
            BrowserOperationRequest::Click(selector_click(target, "#button")),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::Click(result) = result else {
        panic!("click result")
    };
    let postcondition = result.record.postcondition;
    assert_eq!(
        postcondition.target.node,
        krometrail_core::TargetNodeOutcome::Unobserved
    );
    assert_eq!(postcondition.target.checked.after, None);
    session.stop().await.unwrap();
}

#[tokio::test]
async fn disconnected_post_probe_reports_genuine_detachment() {
    let transport = ScriptedCdp::chrome();
    startup_script(&transport);
    let button = actionability_state(json!({"checked": false}));
    script_selector_resolution(&transport, button.clone());
    script_selector_resolution(&transport, button);
    // The probe ran and observed the backing node disconnected: this is the
    // one case that supports a detachment claim.
    script_post_probe(
        &transport,
        actionability_state(json!({"connected": false, "checked": true})),
    );
    observation_script(&transport);
    let session = scripted_session(transport.clone()).await;
    let target = session.status().await.unwrap().pages[0].target.target.id();

    let result = session
        .execute(
            BrowserOperationRequest::Click(selector_click(target, "#button")),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::Click(result) = result else {
        panic!("click result")
    };
    let postcondition = result.record.postcondition;
    assert_eq!(
        postcondition.target.node,
        krometrail_core::TargetNodeOutcome::DetachedOrReplaced
    );
    // A detached node's readable state never enters the after-facts.
    assert_eq!(postcondition.target.checked.after, None);
    assert_eq!(postcondition.target.checked.changed, None);
    session.stop().await.unwrap();
}

#[tokio::test]
async fn page_scoped_press_keys_observes_page_facts_without_a_target() {
    let transport = ScriptedCdp::chrome();
    startup_script(&transport);
    observation_script(&transport);
    let session = scripted_session(transport.clone()).await;
    let target = session.status().await.unwrap().pages[0].target.target.id();

    let result = session
        .execute(
            BrowserOperationRequest::PressKeys(
                PressKeysRequest::new(
                    PageSelection::Target(target),
                    None,
                    vec![KeyChord::new("Escape").unwrap()],
                    false,
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::PressKeys(result) = result else {
        panic!("press keys result")
    };
    let postcondition = result.record.postcondition;
    assert_eq!(
        postcondition.target.node,
        krometrail_core::TargetNodeOutcome::NotEvaluated
    );
    assert_eq!(
        postcondition.target.checked,
        krometrail_core::FlagObservation::unobserved()
    );
    assert_eq!(postcondition.page.url_changed, Some(false));
    session.stop().await.unwrap();
}

fn coordinate_click(target: krometrail_core::TargetId) -> ClickRequest {
    ClickRequest::new(
        PageSelection::Target(target),
        InteractionLocator::coordinate(
            CssPoint::new(20.0, 30.0).unwrap(),
            CoordinateSpace::ViewportCss,
        )
        .unwrap(),
        MouseButton::Left,
        Modifiers::default(),
        1,
        false,
    )
    .unwrap()
}

/// The live popup repro: a click handler opens a window that suspends the
/// opener before Chrome acknowledges the gesture commands, so the
/// press/release responses are lost while the input itself was queued. The
/// click must stay a dispatched interaction, not a hard error.
#[tokio::test]
async fn dispatched_click_survives_lost_gesture_acknowledgement() {
    let transport = ScriptedCdp::chrome();
    startup_script(&transport);
    transport.push_response("Page.getLayoutMetrics", layout());
    transport.push_response(
        "Runtime.evaluate",
        json!({"result":{"value":{"tagName":"BUTTON","x":0,"y":0,"width":20,"height":20}}}),
    );
    // Both pointer moves (target preparation and gesture) are acknowledged;
    // the press/release pair loses both responses.
    transport.push_response("Input.dispatchMouseEvent", json!({}));
    transport.push_response("Input.dispatchMouseEvent", json!({}));
    transport.push_failure("Input.dispatchMouseEvent", TransportError::CommandFailed);
    transport.push_failure("Input.dispatchMouseEvent", TransportError::CommandFailed);
    observation_script(&transport);
    let session = scripted_session(transport).await;
    let target = session.status().await.unwrap().pages[0].target.target.id();

    let result = session
        .execute(
            BrowserOperationRequest::Click(coordinate_click(target)),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("a queued gesture with lost acknowledgements stays dispatched");
    let BrowserOperationResult::Click(result) = result else {
        panic!("click result")
    };
    assert_eq!(result.record.outcome, InteractionOutcome::Dispatched);
    session.stop().await.unwrap();
}

/// A rejected gesture command (protocol-level refusal, not a lost response)
/// remains a hard dispatch failure.
#[tokio::test]
async fn rejected_click_gesture_command_stays_a_hard_error() {
    let transport = ScriptedCdp::chrome();
    startup_script(&transport);
    transport.push_response("Page.getLayoutMetrics", layout());
    transport.push_response(
        "Runtime.evaluate",
        json!({"result":{"value":{"tagName":"BUTTON","x":0,"y":0,"width":20,"height":20}}}),
    );
    transport.push_response("Input.dispatchMouseEvent", json!({}));
    transport.push_response("Input.dispatchMouseEvent", json!({}));
    transport.push_response("Input.dispatchMouseEvent", json!({}));
    transport.push_failure("Input.dispatchMouseEvent", TransportError::Protocol);
    let session = scripted_session(transport).await;
    let target = session.status().await.unwrap().pages[0].target.target.id();

    let error = session
        .execute(
            BrowserOperationRequest::Click(coordinate_click(target)),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::InteractionFailed);
    assert!(error.message.as_str().contains("input command"));
    session.stop().await.unwrap();
}

#[tokio::test]
async fn element_click_uses_box_model_viewport_coordinates_after_nonzero_scroll() {
    let transport = ScriptedCdp::chrome();
    startup_script(&transport);
    transport.push_response("DOM.getDocument", json!({"root":{"nodeId":1}}));
    transport.push_response("DOM.querySelector", json!({"nodeId":2}));
    transport.push_response("DOM.describeNode", json!({"node":{"backendNodeId":42}}));
    transport.push_response("DOM.describeNode", json!({"node":{"backendNodeId":42}}));
    transport.push_response(
        "DOM.resolveNode",
        json!({"object":{"objectId":"private-object"}}),
    );
    transport.push_response(
        "Runtime.callFunctionOn",
        json!({"result":{"value":{"connected":true,"visuallyHidden":false,"interactionBlocked":false,"tagName":"BUTTON","inputType":null,"isEditable":false,"isSelect":false,"isFileInput":false}}}),
    );
    // Chrome reports DOM.getBoxModel quads in viewport CSS space. The non-zero page offsets must
    // therefore validate bounds, not be subtracted from the element center sent to Input.
    transport.push_response(
        "DOM.getBoxModel",
        json!({"model":{"border":[120.0,80.0,220.0,80.0,220.0,120.0,120.0,120.0]}}),
    );
    // Element pointer preparation scrolls and then repeats selector resolution/actionability so
    // layout-triggered replacement cannot receive input at stale geometry.
    transport.push_response("DOM.getDocument", json!({"root":{"nodeId":1}}));
    transport.push_response("DOM.querySelector", json!({"nodeId":2}));
    transport.push_response("DOM.describeNode", json!({"node":{"backendNodeId":42}}));
    transport.push_response("DOM.describeNode", json!({"node":{"backendNodeId":42}}));
    transport.push_response(
        "DOM.resolveNode",
        json!({"object":{"objectId":"private-object"}}),
    );
    transport.push_response(
        "Runtime.callFunctionOn",
        json!({"result":{"value":{"connected":true,"visuallyHidden":false,"interactionBlocked":false,"tagName":"BUTTON","inputType":null,"isEditable":false,"isSelect":false,"isFileInput":false}}}),
    );
    transport.push_response(
        "DOM.getBoxModel",
        json!({"model":{"border":[120.0,80.0,220.0,80.0,220.0,120.0,120.0,120.0]}}),
    );
    transport.push_response("Page.getLayoutMetrics", layout_at(400.0, 900.0));
    observation_script(&transport);
    let session = scripted_session(transport.clone()).await;
    let target = session.status().await.unwrap().pages[0].target.target.id();

    session
        .execute(
            BrowserOperationRequest::Click(
                ClickRequest::new(
                    PageSelection::Target(target),
                    selector("#scrolled-click-target"),
                    MouseButton::Left,
                    Modifiers::default(),
                    1,
                    false,
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();

    let mouse = transport
        .command_calls()
        .into_iter()
        .filter(|call| {
            call.method == "Input.dispatchMouseEvent" && call.params["x"] == json!(170.0)
        })
        .collect::<Vec<_>>();
    assert_eq!(mouse.len(), 3);
    assert!(
        mouse
            .iter()
            .all(|call| call.params["x"] == json!(170.0) && call.params["y"] == json!(100.0))
    );
    session.stop().await.unwrap();
}

#[tokio::test]
async fn selector_replacement_during_scroll_never_receives_pointer_input() {
    let transport = ScriptedCdp::chrome();
    startup_script(&transport);
    for backend_node_id in [42, 43] {
        transport.push_response("DOM.getDocument", json!({"root":{"nodeId":1}}));
        transport.push_response("DOM.querySelector", json!({"nodeId":2}));
        transport.push_response(
            "DOM.describeNode",
            json!({"node":{"backendNodeId":backend_node_id}}),
        );
        transport.push_response(
            "DOM.describeNode",
            json!({"node":{"backendNodeId":backend_node_id}}),
        );
        transport.push_response(
            "DOM.resolveNode",
            json!({"object":{"objectId":"private-object"}}),
        );
        transport.push_response(
            "Runtime.callFunctionOn",
            json!({"result":{"value":{"connected":true,"visuallyHidden":false,"interactionBlocked":false,"tagName":"BUTTON","inputType":null,"isEditable":false,"isSelect":false,"isFileInput":false}}}),
        );
        transport.push_response(
            "DOM.getBoxModel",
            json!({"model":{"border":[120.0,80.0,220.0,80.0,220.0,120.0,120.0,120.0]}}),
        );
    }
    let session = scripted_session(transport.clone()).await;
    let target = session.status().await.unwrap().pages[0].target.target.id();
    let error = session
        .execute(
            BrowserOperationRequest::Click(
                ClickRequest::new(
                    PageSelection::Target(target),
                    selector("#scrolled-click-target"),
                    MouseButton::Left,
                    Modifiers::default(),
                    1,
                    false,
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(
        error.code,
        krometrail_core::ErrorCode::ReferenceNotActionable
    );
    let pointer_dispatches = transport
        .command_calls()
        .into_iter()
        .filter(|call| {
            call.method == "Input.dispatchMouseEvent"
                && matches!(
                    call.params.get("type").and_then(Value::as_str),
                    Some("mousePressed" | "mouseReleased")
                )
        })
        .collect::<Vec<_>>();
    assert!(pointer_dispatches.is_empty(), "{pointer_dispatches:?}");
    session.stop().await.unwrap();
}

#[test]
fn sensitive_action_sanitization_never_contains_full_secrets_or_paths() {
    let target = krometrail_core::TargetId::from_uuid(uuid::Uuid::from_u128(1));
    let locator = InteractionLocator::Element(ElementLocator::CssSelector(
        krometrail_core::NonEmptyText::new("#input").unwrap(),
    ));
    for secret in ["p@ssword", "tok_live_abc123", "482901"] {
        let fill = FillRequest::new(
            PageSelection::Target(target),
            locator.clone(),
            secret,
            FillMode::Replace,
            false,
        )
        .unwrap();
        let sanitized = fill.sanitize();
        let fill_json = serde_json::to_string(sanitized.as_json()).unwrap();
        assert!(!fill_json.contains(secret));
        assert!(sanitized.as_json().get("value_preview").is_none());
        assert_eq!(
            sanitized.as_json()["value_length"],
            json!(secret.chars().count())
        );
    }
    let upload = UploadFilesRequest::new(
        PageSelection::Target(target),
        locator.clone(),
        vec![ValidatedFilePath::new("/private/directory/evidence.txt").unwrap()],
    )
    .unwrap();
    let upload_json = serde_json::to_string(upload.sanitize().as_json()).unwrap();
    assert!(upload_json.contains("evidence.txt"));
    assert!(!upload_json.contains("private"));
    assert!(!upload_json.contains("directory"));
    let dialog = HandleDialogRequest {
        target: PageSelection::Target(target),
        action: DialogAction::Accept {
            prompt_text: Some(krometrail_core::NonEmptyText::new("private prompt").unwrap()),
        },
    };
    let dialog_json = serde_json::to_string(dialog.sanitize().as_json()).unwrap();
    assert!(!dialog_json.contains("private prompt"));
    assert!(dialog_json.contains("prompt_text_length"));
}

fn selector(value: &str) -> InteractionLocator {
    InteractionLocator::Element(ElementLocator::CssSelector(
        krometrail_core::NonEmptyText::new(value).unwrap(),
    ))
}
async fn await_fixture_ready(
    session: &Arc<dyn krometrail_core::BrowserSessionPort>,
    target: krometrail_core::TargetId,
) {
    for _ in 0..20 {
        if evaluate(
            session,
            target,
            "document.readyState === 'complete' && typeof window.fixtureState === 'object'",
        )
        .await
            == json!(true)
        {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("interaction fixture did not become ready");
}

async fn evaluate(
    session: &Arc<dyn krometrail_core::BrowserSessionPort>,
    target: krometrail_core::TargetId,
    expression: &str,
) -> Value {
    let result = session
        .execute(
            BrowserOperationRequest::EvaluatePage(
                ReadOnlyEvaluationRequest::new(target, expression, false).unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap_or_else(|error| panic!("evaluation {expression:?} failed: {error:?}"));
    let BrowserOperationResult::EvaluatePage(value) = result else {
        panic!("evaluate")
    };
    match value.value {
        krometrail_core::EvaluationValue::Json(value) => value,
        _ => Value::Null,
    }
}

async fn wait_for_page_expression(
    session: &Arc<dyn krometrail_core::BrowserSessionPort>,
    target: krometrail_core::TargetId,
    expression: &str,
) {
    let result = session
        .execute(
            BrowserOperationRequest::Wait(
                WaitRequest::new(
                    PageSelection::Target(target),
                    WaitCondition::Page {
                        expression: krometrail_core::NonEmptyText::new(expression).unwrap(),
                    },
                    std::time::Duration::from_secs(30),
                    std::time::Duration::from_millis(25),
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap_or_else(|error| panic!("page wait {expression:?} failed: {error:?}"));
    let BrowserOperationResult::Wait(result) = result else {
        panic!("page wait result")
    };
    assert!(
        matches!(result.outcome, WaitOutcome::Satisfied { .. }),
        "page wait {expression:?} returned {:?}",
        result.outcome
    );
}

async fn query_page(
    session: &Arc<dyn krometrail_core::BrowserSessionPort>,
    target: krometrail_core::TargetId,
    query: SemanticQuery,
    scope: Option<krometrail_core::NodeReference>,
    max_matches: u16,
) -> QueryPageResult {
    let result = session
        .execute(
            BrowserOperationRequest::QueryPage(
                QueryPageRequest::new(PageSelection::Target(target), query, scope, max_matches)
                    .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("semantic page query");
    let BrowserOperationResult::QueryPage(result) = result else {
        panic!("semantic query result")
    };
    *result
}

fn exact_semantic_text(value: &str) -> SemanticTextMatch {
    SemanticTextMatch::new(value, SemanticTextMatchMode::Exact, false).unwrap()
}

#[tokio::test]
async fn opt_in_real_chrome_synchronizes_dialog_open_and_close() {
    if !support::chrome::real_browser_tests_enabled() {
        eprintln!("skipping real Chrome dialog test; set KROMETRAIL_REAL_CHROME_TESTS=1");
        return;
    }
    let _lock = support::chrome::real_browser_lock().await;
    let root = support::chrome::temporary_profile_root("verified-interactions-dialog");
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
    )
    .with_interaction_evidence(support::evidence_sink());
    let session = connector
        .connect(BrowserConnectRequest::Launch(
            krometrail_core::LaunchBrowser {
                executable: None,
                profile: krometrail_core::ManagedProfile::Temporary,
                initial_url: Some(support::chrome::verified_interactions_fixture_url()),
                every_nth_frame: krometrail_core::EveryNthFrame::default(),
                focus: krometrail_core::BrowserFocusPolicy::default(),
            },
        ))
        .await
        .expect("dialog fixture");
    let target = session.status().await.unwrap().pages[0].target.target.id();
    await_fixture_ready(&session, target).await;
    let page = PageSelection::Target(target);
    let scheduled = session
        .execute(
            BrowserOperationRequest::Click(
                ClickRequest::new(
                    page,
                    selector("#confirm-target"),
                    MouseButton::Left,
                    Modifiers::default(),
                    1,
                    false,
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("schedule dialog");
    let BrowserOperationResult::Click(_scheduled) = scheduled else {
        panic!("click result")
    };
    session
        .execute(
            BrowserOperationRequest::HandleDialog(HandleDialogRequest {
                target: page,
                action: DialogAction::Accept { prompt_text: None },
            }),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("accept synchronized dialog");
    assert_eq!(
        evaluate(
            &session,
            target,
            "document.querySelector('#dialog-output').value"
        )
        .await,
        json!("accepted")
    );
    session.stop().await.unwrap();
}

#[tokio::test]
async fn opt_in_real_chrome_checkbox_click_reports_a_checked_postcondition_delta() {
    if !support::chrome::real_browser_tests_enabled() {
        eprintln!(
            "skipping real Chrome postcondition qualification; set KROMETRAIL_REAL_CHROME_TESTS=1"
        );
        return;
    }
    let _lock = support::chrome::real_browser_lock().await;
    let root = support::chrome::temporary_profile_root("verified-postcondition");
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
    )
    .with_interaction_evidence(support::evidence_sink());
    let session = connector
        .connect(BrowserConnectRequest::Launch(
            krometrail_core::LaunchBrowser {
                executable: None,
                profile: krometrail_core::ManagedProfile::Temporary,
                initial_url: Some(support::chrome::verified_interactions_fixture_url()),
                every_nth_frame: krometrail_core::EveryNthFrame::default(),
                focus: krometrail_core::BrowserFocusPolicy::default(),
            },
        ))
        .await
        .expect("postcondition fixture");
    let target = session.status().await.unwrap().pages[0].target.target.id();
    await_fixture_ready(&session, target).await;

    let result = session
        .execute(
            BrowserOperationRequest::Click(
                ClickRequest::new(
                    PageSelection::Target(target),
                    selector("#checkbox"),
                    MouseButton::Left,
                    Modifiers::default(),
                    1,
                    false,
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("checkbox click");
    let BrowserOperationResult::Click(result) = result else {
        panic!("click result")
    };
    let postcondition = result.record.postcondition;
    assert_eq!(
        postcondition.target.node,
        krometrail_core::TargetNodeOutcome::Present
    );
    assert_eq!(postcondition.target.checked.before, Some(false));
    assert_eq!(postcondition.target.checked.after, Some(true));
    assert_eq!(postcondition.target.checked.changed, Some(true));
    assert_eq!(postcondition.page.url_changed, Some(false));
    assert_eq!(
        evaluate(
            &session,
            target,
            "document.querySelector('#checkbox').checked"
        )
        .await,
        json!(true)
    );
    session.stop().await.unwrap();
}

#[tokio::test]
async fn opt_in_real_chrome_qualifies_explicit_clipboard_without_sentinel_leaks() {
    if !support::chrome::real_browser_tests_enabled() {
        eprintln!(
            "skipping real Chrome clipboard qualification; set KROMETRAIL_REAL_CHROME_TESTS=1"
        );
        return;
    }
    let _lock = support::chrome::real_browser_lock().await;
    let root = support::chrome::temporary_profile_root("verified-clipboard");
    let evidence = Arc::new(support::RecordingEvidenceFake::default());
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
    )
    .with_interaction_evidence(evidence.clone());
    let session = connector
        .connect(BrowserConnectRequest::Launch(
            krometrail_core::LaunchBrowser {
                executable: None,
                profile: krometrail_core::ManagedProfile::Temporary,
                initial_url: Some(support::chrome::verified_interactions_fixture_url()),
                every_nth_frame: krometrail_core::EveryNthFrame::default(),
                focus: krometrail_core::BrowserFocusPolicy::Foreground,
            },
        ))
        .await
        .expect("real clipboard fixture");
    let status = session.status().await.unwrap();
    let target = status.pages[0].target.target.id();
    await_fixture_ready(&session, target).await;
    let mut events = session.subscribe().await.unwrap();
    let sentinel = "krometrail-clipboard-sentinel-7d31";
    let write = session
        .execute(
            BrowserOperationRequest::WriteClipboard(WriteClipboardRequest {
                target: PageSelection::Target(target),
                text: sentinel.to_owned(),
            }),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await;

    let mut non_explicit_artifacts = vec![serde_json::to_string(&status).unwrap()];
    match write {
        Ok(result) => {
            let BrowserOperationResult::WriteClipboard(result) = result else {
                panic!("clipboard write result")
            };
            non_explicit_artifacts.push(format!("{result:?}"));
            let read = session
                .execute(
                    BrowserOperationRequest::ReadClipboard(ReadClipboardRequest {
                        target: PageSelection::Target(target),
                    }),
                    krometrail_core::BrowserOperationContext::default(),
                )
                .await;
            match read {
                Ok(BrowserOperationResult::ReadClipboard(value)) => {
                    assert_eq!(value.text, sentinel);
                    assert_eq!(value.utf8_bytes, sentinel.len() as u64);
                }
                Err(error)
                    if matches!(
                        error.code,
                        ErrorCode::Unsupported | ErrorCode::InteractionFailed
                    ) =>
                {
                    non_explicit_artifacts.push(serde_json::to_string(&error).unwrap());
                }
                Ok(_) => panic!("clipboard read result"),
                Err(error) => panic!("unexpected clipboard read failure: {error:?}"),
            }
        }
        Err(error)
            if matches!(
                error.code,
                ErrorCode::Unsupported | ErrorCode::InteractionFailed
            ) =>
        {
            assert!(error.recovery.is_some());
            non_explicit_artifacts.push(serde_json::to_string(&error).unwrap());
        }
        Err(error) => panic!("unexpected clipboard write failure: {error:?}"),
    }

    for record in evidence.records() {
        non_explicit_artifacts.push(serde_json::to_string(&record).unwrap());
    }
    for _ in 0..16 {
        let Ok(event) =
            tokio::time::timeout(std::time::Duration::from_millis(5), events.next()).await
        else {
            break;
        };
        match event.unwrap() {
            Some(event) => non_explicit_artifacts.push(serde_json::to_string(&event).unwrap()),
            None => break,
        }
    }
    assert!(
        non_explicit_artifacts
            .iter()
            .all(|artifact| !artifact.contains(sentinel)),
        "clipboard plaintext escaped its explicit request/read boundary"
    );
    session.stop().await.unwrap();
}

#[tokio::test]
async fn opt_in_real_chrome_executes_verified_interaction_families() {
    if !support::chrome::real_browser_tests_enabled() {
        eprintln!("skipping real Chrome verified interactions; set KROMETRAIL_REAL_CHROME_TESTS=1");
        return;
    }
    let _lock = support::chrome::real_browser_lock().await;
    let root = support::chrome::temporary_profile_root("verified-interactions");
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
    )
    .with_interaction_evidence(support::evidence_sink());
    let session = connector
        .connect(BrowserConnectRequest::Launch(
            krometrail_core::LaunchBrowser {
                executable: None,
                profile: krometrail_core::ManagedProfile::Temporary,
                initial_url: Some(support::chrome::verified_interactions_fixture_url()),
                every_nth_frame: krometrail_core::EveryNthFrame::default(),
                focus: krometrail_core::BrowserFocusPolicy::default(),
            },
        ))
        .await
        .expect("real interaction fixture");
    let target = session.status().await.unwrap().pages[0].target.target.id();
    await_fixture_ready(&session, target).await;
    let page = PageSelection::Target(target);

    session
        .execute(
            BrowserOperationRequest::Click(
                ClickRequest::new(
                    page,
                    selector("#click-target"),
                    MouseButton::Left,
                    Modifiers::default(),
                    1,
                    false,
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("click");
    assert_eq!(
        evaluate(&session, target, "window.fixtureState.clicks").await,
        json!(1)
    );
    session
        .execute(
            BrowserOperationRequest::Fill(
                FillRequest::new(
                    page,
                    selector("#text-input"),
                    "replaced",
                    FillMode::Replace,
                    false,
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("fill replace");
    session
        .execute(
            BrowserOperationRequest::Fill(
                FillRequest::new(
                    page,
                    selector("#text-input"),
                    "+appended",
                    FillMode::Append,
                    false,
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("fill append");
    assert_eq!(
        evaluate(
            &session,
            target,
            "document.querySelector('#text-input').value"
        )
        .await,
        json!("replaced+appended")
    );
    session
        .execute(
            BrowserOperationRequest::Fill(
                FillRequest::new(
                    page,
                    selector("#password-input"),
                    "y".repeat(17),
                    FillMode::Replace,
                    false,
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("password fill replace");
    assert_eq!(
        evaluate(
            &session,
            target,
            "document.querySelector('#password-input').value.length"
        )
        .await,
        json!(17)
    );
    session
        .execute(
            BrowserOperationRequest::PressKeys(
                PressKeysRequest::new(
                    page,
                    Some(selector("#password-input")),
                    vec![KeyChord::new("Enter").unwrap()],
                    false,
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("password form submit");
    assert_eq!(
        evaluate(&session, target, "window.fixtureState.passwordSubmits").await,
        json!(1)
    );
    session
        .execute(
            BrowserOperationRequest::PressKeys(
                PressKeysRequest::new(
                    page,
                    Some(selector("#text-input")),
                    vec![
                        KeyChord::new("Control+S").unwrap(),
                        KeyChord::new("Enter").unwrap(),
                    ],
                    false,
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("press keys");
    assert!(
        evaluate(&session, target, "window.fixtureState.keydowns")
            .await
            .as_array()
            .is_some_and(|values| values
                .iter()
                .any(|value| value == "Control+s" || value == "Control+S"))
    );
    session
        .execute(
            BrowserOperationRequest::SelectOption(
                SelectOptionRequest::new(
                    page,
                    selector("#select"),
                    SelectValue::Label(krometrail_core::NonEmptyText::new("Two").unwrap()),
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("select label");
    assert_eq!(
        evaluate(&session, target, "document.querySelector('#select').value").await,
        json!("two")
    );
    session
        .execute(
            BrowserOperationRequest::Hover(HoverRequest {
                target: page,
                locator: selector("#hover-target"),
            }),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("hover");
    assert!(
        evaluate(
            &session,
            target,
            "getComputedStyle(document.querySelector('#hover-target')).backgroundColor"
        )
        .await
        .as_str()
        .is_some_and(|value| value.contains("128"))
    );
    session
        .execute(
            BrowserOperationRequest::Click(
                ClickRequest::new(
                    page,
                    InteractionLocator::coordinate(
                        CssPoint::new(550.0, 120.0).unwrap(),
                        CoordinateSpace::ViewportCss,
                    )
                    .unwrap(),
                    MouseButton::Left,
                    Modifiers::default(),
                    1,
                    false,
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("coordinate click");
    assert_eq!(
        evaluate(&session, target, "window.fixtureState.coordinateClicks").await,
        json!(1)
    );
    let empty = session
        .execute(
            BrowserOperationRequest::Click(
                ClickRequest::new(
                    page,
                    InteractionLocator::coordinate(
                        CssPoint::new(-10.0, -10.0).unwrap(),
                        CoordinateSpace::ViewportCss,
                    )
                    .unwrap(),
                    MouseButton::Left,
                    Modifiers::default(),
                    1,
                    false,
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap_err();
    assert!(empty.message.as_str().contains("no_hit_target"));

    let drag = session
        .execute(
            BrowserOperationRequest::Drag(DragRequest {
                target: page,
                source: selector("#drag-source"),
                destination: selector("#drop-target"),
            }),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await;
    assert!(
        drag.is_ok(),
        "drag dispatch must complete explicitly: {drag:?}"
    );
    session
        .execute(
            BrowserOperationRequest::Scroll(ScrollRequest {
                target: page,
                delta: ScrollDelta::ByOffset { dx: 0.0, dy: 150.0 },
            }),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("offset scroll");
    session
        .execute(
            BrowserOperationRequest::Scroll(ScrollRequest {
                target: page,
                delta: ScrollDelta::ToElement(match selector("#scroll-destination") {
                    InteractionLocator::Element(value) => value,
                    _ => unreachable!(),
                }),
            }),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("element scroll");
    let upload_path =
        std::env::temp_dir().join(format!("krometrail-real-upload-{}", std::process::id()));
    std::fs::write(&upload_path, b"real upload").unwrap();
    session
        .execute(
            BrowserOperationRequest::UploadFiles(
                UploadFilesRequest::new(
                    page,
                    selector("#file-input"),
                    vec![ValidatedFilePath::new(upload_path.to_string_lossy()).unwrap()],
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("upload");
    assert_eq!(
        evaluate(
            &session,
            target,
            "document.querySelector('#file-input').files.length"
        )
        .await,
        json!(1)
    );
    let _ = std::fs::remove_file(upload_path);
    let snapshot = session
        .execute(
            BrowserOperationRequest::SnapshotPage(SnapshotPageRequest::new(target)),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::SnapshotPage(snapshot) = snapshot else {
        panic!("snapshot")
    };
    let reference = snapshot
        .nodes
        .iter()
        .find_map(|node| {
            (node.name.as_deref() == Some("Click target"))
                .then_some(node.reference)
                .flatten()
        })
        .expect("click reference");
    session
        .execute(
            BrowserOperationRequest::SnapshotPage(SnapshotPageRequest::new(target)),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    session
        .execute(
            BrowserOperationRequest::Click(
                ClickRequest::new(
                    page,
                    InteractionLocator::Element(ElementLocator::Reference(reference)),
                    MouseButton::Left,
                    Modifiers::default(),
                    1,
                    false,
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("same-document reference remains valid after another snapshot");
    evaluate(&session, target, "window.replaceClickTarget()").await;
    let stale = session
        .execute(
            BrowserOperationRequest::Click(
                ClickRequest::new(
                    page,
                    InteractionLocator::Element(ElementLocator::Reference(reference)),
                    MouseButton::Left,
                    Modifiers::default(),
                    1,
                    false,
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(stale.code, krometrail_core::ErrorCode::StaleReference);

    // Schedule the modal after the triggering click has returned so the serialized actor can
    // receive the separate dialog operation rather than blocking behind page JavaScript.
    session
        .execute(
            BrowserOperationRequest::Click(
                ClickRequest::new(
                    page,
                    selector("#confirm-target"),
                    MouseButton::Left,
                    Modifiers::default(),
                    1,
                    false,
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("schedule confirm dialog");
    if let Err(error) = session
        .execute(
            BrowserOperationRequest::HandleDialog(HandleDialogRequest {
                target: page,
                action: DialogAction::Accept { prompt_text: None },
            }),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
    {
        panic!("accept dialog: {error:?}");
    }
    assert_eq!(
        evaluate(
            &session,
            target,
            "document.querySelector('#dialog-output').value"
        )
        .await,
        json!("accepted")
    );

    session
        .execute(
            BrowserOperationRequest::Click(
                ClickRequest::new(
                    page,
                    selector("#confirm-target"),
                    MouseButton::Left,
                    Modifiers::default(),
                    1,
                    false,
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("schedule second confirm dialog");
    session
        .execute(
            BrowserOperationRequest::HandleDialog(HandleDialogRequest {
                target: page,
                action: DialogAction::Dismiss,
            }),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("dismiss dialog");
    assert_eq!(
        evaluate(
            &session,
            target,
            "document.querySelector('#dialog-output').value"
        )
        .await,
        json!("dismissed")
    );

    session
        .execute(
            BrowserOperationRequest::Scroll(ScrollRequest {
                target: page,
                delta: ScrollDelta::ToElement(match selector("#scrolled-click-target") {
                    InteractionLocator::Element(value) => value,
                    _ => unreachable!(),
                }),
            }),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("scroll distant click target into view");
    assert_eq!(
        evaluate(&session, target, "window.scrollY > 0").await,
        json!(true)
    );
    session
        .execute(
            BrowserOperationRequest::Click(
                ClickRequest::new(
                    page,
                    selector("#scrolled-click-target"),
                    MouseButton::Left,
                    Modifiers::default(),
                    1,
                    false,
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("click element after document scroll");
    assert_eq!(
        evaluate(&session, target, "window.fixtureState.scrolledClicks").await,
        json!(1)
    );

    session.stop().await.unwrap();
}

#[tokio::test]
async fn opt_in_real_chrome_resolves_semantic_queries_to_exact_references() {
    if !support::chrome::real_browser_tests_enabled() {
        eprintln!(
            "skipping real Chrome semantic-query qualification; set KROMETRAIL_REAL_CHROME_TESTS=1"
        );
        return;
    }
    let _lock = support::chrome::real_browser_lock().await;
    let root = support::chrome::temporary_profile_root("verified-semantic-query");
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
    )
    .with_interaction_evidence(support::evidence_sink());
    let fixture_url = support::chrome::verified_interactions_fixture_url();
    let session = connector
        .connect(BrowserConnectRequest::Launch(
            krometrail_core::LaunchBrowser {
                executable: None,
                profile: krometrail_core::ManagedProfile::Temporary,
                initial_url: Some(fixture_url.clone()),
                every_nth_frame: krometrail_core::EveryNthFrame::default(),
                focus: krometrail_core::BrowserFocusPolicy::default(),
            },
        ))
        .await
        .expect("semantic query fixture");
    let target = session.status().await.unwrap().pages[0].target.target.id();
    await_fixture_ready(&session, target).await;

    let by_role = query_page(
        &session,
        target,
        SemanticQuery::role("button", Some(exact_semantic_text("Semantic save"))).unwrap(),
        None,
        20,
    )
    .await;
    assert_eq!(by_role.outcome, SemanticQueryOutcome::Unique);
    let save_reference = by_role.matches[0].reference;

    for query in [
        SemanticQuery::Label {
            text: exact_semantic_text("Explicit semantic input"),
        },
        SemanticQuery::Label {
            text: exact_semantic_text("Wrapped semantic input"),
        },
        SemanticQuery::Label {
            text: exact_semantic_text("ARIA semantic input Required field"),
        },
        SemanticQuery::Text {
            text: exact_semantic_text("Semantic save"),
        },
        SemanticQuery::test_id("semantic-primary").unwrap(),
    ] {
        assert_eq!(
            query_page(&session, target, query, None, 20).await.outcome,
            SemanticQueryOutcome::Unique
        );
    }

    let unscoped = query_page(
        &session,
        target,
        SemanticQuery::Text {
            text: exact_semantic_text("Repeated semantic action"),
        },
        None,
        20,
    )
    .await;
    assert_eq!(unscoped.outcome, SemanticQueryOutcome::Ambiguous);
    assert_eq!(unscoped.matches.len(), 3);
    let scope = query_page(
        &session,
        target,
        SemanticQuery::role("region", Some(exact_semantic_text("Semantic scope"))).unwrap(),
        None,
        20,
    )
    .await;
    assert_eq!(scope.outcome, SemanticQueryOutcome::Unique);
    let scoped = query_page(
        &session,
        target,
        SemanticQuery::Text {
            text: exact_semantic_text("Repeated semantic action"),
        },
        Some(scope.matches[0].reference),
        20,
    )
    .await;
    assert_eq!(scoped.outcome, SemanticQueryOutcome::Ambiguous);
    assert_eq!(scoped.matches.len(), 2);

    session
        .execute(
            BrowserOperationRequest::Click(
                ClickRequest::new(
                    PageSelection::Target(target),
                    InteractionLocator::Element(ElementLocator::Reference(save_reference)),
                    MouseButton::Left,
                    Modifiers::default(),
                    1,
                    false,
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("semantic reference click");
    assert_eq!(
        evaluate(&session, target, "window.fixtureState.semanticClicks").await,
        json!(1)
    );

    session
        .execute(
            BrowserOperationRequest::NavigatePage(
                NavigatePageRequest::new(
                    PageSelection::Target(target),
                    format!("{fixture_url}?semantic-document-replacement=1"),
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("replace semantic document");
    await_fixture_ready(&session, target).await;
    let stale = session
        .execute(
            BrowserOperationRequest::Click(
                ClickRequest::new(
                    PageSelection::Target(target),
                    InteractionLocator::Element(ElementLocator::Reference(save_reference)),
                    MouseButton::Left,
                    Modifiers::default(),
                    1,
                    false,
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(stale.code, krometrail_core::ErrorCode::StaleReference);
    session.stop().await.unwrap();
}

#[tokio::test]
async fn opt_in_real_chrome_qualifies_viewport_presets_guidance_and_target_isolation() {
    if !support::chrome::real_browser_tests_enabled() {
        eprintln!(
            "skipping real Chrome viewport qualification; set KROMETRAIL_REAL_CHROME_TESTS=1"
        );
        return;
    }
    let _lock = support::chrome::real_browser_lock().await;
    let root = support::chrome::temporary_profile_root("verified-viewport");
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
    )
    .with_interaction_evidence(support::evidence_sink());
    let fixture_url = support::chrome::verified_interactions_fixture_url()
        .replace("index.html", "no-viewport-meta.html");
    let session = connector
        .connect(BrowserConnectRequest::Launch(
            krometrail_core::LaunchBrowser {
                executable: None,
                profile: krometrail_core::ManagedProfile::Temporary,
                initial_url: Some(fixture_url.clone()),
                every_nth_frame: krometrail_core::EveryNthFrame::default(),
                focus: krometrail_core::BrowserFocusPolicy::default(),
            },
        ))
        .await
        .expect("real viewport fixture");
    let first = session.status().await.unwrap().pages[0].target.target.id();
    await_fixture_ready(&session, first).await;
    let native_first = evaluate(&session, first, "visualViewport.width").await;

    let created = session
        .execute(
            BrowserOperationRequest::CreatePage(
                CreatePageRequest::new(Some(fixture_url.clone())).unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("create isolation target");
    let BrowserOperationResult::CreatePage(created) = created else {
        panic!("create result")
    };
    let second = created.interaction.target_id;
    await_fixture_ready(&session, second).await;
    let native_second = evaluate(&session, second, "visualViewport.width").await;

    session
        .execute(
            BrowserOperationRequest::SelectPage(SelectPageRequest { target_id: first }),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("foreground viewport target");

    let responsive = session
        .execute(
            BrowserOperationRequest::SetViewport(SetViewportRequest {
                target: PageSelection::Target(first),
                viewport: ViewportOverride::Preset {
                    preset: ViewportPreset::ResponsiveSmall,
                },
            }),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("apply responsive viewport");
    let BrowserOperationResult::SetViewport(responsive) = responsive else {
        panic!("responsive viewport result")
    };
    let ObservationPart::Available(responsive_effective) = responsive.effective else {
        panic!("responsive effective viewport")
    };
    assert_eq!(
        responsive.materialization.intent,
        ViewportIntent::ResponsiveCss
    );
    assert_eq!(
        responsive.materialization.preset,
        Some(ViewportPreset::ResponsiveSmall)
    );
    assert_eq!(responsive_effective.css_size.width, 390.0);
    assert_eq!(responsive_effective.layout_css_size.width, 390.0);
    assert!(!responsive_effective.mobile && !responsive_effective.touch);
    assert!(responsive.guidance.is_empty());

    let screenshot = session
        .execute(
            BrowserOperationRequest::TakeScreenshot(
                ScreenshotRequest::new(first, ScreenshotTarget::Viewport, ImageFormat::Png, None)
                    .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("responsive screenshot");
    let BrowserOperationResult::TakeScreenshot(screenshot) = screenshot else {
        panic!("responsive screenshot result")
    };
    assert_eq!(screenshot.metadata().image.width(), 390);
    assert_eq!(screenshot.metadata().image.height(), 844);

    session
        .execute(
            BrowserOperationRequest::NavigatePage(
                NavigatePageRequest::new(
                    PageSelection::Target(first),
                    "https://krometrail.dev/".to_owned(),
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("navigate to public documentation under responsive emulation");
    let public_docs = session
        .execute(
            BrowserOperationRequest::SetViewport(SetViewportRequest {
                target: PageSelection::Target(first),
                viewport: ViewportOverride::Preset {
                    preset: ViewportPreset::ResponsiveSmall,
                },
            }),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("acknowledge responsive viewport on overflowing public documentation");
    let BrowserOperationResult::SetViewport(public_docs) = public_docs else {
        panic!("public documentation viewport result")
    };
    let ObservationPart::Available(public_docs) = public_docs.effective else {
        panic!("public documentation effective viewport")
    };
    assert_eq!(public_docs.layout_css_size.width, 390.0);
    assert_eq!(public_docs.layout_css_size.height, 844.0);
    assert!(public_docs.css_size.width < public_docs.layout_css_size.width);

    session
        .execute(
            BrowserOperationRequest::NavigatePage(
                NavigatePageRequest::new(PageSelection::Target(first), fixture_url.clone())
                    .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("return to no-viewport fixture");
    await_fixture_ready(&session, first).await;

    let configured = session
        .execute(
            BrowserOperationRequest::SetViewport(SetViewportRequest {
                target: PageSelection::Target(first),
                viewport: ViewportOverride::Preset {
                    preset: ViewportPreset::MobilePhone,
                },
            }),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("apply mobile viewport");
    let BrowserOperationResult::SetViewport(configured) = configured else {
        panic!("viewport result")
    };
    let ObservationPart::Available(effective) = configured.effective else {
        panic!("effective viewport")
    };
    assert_eq!(
        configured.materialization.intent,
        ViewportIntent::MobileDevice
    );
    assert_eq!(
        configured.materialization.preset,
        Some(ViewportPreset::MobilePhone)
    );
    assert!(!configured.materialization.user_agent_emulated);
    assert_eq!(effective.css_size.width, 390.0);
    assert_eq!(effective.css_size.height, 844.0);
    assert!(effective.layout_css_size.width >= 585.0);
    assert!(!effective.viewport_meta_present);
    assert_eq!(effective.device_scale_factor.get(), 3.0);
    assert!(effective.mobile && effective.touch && effective.override_active);
    assert_eq!(configured.guidance.len(), 1);
    assert_eq!(
        configured.guidance[0].code,
        ViewportGuidanceCode::LikelyMissingViewportMetadata
    );
    assert_eq!(
        evaluate(
            &session,
            first,
            "({width:visualViewport.width,height:visualViewport.height,dpr:devicePixelRatio,touch:navigator.maxTouchPoints,responsiveWidth:document.querySelector('#responsive-probe').offsetWidth})"
        )
        .await,
        // A page without a viewport meta tag retains Chrome's standards-defined
        // 980px layout viewport even while its visual viewport is emulated at 390px.
        json!({"width":390,"height":844,"dpr":3,"touch":1,"responsiveWidth":11})
    );
    assert_eq!(
        evaluate(&session, second, "visualViewport.width").await,
        native_second
    );

    session
        .execute(
            BrowserOperationRequest::NavigatePage(
                NavigatePageRequest::new(
                    PageSelection::Target(first),
                    format!("{fixture_url}#viewport-persistence"),
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("navigate under viewport override");
    await_fixture_ready(&session, first).await;
    assert_eq!(
        evaluate(
            &session,
            first,
            "[visualViewport.width,visualViewport.height,devicePixelRatio,navigator.maxTouchPoints,document.querySelector('#responsive-probe').offsetWidth]"
        )
        .await,
        json!([390, 844, 3, 1, 11])
    );

    let cleared = session
        .execute(
            BrowserOperationRequest::SetViewport(SetViewportRequest {
                target: PageSelection::Target(first),
                viewport: ViewportOverride::Clear,
            }),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("clear viewport override");
    let BrowserOperationResult::SetViewport(cleared) = cleared else {
        panic!("clear viewport result")
    };
    let ObservationPart::Available(cleared) = cleared.effective else {
        panic!("clear effective viewport")
    };
    assert!(!cleared.override_active && !cleared.mobile && !cleared.touch);
    assert_eq!(
        evaluate(&session, first, "navigator.maxTouchPoints").await,
        json!(0)
    );
    assert_eq!(
        evaluate(&session, second, "visualViewport.width").await,
        native_second
    );
    session
        .execute(
            BrowserOperationRequest::NavigatePage(
                NavigatePageRequest::new(
                    PageSelection::Target(first),
                    format!("{fixture_url}#viewport-cleared"),
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("navigate after clearing viewport");
    await_fixture_ready(&session, first).await;
    assert_eq!(
        evaluate(&session, first, "visualViewport.width").await,
        native_first
    );
    session.stop().await.unwrap();
}

#[tokio::test]
async fn opt_in_real_chrome_qualifies_frame_actions_staleness_and_bounded_assets() {
    if !support::chrome::real_browser_tests_enabled() {
        eprintln!(
            "skipping real Chrome browser-context qualification; set KROMETRAIL_REAL_CHROME_TESTS=1"
        );
        return;
    }
    let _lock = support::chrome::real_browser_lock().await;
    let mut fixture = support::static_fixture::FixtureServer::start().expect("context fixture");
    let mut cross_origin_fixture =
        support::static_fixture::FixtureServer::start().expect("cross-origin context fixture");
    let fixture_url = format!(
        "{}?cross_origin_port={}#context-frame",
        fixture.browser_contexts_url(),
        cross_origin_fixture.address().port()
    );
    let root = support::chrome::temporary_profile_root("verified-browser-contexts");
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
    )
    .with_capture(
        Arc::new(QualificationClock(Instant::now())),
        Arc::new(QualificationIds),
        Arc::new(QualificationRecordingSink::default()),
        Arc::new(support::retention::AlwaysAvailableRetention),
        CaptureConfig::default(),
    )
    .with_interaction_evidence(support::evidence_sink());
    let session = connector
        .connect(BrowserConnectRequest::Launch(
            krometrail_core::LaunchBrowser {
                executable: None,
                profile: krometrail_core::ManagedProfile::Temporary,
                initial_url: Some(fixture_url.clone()),
                every_nth_frame: krometrail_core::EveryNthFrame::default(),
                focus: krometrail_core::BrowserFocusPolicy::Foreground,
            },
        ))
        .await
        .expect("real browser-context fixture");
    let status = session.status().await.unwrap();
    let target = status
        .pages
        .iter()
        .find(|page| page.target.target.url().contains("/browser-contexts/"))
        .unwrap_or_else(|| panic!("browser-context fixture target: {status:#?}"))
        .target
        .target
        .id();
    let last_frames = Arc::new(std::sync::Mutex::new(None));
    let observed_frames = Arc::clone(&last_frames);
    let frame_session = Arc::clone(&session);
    let child = tokio::time::timeout(std::time::Duration::from_secs(30), async move {
        loop {
            let frames = frame_session
                .execute(
                    BrowserOperationRequest::ListFrames(ListFramesRequest {
                        target: PageSelection::Target(target),
                    }),
                    krometrail_core::BrowserOperationContext::default(),
                )
                .await
                .expect("frame inventory");
            let BrowserOperationResult::ListFrames(frames) = frames else {
                panic!("frame inventory result")
            };
            *observed_frames.lock().unwrap() = Some(frames.clone());
            if let Some(child) = frames.frames.iter().find(|frame| {
                frame.depth == 1 && frame.access == FrameAccess::SameOriginSameProcess
            }) {
                break child.reference.clone();
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!(
            "same-origin child frame becomes qualified; last sanitized inventory: {:#?}",
            last_frames.lock().unwrap()
        )
    });

    let frames = session
        .execute(
            BrowserOperationRequest::ListFrames(ListFramesRequest {
                target: PageSelection::Target(target),
            }),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("frame inventory with unsupported children");
    let BrowserOperationResult::ListFrames(frames) = frames else {
        panic!("frame inventory result")
    };
    let opaque = frames
        .frames
        .iter()
        .find(|frame| frame.access == FrameAccess::Indeterminate)
        .unwrap_or_else(|| panic!("opaque child frame: {frames:#?}"));
    let cross_origin = frames
        .frames
        .iter()
        .find(|frame| frame.access == FrameAccess::OutOfProcess)
        .unwrap_or_else(|| panic!("cross-origin OOPIF child frame: {frames:#?}"));
    for frame in [opaque, cross_origin] {
        let mut query = QueryPageRequest::new(
            PageSelection::Target(target),
            SemanticQuery::role("button", None).unwrap(),
            None,
            10,
        )
        .unwrap();
        query.document = SemanticDocumentScope::Frame(frame.reference.clone());
        let error = session
            .execute(
                BrowserOperationRequest::QueryPage(query),
                krometrail_core::BrowserOperationContext::default(),
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::Unsupported);
    }

    let capture_before_frame_navigation =
        tokio::time::timeout(std::time::Duration::from_secs(15), async {
            loop {
                let status = session
                    .status()
                    .await
                    .expect("capture status before frame navigation");
                if let Some(capture) = status
                    .capture
                    .iter()
                    .find(|capture| capture.target_id() == target)
                    && capture.state() == CaptureStreamState::Capturing
                    && capture.statistics().persisted_frames() > 0
                {
                    break capture.statistics().persisted_frames();
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("capture persists a frame before child-frame navigation");

    wait_for_page_expression(&session, target, "window.scrollY > 0").await;
    assert_eq!(
        evaluate(&session, target, "window.scrollY > 0").await,
        json!(true)
    );
    let mut query = QueryPageRequest::new(
        PageSelection::Target(target),
        SemanticQuery::role("link", Some(exact_semantic_text("Navigate child"))).unwrap(),
        None,
        10,
    )
    .unwrap();
    query.document = SemanticDocumentScope::Frame(child);
    let result = session
        .execute(
            BrowserOperationRequest::QueryPage(query),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("query same-origin child frame");
    let BrowserOperationResult::QueryPage(result) = result else {
        panic!("frame query result")
    };
    assert_eq!(result.outcome, SemanticQueryOutcome::Unique);
    let reference = result.matches[0].reference;
    session
        .execute(
            BrowserOperationRequest::Click(
                ClickRequest::new(
                    PageSelection::Target(target),
                    InteractionLocator::Element(ElementLocator::Reference(reference)),
                    MouseButton::Left,
                    Modifiers::default(),
                    1,
                    false,
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("click child-frame semantic reference after root scroll");
    wait_for_page_expression(
        &session,
        target,
        "document.querySelector('#context-frame').contentDocument?.querySelector('#child-navigated') !== null",
    )
    .await;
    assert_eq!(
        evaluate(
            &session,
            target,
            "document.querySelector('#context-frame').contentDocument?.querySelector('#child-navigated') !== null",
        )
        .await,
        json!(true)
    );
    let capture_after_frame_navigation =
        tokio::time::timeout(std::time::Duration::from_secs(15), async {
            loop {
                let status = session
                    .status()
                    .await
                    .expect("capture status after frame navigation");
                if let Some(capture) = status
                    .capture
                    .iter()
                    .find(|capture| capture.target_id() == target)
                    && capture.state() == CaptureStreamState::Capturing
                    && capture.failure().is_none()
                    && capture.statistics().persisted_frames() > capture_before_frame_navigation
                {
                    break (
                        capture.statistics().persisted_frames(),
                        capture.statistics().received_frames(),
                        capture.statistics().acknowledged_frames(),
                    );
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("capture remains healthy and persists after child-frame navigation");
    let (persisted_after, received_after, acknowledged_after) = capture_after_frame_navigation;
    assert_eq!(received_after, acknowledged_after);
    eprintln!(
        "nested-frame capture qualification: persisted_before={capture_before_frame_navigation} persisted_after={persisted_after} received={received_after} acknowledged={acknowledged_after} state=capturing failure_stage=none"
    );
    let stale = session
        .execute(
            BrowserOperationRequest::Click(
                ClickRequest::new(
                    PageSelection::Target(target),
                    InteractionLocator::Element(ElementLocator::Reference(reference)),
                    MouseButton::Left,
                    Modifiers::default(),
                    1,
                    false,
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(stale.code, ErrorCode::StaleReference);

    let assets = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let result = session
                .execute(
                    BrowserOperationRequest::ListPageAssets(ListPageAssetsRequest {
                        target: PageSelection::Target(target),
                    }),
                    krometrail_core::BrowserOperationContext::default(),
                )
                .await
                .expect("bounded page assets");
            let BrowserOperationResult::ListPageAssets(assets) = result else {
                panic!("page asset result")
            };
            if assets.assets.len() == krometrail_core::MAX_PAGE_ASSETS
                && assets.omitted_asset_count > 0
            {
                break assets;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("resource timing inventory reaches its browser-side bound");
    assert_eq!(assets.assets.len(), krometrail_core::MAX_PAGE_ASSETS);
    assert!(assets.omitted_asset_count > 0);
    let projected = serde_json::to_string(&*assets).unwrap();
    assert!(!projected.contains("asset-secret"));
    assert!(!projected.contains("style-secret"));
    assert!(!projected.contains("child-secret"));
    assert!(!projected.contains("browser-contexts/asset-"));
    assert!(!projected.contains(fixture.url().as_str()));

    session.stop().await.unwrap();
    cross_origin_fixture.shutdown();
    fixture.shutdown();
}

#[tokio::test]
async fn interaction_reports_the_new_page_delta_with_opener_match() {
    let transport = ScriptedCdp::chrome();
    startup_script(&transport);
    transport.push_response("Page.getLayoutMetrics", layout());
    transport.push_response(
        "Runtime.evaluate",
        json!({"result":{"value":{"tagName":"A","x":0,"y":0,"width":20,"height":20}}}),
    );
    observation_script(&transport);
    let evidence = Arc::new(support::RecordingEvidenceFake::default());
    let session = scripted_session_with_evidence(transport.clone(), evidence.clone()).await;
    let target = session.status().await.unwrap().pages[0].target.target.id();
    let cursor_before = page_cursor(&session).await;
    // The post-dispatch reconciliation pull observes a page the browser
    // adopted during the click, opened by the acting target.
    transport.push_response(
        "Target.getTargets",
        json!({"targetInfos":[
            {"targetId":"target-a","type":"page","url":"http://fixture/","title":"fixture"},
            {"targetId":"target-b","type":"page","url":"http://popup/","title":"popup","openerId":"target-a"}
        ]}),
    );
    transport.push_response("Target.attachToTarget", json!({"sessionId":"session-b"}));

    let result = session
        .execute(
            BrowserOperationRequest::Click(coordinate_click(target)),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::Click(result) = result else {
        panic!("click result")
    };
    let new_pages = result
        .record
        .postcondition
        .new_pages
        .as_ref()
        .expect("post-dispatch reconciliation attaches the new-page delta");
    assert_eq!(new_pages.cursor_before, cursor_before);
    assert_eq!(new_pages.pages.len(), 1);
    assert!(new_pages.pages[0].opener_matched);
    assert!(new_pages.pages[0].sequence > cursor_before);
    assert_eq!(new_pages.omitted, 0);
    // The retained record and the live response carry identical facts.
    let persisted = evidence.records();
    let persisted = persisted.last().expect("persisted interaction record");
    assert_eq!(
        persisted.postcondition.new_pages,
        result.record.postcondition.new_pages
    );
    session.stop().await.unwrap();
}

#[tokio::test]
async fn reconciliation_failure_degrades_the_delta_and_keeps_the_click_successful() {
    let transport = ScriptedCdp::chrome();
    startup_script(&transport);
    transport.push_response("Page.getLayoutMetrics", layout());
    transport.push_response(
        "Runtime.evaluate",
        json!({"result":{"value":{"tagName":"A","x":0,"y":0,"width":20,"height":20}}}),
    );
    observation_script(&transport);
    let session = scripted_session(transport.clone()).await;
    let target = session.status().await.unwrap().pages[0].target.target.id();
    transport.push_failure("Target.getTargets", TransportError::CommandFailed);

    let result = session
        .execute(
            BrowserOperationRequest::Click(coordinate_click(target)),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("reconciliation failure never fails a dispatched interaction");
    let BrowserOperationResult::Click(result) = result else {
        panic!("click result")
    };
    assert_eq!(result.record.outcome, InteractionOutcome::Dispatched);
    // Reconciliation unavailable: never a claim that nothing opened.
    assert!(result.record.postcondition.new_pages.is_none());
    session.stop().await.unwrap();
}

#[tokio::test]
async fn window_open_and_committed_navigation_signals_reach_the_postcondition() {
    let transport = ScriptedCdp::chrome();
    startup_script(&transport);
    transport.push_response("Page.getLayoutMetrics", layout());
    transport.push_response(
        "Runtime.evaluate",
        json!({"result":{"value":{"tagName":"A","x":0,"y":0,"width":20,"height":20}}}),
    );
    observation_script(&transport);
    let session = scripted_session(transport.clone()).await;
    let target = session.status().await.unwrap().pages[0].target.target.id();
    let request = ClickRequest::new(
        PageSelection::Target(target),
        InteractionLocator::coordinate(
            CssPoint::new(20.0, 30.0).unwrap(),
            CoordinateSpace::ViewportCss,
        )
        .unwrap(),
        MouseButton::Left,
        Modifiers::default(),
        1,
        // The navigation-aware completion parks on the lifecycle signal, so
        // the scripted events below deterministically arrive between the
        // attribution fence and the observation drain.
        true,
    )
    .unwrap();
    let operation = {
        let session = Arc::clone(&session);
        tokio::spawn(async move {
            session
                .execute(
                    BrowserOperationRequest::Click(request),
                    krometrail_core::BrowserOperationContext::default(),
                )
                .await
        })
    };
    transport
        .wait_for_command_count("Input.dispatchMouseEvent", 3)
        .await;
    transport.push_scoped_event(
        "Page.windowOpen",
        Some("session-a"),
        json!({"url":"http://fixture/child","windowName":"child","windowFeatures":[],"userGesture":true}),
    );
    // A committed same-URL main-frame navigation: url_changed stays false
    // while the committed-navigation fact reports the reload.
    transport.push_scoped_event(
        "Page.frameNavigated",
        Some("session-a"),
        json!({"frame":{"id":"main","url":"http://fixture/"},"type":"Navigation"}),
    );
    transport.push_scoped_event(
        "Page.lifecycleEvent",
        Some("session-a"),
        json!({"frameId":"main","loaderId":"loader-1","name":"load","timestamp":2.0}),
    );

    let result = operation.await.unwrap().unwrap();
    let BrowserOperationResult::Click(result) = result else {
        panic!("click result")
    };
    let postcondition = result.record.postcondition;
    assert_eq!(postcondition.signals.window_open_attempts, Some(1));
    assert_eq!(postcondition.signals.download_requests, Some(0));
    assert_eq!(
        postcondition.page.main_frame_navigation_observed,
        Some(true)
    );
    assert!(postcondition.page.navigation_lifecycle_observed);
    assert_eq!(postcondition.page.url_changed, Some(false));
    session.stop().await.unwrap();
}

#[tokio::test]
async fn child_frame_navigation_never_signals_the_main_frame_fact() {
    let transport = ScriptedCdp::chrome();
    startup_script(&transport);
    transport.push_response("Page.getLayoutMetrics", layout());
    transport.push_response(
        "Runtime.evaluate",
        json!({"result":{"value":{"tagName":"A","x":0,"y":0,"width":20,"height":20}}}),
    );
    observation_script(&transport);
    let session = scripted_session(transport.clone()).await;
    let target = session.status().await.unwrap().pages[0].target.target.id();
    let request = ClickRequest::new(
        PageSelection::Target(target),
        InteractionLocator::coordinate(
            CssPoint::new(20.0, 30.0).unwrap(),
            CoordinateSpace::ViewportCss,
        )
        .unwrap(),
        MouseButton::Left,
        Modifiers::default(),
        1,
        true,
    )
    .unwrap();
    let operation = {
        let session = Arc::clone(&session);
        tokio::spawn(async move {
            session
                .execute(
                    BrowserOperationRequest::Click(request),
                    krometrail_core::BrowserOperationContext::default(),
                )
                .await
        })
    };
    transport
        .wait_for_command_count("Input.dispatchMouseEvent", 3)
        .await;
    // A child-frame commit and a download-disposition request in a new tab
    // must not count as main-frame navigation or download attempts.
    transport.push_scoped_event(
        "Page.frameNavigated",
        Some("session-a"),
        json!({"frame":{"id":"child","parentId":"main","url":"http://fixture/frame"}}),
    );
    transport.push_scoped_event(
        "Page.frameRequestedNavigation",
        Some("session-a"),
        json!({"frameId":"main","url":"http://fixture/next","reason":"anchorClick","disposition":"newTab"}),
    );
    transport.push_scoped_event(
        "Page.lifecycleEvent",
        Some("session-a"),
        json!({"frameId":"main","loaderId":"loader-1","name":"load","timestamp":2.0}),
    );

    let result = operation.await.unwrap().unwrap();
    let BrowserOperationResult::Click(result) = result else {
        panic!("click result")
    };
    let postcondition = result.record.postcondition;
    assert_eq!(
        postcondition.page.main_frame_navigation_observed,
        Some(false)
    );
    assert_eq!(postcondition.signals.download_requests, Some(0));
    assert_eq!(postcondition.signals.window_open_attempts, Some(0));
    session.stop().await.unwrap();
}

#[tokio::test]
async fn unavailable_signal_source_degrades_facts_without_failing_the_interaction() {
    let transport = ScriptedCdp::chrome();
    transport.fail_subscription("Page.windowOpen");
    startup_script(&transport);
    transport.push_response("Page.getLayoutMetrics", layout());
    transport.push_response(
        "Runtime.evaluate",
        json!({"result":{"value":{"tagName":"A","x":0,"y":0,"width":20,"height":20}}}),
    );
    observation_script(&transport);
    let session = scripted_session(transport.clone()).await;
    let target = session.status().await.unwrap().pages[0].target.target.id();

    let result = session
        .execute(
            BrowserOperationRequest::Click(coordinate_click(target)),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("signal-only install failure degrades silently");
    let BrowserOperationResult::Click(result) = result else {
        panic!("click result")
    };
    let postcondition = result.record.postcondition;
    assert_eq!(postcondition.signals.window_open_attempts, None);
    assert_eq!(postcondition.signals.download_requests, Some(0));
    session.stop().await.unwrap();
}
