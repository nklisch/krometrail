#![cfg(feature = "cdpkit-transport")]

mod support;

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use krometrail_cdp::{
    CdpTransport, CdpTransportFactory, ProductionBrowserConnector, TransportError, TransportFuture,
};
use krometrail_core::{
    AttachBrowser, BatchFailurePolicy, BatchOptions, BatchOutcome, BatchRequest, BatchSkipReason,
    BatchStepStatus, BrowserConnectRequest, BrowserConnector, BrowserOperationRequest,
    BrowserOperationResult, ClickRequest, CoordinateSpace, CssPoint, DocumentReadiness,
    ElementLocator, ElementState, ErrorCode, EvaluationValue, InteractionAnchor,
    InteractionEvidenceSink, InteractionLocator, InteractionRecord, LaunchBrowser, ManagedProfile,
    Modifiers, MouseButton, NavigationId, ObservationPart, ObservedTime, PageSelection, PortFuture,
    ReadOnlyEvaluationRequest, SnapshotPageRequest, UrlMatch, WaitCondition, WaitOutcome,
    WaitPresence, WaitProbe, WaitRequest, WaitTextMatch,
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
    json!({"frameTree":{"frame":{"id":"main","loaderId":"loader-1","url":"http://fixture/"}}})
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
    assert_eq!(result.outcome, BatchOutcome::Completed);
    assert_eq!(result.steps.len(), 1);
    assert_eq!(result.steps[0].status, BatchStepStatus::Succeeded);
    let anchor = result.steps[0].interaction.as_ref().expect("child anchor");
    let BrowserOperationResult::Click(click) = result.steps[0].result.as_ref().unwrap() else {
        panic!("click child")
    };
    assert_eq!(click.record.parent_batch, Some(result.batch_id));
    assert_eq!(anchor.interaction_id, click.record.id);
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
    assert_eq!(result.outcome, BatchOutcome::Completed);
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
    transport.push_event(
        "Network.requestWillBeSent",
        json!({"requestId":"private-request-id","type":"Fetch"}),
    );
    transport.push_event(
        "Network.requestWillBeSent",
        json!({"requestId":"private-websocket-id","type":"WebSocket"}),
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
        1
    );
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
        ObservationPart::Available(_)
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
    let result = session
        .execute(
            BrowserOperationRequest::Wait(
                WaitRequest::new(
                    PageSelection::Target(target),
                    condition,
                    std::time::Duration::from_secs(3),
                    std::time::Duration::from_millis(25),
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
            .filter(|step| matches!(step.screenshot, ObservationPart::Available(_)))
            .count()
            >= 2,
        "requested screenshots must be returned when Chrome supplies them"
    );
    assert!(result.steps.iter().all(|step| match &step.screenshot {
        ObservationPart::Available(_) => true,
        ObservationPart::Unavailable(error) => error.code == ErrorCode::ScreenshotFailed,
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
