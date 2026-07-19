#![cfg(feature = "cdpkit-transport")]

mod support;

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use krometrail_cdp::{
    CdpTransport, CdpTransportFactory, ProductionBrowserConnector, TransportError, TransportFuture,
};
use krometrail_core::{
    ActivatePageRequest, AttachBrowser, BrowserConnectRequest, BrowserConnector,
    BrowserOperationContext, BrowserOperationRequest, BrowserOperationResult, BrowserSessionState,
    CancellationSignal, CaptureStreamState, ClickRequest, ClosePageRequest, CreatePageRequest,
    ElementLocator, ErrorCode, GoBackRequest, GoForwardRequest, ImageFormat, InspectPageRequest,
    InteractionLocator, LaunchBrowser, ListPageContextsRequest, ListPagesRequest, ManagedProfile,
    Modifiers, MouseButton, NavigatePageRequest, NonEmptyText, ObservationPart,
    PageOperationOutcome, PageSelection, ProfileIdentity, ProfileRef, ReadOnlyEvaluationRequest,
    ReloadPageRequest, ScreenshotRequest, ScreenshotTarget, SelectPageRequest, SnapshotPageRequest,
    WaitForPageRequest,
};
use serde_json::{Value, json};
use support::scripted_cdp::ScriptedCdp;

#[derive(Clone, Default)]
struct RequestCancellation {
    cancelled: Arc<AtomicBool>,
    notify: Arc<tokio::sync::Notify>,
}

impl RequestCancellation {
    fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.notify.notify_waiters();
    }
}

impl CancellationSignal for RequestCancellation {
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    fn cancelled(&self) -> krometrail_core::PortFuture<'_, ()> {
        Box::pin(async move {
            loop {
                if self.is_cancelled() {
                    return;
                }
                let notified = self.notify.notified();
                if self.is_cancelled() {
                    return;
                }
                notified.await;
            }
        })
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

fn png_base64() -> String {
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    bytes.extend_from_slice(&13_u32.to_be_bytes());
    bytes.extend_from_slice(b"IHDR");
    bytes.extend_from_slice(&800_u32.to_be_bytes());
    bytes.extend_from_slice(&600_u32.to_be_bytes());
    STANDARD.encode(bytes)
}

fn layout() -> Value {
    json!({
        "cssLayoutViewport":{"pageX":0.0,"pageY":0.0,"clientWidth":800.0,"clientHeight":600.0},
        "cssVisualViewport":{"pageX":0.0,"pageY":0.0,"clientWidth":800.0,"clientHeight":600.0,"scale":1.0},
        "cssContentSize":{"x":0.0,"y":0.0,"width":800.0,"height":600.0}
    })
}

fn frame(loader: &str, url: &str) -> Value {
    json!({"frameTree":{"frame":{"id":"main","loaderId":loader,"url":url}}})
}

fn history(index: u32, urls: &[&str]) -> Value {
    json!({
        "currentIndex": index,
        "entries": urls.iter().enumerate().map(|(id, url)| json!({"id":id as i64 + 1,"url":url})).collect::<Vec<_>>()
    })
}

fn ax_tree() -> Value {
    json!({"nodes":[
        {"nodeId":"root","ignored":false,"role":{"value":"document"},"name":{"value":"Lifecycle"},"childIds":["button"]},
        {"nodeId":"button","ignored":false,"role":{"value":"button"},"name":{"value":"Push history"},"backendDOMNodeId":42,"properties":[{"name":"focusable","value":{"value":true}}]}
    ]})
}

fn script_initial(transport: &ScriptedCdp) {
    transport.hold_events_open();
    transport.push_response(
        "Runtime.evaluate",
        json!({"result":{"type":"number","value":2}}),
    );
    transport.push_response(
        "Runtime.evaluate",
        json!({"result":{"type":"string","value":"visible"}}),
    );
    transport.push_response("Accessibility.getFullAXTree", json!({}));
}

fn script_live(
    transport: &ScriptedCdp,
    url: &str,
    title: &str,
    loader: &str,
    index: u32,
    urls: &[&str],
) {
    transport.push_response(
        "Runtime.evaluate",
        json!({"result":{"type":"object","value":{
            "url":url,"title":title,"readiness":"complete","deviceScaleFactor":1.0
        }}}),
    );
    transport.push_response(
        "Runtime.evaluate",
        json!({"result":{"type":"number","value":1.0}}),
    );
    transport.push_response("Page.getLayoutMetrics", layout());
    transport.push_response("Page.getLayoutMetrics", layout());
    transport.push_response("Page.getNavigationHistory", history(index, urls));
    transport.push_response("Page.getFrameTree", frame(loader, url));
    transport.push_response("Accessibility.getFullAXTree", ax_tree());
    transport.push_response("Page.captureScreenshot", json!({"data":png_base64()}));
}

async fn scripted_session(transport: &ScriptedCdp) -> Arc<dyn krometrail_core::BrowserSessionPort> {
    script_initial(transport);
    let connector = ProductionBrowserConnector::new(
        Arc::new(krometrail_cdp::SystemChromeLauncher::new(
            krometrail_cdp::LauncherConfig::default(),
        )),
        Arc::new(ScriptedFactory(transport.clone())),
    )
    .with_interaction_evidence(support::evidence_sink());
    connector
        .connect(BrowserConnectRequest::Attach(
            AttachBrowser::new("ws://127.0.0.1:9222/devtools/browser/lifecycle").unwrap(),
        ))
        .await
        .unwrap()
}

#[tokio::test]
async fn unsolicited_auto_attached_session_releases_waiting_popup() {
    let transport = ScriptedCdp::chrome();
    let session = scripted_session(&transport).await;
    transport.push_event(
        "Target.attachedToTarget",
        json!({
            "sessionId": "popup-session",
            "targetInfo": {"targetId": "popup", "type": "page", "url": "", "title": ""}
        }),
    );
    transport
        .wait_for_command("Runtime.runIfWaitingForDebugger")
        .await;
    assert!(transport.command_calls().iter().any(|call| {
        call.method == "Runtime.runIfWaitingForDebugger"
            && call.session.as_deref() == Some("popup-session")
            && call.params == json!({})
    }));
    session.stop().await.unwrap();
}

fn assert_successful_observation(result: &krometrail_core::PageOperationResult) {
    assert!(matches!(result.outcome, PageOperationOutcome::Succeeded(_)));
    assert!(matches!(result.observation, ObservationPart::Available(_)));
    assert!(result.interaction.timing.started_at <= result.interaction.timing.dispatched_at);
    assert!(result.interaction.timing.dispatched_at <= result.interaction.timing.completed_at);
    assert!(result.interaction.timing.observed_at.is_some());
}

#[tokio::test]
async fn status_and_page_mutations_share_exact_selected_target_state() {
    let transport = ScriptedCdp::chrome();
    let session = scripted_session(&transport).await;
    let initial = session.status().await.unwrap();
    assert_eq!(initial.state, BrowserSessionState::Ready);
    assert_eq!(initial.pages.len(), 1);
    assert_eq!(
        initial.selected_target_id,
        Some(initial.pages[0].target.target.id())
    );
    assert_eq!(initial.profile, ProfileRef::External);
    let initial_id = initial.selected_target_id.unwrap();

    transport.push_response(
        "Runtime.evaluate",
        json!({"result":{"type":"string","value":"visible"}}),
    );
    script_live(
        &transport,
        "http://fixture/",
        "fixture",
        "loader-a",
        0,
        &["http://fixture/"],
    );
    let activated = session
        .execute(
            BrowserOperationRequest::ActivatePage(ActivatePageRequest::default()),
            BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::ActivatePage(activated) = activated else {
        panic!("activate")
    };
    assert_successful_observation(&activated);
    assert!(matches!(
        activated.outcome,
        PageOperationOutcome::Succeeded(krometrail_core::PageChange::Activated { target_id })
            if target_id == initial_id
    ));
    assert_eq!(
        session.status().await.unwrap().selected_target_id,
        Some(initial_id)
    );
    assert!(transport.command_calls().iter().any(|call| {
        call.method == "Target.activateTarget" && call.params["targetId"] == "target-a"
    }));
    assert!(
        transport
            .command_calls()
            .iter()
            .any(|call| call.method == "Page.bringToFront")
    );

    let listed = session
        .execute(
            BrowserOperationRequest::ListPages(ListPagesRequest {}),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::ListPages(listed) = listed else {
        panic!("page list")
    };
    assert_eq!(listed.len(), 1);
    assert!(listed[0].selected);
    assert!(
        transport
            .command_calls()
            .iter()
            .all(|call| call.method != "Page.captureScreenshot")
    );

    transport.push_response("Target.createTarget", json!({"targetId":"target-b"}));
    transport.push_response(
        "Target.getTargetInfo",
        json!({"targetInfo":{
            "targetId":"target-b","type":"page","url":"about:blank","title":"","attached":false
        }}),
    );
    transport.push_response("Target.attachToTarget", json!({"sessionId":"session-b"}));
    transport.push_response(
        "Runtime.evaluate",
        json!({"result":{"type":"string","value":"visible"}}),
    );
    script_live(
        &transport,
        "about:blank",
        "",
        "loader-b",
        0,
        &["about:blank"],
    );
    let created = session
        .execute(
            BrowserOperationRequest::CreatePage(CreatePageRequest::new(None::<String>).unwrap()),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::CreatePage(created) = created else {
        panic!("create")
    };
    assert_successful_observation(&created);
    let created_id = created.interaction.target_id;
    assert_ne!(created_id, initial_id);
    let status = session.status().await.unwrap();
    assert_eq!(status.selected_target_id, Some(created_id));
    assert_eq!(status.pages.len(), 2);

    script_live(
        &transport,
        "http://fixture/",
        "fixture",
        "loader-a",
        0,
        &["http://fixture/"],
    );
    let selected = session
        .execute(
            BrowserOperationRequest::SelectPage(SelectPageRequest {
                target_id: initial_id,
            }),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::SelectPage(selected) = selected else {
        panic!("select")
    };
    assert_successful_observation(&selected);
    assert_eq!(
        session.status().await.unwrap().selected_target_id,
        Some(initial_id)
    );

    script_live(
        &transport,
        "http://fixture/",
        "fixture",
        "loader-a",
        0,
        &["http://fixture/"],
    );
    transport.push_response("Target.closeTarget", json!({"success":true}));
    let closed = session
        .execute(
            BrowserOperationRequest::ClosePage(ClosePageRequest {
                target: PageSelection::Target(created_id),
            }),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::ClosePage(closed) = closed else {
        panic!("close")
    };
    assert_successful_observation(&closed);
    assert_eq!(
        session.status().await.unwrap().selected_target_id,
        Some(initial_id)
    );

    transport.push_response("Target.closeTarget", json!({"success":true}));
    let last = session
        .execute(
            BrowserOperationRequest::ClosePage(ClosePageRequest {
                target: PageSelection::Selected,
            }),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::ClosePage(last) = last else {
        panic!("last close")
    };
    assert!(matches!(last.outcome, PageOperationOutcome::Succeeded(_)));
    assert!(matches!(last.observation, ObservationPart::Unavailable(_)));
    assert_eq!(session.status().await.unwrap().selected_target_id, None);

    let calls = transport.command_calls();
    let activate_b = calls
        .iter()
        .position(|call| {
            call.method == "Target.activateTarget" && call.params["targetId"] == "target-b"
        })
        .unwrap();
    let close_b = calls
        .iter()
        .position(|call| {
            call.method == "Target.closeTarget" && call.params["targetId"] == "target-b"
        })
        .unwrap();
    assert!(activate_b < close_b);
    assert!(
        calls
            .iter()
            .filter(|call| call.method == "Target.attachToTarget"
                && call.params["targetId"] == "target-b")
            .all(|call| call.session.is_none())
    );
    session.stop().await.unwrap();
}

#[tokio::test]
async fn create_attach_failure_keeps_the_allocated_interaction_anchor() {
    let transport = ScriptedCdp::chrome();
    let session = scripted_session(&transport).await;

    transport.push_response("Target.createTarget", json!({"targetId":"target-failed"}));
    transport.push_response(
        "Target.getTargetInfo",
        json!({"targetInfo":{
            "targetId":"target-failed","type":"page","url":"about:blank","title":"","attached":false
        }}),
    );
    transport.push_failure("Target.attachToTarget", TransportError::CommandFailed);

    let created = session
        .execute(
            BrowserOperationRequest::CreatePage(CreatePageRequest::new(None::<String>).unwrap()),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("anchored create result");
    let BrowserOperationResult::CreatePage(created) = created else {
        panic!("create failure")
    };
    let PageOperationOutcome::Failed(error) = &created.outcome else {
        panic!("failed create outcome")
    };
    assert_eq!(error.code, ErrorCode::TargetFailed);
    assert_eq!(error.context.target_id, Some(created.interaction.target_id));
    assert_eq!(
        error.context.interaction_id,
        Some(created.interaction.interaction_id)
    );
    assert!(matches!(
        created.observation,
        ObservationPart::Unavailable(_)
    ));
    assert!(
        transport
            .command_calls()
            .iter()
            .all(|call| call.method != "Target.activateTarget")
    );
    assert_eq!(session.status().await.unwrap().pages.len(), 1);
    session.stop().await.unwrap();
}

#[tokio::test]
async fn stable_loader_reload_requires_fresh_document_readiness() {
    let transport = ScriptedCdp::chrome();
    let session = scripted_session(&transport).await;
    let url = "http://fixture/service-worker-cached";

    transport.push_response("Page.getFrameTree", frame("stable-loader", url));
    transport.push_response("Page.getNavigationHistory", history(0, &[url]));
    transport.push_response(
        "Runtime.evaluate",
        json!({"result":{"type":"boolean","value":true}}),
    );
    transport.push_response("Page.reload", json!({}));
    transport.push_response("Page.getFrameTree", frame("stable-loader", url));
    transport.push_response("Page.getNavigationHistory", history(0, &[url]));
    transport.push_response(
        "Runtime.evaluate",
        json!({"result":{"type":"object","value":{
            "markerPresent":false,"readiness":"interactive"
        }}}),
    );
    script_live(&transport, url, "Cached", "stable-loader", 0, &[url]);

    let reloaded = session
        .execute(
            BrowserOperationRequest::ReloadPage(ReloadPageRequest {
                target: PageSelection::Selected,
                bypass_cache: false,
            }),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::ReloadPage(reloaded) = reloaded else {
        panic!("stable-loader reload")
    };
    assert_successful_observation(&reloaded);

    let calls = transport.command_calls();
    let marker_install = calls
        .iter()
        .position(|call| {
            call.method == "Runtime.evaluate"
                && call.params["expression"]
                    .as_str()
                    .is_some_and(|expression| expression.contains("Object.defineProperty"))
        })
        .expect("pre-dispatch freshness marker");
    let reload = calls
        .iter()
        .position(|call| call.method == "Page.reload")
        .expect("reload dispatch");
    let readiness = calls
        .iter()
        .position(|call| {
            call.method == "Runtime.evaluate"
                && call.params["expression"]
                    .as_str()
                    .is_some_and(|expression| expression.contains("markerPresent"))
        })
        .expect("post-dispatch freshness probe");
    assert!(marker_install < reload && reload < readiness);
    assert!(
        calls[marker_install..]
            .iter()
            .all(|call| !call.method.starts_with("Network."))
    );
    session.stop().await.unwrap();
}

#[derive(Clone, Copy)]
enum HeldObservationPart {
    Inspection,
    Snapshot,
    Screenshot,
}

async fn start_select_with_held_observation(
    held: HeldObservationPart,
) -> (
    ScriptedCdp,
    Arc<dyn krometrail_core::BrowserSessionPort>,
    tokio::task::JoinHandle<krometrail_core::Result<BrowserOperationResult>>,
) {
    let transport = ScriptedCdp::chrome();
    let session = scripted_session(&transport).await;
    let target_id = session.status().await.unwrap().selected_target_id.unwrap();
    transport.push_response(
        "Runtime.evaluate",
        json!({"result":{"type":"boolean","value":true}}),
    );
    match held {
        HeldObservationPart::Inspection => transport.hold_method("Runtime.evaluate"),
        HeldObservationPart::Snapshot => {
            transport.push_response(
                "Runtime.evaluate",
                json!({"result":{"type":"object","value":{
                    "url":"http://fixture/","title":"fixture","readiness":"complete","deviceScaleFactor":1.0
                }}}),
            );
            transport.push_response("Page.getLayoutMetrics", layout());
            transport.push_response(
                "Page.getNavigationHistory",
                history(0, &["http://fixture/"]),
            );
            transport.hold_method("Page.getFrameTree");
        }
        HeldObservationPart::Screenshot => {
            script_live(
                &transport,
                "http://fixture/",
                "fixture",
                "loader-a",
                0,
                &["http://fixture/"],
            );
            transport.hold_method("Page.captureScreenshot");
        }
    }
    let operation = {
        let session = Arc::clone(&session);
        tokio::spawn(async move {
            session
                .execute(
                    BrowserOperationRequest::SelectPage(SelectPageRequest { target_id }),
                    krometrail_core::BrowserOperationContext::default(),
                )
                .await
        })
    };
    let method = match held {
        HeldObservationPart::Inspection => "Runtime.evaluate",
        HeldObservationPart::Snapshot => "Page.getFrameTree",
        HeldObservationPart::Screenshot => "Page.captureScreenshot",
    };
    let initial_count = usize::from(matches!(held, HeldObservationPart::Inspection)) * 3;
    transport
        .wait_for_command_count(method, initial_count + 1)
        .await;
    (transport, session, operation)
}

fn assert_interrupted_observation(
    result: BrowserOperationResult,
    code: ErrorCode,
    held: HeldObservationPart,
) {
    let BrowserOperationResult::SelectPage(result) = result else {
        panic!("interrupted select")
    };
    assert!(matches!(result.outcome, PageOperationOutcome::Succeeded(_)));
    let ObservationPart::Available(observation) = &result.observation else {
        panic!("honest partial live observation")
    };
    assert_eq!(
        matches!(observation.page, ObservationPart::Available(_)),
        !matches!(held, HeldObservationPart::Inspection)
    );
    assert_eq!(
        matches!(observation.snapshot, ObservationPart::Available(_)),
        matches!(held, HeldObservationPart::Screenshot)
    );
    let ObservationPart::Unavailable(error) = &observation.screenshot else {
        panic!("interrupted screenshot must be unavailable")
    };
    assert_eq!(error.code, code);
}

#[tokio::test]
async fn stop_interrupts_each_post_operation_observation_transport_phase() {
    for held in [
        HeldObservationPart::Inspection,
        HeldObservationPart::Snapshot,
        HeldObservationPart::Screenshot,
    ] {
        let (_transport, session, operation) = start_select_with_held_observation(held).await;
        let stopped = tokio::time::timeout(Duration::from_secs(1), session.stop())
            .await
            .expect("stop must not wait for transport timeout")
            .unwrap();
        assert_eq!(stopped.closure(), krometrail_core::BrowserClosure::Detached);
        assert_eq!(stopped.quality(), krometrail_core::ShutdownQuality::Clean);
        let result = operation.await.unwrap().unwrap();
        assert_interrupted_observation(result, ErrorCode::Cancelled, held);
    }
}

#[tokio::test]
async fn disconnect_interrupts_post_operation_observation_without_replay() {
    let (transport, session, operation) =
        start_select_with_held_observation(HeldObservationPart::Screenshot).await;
    transport.disconnect();
    let result = tokio::time::timeout(Duration::from_secs(1), operation)
        .await
        .expect("disconnect must interrupt observation")
        .unwrap()
        .unwrap();
    assert_interrupted_observation(
        result,
        ErrorCode::BrowserDisconnected,
        HeldObservationPart::Screenshot,
    );
    assert_eq!(
        transport
            .command_calls()
            .iter()
            .filter(|call| call.method == "Target.activateTarget")
            .count(),
        1
    );
    let _ = tokio::time::timeout(Duration::from_secs(1), session.stop()).await;
}

#[tokio::test]
async fn request_cancellation_isolated_from_the_session_before_and_during_dispatch() {
    let transport = ScriptedCdp::chrome();
    let session = scripted_session(&transport).await;
    let before = transport.command_calls().len();

    let pre_cancelled = RequestCancellation::default();
    pre_cancelled.cancel();
    let error = session
        .execute(
            BrowserOperationRequest::NavigatePage(
                NavigatePageRequest::new(PageSelection::Selected, "http://fixture/never-sent")
                    .unwrap(),
            ),
            BrowserOperationContext::with_cancellation(Arc::new(pre_cancelled)),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::Cancelled);
    assert_eq!(transport.command_calls().len(), before);

    let target_id = session.status().await.unwrap().selected_target_id.unwrap();
    transport.push_response(
        "Page.getFrameTree",
        frame("loader-1", "http://fixture/first"),
    );
    transport.push_response(
        "Page.getNavigationHistory",
        history(0, &["http://fixture/first"]),
    );
    transport.hold_method("Page.navigate");
    let cancellation = RequestCancellation::default();
    let operation = {
        let session = Arc::clone(&session);
        let signal = cancellation.clone();
        tokio::spawn(async move {
            session
                .execute(
                    BrowserOperationRequest::NavigatePage(
                        NavigatePageRequest::new(
                            PageSelection::Target(target_id),
                            "http://fixture/cancelled",
                        )
                        .unwrap(),
                    ),
                    BrowserOperationContext::with_cancellation(Arc::new(signal)),
                )
                .await
        })
    };
    transport.wait_for_command("Page.navigate").await;
    cancellation.cancel();
    let result = tokio::time::timeout(Duration::from_secs(1), operation)
        .await
        .expect("request cancellation must interrupt a held operation")
        .unwrap()
        .unwrap();
    let BrowserOperationResult::NavigatePage(result) = result else {
        panic!("cancelled navigation result")
    };
    let PageOperationOutcome::Failed(error) = result.outcome else {
        panic!("cancelled navigation failure")
    };
    assert_eq!(error.code, ErrorCode::Cancelled);

    // Per-request cancellation must not poison the session or another operation.
    let listed = session
        .execute(
            BrowserOperationRequest::ListPages(ListPagesRequest {}),
            BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    assert!(matches!(listed, BrowserOperationResult::ListPages(_)));
    assert_eq!(
        session.status().await.unwrap().state,
        BrowserSessionState::Ready
    );
    session.stop().await.unwrap();
}

#[tokio::test]
async fn navigation_reload_history_and_stop_cancellation_are_anchored() {
    let transport = ScriptedCdp::chrome();
    let session = scripted_session(&transport).await;
    let target_id = session.status().await.unwrap().selected_target_id.unwrap();
    let first = "http://fixture/first";
    let second = "http://fixture/second";

    transport.push_response("Page.getFrameTree", frame("loader-1", first));
    transport.push_response("Accessibility.getFullAXTree", ax_tree());
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
    let old_reference = snapshot
        .nodes
        .iter()
        .find_map(|node| node.reference)
        .unwrap();

    transport.push_response("Page.getFrameTree", frame("loader-1", first));
    transport.push_response("Page.getNavigationHistory", history(0, &[first]));
    transport.push_response(
        "Page.navigate",
        json!({"frameId":"main","loaderId":"loader-2"}),
    );
    transport.push_response("Page.getFrameTree", frame("loader-2", second));
    transport.push_response("Page.getNavigationHistory", history(1, &[first, second]));
    script_live(
        &transport,
        second,
        "Second",
        "loader-2",
        1,
        &[first, second],
    );
    let navigated = session
        .execute(
            BrowserOperationRequest::NavigatePage(
                NavigatePageRequest::new(PageSelection::Selected, second).unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::NavigatePage(navigated) = navigated else {
        panic!("navigate")
    };
    assert_successful_observation(&navigated);
    assert_eq!(navigated.interaction.target_id, target_id);
    transport.push_response("Page.getLayoutMetrics", layout());
    transport.push_response(
        "Runtime.evaluate",
        json!({"result":{"type":"number","value":1.0}}),
    );
    let stale = session
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
        .unwrap_err();
    assert_eq!(stale.code, ErrorCode::StaleReference);

    transport.push_response("Page.getFrameTree", frame("loader-2", second));
    transport.push_response("Page.getNavigationHistory", history(1, &[first, second]));
    transport.push_response(
        "Runtime.evaluate",
        json!({"result":{"type":"boolean","value":true}}),
    );
    transport.push_response("Page.reload", json!({}));
    transport.push_response("Page.getFrameTree", frame("loader-3", second));
    transport.push_response("Page.getNavigationHistory", history(1, &[first, second]));
    script_live(
        &transport,
        second,
        "Second",
        "loader-3",
        1,
        &[first, second],
    );
    let reloaded = session
        .execute(
            BrowserOperationRequest::ReloadPage(ReloadPageRequest {
                target: PageSelection::Selected,
                bypass_cache: true,
            }),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::ReloadPage(reloaded) = reloaded else {
        panic!("reload")
    };
    assert_successful_observation(&reloaded);

    transport.push_response("Page.getFrameTree", frame("loader-3", second));
    transport.push_response("Page.getNavigationHistory", history(1, &[first, second]));
    transport.push_response("Page.navigateToHistoryEntry", json!({}));
    transport.push_response("Page.getFrameTree", frame("loader-3", first));
    transport.push_response("Page.getNavigationHistory", history(0, &[first, second]));
    script_live(&transport, first, "First", "loader-3", 0, &[first, second]);
    let back = session
        .execute(
            BrowserOperationRequest::GoBack(GoBackRequest {
                target: PageSelection::Selected,
            }),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::GoBack(back) = back else {
        panic!("back")
    };
    assert_successful_observation(&back);

    transport.push_response("Page.getFrameTree", frame("loader-3", first));
    transport.push_response("Page.getNavigationHistory", history(0, &[first, second]));
    let boundary = session
        .execute(
            BrowserOperationRequest::GoBack(GoBackRequest {
                target: PageSelection::Selected,
            }),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(boundary.code, ErrorCode::InvalidInput);

    transport.push_response("Page.getFrameTree", frame("loader-3", first));
    transport.push_response("Page.getNavigationHistory", history(0, &[first, second]));
    transport.push_response("Page.navigateToHistoryEntry", json!({}));
    transport.push_response("Page.getFrameTree", frame("loader-3", second));
    transport.push_response("Page.getNavigationHistory", history(1, &[first, second]));
    script_live(
        &transport,
        second,
        "Second",
        "loader-3",
        1,
        &[first, second],
    );
    let forward = session
        .execute(
            BrowserOperationRequest::GoForward(GoForwardRequest {
                target: PageSelection::Selected,
            }),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::GoForward(forward) = forward else {
        panic!("forward")
    };
    assert_successful_observation(&forward);

    assert!(snapshot.generation.get() > 0);
    let calls = transport.command_calls();
    assert!(calls.iter().any(|call| call.method == "Page.reload"
        && call.session.as_deref() == Some("session-a")
        && call.params["ignoreCache"] == true));
    assert!(calls.iter().any(|call| call.method == "Page.navigateToHistoryEntry" && call.params["entryId"] == 1));

    transport.push_response("Page.getFrameTree", frame("loader-3", second));
    transport.push_response("Page.getNavigationHistory", history(1, &[first, second]));
    transport.hold_method("Page.navigate");
    let operation = {
        let session = Arc::clone(&session);
        tokio::spawn(async move {
            session
                .execute(
                    BrowserOperationRequest::NavigatePage(
                        NavigatePageRequest::new(PageSelection::Selected, "http://fixture/stalled")
                            .unwrap(),
                    ),
                    krometrail_core::BrowserOperationContext::default(),
                )
                .await
        })
    };
    transport.wait_for_command_count("Page.navigate", 2).await;
    let stopped = session.stop().await.unwrap();
    assert_eq!(stopped.closure(), krometrail_core::BrowserClosure::Detached);
    assert_eq!(stopped.quality(), krometrail_core::ShutdownQuality::Clean);
    let result = operation.await.unwrap().unwrap();
    let BrowserOperationResult::NavigatePage(result) = result else {
        panic!("cancelled navigate")
    };
    let PageOperationOutcome::Failed(error) = result.outcome else {
        panic!("failed outcome")
    };
    assert_eq!(error.code, ErrorCode::Cancelled);
    assert_eq!(
        error.context.interaction_id,
        Some(result.interaction.interaction_id)
    );
}

#[tokio::test]
async fn navigation_rejection_and_malformed_preflight_are_source_safe() {
    let transport = ScriptedCdp::chrome();
    let session = scripted_session(&transport).await;
    let target_id = session.status().await.unwrap().selected_target_id.unwrap();

    transport.push_response("Page.getFrameTree", frame("loader-1", "http://fixture/"));
    transport.push_response(
        "Page.getNavigationHistory",
        history(0, &["http://fixture/"]),
    );
    transport.push_response(
        "Page.navigate",
        json!({"frameId":"main","errorText":"private protocol detail"}),
    );
    let rejected = session
        .execute(
            BrowserOperationRequest::NavigatePage(
                NavigatePageRequest::new(
                    PageSelection::Target(target_id),
                    "http://fixture/rejected",
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::NavigatePage(rejected) = rejected else {
        panic!("rejected navigate")
    };
    let PageOperationOutcome::Failed(error) = rejected.outcome else {
        panic!("failed rejection")
    };
    assert_eq!(error.code, ErrorCode::NavigationFailed);
    assert!(!error.message.as_str().contains("private"));
    assert_eq!(
        error.context.interaction_id,
        Some(rejected.interaction.interaction_id)
    );

    transport.push_response("Page.getFrameTree", json!({"frameTree":{}}));
    let malformed = session
        .execute(
            BrowserOperationRequest::NavigatePage(
                NavigatePageRequest::new(PageSelection::Selected, "http://fixture/malformed")
                    .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(malformed.code, ErrorCode::NavigationFailed);
    assert!(malformed.context.interaction_id.is_none());
    session.stop().await.unwrap();
}

#[tokio::test]
async fn opt_in_real_chrome_runs_complete_managed_page_lifecycle() {
    if !support::chrome::real_browser_tests_enabled() {
        eprintln!("skipping real Chrome page-lifecycle test; set KROMETRAIL_REAL_CHROME_TESTS=1");
        return;
    }
    let _browser_lock = support::chrome::real_browser_lock().await;
    let root_guard = support::chrome::temporary_profile_root("page-lifecycle");
    let root = root_guard.path().to_path_buf();
    let connector = ProductionBrowserConnector::new(
        Arc::new(krometrail_cdp::SystemChromeLauncher::new(
            krometrail_cdp::LauncherConfig {
                profile_root: root.clone(),
                startup_timeout: std::time::Duration::from_secs(45),
                shutdown_timeout: std::time::Duration::from_secs(5),
            },
        )),
        Arc::new(
            krometrail_cdp::transport::CdpkitTransportFactory::new()
                .with_command_timeout(std::time::Duration::from_secs(15)),
        ),
    )
    .with_interaction_evidence(support::evidence_sink());
    let first_url = support::chrome::page_lifecycle_fixture_url("index.html");
    let second_url = support::chrome::page_lifecycle_fixture_url("second.html");
    let session = connector
        .connect(BrowserConnectRequest::Launch(LaunchBrowser {
            executable: None,
            profile: ManagedProfile::Temporary,
            initial_url: Some(first_url.clone()),
            every_nth_frame: krometrail_core::EveryNthFrame::default(),
            focus: krometrail_core::BrowserFocusPolicy::default(),
        }))
        .await
        .expect("real lifecycle session");
    let initial = session.status().await.unwrap();
    assert_eq!(initial.state, BrowserSessionState::Ready);
    let ProfileRef::Managed(profile) = &initial.profile else {
        panic!("managed profile")
    };
    assert_eq!(
        profile.persistence,
        krometrail_core::ManagedProfilePersistence::Temporary
    );
    let initial_id = initial.selected_target_id.expect("initial selection");

    let snapshot = session
        .execute(
            BrowserOperationRequest::SnapshotPage(SnapshotPageRequest::new(initial_id)),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::SnapshotPage(snapshot) = snapshot else {
        panic!("initial snapshot")
    };
    let old_reference = snapshot
        .nodes
        .iter()
        .find_map(|node| {
            (node.name.as_deref() == Some("Push history"))
                .then_some(node.reference)
                .flatten()
        })
        .expect("push-history reference");

    let created = session
        .execute(
            BrowserOperationRequest::CreatePage(
                CreatePageRequest::new(Some(second_url.clone())).unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::CreatePage(created) = created else {
        panic!("real create")
    };
    assert_successful_observation(&created);
    let created_id = created.interaction.target_id;
    assert_ne!(created_id, initial_id);

    let navigated_initial = session
        .execute(
            BrowserOperationRequest::NavigatePage(
                NavigatePageRequest::new(PageSelection::Target(initial_id), second_url.clone())
                    .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::NavigatePage(navigated_initial) = navigated_initial else {
        panic!("real direct navigation")
    };
    assert_successful_observation(&navigated_initial);
    let stale = session
        .execute(
            BrowserOperationRequest::TakeScreenshot(
                ScreenshotRequest::new(
                    initial_id,
                    ScreenshotTarget::Element(ElementLocator::Reference(old_reference)),
                    ImageFormat::Png,
                    None,
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(stale.code, ErrorCode::StaleReference);

    session
        .execute(
            BrowserOperationRequest::SelectPage(SelectPageRequest {
                target_id: created_id,
            }),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    session
        .execute(
            BrowserOperationRequest::NavigatePage(
                NavigatePageRequest::new(PageSelection::Selected, first_url.clone()).unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    session
        .execute(
            BrowserOperationRequest::NavigatePage(
                NavigatePageRequest::new(PageSelection::Selected, format!("{first_url}#pushed"))
                    .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let back = session
        .execute(
            BrowserOperationRequest::GoBack(GoBackRequest {
                target: PageSelection::Selected,
            }),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::GoBack(back) = back else {
        panic!("real back")
    };
    assert_successful_observation(&back);
    let forward = session
        .execute(
            BrowserOperationRequest::GoForward(GoForwardRequest {
                target: PageSelection::Selected,
            }),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::GoForward(forward) = forward else {
        panic!("real forward")
    };
    assert_successful_observation(&forward);
    let reload = session
        .execute(
            BrowserOperationRequest::ReloadPage(ReloadPageRequest {
                target: PageSelection::Selected,
                bypass_cache: true,
            }),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::ReloadPage(reload) = reload else {
        panic!("real reload")
    };
    assert_successful_observation(&reload);

    let closed_unselected = session
        .execute(
            BrowserOperationRequest::ClosePage(ClosePageRequest {
                target: PageSelection::Target(initial_id),
            }),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::ClosePage(closed_unselected) = closed_unselected else {
        panic!("real unselected close")
    };
    assert_successful_observation(&closed_unselected);
    assert_eq!(
        session.status().await.unwrap().selected_target_id,
        Some(created_id)
    );
    let closed_selected = session
        .execute(
            BrowserOperationRequest::ClosePage(ClosePageRequest {
                target: PageSelection::Selected,
            }),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::ClosePage(closed_selected) = closed_selected else {
        panic!("real selected close")
    };
    assert!(matches!(
        closed_selected.outcome,
        PageOperationOutcome::Succeeded(_)
    ));
    assert_eq!(session.status().await.unwrap().selected_target_id, None);

    let outcome = session.stop().await.unwrap();
    assert_eq!(
        outcome.closure(),
        krometrail_core::BrowserClosure::ManagedBrowserClosed
    );
    assert_eq!(outcome.quality(), krometrail_core::ShutdownQuality::Clean);
    assert!(support::chrome::process_references(&root).is_empty());
    assert!(
        !root.join("tmp").exists()
            || std::fs::read_dir(root.join("tmp"))
                .unwrap()
                .next()
                .is_none()
    );
}

#[tokio::test]
async fn opt_in_real_chrome_preserve_focus_creates_a_background_tab() {
    if !support::chrome::real_browser_tests_enabled() {
        eprintln!("skipping preserve-focus Chrome test; set KROMETRAIL_REAL_CHROME_TESTS=1");
        return;
    }
    let _browser_lock = support::chrome::real_browser_lock().await;
    let root_guard = support::chrome::temporary_profile_root("preserve-focus");
    let connector = ProductionBrowserConnector::new(
        Arc::new(krometrail_cdp::SystemChromeLauncher::new(
            krometrail_cdp::LauncherConfig {
                profile_root: root_guard.path().to_path_buf(),
                startup_timeout: Duration::from_secs(45),
                shutdown_timeout: Duration::from_secs(5),
            },
        )),
        Arc::new(
            krometrail_cdp::transport::CdpkitTransportFactory::new()
                .with_command_timeout(Duration::from_secs(15)),
        ),
    )
    .with_interaction_evidence(support::evidence_sink());
    let first_url = support::chrome::page_lifecycle_fixture_url("index.html");
    let second_url = support::chrome::page_lifecycle_fixture_url("second.html");
    let session = connector
        .connect(BrowserConnectRequest::Launch(LaunchBrowser {
            executable: None,
            profile: ManagedProfile::Temporary,
            initial_url: Some(first_url),
            every_nth_frame: krometrail_core::EveryNthFrame::default(),
            focus: krometrail_core::BrowserFocusPolicy::Preserve,
        }))
        .await
        .expect("preserve-focus session");
    let initial_id = session.status().await.unwrap().selected_target_id.unwrap();

    let created = session
        .execute(
            BrowserOperationRequest::CreatePage(CreatePageRequest::new(Some(second_url)).unwrap()),
            BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::CreatePage(created) = created else {
        panic!("preserve-focus create")
    };
    assert!(matches!(
        created.outcome,
        PageOperationOutcome::Succeeded(_)
    ));
    let created_id = created.interaction.target_id;

    for (target_id, expected) in [(initial_id, "visible"), (created_id, "hidden")] {
        let visibility = session
            .execute(
                BrowserOperationRequest::EvaluatePage(
                    ReadOnlyEvaluationRequest::new(target_id, "document.visibilityState", false)
                        .unwrap(),
                ),
                BrowserOperationContext::default(),
            )
            .await
            .unwrap();
        let BrowserOperationResult::EvaluatePage(visibility) = visibility else {
            panic!("visibility evaluation")
        };
        assert_eq!(
            visibility.value,
            krometrail_core::EvaluationValue::Json(json!(expected))
        );
    }

    let activated = session
        .execute(
            BrowserOperationRequest::ActivatePage(ActivatePageRequest {
                target: Some(created_id),
            }),
            BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    assert!(matches!(activated, BrowserOperationResult::ActivatePage(_)));
    let activated_status = session.status().await.unwrap();
    let activated_page = activated_status
        .pages
        .iter()
        .find(|page| page.target.target.id() == created_id)
        .expect("activated page remains supervised");
    assert_eq!(
        activated_page.target.visibility,
        krometrail_core::TargetVisibility::Visible
    );

    let clicked = session
        .execute(
            BrowserOperationRequest::Click(
                ClickRequest::new(
                    PageSelection::Target(created_id),
                    InteractionLocator::element(ElementLocator::CssSelector(
                        NonEmptyText::new("#push").unwrap(),
                    )),
                    MouseButton::Left,
                    Modifiers::default(),
                    1,
                    false,
                )
                .unwrap(),
            ),
            BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    assert!(matches!(clicked, BrowserOperationResult::Click(_)));

    let captured = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let status = session.status().await.unwrap();
            if status.capture.iter().any(|capture| {
                capture.target_id() == created_id
                    && capture.state() == CaptureStreamState::Capturing
            }) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    assert!(captured.is_ok(), "activated page did not restart capture");

    session.stop().await.unwrap();
    assert!(support::chrome::process_references(root_guard.path()).is_empty());
}

#[tokio::test]
async fn opt_in_real_chrome_window_open_popup_commits_and_is_adopted() {
    if !support::chrome::real_browser_tests_enabled() {
        eprintln!("skipping popup Chrome test; set KROMETRAIL_REAL_CHROME_TESTS=1");
        return;
    }
    let _browser_lock = support::chrome::real_browser_lock().await;
    let root_guard = support::chrome::temporary_profile_root("popup-adoption");
    let connector = ProductionBrowserConnector::new(
        Arc::new(krometrail_cdp::SystemChromeLauncher::new(
            krometrail_cdp::LauncherConfig {
                profile_root: root_guard.path().to_path_buf(),
                startup_timeout: Duration::from_secs(45),
                shutdown_timeout: Duration::from_secs(5),
            },
        )),
        Arc::new(
            krometrail_cdp::transport::CdpkitTransportFactory::new()
                .with_command_timeout(Duration::from_secs(15)),
        ),
    )
    .with_interaction_evidence(support::evidence_sink());
    let session = connector
        .connect(BrowserConnectRequest::Launch(LaunchBrowser {
            executable: None,
            profile: ManagedProfile::Temporary,
            initial_url: Some(support::chrome::page_lifecycle_fixture_url("index.html")),
            every_nth_frame: krometrail_core::EveryNthFrame::default(),
            focus: krometrail_core::BrowserFocusPolicy::default(),
        }))
        .await
        .expect("popup lifecycle session");
    let opener = session.status().await.unwrap().selected_target_id.unwrap();
    let before = session
        .execute(
            BrowserOperationRequest::ListPageContexts(ListPageContextsRequest::default()),
            BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::ListPageContexts(before) = before else {
        panic!("page contexts");
    };
    for _ in 0..100 {
        let ready = session
            .execute(
                BrowserOperationRequest::EvaluatePage(
                    ReadOnlyEvaluationRequest::new(
                        opener,
                        "document.readyState === 'complete' && document.querySelector('#open-popup') !== null",
                        false,
                    )
                    .unwrap(),
                ),
                BrowserOperationContext::default(),
            )
            .await;
        if matches!(
            ready,
            Ok(BrowserOperationResult::EvaluatePage(result))
                if result.value == krometrail_core::EvaluationValue::Json(json!(true))
        ) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let clicked = session
        .execute(
            BrowserOperationRequest::Click(
                ClickRequest::new(
                    PageSelection::Target(opener),
                    InteractionLocator::element(ElementLocator::CssSelector(
                        NonEmptyText::new("#open-popup").unwrap(),
                    )),
                    MouseButton::Left,
                    Modifiers::default(),
                    1,
                    false,
                )
                .unwrap(),
            ),
            BrowserOperationContext::default(),
        )
        .await
        .expect("popup-opening click dispatch and degraded-safe observation");
    assert!(matches!(clicked, BrowserOperationResult::Click(_)));
    let waited = session
        .execute(
            BrowserOperationRequest::WaitForPage(WaitForPageRequest {
                after: before.cursor,
                opener_target_id: Some(opener),
                timeout_ms: 5_000,
            }),
            BrowserOperationContext::default(),
        )
        .await
        .expect("popup became supervised");
    let BrowserOperationResult::WaitForPage(waited) = waited else {
        panic!("wait for popup");
    };
    assert_eq!(waited.matched.opener_target_id, Some(opener));
    assert!(
        waited
            .matched
            .page
            .target
            .target
            .url()
            .ends_with("detail.html")
    );
    let contexts = session
        .execute(
            BrowserOperationRequest::ListPageContexts(ListPageContextsRequest::default()),
            BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::ListPageContexts(contexts) = contexts else {
        panic!("page contexts after popup");
    };
    assert!(
        contexts
            .pages
            .iter()
            .any(|page| page.opener_target_id == Some(opener))
    );
    session.stop().await.unwrap();
    assert!(support::chrome::process_references(root_guard.path()).is_empty());
}

#[tokio::test]
async fn opt_in_real_chrome_reopens_named_profile_state() {
    if !support::chrome::real_browser_tests_enabled() {
        eprintln!("skipping named-profile lifecycle test; set KROMETRAIL_REAL_CHROME_TESTS=1");
        return;
    }
    let _browser_lock = support::chrome::real_browser_lock().await;
    let root_guard = support::chrome::temporary_profile_root("page-lifecycle-named");
    let launcher = Arc::new(krometrail_cdp::SystemChromeLauncher::new(
        krometrail_cdp::LauncherConfig {
            profile_root: root_guard.path().to_path_buf(),
            startup_timeout: std::time::Duration::from_secs(45),
            shutdown_timeout: std::time::Duration::from_secs(5),
        },
    ));
    let factory = Arc::new(
        krometrail_cdp::transport::CdpkitTransportFactory::new()
            .with_command_timeout(std::time::Duration::from_secs(15)),
    );
    let connector = ProductionBrowserConnector::new(launcher, factory)
        .with_interaction_evidence(support::evidence_sink());
    let profile = ManagedProfile::Reusable {
        name: ProfileIdentity::new("lifecycle-named").unwrap(),
    };
    let url = support::chrome::page_lifecycle_fixture_url("index.html");
    for expected in ["1", "2"] {
        let session = connector
            .connect(BrowserConnectRequest::Launch(LaunchBrowser {
                executable: None,
                profile: profile.clone(),
                initial_url: Some(url.clone()),
                every_nth_frame: krometrail_core::EveryNthFrame::default(),
                focus: krometrail_core::BrowserFocusPolicy::default(),
            }))
            .await
            .unwrap();
        let status = session.status().await.unwrap();
        let ProfileRef::Managed(profile) = status.profile else {
            panic!("managed named profile")
        };
        assert_eq!(profile.identity.as_str(), "lifecycle-named");
        assert_eq!(
            profile.persistence,
            krometrail_core::ManagedProfilePersistence::Reusable
        );
        let target_id = status.selected_target_id.unwrap();
        let result = session
            .execute(
                BrowserOperationRequest::EvaluatePage(
                    ReadOnlyEvaluationRequest::new(
                        target_id,
                        "document.querySelector('#profile-visits').textContent",
                        false,
                    )
                    .unwrap(),
                ),
                krometrail_core::BrowserOperationContext::default(),
            )
            .await
            .unwrap();
        let BrowserOperationResult::EvaluatePage(result) = result else {
            panic!("profile evaluation")
        };
        assert_eq!(
            result.value,
            krometrail_core::EvaluationValue::Json(json!(expected))
        );
        session.stop().await.unwrap();
    }
}

#[test]
fn lifecycle_fixture_is_standalone_and_has_stable_markers() {
    let first = include_str!("../../../tests/fixtures/browser/page-lifecycle/index.html");
    let second = include_str!("../../../tests/fixtures/browser/page-lifecycle/second.html");
    assert!(first.contains("history.pushState") && first.contains("Lifecycle first"));
    assert!(second.contains("second-page-ready") && second.contains("Lifecycle second"));
    assert!(!first.contains("krometrail") && !second.contains("krometrail"));
    let _ = ImageFormat::Png;
    let _ = InspectPageRequest::new(krometrail_core::TargetId::from_uuid(uuid::Uuid::from_u128(
        1,
    )));
}
