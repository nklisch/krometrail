#![cfg(feature = "cdpkit-transport")]

mod support;

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use krometrail_cdp::{
    CdpTransport, CdpTransportFactory, ProductionBrowserConnector, TransportError, TransportFuture,
};
use krometrail_core::{
    AnchorScope, AttachBrowser, BatchOptions, BatchRequest, BrowserConnectRequest,
    BrowserConnector, BrowserOperationContext, BrowserOperationRequest, BrowserOperationResult,
    CaptureOrdinal, CapturedFrame, ClickRequest, CoordinateSpace, CssPoint, DeviceScaleFactor,
    EncodedFrame, ErrorCode, FrameId, ImageFormat, InspectPageRequest, InteractionAnchor,
    InteractionEvidenceSink, InteractionLocator, InteractionRecord, Modifiers, MouseButton,
    NavigationId, ObservedTime, PageSelection, PixelDimensions, PortFuture, RecordingSink,
    RetryAdvice, SelectPageRequest, SessionTime, TemporalQuery, TemporalQueryRequest,
    TemporalRangeAnchor,
};
use krometrail_store::{
    IndexStoreConfig, RecordingStore, RotationConfig, SegmentStoreConfig, SegmentWriter,
    SqliteIndex,
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

fn script_coordinate_click(transport: &ScriptedCdp) {
    transport.push_response("Page.getLayoutMetrics", layout());
    transport.push_response(
        "Runtime.evaluate",
        json!({"result":{"value":{"tagName":"BUTTON","x":10,"y":10,"width":20,"height":20}}}),
    );
    transport.push_response("Runtime.evaluate", json!({"result":{"value":true}}));
    script_live_observation(transport);
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

struct TemporaryStoreRoot(std::path::PathBuf);

impl TemporaryStoreRoot {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "krometrail-temporal-evidence-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TemporaryStoreRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn recording_store() -> (TemporaryStoreRoot, Arc<RecordingStore>) {
    let root = TemporaryStoreRoot::new();
    let segments = root.0.join("segments");
    let index = Arc::new(
        SqliteIndex::open(IndexStoreConfig {
            database_path: root.0.join("index.sqlite3"),
            segments_directory: segments.clone(),
            busy_timeout: Duration::from_secs(1),
        })
        .unwrap(),
    );
    let writer = Arc::new(
        SegmentWriter::open(SegmentStoreConfig {
            directory: segments,
            rotation: RotationConfig::suggested(),
        })
        .unwrap(),
    );
    let store = Arc::new(RecordingStore::new(writer, index).unwrap());
    (root, store)
}

fn evidence_frame(
    id: u128,
    session_id: krometrail_core::SessionId,
    target_id: krometrail_core::TargetId,
    ordinal: u64,
    at: Duration,
) -> EncodedFrame {
    EncodedFrame::new(
        CapturedFrame::new(
            FrameId::from_uuid(uuid::Uuid::from_u128(id)),
            session_id,
            target_id,
            CaptureOrdinal::new(ordinal).unwrap(),
            None,
            ObservedTime::from_nanos(u64::try_from(at.as_nanos()).unwrap()),
            SessionTime::from_nanos(u64::try_from(at.as_nanos()).unwrap()),
            ImageFormat::Jpeg,
            PixelDimensions::new(1, 1).unwrap(),
            PixelDimensions::new(1, 1).unwrap(),
            DeviceScaleFactor::new(1.0).unwrap(),
            vec![],
        )
        .unwrap(),
        vec![ordinal as u8],
    )
    .unwrap()
}

#[tokio::test]
async fn successful_operation_is_immediately_queryable_through_the_same_recording_store() {
    let (_root, store) = recording_store();
    let transport = ScriptedCdp::chrome();
    let session = build_session(
        &transport,
        Some(Arc::clone(&store) as Arc<dyn InteractionEvidenceSink>),
    )
    .await;
    let status = session.status().await.unwrap();
    let target = status.pages[0].target.target.id();
    store
        .append_frame(evidence_frame(
            100,
            status.session_id,
            target,
            1,
            Duration::ZERO,
        ))
        .await
        .unwrap();
    store
        .append_frame(evidence_frame(
            101,
            status.session_id,
            target,
            2,
            Duration::from_secs(10),
        ))
        .await
        .unwrap();

    script_live_observation(&transport);
    let result = session
        .execute(
            BrowserOperationRequest::SelectPage(SelectPageRequest { target_id: target }),
            BrowserOperationContext::default(),
        )
        .await
        .unwrap();
    let BrowserOperationResult::SelectPage(result) = result else {
        panic!("select-page result")
    };
    let interaction_id = result.interaction.interaction_id;
    let resolved = store
        .resolve_range(
            TemporalQueryRequest::strict(TemporalRangeAnchor::Interaction {
                scope: AnchorScope::new(Some(status.session_id), Some(target)),
                interaction_id,
                window: None,
            })
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resolved.interaction_ids, vec![interaction_id]);
    assert_eq!(
        resolved.requested_range.start().as_nanos(),
        result
            .interaction
            .timing
            .started_at
            .as_nanos()
            .saturating_sub(150_000_000)
    );
    assert_eq!(
        resolved.requested_range.end().as_nanos(),
        result
            .interaction
            .timing
            .observed_at
            .unwrap_or(result.interaction.timing.completed_at)
            .as_nanos()
            .checked_add(250_000_000)
            .unwrap()
    );

    script_coordinate_click(&transport);
    script_coordinate_click(&transport);
    script_live_observation(&transport);
    let batch = BatchRequest::new(
        PageSelection::Target(target),
        vec![coordinate_click(target), coordinate_click(target)],
        Duration::from_secs(5),
        BatchOptions::default(),
    )
    .unwrap();
    let BrowserOperationResult::Batch(batch) = session
        .execute(
            BrowserOperationRequest::Batch(batch),
            BrowserOperationContext::default(),
        )
        .await
        .unwrap()
    else {
        panic!("batch result")
    };
    for step in &batch.steps {
        let interaction_id = step.interaction.as_ref().unwrap().interaction_id;
        let resolved = store
            .resolve_range(
                TemporalQueryRequest::strict(TemporalRangeAnchor::Interaction {
                    scope: AnchorScope::new(Some(status.session_id), Some(target)),
                    interaction_id,
                    window: None,
                })
                .unwrap(),
            )
            .await
            .unwrap();
        assert!(resolved.interaction_ids.contains(&interaction_id));
    }
    session.stop().await.unwrap();
}
