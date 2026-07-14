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
        DialogAction, ErrorCode, FillMode, FillRequest, HandleDialogRequest, InteractionLocator,
        KeyChord, Modifiers, MouseButton, PageSelection, PressKeysRequest, SelectOptionRequest,
        SelectValue, UploadFilesRequest, ValidatedFilePath,
    };

    use serde_json::json;

    use super::super::{
        BoundTarget, dialog, form, interaction::ResolvedTarget, keyboard,
        navigation::OperationCancellation, pointer, snapshot::ResolvedNode,
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
            self.calls.lock().unwrap().push((method.to_owned(), params));
            let result = self
                .responses
                .lock()
                .unwrap()
                .get_mut(method)
                .and_then(VecDeque::pop_front)
                .unwrap_or_else(|| Ok(json!({})));
            Box::pin(std::future::ready(result))
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
            attachment_generation: 1,
            transport_session: TransportSessionId::new("session").unwrap(),
        }
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
        keyboard::fill(&transport, &bound, &fill, &element(), &cancel, 0)
            .await
            .unwrap();
        assert_eq!(transport.calls("DOM.focus")[0]["backendNodeId"], json!(42));
        assert_eq!(
            transport.calls("Input.insertText")[0]["text"],
            json!("replacement")
        );
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
        let call = &transport.calls("Runtime.callFunctionOn")[0];
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
}
