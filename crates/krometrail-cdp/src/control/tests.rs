use serde_json::json;

use super::evaluation::decode_evaluation;
use krometrail_core::{ErrorCode, EvaluationValue, TargetId};

const UUID: &str = "123e4567-e89b-12d3-a456-426614174000";
fn target() -> TargetId {
    TargetId::from_uuid(UUID.parse().unwrap())
}

#[test]
fn evaluation_distinguishes_undefined_null_and_refuses_remote_values() {
    assert_eq!(
        decode_evaluation(&json!({"result":{"type":"undefined"}}), target()).unwrap(),
        EvaluationValue::Undefined
    );
    assert_eq!(
        decode_evaluation(&json!({"result":{"type":"object","value":null}}), target()).unwrap(),
        EvaluationValue::Json(json!(null))
    );
    let error = decode_evaluation(
        &json!({"result":{"type":"object","objectId":"private"}}),
        target(),
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::EvaluationFailed);
    assert!(!error.message.as_str().contains("private"));
}

#[test]
fn evaluation_refuses_exceptions_and_oversized_values() {
    let error = decode_evaluation(
        &json!({"exceptionDetails":{"text":"private stack"}}),
        target(),
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::EvaluationFailed);
    assert!(!error.message.as_str().contains("private stack"));
    let oversized = "x".repeat((1 << 20) + 1);
    let error = decode_evaluation(
        &json!({"result":{"type":"string","value":oversized}}),
        target(),
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::EvaluationFailed);
}

mod interactions {
    use std::{
        collections::{HashMap, VecDeque},
        sync::Mutex,
    };

    use krometrail_core::{
        DialogAction, ErrorCode, FillMode, FillRequest, HandleDialogRequest, IdSource, IdValue,
        InteractionLocator, KeyChord, Modifiers, MonotonicClock, MouseButton, ObservedTime,
        PageSelection, PressKeysRequest, SelectOptionRequest, SelectValue, SessionId,
        SessionOrigin, UploadFilesRequest, ValidatedFilePath, ViewportMetrics,
    };

    use serde_json::json;

    use super::super::{
        BoundTarget, PageControl, dialog, form, interaction::ResolvedTarget, keyboard,
        navigation::OperationCancellation, pointer, snapshot::ResolvedNode, viewport,
    };
    use super::target;
    use crate::transport::{
        CdpTransport, CommandScope, TransportClose, TransportError, TransportEvents,
        TransportFuture, TransportSessionId,
    };

    #[derive(Default)]
    struct RecordingTransport {
        calls: Mutex<Vec<(String, serde_json::Value)>>,
        responses: Mutex<
            HashMap<String, VecDeque<std::result::Result<serde_json::Value, TransportError>>>,
        >,
        hold_stateful_pointer_responses: Mutex<bool>,
    }
    impl RecordingTransport {
        fn push(
            &self,
            method: &str,
            value: std::result::Result<serde_json::Value, TransportError>,
        ) {
            self.responses
                .lock()
                .unwrap()
                .entry(method.to_owned())
                .or_default()
                .push_back(value);
        }
        fn calls(&self, method: &str) -> Vec<serde_json::Value> {
            self.calls
                .lock()
                .unwrap()
                .iter()
                .filter_map(|(candidate, value)| (candidate == method).then_some(value.clone()))
                .collect()
        }
        fn hold_stateful_pointer_responses(&self) {
            *self.hold_stateful_pointer_responses.lock().unwrap() = true;
        }
    }
    struct EmptyEvents;
    impl TransportEvents for EmptyEvents {
        fn next(
            &mut self,
        ) -> TransportFuture<
            '_,
            std::result::Result<Option<crate::transport::NamedEvent>, TransportError>,
        > {
            Box::pin(std::future::ready(Ok(None)))
        }
    }

    impl CdpTransport for RecordingTransport {
        fn send_raw(
            &self,
            _scope: &CommandScope,
            method: &str,
            params: serde_json::Value,
        ) -> TransportFuture<'_, std::result::Result<serde_json::Value, TransportError>> {
            let hold = method == "Input.dispatchMouseEvent"
                && params["type"] != "mouseMoved"
                && *self.hold_stateful_pointer_responses.lock().unwrap();
            self.calls.lock().unwrap().push((method.to_owned(), params));
            let result = self
                .responses
                .lock()
                .unwrap()
                .get_mut(method)
                .and_then(VecDeque::pop_front)
                .unwrap_or_else(|| Ok(json!({})));
            if hold {
                Box::pin(std::future::pending())
            } else {
                Box::pin(std::future::ready(result))
            }
        }
        fn subscribe_named(
            &self,
            _scope: &CommandScope,
            _method: &str,
        ) -> TransportFuture<'_, std::result::Result<Box<dyn TransportEvents>, TransportError>>
        {
            Box::pin(std::future::ready(Ok(
                Box::new(EmptyEvents) as Box<dyn TransportEvents>
            )))
        }
        fn close_reason(&self) -> Option<TransportClose> {
            None
        }
        fn is_closed(&self) -> bool {
            false
        }
    }
    fn bound() -> BoundTarget {
        BoundTarget {
            target_id: target(),
            browser_target_key: "target-a".into(),
            attachment_generation: 1,
            transport_session: TransportSessionId::new("session").unwrap(),
            visibility: krometrail_core::TargetVisibility::Visible,
        }
    }

    struct TestClock;
    impl MonotonicClock for TestClock {
        fn now(&self) -> ObservedTime {
            ObservedTime::from_nanos(0)
        }
    }
    struct TestIds;
    impl IdSource for TestIds {
        fn next(&self) -> IdValue {
            IdValue::from_uuid(uuid::Uuid::from_u128(9))
        }
    }
    fn page_control() -> PageControl {
        PageControl::new(
            std::sync::Arc::new(TestClock),
            std::sync::Arc::new(TestIds),
            SessionId::from_uuid(uuid::Uuid::from_u128(8)),
            SessionOrigin::new(ObservedTime::from_nanos(0)),
        )
    }

    #[tokio::test]
    async fn visible_pointer_target_has_no_activation_overhead() {
        let transport = RecordingTransport::default();
        page_control()
            .prepare_pointer_target(&transport, &bound(), &OperationCancellation::default(), 0)
            .await
            .unwrap();
        assert!(transport.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn hidden_pointer_target_activates_and_fails_specifically_if_still_hidden() {
        let transport = RecordingTransport::default();
        transport.push(
            "Runtime.evaluate",
            Ok(json!({"result":{"result":{"value":"hidden"}}})),
        );
        let mut hidden = bound();
        hidden.visibility = krometrail_core::TargetVisibility::Hidden;
        let error = page_control()
            .prepare_pointer_target(&transport, &hidden, &OperationCancellation::default(), 0)
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::TargetHidden);
        assert_eq!(transport.calls("Target.activateTarget").len(), 1);
        assert_eq!(transport.calls("Page.bringToFront").len(), 1);
        assert_eq!(transport.calls("Runtime.evaluate").len(), 1);
        assert!(transport.calls("Input.dispatchMouseEvent").is_empty());
    }
    fn element() -> ResolvedTarget {
        ResolvedTarget::Element {
            node: ResolvedNode {
                backend_node_id: 42,
                document_quad: [10.0, 20.0, 30.0, 20.0, 30.0, 40.0, 10.0, 40.0],
            },
            viewport_point: krometrail_core::CssPoint::new(20.0, 30.0).unwrap(),
        }
    }

    #[tokio::test]
    async fn pointer_click_emits_exact_move_press_release_contract() {
        let transport = RecordingTransport::default();
        let bound = bound();
        let cancel = OperationCancellation::default();
        let request = krometrail_core::ClickRequest::new(
            PageSelection::Target(target()),
            InteractionLocator::coordinate(
                krometrail_core::CssPoint::new(20.0, 30.0).unwrap(),
                krometrail_core::CoordinateSpace::ViewportCss,
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
        pointer::click(
            &transport,
            &bound,
            &request,
            &ResolvedTarget::Coordinate {
                viewport_point: krometrail_core::CssPoint::new(20.0, 30.0).unwrap(),
            },
            &cancel,
            0,
        )
        .await
        .unwrap();
        let calls = transport.calls("Input.dispatchMouseEvent");
        assert_eq!(calls.len(), 3);
        assert_eq!(calls[0]["type"], json!("mouseMoved"));
        assert_eq!(calls[1]["type"], json!("mousePressed"));
        assert_eq!(calls[2]["type"], json!("mouseReleased"));
        assert_eq!(calls[1]["modifiers"], json!(2));
        assert_eq!(calls[1]["clickCount"], json!(2));
        assert_eq!(calls[1]["buttons"], json!(1));
        assert_eq!(calls[2]["buttons"], json!(0));
    }

    #[tokio::test]
    async fn abandoning_pointer_dispatch_cannot_split_press_from_release() {
        let transport = RecordingTransport::default();
        transport.hold_stateful_pointer_responses();
        let bound = bound();
        let cancel = OperationCancellation::default();
        let request = krometrail_core::ClickRequest::new(
            PageSelection::Target(target()),
            InteractionLocator::coordinate(
                krometrail_core::CssPoint::new(20.0, 30.0).unwrap(),
                krometrail_core::CoordinateSpace::ViewportCss,
            )
            .unwrap(),
            MouseButton::Left,
            Modifiers::default(),
            1,
            false,
        )
        .unwrap();

        let target = ResolvedTarget::Coordinate {
            viewport_point: krometrail_core::CssPoint::new(20.0, 30.0).unwrap(),
        };
        {
            let dispatch = pointer::click(&transport, &bound, &request, &target, &cancel, 0);
            tokio::pin!(dispatch);
            assert!(
                futures_util::poll!(dispatch.as_mut()).is_pending(),
                "the simulated modal keeps pointer command acknowledgements pending"
            );
        }

        let types = transport
            .calls("Input.dispatchMouseEvent")
            .into_iter()
            .map(|call| call["type"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(types, ["mouseMoved", "mousePressed", "mouseReleased"]);
    }

    #[tokio::test]
    async fn fill_key_chords_and_select_share_verified_backend_target() {
        let transport = RecordingTransport::default();
        let bound = bound();
        let cancel = OperationCancellation::default();
        let locator = InteractionLocator::Element(krometrail_core::ElementLocator::CssSelector(
            krometrail_core::NonEmptyText::new("#input").unwrap(),
        ));
        let fill = FillRequest::new(
            PageSelection::Target(target()),
            locator.clone(),
            "replacement",
            FillMode::Replace,
            false,
        )
        .unwrap();
        transport.push(
            "DOM.resolveNode",
            Ok(json!({"object":{"objectId":"editable"}})),
        );
        transport.push(
            "Runtime.callFunctionOn",
            Ok(json!({"result":{"value":true}})),
        );
        transport.push("Runtime.callFunctionOn", Ok(json!({"result":{"value":0}})));
        keyboard::fill(&transport, &bound, &fill, &element(), &cancel, 0)
            .await
            .unwrap();
        assert_eq!(transport.calls("DOM.focus")[0]["backendNodeId"], json!(42));
        assert_eq!(
            transport.calls("Input.insertText")[0]["text"],
            json!("replacement")
        );
        let replace_events = transport.calls("Input.dispatchKeyEvent");
        assert_eq!(replace_events.len(), 2);
        assert_eq!(replace_events[0]["key"], json!("Backspace"));
        let press = PressKeysRequest::new(
            PageSelection::Target(target()),
            None,
            vec![
                KeyChord::new("Control+S").unwrap(),
                KeyChord::new("Enter").unwrap(),
            ],
            false,
        )
        .unwrap();
        keyboard::press_keys(
            &transport,
            &bound,
            &press,
            &ResolvedTarget::TargetWide,
            &cancel,
            0,
        )
        .await
        .unwrap();
        assert!(
            transport
                .calls("Input.dispatchKeyEvent")
                .iter()
                .any(|call| call["key"] == json!("Enter"))
        );
        let key_events = transport.calls("Input.dispatchKeyEvent");
        let shortcut_s = key_events
            .iter()
            .find(|call| call["key"] == json!("s"))
            .expect("shortcut action key");
        assert_eq!(shortcut_s["type"], json!("rawKeyDown"));
        assert!(shortcut_s.get("text").is_none());
        let enter = key_events
            .iter()
            .find(|call| call["key"] == json!("Enter") && call["type"] == json!("keyDown"))
            .expect("text-bearing Enter key down");
        assert_eq!(enter["text"], json!("\r"));
        transport.push(
            "DOM.resolveNode",
            Ok(json!({"object":{"objectId":"private"}})),
        );
        transport.push(
            "Runtime.callFunctionOn",
            Ok(json!({"result":{"value":true}})),
        );
        let select = SelectOptionRequest::new(
            PageSelection::Target(target()),
            locator,
            SelectValue::Label(krometrail_core::NonEmptyText::new("Two").unwrap()),
        )
        .unwrap();
        form::select_option(&transport, &bound, &select, &element(), &cancel, 0)
            .await
            .unwrap();
        let calls = transport.calls("Runtime.callFunctionOn");
        let call = calls.last().expect("select option call");
        assert_eq!(call["arguments"][0]["value"], json!("label"));
    }

    #[tokio::test]
    async fn upload_and_dialog_failures_are_source_safe() {
        let transport = RecordingTransport::default();
        let bound = bound();
        let cancel = OperationCancellation::default();
        let path = std::env::temp_dir().join(format!("krometrail-upload-{}", std::process::id()));
        std::fs::write(&path, b"payload").unwrap();
        let request = UploadFilesRequest::new(
            PageSelection::Target(target()),
            InteractionLocator::Element(krometrail_core::ElementLocator::CssSelector(
                krometrail_core::NonEmptyText::new("#file").unwrap(),
            )),
            vec![ValidatedFilePath::new(path.to_string_lossy()).unwrap()],
        )
        .unwrap();
        super::super::upload::upload_files(&transport, &bound, &request, &element(), &cancel, 0)
            .await
            .unwrap();
        let upload = &transport.calls("DOM.setFileInputFiles")[0];
        assert_eq!(upload["backendNodeId"], json!(42));
        assert_eq!(upload["files"].as_array().unwrap().len(), 1);
        let _ = std::fs::remove_file(path);
        transport.push(
            "Page.handleJavaScriptDialog",
            Err(TransportError::CommandFailed),
        );
        let dialog_request = HandleDialogRequest {
            target: PageSelection::Target(target()),
            action: DialogAction::Dismiss,
        };
        let error = dialog::handle_dialog(&transport, &bound, &dialog_request, &cancel, 0)
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::NotFound);
        assert!(!error.message.as_str().contains("private"));
    }

    #[tokio::test]
    async fn viewport_override_and_clear_emit_target_scoped_commands_in_order() {
        let transport = RecordingTransport::default();
        let bound = bound();
        let metrics = ViewportMetrics::new(390, 844, 3.0, true, true).unwrap();
        viewport::apply_viewport(&transport, &bound, Some(metrics))
            .await
            .unwrap();
        assert_eq!(
            transport.calls("Emulation.setDeviceMetricsOverride"),
            vec![
                json!({"width":390,"height":844,"deviceScaleFactor":3.0,"mobile":true,"screenWidth":390,"screenHeight":844})
            ]
        );
        assert_eq!(
            transport.calls("Emulation.setTouchEmulationEnabled"),
            vec![json!({"enabled":true,"maxTouchPoints":1})]
        );

        viewport::apply_viewport(&transport, &bound, None)
            .await
            .unwrap();
        assert_eq!(
            transport.calls("Emulation.setTouchEmulationEnabled").last(),
            Some(&json!({"enabled":false}))
        );
        assert_eq!(
            transport.calls("Emulation.setDeviceMetricsOverride"),
            vec![
                json!({"width":390,"height":844,"deviceScaleFactor":3.0,"mobile":true,"screenWidth":390,"screenHeight":844})
            ]
        );
        assert_eq!(
            transport.calls("Emulation.clearDeviceMetricsOverride"),
            vec![json!({})]
        );
    }

    #[tokio::test]
    async fn effective_viewport_is_independently_observed() {
        let transport = RecordingTransport::default();
        transport.push(
            "Page.getLayoutMetrics",
            Ok(json!({"result":{"cssVisualViewport":{"clientWidth":800,"clientHeight":600}}})),
        );
        transport.push(
            "Runtime.evaluate",
            Ok(json!({"result":{"result":{"value":{"width":800,"height":600,"scale":2.0,"touchPoints":0}}}})),
        );
        let metrics = ViewportMetrics::new(800, 600, 2.0, false, false).unwrap();
        let effective = viewport::observe_effective_viewport(&transport, &bound(), Some(metrics))
            .await
            .unwrap();
        assert_eq!(effective.css_size.width, 800.0);
        assert_eq!(effective.device_scale_factor.get(), 2.0);
        assert!(!effective.mobile && !effective.touch && effective.override_active);
    }
}
