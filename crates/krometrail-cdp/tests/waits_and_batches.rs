#![cfg(feature = "cdpkit-transport")]

mod support;

use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use krometrail_cdp::{
    CdpTransport, CdpTransportFactory, ProductionBrowserConnector, TransportError, TransportFuture,
};
use krometrail_core::{
    AttachBrowser, BatchFailurePolicy, BatchOptions, BatchOutcome, BatchRequest, BatchSkipReason,
    BatchStepStatus, BrowserConnectRequest, BrowserConnector, BrowserOperationRequest,
    BrowserOperationResult, CancellationSignal, ClickRequest, CoordinateSpace, CssPoint,
    DocumentReadiness, ElementLocator, ElementState, ErrorCode, EvaluationValue, InteractionAnchor,
    InteractionEvidenceSink, InteractionLocator, InteractionRecord, LaunchBrowser,
    ListPageContextsRequest, ManagedProfile, Modifiers, MouseButton, NavigationId, ObservationPart,
    ObservedTime, PageSelection, PortFuture, ReadOnlyEvaluationRequest, SemanticQuery,
    SemanticQueryOutcome, SemanticTextMatch, SemanticTextMatchMode, SnapshotPageRequest, UrlMatch,
    WaitCondition, WaitForPageRequest, WaitOutcome, WaitPresence, WaitProbe, WaitRequest,
    WaitTextMatch,
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
        "cssLayoutViewport":{"pageX":0.0,"pageY":0.0,"clientWidth":800.0,"clientHeight":600.0},
        "cssVisualViewport":{"pageX":0.0,"pageY":0.0,"clientWidth":800.0,"clientHeight":600.0,"scale":1.0},
        "cssContentSize":{"x":0.0,"y":0.0,"width":800.0,"height":600.0}
    })
}

fn identity() -> Value {
    json!({"result":{"value":{"url":"http://fixture/","title":"Wait fixture","readiness":"complete","deviceScaleFactor":1.0}}})
}

fn history() -> Value {
    json!({"currentIndex":0,"entries":[{"id":1,"url":"http://fixture/"}]})
}

fn frame_tree() -> Value {
    frame_tree_with_loader("loader-1")
}

fn frame_tree_with_loader(loader_id: &str) -> Value {
    json!({"frameTree":{"frame":{"id":"main","loaderId":loader_id,"url":"http://fixture/"}}})
}

fn semantic_ax_tree(name: Option<&str>) -> Value {
    let mut button = json!({
        "nodeId":"button",
        "ignored":false,
        "role":{"value":"button"},
        "backendDOMNodeId":42
    });
    if let Some(name) = name {
        button["name"] = json!({"value":name});
    }
    json!({
        "nodes":[
            {"nodeId":"root","ignored":false,"role":{"value":"document"},"childIds":["button"]},
            button
        ]
    })
}

fn semantic_dom_snapshot() -> Value {
    json!({
        "strings":["main","DIV","BUTTON","#text","Ready"],
        "documents":[{
            "frameId":0,
            "nodes":{
                "parentIndex":[-1,0,1],
                "nodeName":[1,2,3],
                "backendNodeId":[1,42,43],
                "attributes":[[],[],[]]
            },
            "layout":{"nodeIndex":[2],"text":[4]}
        }]
    })
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

fn live_observation_script(transport: &ScriptedCdp) {
    transport.push_response("Runtime.evaluate", identity());
    transport.push_response("Runtime.evaluate", json!({"result":{"value":1.0}}));
    transport.push_response("Page.getLayoutMetrics", layout());
    transport.push_response("Page.getLayoutMetrics", layout());
    transport.push_response("Page.getNavigationHistory", history());
    transport.push_response("Page.getFrameTree", frame_tree());
    transport.push_response(
        "Accessibility.getFullAXTree",
        json!({"nodes":[{"nodeId":"root","ignored":false,"role":{"value":"document"}}]}),
    );
    transport.push_response("Page.captureScreenshot", json!({"data":png_base64()}));
}

async fn scripted_session_with_sink(
    transport: ScriptedCdp,
    sink: Arc<dyn InteractionEvidenceSink>,
) -> Arc<dyn krometrail_core::BrowserSessionPort> {
    startup_script(&transport);
    ProductionBrowserConnector::new(
        Arc::new(krometrail_cdp::SystemChromeLauncher::new(
            krometrail_cdp::LauncherConfig::default(),
        )),
        Arc::new(ScriptedFactory(transport)),
    )
    .with_interaction_evidence(sink)
    .connect(BrowserConnectRequest::Attach(
        AttachBrowser::new("ws://127.0.0.1:9222/devtools/browser/waits-batches").unwrap(),
    ))
    .await
    .unwrap()
}

async fn scripted_session(transport: ScriptedCdp) -> Arc<dyn krometrail_core::BrowserSessionPort> {
    scripted_session_with_sink(transport, support::evidence_sink()).await
}

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

    fn cancelled(&self) -> PortFuture<'_, ()> {
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

async fn page_contexts(
    session: &Arc<dyn krometrail_core::BrowserSessionPort>,
) -> krometrail_core::PageContextInventory {
    let result = session
        .execute(
            BrowserOperationRequest::ListPageContexts(ListPageContextsRequest::default()),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::ListPageContexts(result) = result else {
        panic!("page context inventory")
    };
    *result
}

fn wait_for_page_request(
    after: krometrail_core::PageSequence,
    opener_target_id: Option<krometrail_core::TargetId>,
    timeout_ms: u64,
) -> BrowserOperationRequest {
    BrowserOperationRequest::WaitForPage(WaitForPageRequest {
        after,
        opener_target_id,
        timeout_ms,
    })
}

#[tokio::test]
async fn wait_for_page_polls_reconciles_opener_and_never_activates_a_target() {
    let transport = ScriptedCdp::chrome();
    let session = scripted_session(transport.clone()).await;
    let initial = page_contexts(&session).await;
    let opener = initial.pages[0].page.target.target.id();
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
            wait_for_page_request(initial.cursor, Some(opener), 1_000),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::WaitForPage(result) = result else {
        panic!("wait for page result")
    };
    assert!(result.matched.sequence > initial.cursor);
    assert_eq!(result.matched.opener_target_id, Some(opener));
    assert_eq!(
        result.matched.page.target.target.browser_target_key(),
        "target-b"
    );
    assert!(
        transport
            .command_calls()
            .iter()
            .all(|call| call.method != "Target.activateTarget")
    );
    session.stop().await.unwrap();
}

#[tokio::test]
async fn wait_for_page_reports_timeout_and_caller_cancellation() {
    let transport = ScriptedCdp::chrome();
    let session = scripted_session(transport.clone()).await;
    let cursor = page_contexts(&session).await.cursor;
    let error = session
        .execute(
            wait_for_page_request(cursor, None, 1),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::WaitTimedOut);

    let command_count = transport
        .command_calls()
        .iter()
        .filter(|call| call.method == "Target.getTargets")
        .count();
    let cancellation = RequestCancellation::default();
    let operation = {
        let session = Arc::clone(&session);
        let signal = cancellation.clone();
        tokio::spawn(async move {
            session
                .execute(
                    wait_for_page_request(cursor, None, 1_000),
                    krometrail_core::BrowserOperationContext::with_cancellation(Arc::new(signal)),
                )
                .await
        })
    };
    transport
        .wait_for_command_count("Target.getTargets", command_count + 1)
        .await;
    cancellation.cancel();
    let error = tokio::time::timeout(std::time::Duration::from_secs(1), operation)
        .await
        .expect("caller cancellation interrupts page wait")
        .unwrap()
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::Cancelled);
    session.stop().await.unwrap();
}

#[tokio::test]
async fn wait_for_page_reports_session_cancellation_and_disconnect() {
    let transport = ScriptedCdp::chrome();
    let session = scripted_session(transport.clone()).await;
    let cursor = page_contexts(&session).await.cursor;
    let command_count = transport
        .command_calls()
        .iter()
        .filter(|call| call.method == "Target.getTargets")
        .count();
    let operation = {
        let session = Arc::clone(&session);
        tokio::spawn(async move {
            session
                .execute(
                    wait_for_page_request(cursor, None, 1_000),
                    krometrail_core::BrowserOperationContext::default(),
                )
                .await
        })
    };
    transport
        .wait_for_command_count("Target.getTargets", command_count + 1)
        .await;
    let stop = {
        let session = Arc::clone(&session);
        tokio::spawn(async move { session.stop().await })
    };
    let error = tokio::time::timeout(std::time::Duration::from_secs(1), operation)
        .await
        .expect("session shutdown interrupts page wait")
        .unwrap()
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::Cancelled);
    tokio::time::timeout(std::time::Duration::from_secs(1), stop)
        .await
        .expect("session stop completes after wait cancellation")
        .unwrap()
        .unwrap();

    let transport = ScriptedCdp::chrome();
    let session = scripted_session(transport.clone()).await;
    let cursor = page_contexts(&session).await.cursor;
    let command_count = transport
        .command_calls()
        .iter()
        .filter(|call| call.method == "Target.getTargets")
        .count();
    let operation = {
        let session = Arc::clone(&session);
        tokio::spawn(async move {
            session
                .execute(
                    wait_for_page_request(cursor, None, 1_000),
                    krometrail_core::BrowserOperationContext::default(),
                )
                .await
        })
    };
    transport
        .wait_for_command_count("Target.getTargets", command_count + 1)
        .await;
    transport.disconnect();
    let error = tokio::time::timeout(std::time::Duration::from_secs(1), operation)
        .await
        .expect("disconnect interrupts page wait")
        .unwrap()
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::BrowserDisconnected);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(1), session.stop()).await;
}

fn page_wait(target: krometrail_core::TargetId, expression: &str) -> BrowserOperationRequest {
    BrowserOperationRequest::Wait(
        WaitRequest::new(
            PageSelection::Target(target),
            WaitCondition::Page {
                expression: krometrail_core::NonEmptyText::new(expression).unwrap(),
            },
            std::time::Duration::from_millis(100),
            std::time::Duration::from_millis(10),
        )
        .unwrap(),
    )
}

struct GateEvidenceSink {
    calls: AtomicUsize,
    started: tokio::sync::Notify,
    releases: tokio::sync::Semaphore,
    fail: bool,
}

impl GateEvidenceSink {
    fn new(fail: bool) -> Arc<Self> {
        Arc::new(Self {
            calls: AtomicUsize::new(0),
            started: tokio::sync::Notify::new(),
            releases: tokio::sync::Semaphore::new(0),
            fail,
        })
    }

    async fn wait_for_calls(&self, expected: usize) {
        loop {
            let notified = self.started.notified();
            if self.calls.load(Ordering::Acquire) >= expected {
                return;
            }
            notified.await;
        }
    }
}

impl InteractionEvidenceSink for GateEvidenceSink {
    fn append_operation_evidence(
        &self,
        _anchor: InteractionAnchor,
        _record: Option<InteractionRecord>,
        _persisted_at: ObservedTime,
        _navigation_id: Option<NavigationId>,
    ) -> PortFuture<'_, krometrail_core::Result<()>> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::AcqRel);
            self.started.notify_waiters();
            if self.fail {
                return Err(krometrail_core::KrometrailError::new(
                    ErrorCode::PersistenceFailed,
                    krometrail_core::NonEmptyText::new("deliberate evidence failure").unwrap(),
                ));
            }
            self.releases.acquire().await.unwrap().forget();
            Ok(())
        })
    }
}

fn script_coordinate_click(transport: &ScriptedCdp) {
    transport.push_response("Page.getLayoutMetrics", layout());
    transport.push_response(
        "Runtime.evaluate",
        json!({"result":{"value":{"tagName":"BUTTON","x":10,"y":10,"width":20,"height":20}}}),
    );
    transport.push_response("Runtime.evaluate", json!({"result":{"value":true}}));
    transport.push_response("Runtime.evaluate", json!({"result":{"value":true}}));
    live_observation_script(transport);
}

fn coordinate_click(target: krometrail_core::TargetId) -> BrowserOperationRequest {
    BrowserOperationRequest::Click(
        ClickRequest::new(
            PageSelection::Target(target),
            InteractionLocator::coordinate(
                CssPoint::new(20.0, 20.0).unwrap(),
                CoordinateSpace::ViewportCss,
            )
            .unwrap(),
            MouseButton::Left,
            Modifiers::default(),
            1,
            false,
        )
        .unwrap(),
    )
}

#[tokio::test]
async fn sequential_batch_reuses_dispatcher_and_propagates_parent_anchor() {
    let transport = ScriptedCdp::chrome();
    let session = scripted_session(transport.clone()).await;
    let target = session.status().await.unwrap().selected_target_id.unwrap();

    transport.push_response("Page.getLayoutMetrics", layout());
    transport.push_response(
        "Runtime.evaluate",
        json!({"result":{"value":{"tagName":"BUTTON","x":10,"y":10,"width":20,"height":20}}}),
    );
    transport.push_response("Runtime.evaluate", json!({"result":{"value":true}}));
    transport.push_response("Runtime.evaluate", json!({"result":{"value":true}}));
    live_observation_script(&transport);
    live_observation_script(&transport);

    let request = BatchRequest::new(
        PageSelection::Target(target),
        vec![BrowserOperationRequest::Click(
            ClickRequest::new(
                PageSelection::Target(target),
                InteractionLocator::coordinate(
                    CssPoint::new(20.0, 20.0).unwrap(),
                    CoordinateSpace::ViewportCss,
                )
                .unwrap(),
                MouseButton::Left,
                Modifiers::default(),
                1,
                false,
            )
            .unwrap(),
        )],
        std::time::Duration::from_secs(2),
        BatchOptions::default(),
    )
    .unwrap();
    let result = session
        .execute(
            BrowserOperationRequest::Batch(request),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::Batch(result) = result else {
        panic!("batch result")
    };
    assert_eq!(result.outcome, BatchOutcome::Completed, "{result:#?}");
    assert_eq!(result.steps.len(), 1);
    assert_eq!(result.steps[0].status, BatchStepStatus::Succeeded);
    let anchor = result.steps[0].interaction.as_ref().expect("child anchor");
    let BrowserOperationResult::Click(click) = result.steps[0].result.as_ref().unwrap() else {
        panic!("click child")
    };
    assert_eq!(click.record.parent_batch, Some(result.batch_id));
    assert_eq!(anchor.interaction_id, click.record.id);
    // Batch steps recurse through the same execute seam, so each step's
    // record carries its own side-channel block.
    let new_pages = click
        .record
        .postcondition
        .new_pages
        .as_ref()
        .expect("batch step inherits post-dispatch reconciliation");
    assert!(new_pages.pages.is_empty());
    assert_eq!(new_pages.omitted, 0);
    assert!(matches!(
        result.final_observation,
        ObservationPart::Available(_)
    ));
    assert_eq!(
        transport
            .command_calls()
            .iter()
            .filter(|call| call.method == "Page.captureScreenshot")
            .count(),
        2,
        "one child live screenshot and exactly one final live observation"
    );
    session.stop().await.unwrap();
}

#[tokio::test]
async fn each_batch_step_crosses_the_evidence_fence_before_the_next_dispatch() {
    let transport = ScriptedCdp::chrome();
    let sink = GateEvidenceSink::new(false);
    let session = scripted_session_with_sink(
        transport.clone(),
        Arc::clone(&sink) as Arc<dyn InteractionEvidenceSink>,
    )
    .await;
    let target = session.status().await.unwrap().selected_target_id.unwrap();
    script_coordinate_click(&transport);
    script_coordinate_click(&transport);
    live_observation_script(&transport);
    let request = BatchRequest::new(
        PageSelection::Target(target),
        vec![coordinate_click(target), coordinate_click(target)],
        std::time::Duration::from_secs(5),
        BatchOptions::default(),
    )
    .unwrap();
    let task = {
        let session = Arc::clone(&session);
        tokio::spawn(async move {
            session
                .execute(
                    BrowserOperationRequest::Batch(request),
                    krometrail_core::BrowserOperationContext::default(),
                )
                .await
        })
    };

    sink.wait_for_calls(1).await;
    assert_eq!(
        transport
            .command_calls()
            .iter()
            .filter(|call| call.method == "Input.dispatchMouseEvent")
            .count(),
        4
    );
    sink.releases.add_permits(1);
    sink.wait_for_calls(2).await;
    assert_eq!(
        transport
            .command_calls()
            .iter()
            .filter(|call| call.method == "Input.dispatchMouseEvent")
            .count(),
        7
    );
    sink.releases.add_permits(1);
    let BrowserOperationResult::Batch(result) = task.await.unwrap().unwrap() else {
        panic!("batch result")
    };
    assert_eq!(result.outcome, BatchOutcome::Completed, "{result:#?}");
    assert!(
        result
            .steps
            .iter()
            .all(|step| step.status == BatchStepStatus::Succeeded)
    );

    let failing_transport = ScriptedCdp::chrome();
    let failing_sink = GateEvidenceSink::new(true);
    let failed_session = scripted_session_with_sink(
        failing_transport.clone(),
        failing_sink as Arc<dyn InteractionEvidenceSink>,
    )
    .await;
    let failed_target = failed_session
        .status()
        .await
        .unwrap()
        .selected_target_id
        .unwrap();
    script_coordinate_click(&failing_transport);
    live_observation_script(&failing_transport);
    let failed = BatchRequest::new(
        PageSelection::Target(failed_target),
        vec![
            coordinate_click(failed_target),
            coordinate_click(failed_target),
        ],
        std::time::Duration::from_secs(5),
        BatchOptions::default(),
    )
    .unwrap();
    let BrowserOperationResult::Batch(failed) = failed_session
        .execute(
            BrowserOperationRequest::Batch(failed),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap()
    else {
        panic!("failed batch result")
    };
    assert_eq!(failed.outcome, BatchOutcome::StoppedOnFailure);
    assert_eq!(failed.steps[0].status, BatchStepStatus::Failed);
    assert_eq!(failed.steps[1].status, BatchStepStatus::Skipped);
    assert_eq!(
        failed.steps[0].error.as_ref().unwrap().code,
        ErrorCode::PersistenceFailed
    );
    assert_eq!(
        failing_transport
            .command_calls()
            .iter()
            .filter(|call| call.method == "Input.dispatchMouseEvent")
            .count(),
        4
    );
}

#[tokio::test]
async fn batch_stop_and_continue_policies_preserve_failed_wait_results() {
    for (policy, expected_outcome, expected_status) in [
        (
            BatchFailurePolicy::StopOnFailure,
            BatchOutcome::StoppedOnFailure,
            BatchStepStatus::Skipped,
        ),
        (
            BatchFailurePolicy::ContinueOnFailure,
            BatchOutcome::CompletedWithFailures,
            BatchStepStatus::Succeeded,
        ),
    ] {
        let transport = ScriptedCdp::chrome();
        let session = scripted_session(transport.clone()).await;
        let target = session.status().await.unwrap().selected_target_id.unwrap();
        transport.push_response(
            "Runtime.evaluate",
            json!({"result":{"type":"number","value":1}}),
        );
        if policy == BatchFailurePolicy::ContinueOnFailure {
            transport.push_response(
                "Runtime.evaluate",
                json!({"result":{"type":"string","value":"after"}}),
            );
        }
        live_observation_script(&transport);
        let request = BatchRequest::new(
            PageSelection::Target(target),
            vec![
                page_wait(target, "42"),
                BrowserOperationRequest::EvaluatePage(
                    ReadOnlyEvaluationRequest::new(target, "'after'", false).unwrap(),
                ),
            ],
            std::time::Duration::from_secs(1),
            BatchOptions {
                failure_policy: policy,
                include_step_screenshots: false,
            },
        )
        .unwrap();
        let result = session
            .execute(
                BrowserOperationRequest::Batch(request),
                krometrail_core::BrowserOperationContext::default(),
            )
            .await
            .unwrap();
        let BrowserOperationResult::Batch(result) = result else {
            panic!("batch")
        };
        assert_eq!(result.outcome, expected_outcome);
        assert_eq!(result.steps[0].status, BatchStepStatus::Failed);
        assert_eq!(
            result.steps[0].error.as_ref().unwrap().code,
            krometrail_core::ErrorCode::EvaluationFailed
        );
        assert_eq!(result.steps[1].status, expected_status);
        assert!(result.steps.iter().all(|step| step.screenshot.is_none()));
        assert_eq!(
            transport
                .command_calls()
                .iter()
                .filter(|call| call.method == "Page.captureScreenshot")
                .count(),
            1,
            "disabled step evidence must not add screenshot commands beyond final observation"
        );
        if policy == BatchFailurePolicy::ContinueOnFailure {
            let BrowserOperationResult::EvaluatePage(value) =
                result.steps[1].result.as_ref().unwrap()
            else {
                panic!("continued evaluation")
            };
            assert_eq!(value.value, EvaluationValue::Json(json!("after")));
        }
        assert!(matches!(
            result.final_observation,
            ObservationPart::Available(_)
        ));
        session.stop().await.unwrap();
    }
}

#[tokio::test]
async fn wait_deadline_stops_a_held_probe_without_an_extra_poll() {
    let transport = ScriptedCdp::chrome();
    let session = scripted_session(transport.clone()).await;
    let target = session.status().await.unwrap().selected_target_id.unwrap();
    transport.hold_method("Runtime.evaluate");
    let started = tokio::time::Instant::now();
    let result = session
        .execute(
            page_wait(target, "false"),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .expect("timeout is a structured wait result");
    let BrowserOperationResult::Wait(result) = result else {
        panic!("wait result")
    };
    assert!(matches!(result.outcome, WaitOutcome::TimedOut { .. }));
    assert!(started.elapsed() < std::time::Duration::from_millis(500));
    assert_eq!(
        transport
            .command_calls()
            .iter()
            .filter(|call| {
                call.method == "Runtime.evaluate" && call.params["expression"] == json!("false")
            })
            .count(),
        1,
        "the absolute deadline must not start another probe"
    );
    session.stop().await.unwrap();
}

#[tokio::test]
async fn session_stop_cancels_a_held_wait_probe_before_its_deadline() {
    let transport = ScriptedCdp::chrome();
    let session = scripted_session(transport.clone()).await;
    let target = session.status().await.unwrap().selected_target_id.unwrap();
    transport.hold_method("Runtime.evaluate");
    let operation = {
        let session = Arc::clone(&session);
        tokio::spawn(async move {
            session
                .execute(
                    BrowserOperationRequest::Wait(
                        WaitRequest::new(
                            PageSelection::Target(target),
                            WaitCondition::Page {
                                expression: krometrail_core::NonEmptyText::new("false").unwrap(),
                            },
                            std::time::Duration::from_secs(10),
                            std::time::Duration::from_millis(10),
                        )
                        .unwrap(),
                    ),
                    krometrail_core::BrowserOperationContext::default(),
                )
                .await
        })
    };
    transport
        .wait_for_command_count("Runtime.evaluate", 3)
        .await;
    session.stop().await.unwrap();
    let error = operation.await.unwrap().unwrap_err();
    assert_eq!(error.code, ErrorCode::Cancelled);
}

#[tokio::test]
async fn explicit_network_quiet_tracks_finite_events_and_discloses_limits() {
    let transport = ScriptedCdp::chrome();
    let session = scripted_session(transport.clone()).await;
    let target = session.status().await.unwrap().selected_target_id.unwrap();
    let enables_before_wait = transport
        .command_calls()
        .iter()
        .filter(|call| call.method == "Network.enable")
        .count();
    transport.push_event(
        "Network.requestWillBeSent",
        json!({
            "requestId":"private-request-id",
            "type":"Fetch",
            "request":{"method":"GET","url":"https://fixture.test/data"}
        }),
    );
    transport.push_event(
        "Network.requestWillBeSent",
        json!({
            "requestId":"private-websocket-id",
            "type":"WebSocket",
            "request":{"method":"GET","url":"wss://fixture.test/socket"}
        }),
    );
    transport.push_event(
        "Network.loadingFinished",
        json!({"requestId":"private-request-id"}),
    );
    let result = session
        .execute(
            BrowserOperationRequest::Wait(
                WaitRequest::new(
                    PageSelection::Target(target),
                    WaitCondition::NetworkQuiet {
                        quiet_for: std::time::Duration::from_millis(20),
                    },
                    std::time::Duration::from_millis(250),
                    std::time::Duration::from_millis(10),
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::Wait(result) = result else {
        panic!("network wait")
    };
    assert!(
        matches!(result.outcome, WaitOutcome::Satisfied { .. }),
        "network wait did not satisfy: {result:?}"
    );
    let Some(WaitProbe::NetworkQuiet {
        in_flight,
        tracks_from_subscription,
        excludes_long_lived_connections,
        ..
    }) = result.last_probe
    else {
        panic!("network probe")
    };
    assert_eq!(in_flight, 0);
    assert!(tracks_from_subscription && excludes_long_lived_connections);
    let encoded = format!("{result:?}");
    assert!(!encoded.contains("private-request-id"));
    assert!(!encoded.contains("private-websocket-id"));
    assert_eq!(
        transport
            .command_calls()
            .iter()
            .filter(|call| call.method == "Network.enable")
            .count(),
        enables_before_wait + 1
    );
    for method in [
        "Network.requestWillBeSent",
        "Network.responseReceived",
        "Network.loadingFinished",
        "Network.loadingFailed",
    ] {
        assert_eq!(
            transport
                .subscriptions()
                .iter()
                .filter(|(subscribed, _)| subscribed == method)
                .count(),
            1,
            "network authority installed {method} more than once"
        );
    }
    session.stop().await.unwrap();
}

#[tokio::test]
async fn batch_global_deadline_fails_the_active_step_and_skips_the_rest() {
    let transport = ScriptedCdp::chrome();
    let session = scripted_session(transport.clone()).await;
    let target = session.status().await.unwrap().selected_target_id.unwrap();
    transport.hold_method("Runtime.evaluate");
    let request = BatchRequest::new(
        PageSelection::Target(target),
        vec![
            BrowserOperationRequest::EvaluatePage(
                ReadOnlyEvaluationRequest::new(target, "true", false).unwrap(),
            ),
            BrowserOperationRequest::EvaluatePage(
                ReadOnlyEvaluationRequest::new(target, "false", false).unwrap(),
            ),
        ],
        std::time::Duration::from_millis(30),
        BatchOptions::default(),
    )
    .unwrap();
    let result = session
        .execute(
            BrowserOperationRequest::Batch(request),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::Batch(result) = result else {
        panic!("batch")
    };
    assert_eq!(result.outcome, BatchOutcome::TimedOut);
    assert_eq!(result.steps[0].status, BatchStepStatus::Failed);
    assert_eq!(
        result.steps[0].error.as_ref().unwrap().code,
        ErrorCode::WaitTimedOut
    );
    assert_eq!(result.steps[1].status, BatchStepStatus::Skipped);
    assert_eq!(
        result.steps[1].skip_reason,
        Some(BatchSkipReason::BatchTimedOut)
    );
    session.stop().await.unwrap();
}

#[tokio::test]
async fn batch_deadline_after_dispatch_preserves_and_persists_degraded_record() {
    let transport = ScriptedCdp::chrome();
    let sink = Arc::new(support::RecordingEvidenceFake::default());
    let session = scripted_session_with_sink(
        transport.clone(),
        Arc::clone(&sink) as Arc<dyn InteractionEvidenceSink>,
    )
    .await;
    let target = session.status().await.unwrap().selected_target_id.unwrap();
    let input_calls_before = transport
        .command_calls()
        .iter()
        .filter(|call| call.method == "Input.dispatchMouseEvent")
        .count();
    script_coordinate_click(&transport);
    live_observation_script(&transport);

    let runtime_evaluations = transport
        .command_calls()
        .iter()
        .filter(|call| call.method == "Runtime.evaluate")
        .count();
    // Coordinate hit testing, the pre-dispatch URL read, and action completion are allowed to
    // finish. The first compositor/observation command after input dispatch then ignores the
    // cooperative deadline and exercises the degraded observation path.
    transport.hold_method_after("Runtime.evaluate", runtime_evaluations + 3);
    let request = BatchRequest::new(
        PageSelection::Target(target),
        vec![coordinate_click(target), page_wait(target, "false")],
        std::time::Duration::from_millis(80),
        BatchOptions::default(),
    )
    .unwrap();
    let BrowserOperationResult::Batch(result) = tokio::time::timeout(
        std::time::Duration::from_millis(400),
        session.execute(
            BrowserOperationRequest::Batch(request),
            krometrail_core::BrowserOperationContext::default(),
        ),
    )
    .await
    .expect("cooperative post-dispatch timeout completes before the hard backstop")
    .unwrap() else {
        panic!("batch result")
    };

    assert_eq!(result.outcome, BatchOutcome::TimedOut, "{result:#?}");
    assert_eq!(result.steps[0].status, BatchStepStatus::Failed);
    assert_eq!(
        result.steps[0].error.as_ref().map(|error| error.code),
        Some(ErrorCode::WaitTimedOut)
    );
    assert!(result.steps[0].interaction.is_some());
    let BrowserOperationResult::Click(click) = result.steps[0].result.as_ref().unwrap() else {
        panic!("preserved click result")
    };
    assert!(matches!(
        &click.observation.page,
        ObservationPart::Unavailable(error)
            if error.code == ErrorCode::PageObservationFailed
                && error.message.as_str().contains("budget exhausted")
    ));
    assert!(matches!(
        &click.observation.snapshot,
        ObservationPart::Unavailable(error) if error.message.as_str().contains("budget exhausted")
    ));
    assert!(matches!(
        &click.observation.screenshot,
        ObservationPart::Unavailable(error) if error.message.as_str().contains("budget exhausted")
    ));
    assert_eq!(result.steps[1].status, BatchStepStatus::Skipped);
    assert_eq!(
        result.steps[1].skip_reason,
        Some(BatchSkipReason::BatchTimedOut)
    );
    let persisted = sink.records();
    assert_eq!(
        persisted.len(),
        1,
        "the dispatched record must cross the evidence seam"
    );
    assert_eq!(persisted[0].id, click.record.id);
    assert_eq!(
        transport
            .command_calls()
            .iter()
            .filter(|call| call.method == "Input.dispatchMouseEvent")
            .count()
            - input_calls_before,
        3,
        "input was dispatched exactly once before observation degraded"
    );
    session.stop().await.unwrap();
}

#[tokio::test]
async fn batch_deadline_before_dispatch_keeps_the_current_no_record_timeout() {
    let transport = ScriptedCdp::chrome();
    let sink = Arc::new(support::RecordingEvidenceFake::default());
    let session = scripted_session_with_sink(
        transport.clone(),
        Arc::clone(&sink) as Arc<dyn InteractionEvidenceSink>,
    )
    .await;
    let target = session.status().await.unwrap().selected_target_id.unwrap();
    let input_calls_before = transport
        .command_calls()
        .iter()
        .filter(|call| call.method == "Input.dispatchMouseEvent")
        .count();
    transport.hold_method("Page.getLayoutMetrics");
    let request = BatchRequest::new(
        PageSelection::Target(target),
        vec![coordinate_click(target)],
        std::time::Duration::from_millis(30),
        BatchOptions::default(),
    )
    .unwrap();

    let BrowserOperationResult::Batch(result) = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        session.execute(
            BrowserOperationRequest::Batch(request),
            krometrail_core::BrowserOperationContext::default(),
        ),
    )
    .await
    .expect("pre-dispatch timeout reaches the batch hard backstop")
    .unwrap() else {
        panic!("batch result")
    };

    assert_eq!(result.outcome, BatchOutcome::TimedOut, "{result:#?}");
    assert_eq!(result.steps[0].status, BatchStepStatus::Failed);
    assert_eq!(
        result.steps[0].error.as_ref().map(|error| error.code),
        Some(ErrorCode::WaitTimedOut)
    );
    assert!(result.steps[0].result.is_none());
    assert!(result.steps[0].interaction.is_none());
    assert!(sink.records().is_empty());
    assert_eq!(
        transport
            .command_calls()
            .iter()
            .filter(|call| call.method == "Input.dispatchMouseEvent")
            .count()
            - input_calls_before,
        0,
        "a pre-dispatch timeout must not send input: {:?}",
        transport.command_calls()
    );
    session.stop().await.unwrap();
}

#[tokio::test]
async fn batch_backstop_fires_when_post_dispatch_persistence_ignores_budget() {
    let transport = ScriptedCdp::chrome();
    let sink = GateEvidenceSink::new(false);
    let session = scripted_session_with_sink(
        transport.clone(),
        Arc::clone(&sink) as Arc<dyn InteractionEvidenceSink>,
    )
    .await;
    let target = session.status().await.unwrap().selected_target_id.unwrap();
    let input_calls_before = transport
        .command_calls()
        .iter()
        .filter(|call| call.method == "Input.dispatchMouseEvent")
        .count();
    script_coordinate_click(&transport);
    live_observation_script(&transport);
    let started = tokio::time::Instant::now();
    let request = BatchRequest::new(
        PageSelection::Target(target),
        vec![coordinate_click(target)],
        std::time::Duration::from_millis(30),
        BatchOptions::default(),
    )
    .unwrap();
    let operation = {
        let session = Arc::clone(&session);
        tokio::spawn(async move {
            session
                .execute(
                    BrowserOperationRequest::Batch(request),
                    krometrail_core::BrowserOperationContext::default(),
                )
                .await
        })
    };
    sink.wait_for_calls(1).await;
    let BrowserOperationResult::Batch(result) =
        tokio::time::timeout(std::time::Duration::from_secs(2), operation)
            .await
            .expect("the batch hard backstop must terminate wedged persistence")
            .unwrap()
            .unwrap()
    else {
        panic!("batch result")
    };

    assert!(started.elapsed() >= std::time::Duration::from_millis(400));
    assert_eq!(result.outcome, BatchOutcome::TimedOut, "{result:#?}");
    assert_eq!(result.steps[0].status, BatchStepStatus::Failed);
    assert_eq!(
        result.steps[0].error.as_ref().map(|error| error.code),
        Some(ErrorCode::WaitTimedOut)
    );
    assert!(result.steps[0].result.is_none());
    assert_eq!(
        transport
            .command_calls()
            .iter()
            .filter(|call| call.method == "Input.dispatchMouseEvent")
            .count()
            - input_calls_before,
        3,
        "the wedged path reached persistence after dispatch"
    );
    session.stop().await.unwrap();
}

#[tokio::test]
async fn session_stop_cancels_an_active_batch_and_marks_remaining_steps() {
    let transport = ScriptedCdp::chrome();
    let session = scripted_session(transport.clone()).await;
    let target = session.status().await.unwrap().selected_target_id.unwrap();
    transport.hold_method("Runtime.evaluate");
    let operation = {
        let session = Arc::clone(&session);
        tokio::spawn(async move {
            session
                .execute(
                    BrowserOperationRequest::Batch(
                        BatchRequest::new(
                            PageSelection::Target(target),
                            vec![
                                BrowserOperationRequest::EvaluatePage(
                                    ReadOnlyEvaluationRequest::new(target, "true", false).unwrap(),
                                ),
                                BrowserOperationRequest::EvaluatePage(
                                    ReadOnlyEvaluationRequest::new(target, "false", false).unwrap(),
                                ),
                            ],
                            std::time::Duration::from_secs(10),
                            BatchOptions::default(),
                        )
                        .unwrap(),
                    ),
                    krometrail_core::BrowserOperationContext::default(),
                )
                .await
        })
    };
    transport
        .wait_for_command_count("Runtime.evaluate", 3)
        .await;
    session.stop().await.unwrap();
    let result = operation.await.unwrap().unwrap();
    let BrowserOperationResult::Batch(result) = result else {
        panic!("batch")
    };
    assert_eq!(result.outcome, BatchOutcome::Cancelled);
    assert_eq!(
        result.steps[0].error.as_ref().unwrap().code,
        ErrorCode::Cancelled
    );
    assert_eq!(
        result.steps[1].skip_reason,
        Some(BatchSkipReason::BatchCancelled)
    );
}

#[tokio::test]
async fn requested_step_screenshot_uses_standalone_path_before_one_final_observation() {
    let transport = ScriptedCdp::chrome();
    let session = scripted_session(transport.clone()).await;
    let target = session.status().await.unwrap().selected_target_id.unwrap();
    transport.push_response(
        "Runtime.evaluate",
        json!({"result":{"type":"boolean","value":true}}),
    );
    transport.push_response("Page.getLayoutMetrics", layout());
    transport.push_response("Runtime.evaluate", json!({"result":{"value":1.0}}));
    transport.push_response("Page.captureScreenshot", json!({"data":png_base64()}));
    live_observation_script(&transport);
    let request = BatchRequest::new(
        PageSelection::Target(target),
        vec![BrowserOperationRequest::EvaluatePage(
            ReadOnlyEvaluationRequest::new(target, "true", false).unwrap(),
        )],
        std::time::Duration::from_secs(1),
        BatchOptions {
            failure_policy: BatchFailurePolicy::StopOnFailure,
            include_step_screenshots: true,
        },
    )
    .unwrap();
    let result = session
        .execute(
            BrowserOperationRequest::Batch(request),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::Batch(result) = result else {
        panic!("batch")
    };
    assert!(matches!(
        result.steps[0].screenshot,
        Some(ObservationPart::Available(_))
    ));
    assert!(matches!(
        result.final_observation,
        ObservationPart::Available(_)
    ));
    assert_eq!(
        transport
            .command_calls()
            .iter()
            .filter(|call| call.method == "Page.captureScreenshot")
            .count(),
        2
    );
    session.stop().await.unwrap();
}

fn selector(value: &str) -> InteractionLocator {
    InteractionLocator::Element(ElementLocator::CssSelector(
        krometrail_core::NonEmptyText::new(value).unwrap(),
    ))
}

async fn click(
    session: &Arc<dyn krometrail_core::BrowserSessionPort>,
    target: krometrail_core::TargetId,
    selector_value: &str,
) -> BrowserOperationResult {
    session
        .execute(
            BrowserOperationRequest::Click(
                ClickRequest::new(
                    PageSelection::Target(target),
                    selector(selector_value),
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
        .unwrap()
}

async fn wait_for(
    session: &Arc<dyn krometrail_core::BrowserSessionPort>,
    target: krometrail_core::TargetId,
    condition: WaitCondition,
) -> krometrail_core::WaitResult {
    wait_for_with_timeout(
        session,
        target,
        condition,
        std::time::Duration::from_secs(3),
    )
    .await
}

async fn wait_for_with_timeout(
    session: &Arc<dyn krometrail_core::BrowserSessionPort>,
    target: krometrail_core::TargetId,
    condition: WaitCondition,
    timeout: std::time::Duration,
) -> krometrail_core::WaitResult {
    let poll_interval = if matches!(&condition, WaitCondition::Semantic { .. }) {
        std::time::Duration::from_millis(100)
    } else {
        std::time::Duration::from_millis(25)
    };
    let result = session
        .execute(
            BrowserOperationRequest::Wait(
                WaitRequest::new(
                    PageSelection::Target(target),
                    condition,
                    timeout,
                    poll_interval,
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::Wait(result) = result else {
        panic!("wait result")
    };
    *result
}

#[tokio::test]
async fn scripted_semantic_wait_present_satisfies_in_one_poll() {
    let transport = ScriptedCdp::chrome();
    let session = scripted_session(transport.clone()).await;
    let target = session.status().await.unwrap().selected_target_id.unwrap();
    transport.push_response("Page.getFrameTree", frame_tree());
    transport.push_response(
        "Accessibility.getFullAXTree",
        semantic_ax_tree(Some("Save")),
    );

    let result = wait_for(
        &session,
        target,
        WaitCondition::Semantic {
            query: SemanticQuery::role(
                "button",
                Some(SemanticTextMatch::new("Save", SemanticTextMatchMode::Exact, false).unwrap()),
            )
            .unwrap(),
            presence: WaitPresence::Present,
        },
    )
    .await;
    assert!(matches!(result.outcome, WaitOutcome::Satisfied { .. }));
    assert!(matches!(
        result.last_probe,
        Some(WaitProbe::Semantic {
            matched: true,
            outcome: SemanticQueryOutcome::Unique,
            match_count: 1,
            relaxed_match_candidates: None,
        })
    ));
    session.stop().await.unwrap();
}

#[tokio::test]
async fn scripted_semantic_wait_absent_satisfies_in_one_poll() {
    let transport = ScriptedCdp::chrome();
    let session = scripted_session(transport.clone()).await;
    let target = session.status().await.unwrap().selected_target_id.unwrap();
    transport.push_response("Page.getFrameTree", frame_tree());
    transport.push_response("Accessibility.getFullAXTree", semantic_ax_tree(None));

    let result = wait_for(
        &session,
        target,
        WaitCondition::Semantic {
            query: SemanticQuery::role(
                "button",
                Some(
                    SemanticTextMatch::new("Never rendered", SemanticTextMatchMode::Exact, false)
                        .unwrap(),
                ),
            )
            .unwrap(),
            presence: WaitPresence::Absent,
        },
    )
    .await;
    assert!(matches!(result.outcome, WaitOutcome::Satisfied { .. }));
    assert!(matches!(
        result.last_probe,
        Some(WaitProbe::Semantic {
            matched: true,
            outcome: SemanticQueryOutcome::NoMatch,
            match_count: 0,
            relaxed_match_candidates: None,
        })
    ));
    session.stop().await.unwrap();
}

#[tokio::test]
async fn scripted_semantic_wait_timeout_carries_relaxed_match_candidates() {
    let transport = ScriptedCdp::chrome();
    let session = scripted_session(transport.clone()).await;
    let target = session.status().await.unwrap().selected_target_id.unwrap();
    transport.push_response("Page.getFrameTree", frame_tree());
    transport.push_response(
        "Accessibility.getFullAXTree",
        semantic_ax_tree(Some("Save now")),
    );

    let result = wait_for_with_timeout(
        &session,
        target,
        WaitCondition::Semantic {
            query: SemanticQuery::role(
                "button",
                Some(SemanticTextMatch::new("Save", SemanticTextMatchMode::Exact, false).unwrap()),
            )
            .unwrap(),
            presence: WaitPresence::Present,
        },
        std::time::Duration::from_millis(50),
    )
    .await;
    assert!(matches!(result.outcome, WaitOutcome::TimedOut { .. }));
    let Some(WaitProbe::Semantic {
        matched: false,
        outcome: SemanticQueryOutcome::NoMatch,
        match_count: 0,
        relaxed_match_candidates: Some(candidates),
    }) = result.last_probe
    else {
        panic!("exact semantic timeout retains relaxed candidate evidence: {result:?}");
    };
    assert_eq!(candidates.count, 1);
    assert!(!candidates.saturated);
    session.stop().await.unwrap();
}

#[tokio::test]
async fn scripted_semantic_wait_continues_after_stale_snapshot_poll() {
    let transport = ScriptedCdp::chrome();
    let session = scripted_session(transport.clone()).await;
    let target = session.status().await.unwrap().selected_target_id.unwrap();
    // The first DOM-semantic capture observes a different document on its
    // post-capture fingerprint check, producing StaleReference. The next
    // poll sees a stable document and can satisfy the same query.
    transport.push_response("Page.getFrameTree", frame_tree_with_loader("loader-1"));
    transport.push_response(
        "Accessibility.getFullAXTree",
        semantic_ax_tree(Some("Ready")),
    );
    transport.push_response("DOMSnapshot.captureSnapshot", semantic_dom_snapshot());
    transport.push_response("Page.getFrameTree", frame_tree_with_loader("loader-2"));
    transport.push_response("Page.getFrameTree", frame_tree_with_loader("loader-2"));
    transport.push_response(
        "Accessibility.getFullAXTree",
        semantic_ax_tree(Some("Ready")),
    );
    transport.push_response("DOMSnapshot.captureSnapshot", semantic_dom_snapshot());
    transport.push_response("Page.getFrameTree", frame_tree_with_loader("loader-2"));

    let result = wait_for(
        &session,
        target,
        WaitCondition::Semantic {
            query: SemanticQuery::Text {
                text: SemanticTextMatch::new("Ready", SemanticTextMatchMode::Exact, false).unwrap(),
            },
            presence: WaitPresence::Present,
        },
    )
    .await;
    assert!(matches!(result.outcome, WaitOutcome::Satisfied { .. }));
    assert!(matches!(
        result.last_probe,
        Some(WaitProbe::Semantic {
            matched: true,
            outcome: SemanticQueryOutcome::Unique,
            match_count: 1,
            relaxed_match_candidates: None,
        })
    ));
    assert_eq!(
        transport
            .command_calls()
            .iter()
            .filter(|call| call.method == "Page.getFrameTree")
            .count(),
        4,
        "the stale semantic poll must be followed by a fresh poll"
    );
    session.stop().await.unwrap();
}

async fn launch_real_fixture(
    name: &str,
) -> (
    Arc<dyn krometrail_core::BrowserSessionPort>,
    support::chrome::TemporaryRootGuard,
) {
    let root = support::chrome::temporary_profile_root(name);
    let connector = ProductionBrowserConnector::new(
        Arc::new(krometrail_cdp::SystemChromeLauncher::new(
            krometrail_cdp::LauncherConfig {
                profile_root: root.path().to_path_buf(),
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
    let session = connector
        .connect(BrowserConnectRequest::Launch(LaunchBrowser {
            executable: None,
            profile: ManagedProfile::Temporary,
            initial_url: Some(support::chrome::waits_and_batches_fixture_url("index.html")),
            every_nth_frame: krometrail_core::EveryNthFrame::default(),
            focus: krometrail_core::BrowserFocusPolicy::default(),
        }))
        .await
        .expect("real waits/batches fixture");
    (session, root)
}

#[tokio::test]
async fn opt_in_real_chrome_qualifies_every_wait_family_and_stale_references() {
    if !support::chrome::real_browser_tests_enabled() {
        eprintln!("skipping real Chrome wait families; set KROMETRAIL_REAL_CHROME_TESTS=1");
        return;
    }
    let _lock = support::chrome::real_browser_lock().await;
    let (session, _root) = launch_real_fixture("waits-and-batches-waits").await;
    let target = session.status().await.unwrap().selected_target_id.unwrap();

    assert!(matches!(
        wait_for(
            &session,
            target,
            WaitCondition::Page {
                expression: krometrail_core::NonEmptyText::new(
                    "typeof window.fixtureState === 'object'"
                )
                .unwrap(),
            },
        )
        .await
        .outcome,
        WaitOutcome::Satisfied { .. }
    ));
    assert!(matches!(
        wait_for(
            &session,
            target,
            WaitCondition::Elapsed {
                duration: std::time::Duration::from_millis(20),
            },
        )
        .await
        .outcome,
        WaitOutcome::Satisfied { .. }
    ));

    click(&session, target, "#start-delays").await;
    for state in [
        ElementState::Attached,
        ElementState::Hidden,
        ElementState::Disabled,
    ] {
        assert!(matches!(
            wait_for(
                &session,
                target,
                WaitCondition::Element {
                    locator: ElementLocator::CssSelector(
                        krometrail_core::NonEmptyText::new("#dynamic-button").unwrap(),
                    ),
                    state,
                },
            )
            .await
            .outcome,
            WaitOutcome::Satisfied { .. }
        ));
    }
    for (locator, state) in [
        ("#check", ElementState::Unchecked),
        ("#editable", ElementState::Editable),
        ("#dynamic-button", ElementState::Visible),
        ("#dynamic-button", ElementState::Enabled),
        ("#check", ElementState::Checked),
    ] {
        assert!(matches!(
            wait_for(
                &session,
                target,
                WaitCondition::Element {
                    locator: ElementLocator::CssSelector(
                        krometrail_core::NonEmptyText::new(locator).unwrap(),
                    ),
                    state,
                },
            )
            .await
            .outcome,
            WaitOutcome::Satisfied { .. }
        ));
    }
    let text = wait_for(
        &session,
        target,
        WaitCondition::Text {
            locator: Some(ElementLocator::CssSelector(
                krometrail_core::NonEmptyText::new("#delayed-text").unwrap(),
            )),
            text: krometrail_core::NonEmptyText::new("DELAYED TEXT READY").unwrap(),
            match_mode: WaitTextMatch::Exact,
            presence: WaitPresence::Present,
            case_sensitive: false,
        },
    )
    .await;
    assert!(matches!(text.outcome, WaitOutcome::Satisfied { .. }));
    assert!(matches!(
        text.last_probe,
        Some(WaitProbe::Text {
            observed_length: Some(length),
            ..
        }) if length > 0
    ));
    assert!(matches!(
        wait_for(
            &session,
            target,
            WaitCondition::Text {
                locator: Some(ElementLocator::CssSelector(
                    krometrail_core::NonEmptyText::new("#missing").unwrap(),
                )),
                text: krometrail_core::NonEmptyText::new("private text").unwrap(),
                match_mode: WaitTextMatch::Contains,
                presence: WaitPresence::Absent,
                case_sensitive: true,
            },
        )
        .await
        .outcome,
        WaitOutcome::Satisfied { .. }
    ));
    assert!(matches!(
        wait_for(
            &session,
            target,
            WaitCondition::Page {
                expression: krometrail_core::NonEmptyText::new("window.fixtureState.ready")
                    .unwrap(),
            },
        )
        .await
        .outcome,
        WaitOutcome::Satisfied { .. }
    ));

    click(&session, target, "#start-network").await;
    let network = wait_for(
        &session,
        target,
        WaitCondition::NetworkQuiet {
            quiet_for: std::time::Duration::from_millis(300),
        },
    )
    .await;
    assert!(matches!(network.outcome, WaitOutcome::Satisfied { .. }));
    assert!(matches!(
        network.last_probe,
        Some(WaitProbe::NetworkQuiet {
            in_flight: 0,
            tracks_from_subscription: true,
            excludes_long_lived_connections: true,
            ..
        })
    ));
    let loaded = session
        .execute(
            BrowserOperationRequest::EvaluatePage(
                ReadOnlyEvaluationRequest::new(
                    target,
                    "document.querySelector('#network-image').currentSrc.includes('payload.svg')",
                    false,
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::EvaluatePage(loaded) = loaded else {
        panic!("network fixture state")
    };
    assert_eq!(loaded.value, EvaluationValue::Json(json!(true)));

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
            (node.name.as_deref() == Some("Replaceable target"))
                .then_some(node.reference)
                .flatten()
        })
        .expect("replaceable reference");
    click(&session, target, "#replace-node").await;
    let stale = session
        .execute(
            BrowserOperationRequest::Wait(
                WaitRequest::new(
                    PageSelection::Target(target),
                    WaitCondition::Text {
                        locator: Some(ElementLocator::Reference(reference)),
                        text: krometrail_core::NonEmptyText::new("Replacement").unwrap(),
                        match_mode: WaitTextMatch::Contains,
                        presence: WaitPresence::Present,
                        case_sensitive: true,
                    },
                    std::time::Duration::from_secs(1),
                    std::time::Duration::from_millis(25),
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(stale.code, ErrorCode::StaleReference);

    let deadline = session
        .execute(
            BrowserOperationRequest::Wait(
                WaitRequest::new(
                    PageSelection::Target(target),
                    WaitCondition::Page {
                        expression: krometrail_core::NonEmptyText::new("false").unwrap(),
                    },
                    std::time::Duration::from_millis(30),
                    std::time::Duration::from_millis(10),
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::Wait(deadline) = deadline else {
        panic!("deadline wait")
    };
    assert!(matches!(deadline.outcome, WaitOutcome::TimedOut { .. }));

    click(&session, target, "#navigate").await;
    let navigation = wait_for(
        &session,
        target,
        WaitCondition::Navigation {
            readiness: DocumentReadiness::Complete,
            url: Some((
                UrlMatch::Prefix,
                krometrail_core::NonEmptyText::new(support::chrome::waits_and_batches_fixture_url(
                    "second.html",
                ))
                .unwrap(),
            )),
        },
    )
    .await;
    assert!(
        matches!(navigation.outcome, WaitOutcome::Satisfied { .. }),
        "navigation wait did not satisfy: {navigation:?}"
    );
    assert!(matches!(
        navigation.last_probe,
        Some(WaitProbe::Navigation {
            readiness: DocumentReadiness::Complete,
            url_matched: Some(true),
            ..
        })
    ));
    let semantic_present = wait_for(
        &session,
        target,
        WaitCondition::Semantic {
            query: SemanticQuery::role(
                "heading",
                Some(
                    SemanticTextMatch::new(
                        "Navigation complete",
                        SemanticTextMatchMode::Exact,
                        false,
                    )
                    .unwrap(),
                ),
            )
            .unwrap(),
            presence: WaitPresence::Present,
        },
    )
    .await;
    assert!(matches!(
        semantic_present.outcome,
        WaitOutcome::Satisfied { .. }
    ));
    assert!(matches!(
        semantic_present.last_probe,
        Some(WaitProbe::Semantic {
            outcome: SemanticQueryOutcome::Unique,
            match_count: 1,
            relaxed_match_candidates: None,
            ..
        })
    ));
    let semantic_absent = wait_for(
        &session,
        target,
        WaitCondition::Semantic {
            query: SemanticQuery::role(
                "button",
                Some(
                    SemanticTextMatch::new("Never rendered", SemanticTextMatchMode::Exact, false)
                        .unwrap(),
                ),
            )
            .unwrap(),
            presence: WaitPresence::Absent,
        },
    )
    .await;
    assert!(matches!(
        semantic_absent.outcome,
        WaitOutcome::Satisfied { .. }
    ));
    assert!(matches!(
        semantic_absent.last_probe,
        Some(WaitProbe::Semantic {
            outcome: SemanticQueryOutcome::NoMatch,
            match_count: 0,
            relaxed_match_candidates: None,
            ..
        })
    ));

    let operation = {
        let session = Arc::clone(&session);
        tokio::spawn(async move {
            session
                .execute(
                    BrowserOperationRequest::Wait(
                        WaitRequest::new(
                            PageSelection::Target(target),
                            WaitCondition::Page {
                                expression: krometrail_core::NonEmptyText::new("false").unwrap(),
                            },
                            std::time::Duration::from_secs(10),
                            std::time::Duration::from_millis(25),
                        )
                        .unwrap(),
                    ),
                    krometrail_core::BrowserOperationContext::default(),
                )
                .await
        })
    };
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    session.stop().await.unwrap();
    assert_eq!(
        operation.await.unwrap().unwrap_err().code,
        ErrorCode::Cancelled
    );
}

#[tokio::test]
async fn opt_in_real_chrome_qualifies_semantic_wait_present_and_absent() {
    if !support::chrome::real_browser_tests_enabled() {
        eprintln!("skipping real Chrome semantic waits; set KROMETRAIL_REAL_CHROME_TESTS=1");
        return;
    }
    let _lock = support::chrome::real_browser_lock().await;
    let (session, _root) = launch_real_fixture("waits-and-batches-semantic-waits").await;
    let target = session.status().await.unwrap().selected_target_id.unwrap();

    let present = wait_for(
        &session,
        target,
        WaitCondition::Semantic {
            query: SemanticQuery::role(
                "button",
                Some(
                    SemanticTextMatch::new(
                        "Start delayed states",
                        SemanticTextMatchMode::Exact,
                        false,
                    )
                    .unwrap(),
                ),
            )
            .unwrap(),
            presence: WaitPresence::Present,
        },
    )
    .await;
    assert!(matches!(present.outcome, WaitOutcome::Satisfied { .. }));
    assert!(matches!(
        present.last_probe,
        Some(WaitProbe::Semantic {
            outcome: SemanticQueryOutcome::Unique,
            match_count: 1,
            relaxed_match_candidates: None,
            ..
        })
    ));

    let absent = wait_for(
        &session,
        target,
        WaitCondition::Semantic {
            query: SemanticQuery::role(
                "button",
                Some(
                    SemanticTextMatch::new("Never rendered", SemanticTextMatchMode::Exact, false)
                        .unwrap(),
                ),
            )
            .unwrap(),
            presence: WaitPresence::Absent,
        },
    )
    .await;
    assert!(matches!(absent.outcome, WaitOutcome::Satisfied { .. }));
    assert!(matches!(
        absent.last_probe,
        Some(WaitProbe::Semantic {
            outcome: SemanticQueryOutcome::NoMatch,
            match_count: 0,
            relaxed_match_candidates: None,
            ..
        })
    ));
    session.stop().await.unwrap();
}

#[tokio::test]
async fn opt_in_real_chrome_qualifies_ordered_batches_and_failure_policies() {
    if !support::chrome::real_browser_tests_enabled() {
        eprintln!("skipping real Chrome batches; set KROMETRAIL_REAL_CHROME_TESTS=1");
        return;
    }
    let _lock = support::chrome::real_browser_lock().await;
    let (session, _root) = launch_real_fixture("waits-and-batches-batches").await;
    let target = session.status().await.unwrap().selected_target_id.unwrap();
    let increment = || {
        BrowserOperationRequest::Click(
            ClickRequest::new(
                PageSelection::Target(target),
                selector("#increment"),
                MouseButton::Left,
                Modifiers::default(),
                1,
                false,
            )
            .unwrap(),
        )
    };
    let count_wait = |expected: &str| {
        BrowserOperationRequest::Wait(
            WaitRequest::new(
                PageSelection::Target(target),
                WaitCondition::Page {
                    expression: krometrail_core::NonEmptyText::new(format!(
                        "window.fixtureState.count === {expected}"
                    ))
                    .unwrap(),
                },
                std::time::Duration::from_secs(2),
                std::time::Duration::from_millis(25),
            )
            .unwrap(),
        )
    };
    let batch = BatchRequest::new(
        PageSelection::Target(target),
        vec![
            increment(),
            count_wait("1"),
            BrowserOperationRequest::EvaluatePage(
                ReadOnlyEvaluationRequest::new(target, "window.fixtureState.count === 1", false)
                    .unwrap(),
            ),
        ],
        std::time::Duration::from_secs(60),
        BatchOptions {
            failure_policy: BatchFailurePolicy::StopOnFailure,
            include_step_screenshots: true,
        },
    )
    .unwrap();
    let result = session
        .execute(
            BrowserOperationRequest::Batch(batch),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::Batch(result) = result else {
        panic!("batch")
    };
    let observed_count = session
        .execute(
            BrowserOperationRequest::EvaluatePage(
                ReadOnlyEvaluationRequest::new(target, "window.fixtureState.count", false).unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::EvaluatePage(observed_count) = observed_count else {
        panic!("count evaluation")
    };
    let final_degraded = match &result.final_observation {
        ObservationPart::Unavailable(_) => true,
        ObservationPart::Available(observation) => {
            matches!(observation.page, ObservationPart::Unavailable(_))
                || matches!(observation.snapshot, ObservationPart::Unavailable(_))
                || matches!(observation.screenshot, ObservationPart::Unavailable(_))
        }
    };
    assert_eq!(
        result.outcome,
        if final_degraded {
            BatchOutcome::CompletedWithFailures
        } else {
            BatchOutcome::Completed
        },
        "golden batch failed: statuses={:?}, errors={:?}, timings={:?}, count={:?}",
        result
            .steps
            .iter()
            .map(|step| step.status)
            .collect::<Vec<_>>(),
        result
            .steps
            .iter()
            .map(|step| step.error.as_ref().map(|error| error.code))
            .collect::<Vec<_>>(),
        result
            .steps
            .iter()
            .map(|step| (
                step.started_at.map(|time| time.as_nanos()),
                step.completed_at.map(|time| time.as_nanos())
            ))
            .collect::<Vec<_>>(),
        observed_count.value,
    );
    assert!(result.steps.iter().all(|step| {
        step.status == BatchStepStatus::Succeeded && step.started_at <= step.completed_at
    }));
    assert!(
        result
            .steps
            .iter()
            .filter(|step| matches!(step.screenshot, Some(ObservationPart::Available(_))))
            .count()
            >= 2,
        "requested screenshots must be returned when Chrome supplies them"
    );
    assert!(result.steps.iter().all(|step| match &step.screenshot {
        Some(ObservationPart::Available(_)) => true,
        Some(ObservationPart::Unavailable(error)) => error.code == ErrorCode::ScreenshotFailed,
        None => false,
    }));
    let BrowserOperationResult::Click(click) = result.steps[0].result.as_ref().unwrap() else {
        panic!("click child")
    };
    assert_eq!(click.record.parent_batch, Some(result.batch_id));
    assert_eq!(
        result.steps[0].interaction.as_ref().unwrap().interaction_id,
        click.record.id
    );
    assert!(matches!(
        result.final_observation,
        ObservationPart::Available(_)
    ));

    let failing_wait = || {
        BrowserOperationRequest::Wait(
            WaitRequest::new(
                PageSelection::Target(target),
                WaitCondition::Page {
                    expression: krometrail_core::NonEmptyText::new("false").unwrap(),
                },
                std::time::Duration::from_millis(40),
                std::time::Duration::from_millis(10),
            )
            .unwrap(),
        )
    };
    let stopped = session
        .execute(
            BrowserOperationRequest::Batch(
                BatchRequest::new(
                    PageSelection::Target(target),
                    vec![failing_wait(), increment()],
                    std::time::Duration::from_secs(10),
                    BatchOptions::default(),
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::Batch(stopped) = stopped else {
        panic!("stopped batch")
    };
    assert_eq!(stopped.outcome, BatchOutcome::StoppedOnFailure);
    assert_eq!(stopped.steps[0].status, BatchStepStatus::Failed);
    assert!(matches!(
        stopped.steps[0].result,
        Some(BrowserOperationResult::Wait(_))
    ));
    assert_eq!(stopped.steps[1].status, BatchStepStatus::Skipped);
    assert_eq!(
        stopped.steps[1].skip_reason,
        Some(BatchSkipReason::PriorFailure)
    );

    let continued = session
        .execute(
            BrowserOperationRequest::Batch(
                BatchRequest::new(
                    PageSelection::Target(target),
                    vec![failing_wait(), increment(), count_wait("2")],
                    std::time::Duration::from_secs(45),
                    BatchOptions {
                        failure_policy: BatchFailurePolicy::ContinueOnFailure,
                        include_step_screenshots: false,
                    },
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::Batch(continued) = continued else {
        panic!("continued batch")
    };
    assert_eq!(continued.outcome, BatchOutcome::CompletedWithFailures);
    assert_eq!(continued.steps[0].status, BatchStepStatus::Failed);
    assert_eq!(continued.steps[1].status, BatchStepStatus::Succeeded);
    assert_eq!(continued.steps[2].status, BatchStepStatus::Succeeded);
    assert!(matches!(
        continued.final_observation,
        ObservationPart::Available(_)
    ));

    let navigation = session
        .execute(
            BrowserOperationRequest::Batch(
                BatchRequest::new(
                    PageSelection::Target(target),
                    vec![
                        BrowserOperationRequest::Click(
                            ClickRequest::new(
                                PageSelection::Target(target),
                                selector("#navigate"),
                                MouseButton::Left,
                                Modifiers::default(),
                                1,
                                false,
                            )
                            .unwrap(),
                        ),
                        BrowserOperationRequest::Wait(
                            WaitRequest::new(
                                PageSelection::Target(target),
                                WaitCondition::Navigation {
                                    readiness: DocumentReadiness::Complete,
                                    url: Some((
                                        UrlMatch::Prefix,
                                        krometrail_core::NonEmptyText::new(
                                            support::chrome::waits_and_batches_fixture_url(
                                                "second.html",
                                            ),
                                        )
                                        .unwrap(),
                                    )),
                                },
                                std::time::Duration::from_secs(5),
                                std::time::Duration::from_millis(25),
                            )
                            .unwrap(),
                        ),
                    ],
                    std::time::Duration::from_secs(60),
                    BatchOptions::default(),
                )
                .unwrap(),
            ),
            krometrail_core::BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::Batch(navigation) = navigation else {
        panic!("navigation batch")
    };
    assert!(matches!(
        navigation.outcome,
        BatchOutcome::Completed | BatchOutcome::CompletedWithFailures
    ));
    assert!(
        navigation
            .steps
            .iter()
            .all(|step| step.status == BatchStepStatus::Succeeded)
    );
    session.stop().await.unwrap();
}

#[test]
fn waits_and_batches_fixture_is_standalone_and_has_stable_markers() {
    let index = include_str!("../../../tests/fixtures/browser/waits-and-batches/index.html");
    let second = include_str!("../../../tests/fixtures/browser/waits-and-batches/second.html");
    let payload = include_str!("../../../tests/fixtures/browser/waits-and-batches/payload.svg");
    assert!(index.contains("start-delays") && index.contains("start-network"));
    assert!(index.contains("WebSocket") && index.contains("replace-node"));
    assert!(second.contains("second page ready") && payload.contains("<svg"));
    assert!(!index.to_ascii_lowercase().contains("krometrail"));
}
