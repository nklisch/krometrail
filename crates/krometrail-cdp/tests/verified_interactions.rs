#![cfg(feature = "cdpkit-transport")]

mod support;

use std::sync::Arc;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use krometrail_cdp::{
    CdpTransport, CdpTransportFactory, ProductionBrowserConnector, TransportError, TransportFuture,
};
use krometrail_core::{
    BrowserActionRequest, BrowserConnectRequest, BrowserConnector, BrowserOperationRequest,
    BrowserOperationResult, ClickRequest, CoordinateSpace, CssPoint, DialogAction, DragRequest,
    ElementLocator, FillMode, FillRequest, HandleDialogRequest, HoverRequest, InteractionLocator,
    InteractionOutcome, KeyChord, Modifiers, MouseButton, PageSelection, PressKeysRequest,
    ReadOnlyEvaluationRequest, ScrollDelta, ScrollRequest, SelectOptionRequest, SelectValue,
    SnapshotPageRequest, UploadFilesRequest, ValidatedFilePath,
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
    let connector = ProductionBrowserConnector::new(
        Arc::new(krometrail_cdp::SystemChromeLauncher::new(
            krometrail_cdp::LauncherConfig::default(),
        )),
        Arc::new(ScriptedFactory(transport)),
    );
    connector
        .connect(BrowserConnectRequest::Attach(
            krometrail_core::AttachBrowser::new("ws://127.0.0.1:9222/devtools/browser/scripted")
                .unwrap(),
        ))
        .await
        .unwrap()
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
        calls[attach_index..attach_index + 4]
            .iter()
            .map(|call| call.method.as_str())
            .collect::<Vec<_>>(),
        vec![
            "Target.attachToTarget",
            "Page.enable",
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
        .execute(BrowserOperationRequest::Click(request))
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
        .execute(BrowserOperationRequest::Click(request))
        .await
        .unwrap_err();
    assert_eq!(error.code, krometrail_core::ErrorCode::InteractionFailed);
    assert!(error.message.as_str().contains("no_hit_target"));
    session.stop().await.unwrap();
}

#[test]
fn sensitive_action_sanitization_never_contains_full_secrets_or_paths() {
    let target = krometrail_core::TargetId::from_uuid(uuid::Uuid::from_u128(1));
    let locator = InteractionLocator::Element(ElementLocator::CssSelector(
        krometrail_core::NonEmptyText::new("#input").unwrap(),
    ));
    let fill = FillRequest::new(
        PageSelection::Target(target),
        locator.clone(),
        "01234567890123456789012345678901-secret",
        FillMode::Replace,
        false,
    )
    .unwrap();
    let fill_json = serde_json::to_string(fill.sanitize().as_json()).unwrap();
    assert!(!fill_json.contains("secret"));
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
        .execute(BrowserOperationRequest::EvaluatePage(
            ReadOnlyEvaluationRequest::new(target, expression, false).unwrap(),
        ))
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
    );
    let session = connector
        .connect(BrowserConnectRequest::Launch(
            krometrail_core::LaunchBrowser {
                executable: None,
                profile: krometrail_core::ManagedProfile::Temporary,
                initial_url: Some(support::chrome::verified_interactions_fixture_url()),
            },
        ))
        .await
        .expect("dialog fixture");
    let target = session.status().await.unwrap().pages[0].target.target.id();
    await_fixture_ready(&session, target).await;
    let page = PageSelection::Target(target);
    let scheduled = session
        .execute(BrowserOperationRequest::Click(
            ClickRequest::new(
                page,
                selector("#confirm-target"),
                MouseButton::Left,
                Modifiers::default(),
                1,
                false,
            )
            .unwrap(),
        ))
        .await
        .expect("schedule dialog");
    let BrowserOperationResult::Click(_scheduled) = scheduled else {
        panic!("click result")
    };
    session
        .execute(BrowserOperationRequest::HandleDialog(HandleDialogRequest {
            target: page,
            action: DialogAction::Accept { prompt_text: None },
        }))
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
    );
    let session = connector
        .connect(BrowserConnectRequest::Launch(
            krometrail_core::LaunchBrowser {
                executable: None,
                profile: krometrail_core::ManagedProfile::Temporary,
                initial_url: Some(support::chrome::verified_interactions_fixture_url()),
            },
        ))
        .await
        .expect("real interaction fixture");
    let target = session.status().await.unwrap().pages[0].target.target.id();
    await_fixture_ready(&session, target).await;
    let page = PageSelection::Target(target);

    session
        .execute(BrowserOperationRequest::Click(
            ClickRequest::new(
                page,
                selector("#click-target"),
                MouseButton::Left,
                Modifiers::default(),
                1,
                false,
            )
            .unwrap(),
        ))
        .await
        .expect("click");
    assert_eq!(
        evaluate(&session, target, "window.fixtureState.clicks").await,
        json!(1)
    );
    session
        .execute(BrowserOperationRequest::Fill(
            FillRequest::new(
                page,
                selector("#text-input"),
                "replaced",
                FillMode::Replace,
                false,
            )
            .unwrap(),
        ))
        .await
        .expect("fill replace");
    session
        .execute(BrowserOperationRequest::Fill(
            FillRequest::new(
                page,
                selector("#text-input"),
                "+appended",
                FillMode::Append,
                false,
            )
            .unwrap(),
        ))
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
        .execute(BrowserOperationRequest::PressKeys(
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
        ))
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
        .execute(BrowserOperationRequest::SelectOption(
            SelectOptionRequest::new(
                page,
                selector("#select"),
                SelectValue::Label(krometrail_core::NonEmptyText::new("Two").unwrap()),
            )
            .unwrap(),
        ))
        .await
        .expect("select label");
    assert_eq!(
        evaluate(&session, target, "document.querySelector('#select').value").await,
        json!("two")
    );
    session
        .execute(BrowserOperationRequest::Hover(HoverRequest {
            target: page,
            locator: selector("#hover-target"),
        }))
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
        .execute(BrowserOperationRequest::Click(
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
        ))
        .await
        .expect("coordinate click");
    assert_eq!(
        evaluate(&session, target, "window.fixtureState.coordinateClicks").await,
        json!(1)
    );
    let empty = session
        .execute(BrowserOperationRequest::Click(
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
        ))
        .await
        .unwrap_err();
    assert!(empty.message.as_str().contains("no_hit_target"));

    let drag = session
        .execute(BrowserOperationRequest::Drag(DragRequest {
            target: page,
            source: selector("#drag-source"),
            destination: selector("#drop-target"),
        }))
        .await;
    assert!(
        drag.is_ok(),
        "drag dispatch must complete explicitly: {drag:?}"
    );
    session
        .execute(BrowserOperationRequest::Scroll(ScrollRequest {
            target: page,
            delta: ScrollDelta::ByOffset { dx: 0.0, dy: 150.0 },
        }))
        .await
        .expect("offset scroll");
    session
        .execute(BrowserOperationRequest::Scroll(ScrollRequest {
            target: page,
            delta: ScrollDelta::ToElement(match selector("#scroll-destination") {
                InteractionLocator::Element(value) => value,
                _ => unreachable!(),
            }),
        }))
        .await
        .expect("element scroll");
    let upload_path =
        std::env::temp_dir().join(format!("krometrail-real-upload-{}", std::process::id()));
    std::fs::write(&upload_path, b"real upload").unwrap();
    session
        .execute(BrowserOperationRequest::UploadFiles(
            UploadFilesRequest::new(
                page,
                selector("#file-input"),
                vec![ValidatedFilePath::new(upload_path.to_string_lossy()).unwrap()],
            )
            .unwrap(),
        ))
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
        .execute(BrowserOperationRequest::SnapshotPage(
            SnapshotPageRequest::new(target),
        ))
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
        .execute(BrowserOperationRequest::SnapshotPage(
            SnapshotPageRequest::new(target),
        ))
        .await
        .unwrap();
    let stale = session
        .execute(BrowserOperationRequest::Click(
            ClickRequest::new(
                page,
                InteractionLocator::Element(ElementLocator::Reference(reference)),
                MouseButton::Left,
                Modifiers::default(),
                1,
                false,
            )
            .unwrap(),
        ))
        .await
        .unwrap_err();
    assert_eq!(stale.code, krometrail_core::ErrorCode::StaleReference);

    // Schedule the modal after the triggering click has returned so the serialized actor can
    // receive the separate dialog operation rather than blocking behind page JavaScript.
    session
        .execute(BrowserOperationRequest::Click(
            ClickRequest::new(
                page,
                selector("#confirm-target"),
                MouseButton::Left,
                Modifiers::default(),
                1,
                false,
            )
            .unwrap(),
        ))
        .await
        .expect("schedule confirm dialog");
    if let Err(error) = session
        .execute(BrowserOperationRequest::HandleDialog(HandleDialogRequest {
            target: page,
            action: DialogAction::Accept { prompt_text: None },
        }))
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
        .execute(BrowserOperationRequest::Click(
            ClickRequest::new(
                page,
                selector("#confirm-target"),
                MouseButton::Left,
                Modifiers::default(),
                1,
                false,
            )
            .unwrap(),
        ))
        .await
        .expect("schedule second confirm dialog");
    session
        .execute(BrowserOperationRequest::HandleDialog(HandleDialogRequest {
            target: page,
            action: DialogAction::Dismiss,
        }))
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

    session.stop().await.unwrap();
}
