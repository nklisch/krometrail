#![cfg(feature = "cdpkit-transport")]

mod support;

use std::sync::Arc;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use krometrail_cdp::{
    CdpTransport, CdpTransportFactory, ProductionBrowserConnector, TransportError, TransportFuture,
};
use krometrail_core::{
    AttachBrowser, BatchFailurePolicy, BatchOptions, BatchOutcome, BatchRequest, BatchStepStatus,
    BrowserConnectRequest, BrowserConnector, BrowserOperationRequest, BrowserOperationResult,
    ClickRequest, CoordinateSpace, CssPoint, EvaluationValue, InteractionLocator, Modifiers,
    MouseButton, ObservationPart, PageSelection, ReadOnlyEvaluationRequest, WaitCondition,
    WaitRequest,
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

async fn scripted_session(transport: ScriptedCdp) -> Arc<dyn krometrail_core::BrowserSessionPort> {
    startup_script(&transport);
    ProductionBrowserConnector::new(
        Arc::new(krometrail_cdp::SystemChromeLauncher::new(
            krometrail_cdp::LauncherConfig::default(),
        )),
        Arc::new(ScriptedFactory(transport)),
    )
    .connect(BrowserConnectRequest::Attach(
        AttachBrowser::new("ws://127.0.0.1:9222/devtools/browser/waits-batches").unwrap(),
    ))
    .await
    .unwrap()
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
        .execute(BrowserOperationRequest::Batch(request))
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
            .execute(BrowserOperationRequest::Batch(request))
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
        .execute(BrowserOperationRequest::Batch(request))
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
