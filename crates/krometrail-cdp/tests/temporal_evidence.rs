#![cfg(feature = "cdpkit-transport")]

mod support;

use std::sync::{Arc, Mutex};

use krometrail_cdp::{
    CdpTransport, CdpTransportFactory, ProductionBrowserConnector, TransportError, TransportFuture,
};
use krometrail_core::{
    AttachBrowser, BrowserConnectRequest, BrowserConnector, BrowserOperationContext,
    BrowserOperationRequest, BrowserOperationResult, ErrorCode, InspectPageRequest,
    InteractionAnchor, InteractionEvidenceSink, InteractionRecord, NavigationId, ObservedTime,
    PortFuture, RetryAdvice, SelectPageRequest,
};
use serde_json::{Value, json};
use support::scripted_cdp::ScriptedCdp;
use tokio::sync::Notify;

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

fn layout() -> Value {
    json!({
        "cssLayoutViewport":{"pageX":0.0,"pageY":0.0,"clientWidth":800.0,"clientHeight":600.0},
        "cssVisualViewport":{"pageX":0.0,"pageY":0.0,"clientWidth":800.0,"clientHeight":600.0,"scale":1.0},
        "cssContentSize":{"x":0.0,"y":0.0,"width":800.0,"height":600.0}
    })
}

fn script_inspection(transport: &ScriptedCdp) {
    transport.push_response(
        "Runtime.evaluate",
        json!({"result":{"type":"object","value":{
            "url":"http://fixture/","title":"Fixture","readiness":"complete","deviceScaleFactor":1.0
        }}}),
    );
    transport.push_response("Page.getLayoutMetrics", layout());
    transport.push_response(
        "Page.getNavigationHistory",
        json!({"currentIndex":0,"entries":[{"id":1,"url":"http://fixture/"}]}),
    );
}

fn script_live_observation(transport: &ScriptedCdp) {
    use base64::{Engine as _, engine::general_purpose::STANDARD};

    script_inspection(transport);
    transport.push_response(
        "Runtime.evaluate",
        json!({"result":{"type":"number","value":1.0}}),
    );
    transport.push_response("Page.getLayoutMetrics", layout());
    transport.push_response(
        "Page.getFrameTree",
        json!({"frameTree":{"frame":{"id":"main","loaderId":"loader","url":"http://fixture/"}}}),
    );
    transport.push_response(
        "Accessibility.getFullAXTree",
        json!({"nodes":[{"nodeId":"root","ignored":false,"role":{"value":"document"},"name":{"value":"Fixture"}}]}),
    );
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png.extend_from_slice(&13_u32.to_be_bytes());
    png.extend_from_slice(b"IHDR");
    png.extend_from_slice(&800_u32.to_be_bytes());
    png.extend_from_slice(&600_u32.to_be_bytes());
    transport.push_response(
        "Page.captureScreenshot",
        json!({"data":STANDARD.encode(png)}),
    );
}

#[derive(Default)]
struct ControlledSink {
    started: Notify,
    release: Notify,
    fail: bool,
    entries: Mutex<Vec<InteractionAnchor>>,
}

impl ControlledSink {
    fn delayed() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn failing() -> Arc<Self> {
        Arc::new(Self {
            fail: true,
            ..Self::default()
        })
    }
}

impl InteractionEvidenceSink for ControlledSink {
    fn append_operation_evidence(
        &self,
        anchor: InteractionAnchor,
        _record: Option<InteractionRecord>,
        _persisted_at: ObservedTime,
        _navigation_id: Option<NavigationId>,
    ) -> PortFuture<'_, krometrail_core::Result<()>> {
        Box::pin(async move {
            self.started.notify_one();
            if !self.fail {
                self.release.notified().await;
                self.entries.lock().unwrap().push(anchor);
                Ok(())
            } else {
                Err(krometrail_core::KrometrailError::new(
                    ErrorCode::PersistenceFailed,
                    krometrail_core::NonEmptyText::new("deliberate sink failure").unwrap(),
                ))
            }
        })
    }
}

async fn build_session(
    transport: &ScriptedCdp,
    sink: Option<Arc<dyn InteractionEvidenceSink>>,
) -> Arc<dyn krometrail_core::BrowserSessionPort> {
    script_initial(transport);
    let mut connector = ProductionBrowserConnector::new(
        Arc::new(krometrail_cdp::SystemChromeLauncher::new(
            krometrail_cdp::LauncherConfig::default(),
        )),
        Arc::new(ScriptedFactory(transport.clone())),
    );
    if let Some(sink) = sink {
        connector = connector.with_interaction_evidence(sink);
    }
    connector
        .connect(BrowserConnectRequest::Attach(
            AttachBrowser::new("ws://127.0.0.1:9222/devtools/browser/evidence").unwrap(),
        ))
        .await
        .unwrap()
}

#[tokio::test]
async fn missing_sink_rejects_before_dispatch_while_read_only_work_remains_available() {
    let transport = ScriptedCdp::chrome();
    let session = build_session(&transport, None).await;
    let target = session.status().await.unwrap().pages[0].target.target.id();
    let before = transport.commands().len();
    let error = session
        .execute(
            BrowserOperationRequest::SelectPage(SelectPageRequest { target_id: target }),
            BrowserOperationContext::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::PersistenceFailed);
    assert_eq!(error.retry, RetryAdvice::Never);
    assert_eq!(transport.commands().len(), before);

    script_inspection(&transport);
    assert!(matches!(
        session
            .execute(
                BrowserOperationRequest::InspectPage(InspectPageRequest::new(target)),
                BrowserOperationContext::default(),
            )
            .await
            .unwrap(),
        BrowserOperationResult::InspectPage(_)
    ));
}

#[tokio::test]
async fn publication_waits_for_commit_and_failure_reports_inspect_before_repeat_uncertainty() {
    let transport = ScriptedCdp::chrome();
    let sink = ControlledSink::delayed();
    let session = build_session(
        &transport,
        Some(Arc::clone(&sink) as Arc<dyn InteractionEvidenceSink>),
    )
    .await;
    let target = session.status().await.unwrap().pages[0].target.target.id();
    script_live_observation(&transport);
    let operation = {
        let session = Arc::clone(&session);
        tokio::spawn(async move {
            session
                .execute(
                    BrowserOperationRequest::SelectPage(SelectPageRequest { target_id: target }),
                    BrowserOperationContext::default(),
                )
                .await
        })
    };
    sink.started.notified().await;
    assert!(!operation.is_finished());
    sink.release.notify_one();
    assert!(matches!(
        operation.await.unwrap().unwrap(),
        BrowserOperationResult::SelectPage(_)
    ));
    assert_eq!(sink.entries.lock().unwrap().len(), 1);

    let failing_transport = ScriptedCdp::chrome();
    let failing = ControlledSink::failing();
    let failed_session = build_session(
        &failing_transport,
        Some(failing as Arc<dyn InteractionEvidenceSink>),
    )
    .await;
    let failed_target = failed_session.status().await.unwrap().pages[0]
        .target
        .target
        .id();
    script_live_observation(&failing_transport);
    let error = failed_session
        .execute(
            BrowserOperationRequest::SelectPage(SelectPageRequest {
                target_id: failed_target,
            }),
            BrowserOperationContext::default(),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::PersistenceFailed);
    assert_eq!(error.retry, RetryAdvice::Never);
    assert!(error.context.interaction_id.is_some());
    assert!(
        error
            .recovery
            .as_ref()
            .unwrap()
            .as_str()
            .contains("inspect the current page")
    );
    assert!(
        failing_transport
            .commands()
            .iter()
            .any(|(method, _)| method == "Target.activateTarget")
    );
}
