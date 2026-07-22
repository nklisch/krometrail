use serde_json::json;

use super::evaluation::decode_evaluation;
use krometrail_core::{ErrorCode, EvaluationValue, MAX_REDACTED_TEXT_BYTES, TargetId};

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
fn evaluation_separates_side_effect_refusal_from_thrown_exceptions() {
    let error = decode_evaluation(
        &json!({
            "exceptionDetails": {
                "text": "side effect refusal",
                "exception": {
                    "className": "EvalError",
                    "description": "EvalError: page code mentioned side effect",
                    "stackTrace": "private stack"
                }
            }
        }),
        target(),
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::EvaluationFailed);
    assert!(error.message.as_str().contains("page evaluation threw"));
    assert!(!error.message.as_str().contains("refused as side-effecting"));

    let error = decode_evaluation(
        &json!({
            "exceptionDetails": {
                "exception": {
                    "className": "EvalError",
                    "description": "Possible side-effect in debug-evaluate"
                }
            }
        }),
        target(),
    )
    .unwrap_err();
    assert!(error.message.as_str().contains("refused as side-effecting"));

    let error = decode_evaluation(
        &json!({
            "exceptionDetails": {
                "exception": {
                    "className": "EvalError",
                    "description": "EvalError: Possible side-effect in debug-evaluate"
                }
            }
        }),
        target(),
    )
    .unwrap_err();
    assert!(error.message.as_str().contains("refused as side-effecting"));

    let error = decode_evaluation(
        &json!({
            "exceptionDetails": {
                "exception": {
                    "className": "EvalError",
                    "description": "EvalError: page code mentioned side-effect"
                }
            }
        }),
        target(),
    )
    .unwrap_err();
    assert!(error.message.as_str().contains("page evaluation threw"));

    let error = decode_evaluation(
        &json!({
            "result": {
                "exceptionDetails": {
                    "text": "Uncaught",
                    "exception": {
                        "className": "Error",
                        "description": "Error: boom",
                        "stackTrace": "private stack"
                    }
                }
            }
        }),
        target(),
    )
    .unwrap_err();
    assert!(
        error
            .message
            .as_str()
            .contains("page evaluation threw: Error: boom"),
        "{}",
        error.message.as_str()
    );
    assert!(!error.message.as_str().contains("private stack"));
    assert!(
        error
            .recovery
            .as_ref()
            .unwrap()
            .as_str()
            .contains("handle the thrown error")
    );

    let oversized = "x".repeat(MAX_REDACTED_TEXT_BYTES + 1_024);
    let error = decode_evaluation(
        &json!({
            "exceptionDetails": {
                "exception": {"description": oversized}
            }
        }),
        target(),
    )
    .unwrap_err();
    assert!(
        error.message.as_str().len() <= "page evaluation threw: ".len() + MAX_REDACTED_TEXT_BYTES
    );

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
        for focus in [
            krometrail_core::BrowserFocusPolicy::Foreground,
            krometrail_core::BrowserFocusPolicy::Preserve,
        ] {
            let transport = RecordingTransport::default();
            page_control()
                .prepare_pointer_target(
                    &transport,
                    &bound(),
                    focus,
                    &OperationCancellation::default(),
                    0,
                )
                .await
                .unwrap();
            assert!(transport.calls.lock().unwrap().is_empty());
        }
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
        let mut control = page_control();
        control.config.evaluation_timeout = std::time::Duration::from_millis(20);
        let error = control
            .prepare_pointer_target(
                &transport,
                &hidden,
                krometrail_core::BrowserFocusPolicy::Foreground,
                &OperationCancellation::default(),
                0,
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::TargetHidden);
        assert!(
            error
                .message
                .as_str()
                .contains("bounded foreground activation")
        );
        assert!(
            error
                .recovery
                .as_ref()
                .is_some_and(|recovery| recovery.as_str().contains("Chrome can foreground"))
        );
        assert_eq!(transport.calls("Target.activateTarget").len(), 1);
        assert_eq!(transport.calls("Page.bringToFront").len(), 1);
        assert!(!transport.calls("Runtime.evaluate").is_empty());
        assert!(transport.calls("Input.dispatchMouseEvent").is_empty());
    }

    #[tokio::test]
    async fn hidden_pointer_target_allows_activation_visibility_to_settle() {
        let transport = RecordingTransport::default();
        transport.push(
            "Runtime.evaluate",
            Ok(json!({"result":{"result":{"value":"hidden"}}})),
        );
        transport.push(
            "Runtime.evaluate",
            Ok(json!({"result":{"result":{"value":"visible"}}})),
        );
        let mut hidden = bound();
        hidden.visibility = krometrail_core::TargetVisibility::Hidden;
        let mut control = page_control();
        control.config.evaluation_timeout = std::time::Duration::from_millis(100);
        let observed = control
            .prepare_pointer_target(
                &transport,
                &hidden,
                krometrail_core::BrowserFocusPolicy::Foreground,
                &OperationCancellation::default(),
                0,
            )
            .await
            .unwrap();
        assert_eq!(observed, Some(krometrail_core::TargetVisibility::Visible));
        assert_eq!(transport.calls("Target.activateTarget").len(), 1);
        assert_eq!(transport.calls("Page.bringToFront").len(), 1);
        assert_eq!(transport.calls("Runtime.evaluate").len(), 2);
    }

    #[tokio::test]
    async fn explicit_activation_always_foregrounds_and_waits_for_visible_state() {
        let transport = RecordingTransport::default();
        transport.push(
            "Runtime.evaluate",
            Ok(json!({"result":{"result":{"value":"visible"}}})),
        );
        let observed = page_control()
            .activate_target(&transport, &bound(), &OperationCancellation::default(), 0)
            .await
            .unwrap();
        assert_eq!(observed, krometrail_core::TargetVisibility::Visible);
        assert_eq!(transport.calls("Target.activateTarget").len(), 1);
        assert_eq!(transport.calls("Page.bringToFront").len(), 1);
        assert_eq!(transport.calls("Runtime.evaluate").len(), 1);
        assert!(transport.calls("Input.dispatchMouseEvent").is_empty());
    }

    #[tokio::test]
    async fn hidden_pointer_target_in_preserve_mode_fails_without_foreground_or_input() {
        let transport = RecordingTransport::default();
        let mut hidden = bound();
        hidden.visibility = krometrail_core::TargetVisibility::Hidden;
        let error = page_control()
            .prepare_pointer_target(
                &transport,
                &hidden,
                krometrail_core::BrowserFocusPolicy::Preserve,
                &OperationCancellation::default(),
                0,
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::TargetHidden);
        assert!(error.message.as_str().contains("preserve focus policy"));
        assert!(
            error
                .recovery
                .as_ref()
                .is_some_and(|recovery| recovery.as_str().contains("activate_page"))
        );
        assert!(
            error
                .recovery
                .as_ref()
                .is_none_or(|recovery| !recovery.as_str().contains("select or foreground"))
        );
        assert!(transport.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn explicit_activation_preserves_cancellation_disconnect_and_command_errors() {
        let cancelled = OperationCancellation::default();
        cancelled.stop();
        assert_eq!(
            page_control()
                .activate_target(&RecordingTransport::default(), &bound(), &cancelled, 0)
                .await
                .unwrap_err()
                .code,
            ErrorCode::Cancelled
        );

        let disconnected = OperationCancellation::default();
        disconnected.disconnect(0);
        assert_eq!(
            page_control()
                .activate_target(&RecordingTransport::default(), &bound(), &disconnected, 0,)
                .await
                .unwrap_err()
                .code,
            ErrorCode::BrowserDisconnected
        );

        let rejected = RecordingTransport::default();
        rejected.push("Target.activateTarget", Err(TransportError::CommandFailed));
        assert_eq!(
            page_control()
                .activate_target(&rejected, &bound(), &OperationCancellation::default(), 0)
                .await
                .unwrap_err()
                .code,
            ErrorCode::InteractionFailed
        );

        let closed = RecordingTransport::default();
        closed.push("Target.activateTarget", Err(TransportError::Closed));
        assert_eq!(
            page_control()
                .activate_target(&closed, &bound(), &OperationCancellation::default(), 0)
                .await
                .unwrap_err()
                .code,
            ErrorCode::BrowserDisconnected
        );
    }
    fn element() -> ResolvedTarget {
        ResolvedTarget::Element {
            node: ResolvedNode {
                backend_node_id: 42,
                document_quad: [10.0, 20.0, 30.0, 20.0, 30.0, 40.0, 10.0, 40.0],
                facts: krometrail_core::NodeStateFacts::default(),
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

    /// Replace-mode clearing is a selection followed by a separate Backspace, so it is not atomic:
    /// a dialog (or any focus steal) opening between the two leaves the selection made and the
    /// deletion swallowed. That asymmetry is deliberate — pointer dispatch cannot be made atomic
    /// either — and is made safe by re-reading the field afterwards instead of trusting the
    /// dispatch. This pins the consequence: a clear that did not take effect is an explicit
    /// actionable failure, and the new value is never appended onto surviving contents.
    #[tokio::test]
    async fn fill_replace_fails_actionably_when_clearing_is_swallowed() {
        let transport = RecordingTransport::default();
        let bound = bound();
        let cancel = OperationCancellation::default();
        let fill = FillRequest::new(
            PageSelection::Target(target()),
            InteractionLocator::Element(krometrail_core::ElementLocator::CssSelector(
                krometrail_core::NonEmptyText::new("#input").unwrap(),
            )),
            "replacement",
            FillMode::Replace,
            false,
        )
        .unwrap();
        transport.push(
            "DOM.resolveNode",
            Ok(json!({"object":{"objectId":"editable"}})),
        );
        // The selection lands...
        transport.push(
            "Runtime.callFunctionOn",
            Ok(json!({"result":{"value":true}})),
        );
        // ...but the field still holds its contents when re-read, exactly as it would if a dialog
        // had consumed the Backspace.
        transport.push("Runtime.callFunctionOn", Ok(json!({"result":{"value":7}})));

        let error = keyboard::fill(&transport, &bound, &fill, &element(), &cancel, 0)
            .await
            .expect_err("a swallowed clear must not be reported as a successful replace");

        assert!(
            error.message.as_str().contains("could not be cleared"),
            "the failure must name the unclearable field, got: {}",
            error.message.as_str()
        );
        assert!(
            transport.calls("Input.insertText").is_empty(),
            "replace must not append onto contents it failed to clear"
        );
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

    /// Chrome answers `Page.handleJavaScriptDialog` with a protocol error, not a transport
    /// command failure, when no dialog is showing. That is the same "not open" boundary and must
    /// not surface as a generic interaction rejection without a structured code.
    #[tokio::test]
    async fn dialog_rejection_reports_the_not_open_boundary_for_protocol_errors() {
        let transport = RecordingTransport::default();
        let bound = bound();
        let cancel = OperationCancellation::default();
        transport.push("Page.handleJavaScriptDialog", Err(TransportError::Protocol));
        let request = HandleDialogRequest {
            target: PageSelection::Target(target()),
            action: DialogAction::Dismiss,
        };
        let error = dialog::handle_dialog(&transport, &bound, &request, &cancel, 0)
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::NotFound);
        assert!(error.message.as_str().contains("dialog_not_open"));
    }

    /// Reported open-dialog state must never gate the dialog action.
    ///
    /// That state is maintained by the event pump, so it lags Chrome by however long the opening
    /// event takes to process — and that lag coincides exactly with the case that matters, since
    /// a dialog that just opened is one blocking the renderer right now. Refusing to dispatch on
    /// a stale `None` would deny the recovery action precisely when it is needed. Chrome is the
    /// authority, and it answers.
    #[tokio::test]
    async fn stale_absent_dialog_state_still_dispatches_the_dialog_action() {
        let transport = RecordingTransport::default();
        let bound = bound();
        let cancel = OperationCancellation::default();
        transport.push("Page.handleJavaScriptDialog", Ok(serde_json::Value::Null));
        let request = HandleDialogRequest {
            target: PageSelection::Target(target()),
            action: DialogAction::Dismiss,
        };
        dialog::handle_dialog(&transport, &bound, &request, &cancel, 0)
            .await
            .expect("a dialog open in Chrome must be handled even when reported state lags");
        assert_eq!(transport.calls("Page.handleJavaScriptDialog").len(), 1);
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
        assert_eq!(
            transport.calls("Emulation.setPageScaleFactor"),
            vec![json!({"pageScaleFactor":1})]
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
        assert_eq!(
            transport.calls("Emulation.resetPageScaleFactor"),
            vec![json!({})]
        );
    }

    #[tokio::test]
    async fn effective_viewport_is_independently_observed() {
        let transport = RecordingTransport::default();
        transport.push(
            "Page.getLayoutMetrics",
            Ok(json!({"result":{
                "cssVisualViewport":{"clientWidth":800,"clientHeight":600},
                "cssLayoutViewport":{"clientWidth":800,"clientHeight":600}
            }})),
        );
        transport.push(
            "Runtime.evaluate",
            Ok(json!({"result":{"result":{"value":{
                "layoutWidth":800,"layoutHeight":600,"scale":2.0,
                "touchPoints":0,"viewportMetaPresent":true
            }}}})),
        );
        let metrics = ViewportMetrics::new(800, 600, 2.0, false, false).unwrap();
        let effective = viewport::observe_effective_viewport(&transport, &bound(), Some(metrics))
            .await
            .unwrap();
        assert_eq!(effective.css_size.width, 800.0);
        assert_eq!(effective.layout_css_size.width, 800.0);
        assert!(effective.viewport_meta_present);
        assert_eq!(effective.device_scale_factor.get(), 2.0);
        assert!(!effective.mobile && !effective.touch && effective.override_active);
    }
}
