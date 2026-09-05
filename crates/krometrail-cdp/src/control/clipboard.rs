use krometrail_core::{
    ClipboardRead, ErrorCode, ErrorContext, KrometrailError, MAX_CLIPBOARD_TEXT_BYTES,
    NonEmptyText, ReadClipboardRequest, Result, RetryAdvice, TargetVisibility,
    WriteClipboardRequest,
};
use serde_json::json;

use super::{BoundTarget, PageControl, operation_error, transport_error};
use crate::transport::{CdpTransport, CommandScope, TransportError};

const READ_CLIPBOARD: &str = "async function(){if(!globalThis.isSecureContext)throw new Error('secure_context_required');if(document.visibilityState!=='visible'||!document.hasFocus())throw new Error('focus_required');if(!navigator.clipboard)throw new Error('clipboard_unavailable');return await navigator.clipboard.readText();}";
const WRITE_CLIPBOARD: &str = "async function(value){if(!globalThis.isSecureContext)throw new Error('secure_context_required');if(document.visibilityState!=='visible'||!document.hasFocus())throw new Error('focus_required');if(!navigator.clipboard)throw new Error('clipboard_unavailable');await navigator.clipboard.writeText(value);return true;}";
const CLIPBOARD_WORLD: &str = "__krometrail_clipboard_v1";

impl PageControl {
    pub(crate) async fn read_clipboard(
        &self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        _request: ReadClipboardRequest,
    ) -> Result<ClipboardRead> {
        require_visible(bound)?;
        let execution_object_id = clipboard_execution_object(transport, bound).await?;
        let response = transport.send_raw(
            &CommandScope::Session(bound.transport_session.clone()),
            "Runtime.callFunctionOn",
            json!({"functionDeclaration": READ_CLIPBOARD, "objectId": execution_object_id, "awaitPromise": true, "returnByValue": true}),
        ).await.map_err(|error| clipboard_dispatch_error(error, bound))?;
        let value = clipboard_bridge_value(bound, &response)?;
        let text = value
            .as_str()
            .ok_or_else(|| clipboard_uninterpretable_error(bound))?;
        if text.len() > MAX_CLIPBOARD_TEXT_BYTES {
            return Err(clipboard_failure(
                ErrorCode::InteractionFailed,
                bound,
                "clipboard text exceeds the 65536-byte limit",
                "copy a smaller text value and retry",
            ));
        }
        Ok(ClipboardRead {
            target_id: bound.target_id,
            text: text.to_owned(),
            utf8_bytes: text.len() as u64,
        })
    }

    pub(crate) async fn write_clipboard(
        &self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        request: &WriteClipboardRequest,
    ) -> Result<()> {
        require_visible(bound)?;
        if request.text.len() > MAX_CLIPBOARD_TEXT_BYTES {
            return Err(clipboard_failure(
                ErrorCode::InteractionFailed,
                bound,
                "clipboard text exceeds the 65536-byte limit",
                "write a smaller text value and retry",
            ));
        }
        let execution_object_id = clipboard_execution_object(transport, bound).await?;
        let response = transport.send_raw(
            &CommandScope::Session(bound.transport_session.clone()),
            "Runtime.callFunctionOn",
            json!({"functionDeclaration": WRITE_CLIPBOARD, "objectId": execution_object_id, "arguments": [{"value": request.text}], "awaitPromise": true, "returnByValue": true}),
        ).await.map_err(|error| clipboard_dispatch_error(error, bound))?;
        let value = clipboard_bridge_value(bound, &response)?;
        if value.as_bool() != Some(true) {
            return Err(clipboard_uninterpretable_error(bound));
        }
        Ok(())
    }
}

async fn clipboard_execution_object(
    transport: &dyn CdpTransport,
    bound: &BoundTarget,
) -> Result<String> {
    let frame_tree = transport
        .send_raw(
            &CommandScope::Session(bound.transport_session.clone()),
            "Page.getFrameTree",
            json!({}),
        )
        .await
        .map_err(|error| {
            clipboard_transport_error(
                error,
                bound,
                "clipboard document became stale while resolving its frame",
            )
        })?;
    let root = frame_tree.get("frameTree").ok_or_else(|| {
        operation_error(
            ErrorCode::StaleReference,
            bound.target_id,
            "clipboard document changed before its isolated world could be resolved",
        )
    })?;
    let frame_id = root
        .pointer("/frame/id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            operation_error(
                ErrorCode::StaleReference,
                bound.target_id,
                "clipboard document changed before its isolated world could be resolved",
            )
        })?;
    let world = transport
        .send_raw(
            &CommandScope::Session(bound.transport_session.clone()),
            "Page.createIsolatedWorld",
            json!({
                "frameId": frame_id,
                "worldName": CLIPBOARD_WORLD,
                "grantUniveralAccess": false,
            }),
        )
        .await
        .map_err(|error| {
            clipboard_transport_error(
                error,
                bound,
                "clipboard document became stale while creating its isolated world",
            )
        })?;
    let execution_context_id = world
        .get("executionContextId")
        .and_then(serde_json::Value::as_i64)
        .ok_or_else(|| {
            operation_error(
                ErrorCode::StaleReference,
                bound.target_id,
                "clipboard document changed before its isolated world became available",
            )
        })?;
    let global = transport
        .send_raw(
            &CommandScope::Session(bound.transport_session.clone()),
            "Runtime.evaluate",
            json!({
                "expression": "globalThis",
                "contextId": execution_context_id,
                "returnByValue": false,
                "silent": true,
            }),
        )
        .await
        .map_err(|error| {
            clipboard_transport_error(
                error,
                bound,
                "clipboard isolated world became stale before its global object resolved",
            )
        })?;
    global
        .pointer("/result/objectId")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| {
            operation_error(
                ErrorCode::StaleReference,
                bound.target_id,
                "clipboard isolated world became stale before its global object resolved",
            )
        })
}

fn clipboard_transport_error(
    error: crate::transport::TransportError,
    bound: &BoundTarget,
    stale_message: &'static str,
) -> KrometrailError {
    let error = transport_error(error, ErrorCode::StaleReference, bound.target_id);
    if error.code == ErrorCode::BrowserDisconnected {
        error
    } else {
        operation_error(ErrorCode::StaleReference, bound.target_id, stale_message)
    }
}

/// Classifies a clipboard bridge dispatch death by what is actually knowable
/// from the transport outcome. A command timeout means the in-page clipboard
/// promise never settled — consistent with a pending/suppressed permission
/// decision or an OS-unfocused window — and is named as such rather than
/// blamed on the transport (the #8 root cause). A browser command rejection
/// means the document or isolated world died mid-flight.
fn clipboard_dispatch_error(error: TransportError, bound: &BoundTarget) -> KrometrailError {
    match error {
        TransportError::Timeout => clipboard_failure(
            ErrorCode::InteractionFailed,
            bound,
            "clipboard operation did not settle before the command deadline — the browser may be \
             holding an unresolved clipboard permission decision or the window is not focused at \
             the OS level (class: command_timeout)",
            "focus the managed browser window at the OS level, resolve any pending clipboard \
             permission prompt, and retry",
        ),
        TransportError::Protocol => clipboard_failure(
            ErrorCode::StaleReference,
            bound,
            "clipboard document or isolated world was destroyed while the operation was in flight",
            "re-inspect the page and retry the explicit clipboard operation",
        ),
        error => {
            let transport_class = transport_error_class(&error);
            let mapped = transport_error(error, ErrorCode::InteractionFailed, bound.target_id);
            if mapped.code == ErrorCode::BrowserDisconnected {
                mapped
            } else {
                clipboard_failure(
                    ErrorCode::InteractionFailed,
                    bound,
                    format!(
                        "clipboard script dispatch failed before the page could respond (transport error: {transport_class})"
                    ),
                    "focus the visible page, allow clipboard access if prompted, and retry",
                )
            }
        }
    }
}

fn transport_error_class(error: &TransportError) -> &'static str {
    match error {
        TransportError::InvalidInput => "invalid_input",
        TransportError::ConnectFailed => "connect_failed",
        TransportError::CommandFailed => "command_failed",
        TransportError::Protocol => "protocol",
        TransportError::Timeout => "command_timeout",
        TransportError::Disconnected => "disconnected",
        TransportError::SubscriptionClosed => "subscription_closed",
        TransportError::Closed => "closed",
    }
}

/// Decodes one clipboard bridge outcome from the production transport
/// shape. `CdpTransport::send_raw` forwards the unwrapped CDP command
/// result, so a `Runtime.callFunctionOn` outcome is `{"result": <remote
/// object>, "exceptionDetails": <optional>}` with the exception beside the
/// remote object. The exception is inspected first so a rejected bridge
/// call can never be accepted as a successful value.
fn clipboard_bridge_value<'a>(
    bound: &BoundTarget,
    response: &'a serde_json::Value,
) -> Result<&'a serde_json::Value> {
    if let Some(details) = response.get("exceptionDetails") {
        return Err(clipboard_exception_error(bound, details));
    }
    response
        .pointer("/result/value")
        .ok_or_else(|| clipboard_uninterpretable_error(bound))
}

/// Classifies a bridge exception from the first line of its description.
/// Chrome formats `exception.description` as `ClassName: message` followed
/// by stack frames, so content further down — a `NotAllowedError` mention,
/// a bridge sentinel, even a denial message — cannot manufacture a known
/// cause. Permission denial is claimed only from Chrome's source-grounded
/// "Read permission denied." / "Write permission denied." rejections;
/// other `NotAllowedError` shapes (permission service unavailable,
/// document detached, permissions-policy blocking, system denial) carry
/// no confident cause and stay neutral. Raw descriptions, class names,
/// and clipboard text never reach the agent-facing error.
fn clipboard_exception_error(bound: &BoundTarget, details: &serde_json::Value) -> KrometrailError {
    let first_line = details
        .pointer("/exception/description")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .lines()
        .next()
        .unwrap_or_default();
    let (code, message, recovery) = if first_line.contains("secure_context_required") {
        (
            ErrorCode::Unsupported,
            "clipboard access requires a secure page context",
            "navigate the managed page to HTTPS or another secure context and retry",
        )
    } else if first_line.contains("clipboard_unavailable") {
        (
            ErrorCode::Unsupported,
            "the page clipboard API is unavailable",
            "use a supported secure Chromium page and retry",
        )
    } else if first_line.contains("focus_required") || first_line.contains("not focused") {
        // The document can lose focus between the bridge pre-check and the
        // clipboard call; Chrome then rejects with
        // `NotAllowedError: Document is not focused.` — a focus failure.
        (
            ErrorCode::InteractionFailed,
            "clipboard access requires a visible focused page",
            "focus the managed browser page and retry; Krometrail will not steal focus",
        )
    } else if first_line.contains("Read permission denied.")
        || first_line.contains("Write permission denied.")
    {
        (
            ErrorCode::InteractionFailed,
            "browser clipboard permission denied the explicit request",
            "focus the managed page, allow clipboard access in Chrome, and retry",
        )
    } else {
        (
            ErrorCode::InteractionFailed,
            "clipboard operation failed for an unidentified reason",
            "retry the operation; if it persists, re-select the page and try again",
        )
    };
    clipboard_failure(code, bound, message, recovery)
}

/// A response with neither an exception nor a decodable value — or a value
/// of the wrong shape — is not evidence of any specific browser state, so
/// the error stays neutral and privacy-bounded.
fn clipboard_uninterpretable_error(bound: &BoundTarget) -> KrometrailError {
    clipboard_failure(
        ErrorCode::InteractionFailed,
        bound,
        "clipboard bridge returned a response Krometrail could not interpret",
        "retry the operation; if it persists, re-select the page and try again",
    )
}

fn require_visible(bound: &BoundTarget) -> Result<()> {
    if bound.visibility != TargetVisibility::Visible {
        return Err(clipboard_failure(
            ErrorCode::InteractionFailed,
            bound,
            "clipboard access requires a visible focused page",
            "focus the managed browser page without asking Krometrail to activate it, then retry",
        ));
    }
    Ok(())
}

fn clipboard_failure(
    code: ErrorCode,
    bound: &BoundTarget,
    message: impl Into<String>,
    recovery: &'static str,
) -> KrometrailError {
    KrometrailError::new(code, NonEmptyText::new(message).unwrap())
        .with_context(ErrorContext {
            target_id: Some(bound.target_id),
            ..ErrorContext::default()
        })
        .with_retry(RetryAdvice::AfterRecovery)
        .with_recovery(NonEmptyText::new(recovery).unwrap())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::transport::{
        NamedEvent, TransportClose, TransportError, TransportEvents, TransportFuture,
        TransportSessionId,
    };

    struct EmptyEvents;
    impl TransportEvents for EmptyEvents {
        fn next(
            &mut self,
        ) -> TransportFuture<'_, std::result::Result<Option<NamedEvent>, TransportError>> {
            Box::pin(std::future::ready(Ok(None)))
        }
    }

    struct ScriptedTransport {
        calls: Mutex<Vec<(String, serde_json::Value)>>,
        responses: Mutex<Vec<serde_json::Value>>,
    }
    impl CdpTransport for ScriptedTransport {
        fn send_raw(
            &self,
            _scope: &CommandScope,
            method: &str,
            params: serde_json::Value,
        ) -> TransportFuture<'_, std::result::Result<serde_json::Value, TransportError>> {
            self.calls.lock().unwrap().push((method.to_owned(), params));
            Box::pin(std::future::ready(Ok(self
                .responses
                .lock()
                .unwrap()
                .remove(0))))
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

    fn bound(visibility: TargetVisibility) -> BoundTarget {
        BoundTarget {
            target_id: krometrail_core::TargetId::from_uuid(uuid::Uuid::from_u128(1)),
            browser_target_key: "opaque-target".into(),
            attachment_generation: 1,
            transport_session: TransportSessionId::new("opaque-session").unwrap(),
            visibility,
        }
    }

    fn control() -> PageControl {
        struct Clock;
        impl krometrail_core::MonotonicClock for Clock {
            fn now(&self) -> krometrail_core::ObservedTime {
                krometrail_core::ObservedTime::from_nanos(0)
            }
        }
        struct Ids;
        impl krometrail_core::IdSource for Ids {
            fn next(&self) -> krometrail_core::IdValue {
                krometrail_core::IdValue::from_uuid(uuid::Uuid::from_u128(2))
            }
        }
        PageControl::new(
            std::sync::Arc::new(Clock),
            std::sync::Arc::new(Ids),
            krometrail_core::SessionId::from_uuid(uuid::Uuid::from_u128(3)),
            krometrail_core::SessionOrigin::new(krometrail_core::ObservedTime::from_nanos(0)),
        )
    }

    #[tokio::test]
    async fn write_uses_a_value_argument_and_never_mutates_permissions_or_focus() {
        let transport = ScriptedTransport {
            calls: Mutex::new(Vec::new()),
            responses: Mutex::new(vec![
                json!({"frameTree":{"frame":{"id":"main-frame"}}}),
                json!({"executionContextId": 41}),
                json!({"result":{"objectId":"isolated-global"}}),
                json!({"result":{"type":"boolean","value":true}}),
            ]),
        };
        let request = WriteClipboardRequest {
            target: krometrail_core::PageSelection::Selected,
            text: "sentinel-value".into(),
        };
        control()
            .write_clipboard(&transport, &bound(TargetVisibility::Visible), &request)
            .await
            .unwrap();
        let calls = transport.calls.lock().unwrap();
        assert_eq!(calls.len(), 4);
        assert_eq!(calls[0].0, "Page.getFrameTree");
        assert_eq!(calls[1].0, "Page.createIsolatedWorld");
        assert_eq!(calls[1].1["frameId"], "main-frame");
        assert_eq!(calls[1].1["worldName"], CLIPBOARD_WORLD);
        assert_eq!(calls[1].1["grantUniveralAccess"], false);
        assert_eq!(calls[2].0, "Runtime.evaluate");
        assert_eq!(calls[2].1["contextId"], 41);
        assert_eq!(calls[2].1["expression"], "globalThis");
        assert_eq!(calls[3].0, "Runtime.callFunctionOn");
        assert_eq!(calls[3].1["objectId"], "isolated-global");
        assert_eq!(calls[3].1["arguments"][0]["value"], "sentinel-value");
        assert!(
            !calls[3].1["functionDeclaration"]
                .as_str()
                .unwrap()
                .contains("sentinel-value")
        );
        assert!(
            !calls.iter().any(|(method, _)| method.contains("Permission")
                || method == "Target.activateTarget"
                || method == "Page.bringToFront")
        );
    }

    #[tokio::test]
    async fn hidden_page_fails_before_dispatch() {
        let transport = ScriptedTransport {
            calls: Mutex::new(Vec::new()),
            responses: Mutex::new(Vec::new()),
        };
        let error = control()
            .read_clipboard(
                &transport,
                &bound(TargetVisibility::Hidden),
                ReadClipboardRequest::default(),
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::InteractionFailed);
        assert!(transport.calls.lock().unwrap().is_empty());
    }

    /// Known exceptions classify from the production `exceptionDetails`
    /// shape (beside the remote object, not nested under a result
    /// envelope). Only the description's first `ClassName: message` line
    /// is evidence; permission denial is claimed only from Chrome's
    /// source-grounded "Read/Write permission denied." messages; unknown
    /// content — including a missing description — stays neutral and never
    /// echoes the raw description.
    #[test]
    fn exception_shapes_classify_known_failures_and_stay_neutral_on_unknown() {
        let bound = bound(TargetVisibility::Visible);
        let details = |class_name: &str, description: &str| {
            json!({
                "exceptionId": 1,
                "text": "Uncaught (in promise)",
                "exception": {"type": "object", "subtype": "error", "className": class_name, "description": description},
            })
        };

        for description in ["secure_context_required", "clipboard_unavailable"] {
            let error = clipboard_exception_error(&bound, &details("Error", description));
            assert_eq!(error.code, ErrorCode::Unsupported);
        }
        for (class_name, description) in [
            ("Error", "focus_required"),
            ("DOMException", "NotAllowedError: Document is not focused."),
        ] {
            let error = clipboard_exception_error(&bound, &details(class_name, description));
            assert_eq!(error.code, ErrorCode::InteractionFailed);
            assert!(error.message.as_str().contains("focused"));
            assert!(!error.message.as_str().contains("denied"));
        }
        let error = clipboard_exception_error(
            &bound,
            &details("DOMException", "NotAllowedError: Read permission denied."),
        );
        assert_eq!(error.code, ErrorCode::InteractionFailed);
        assert!(error.message.as_str().contains("permission denied"));
        assert!(
            error
                .recovery
                .is_some_and(|recovery| recovery.as_str().contains("allow clipboard access"))
        );

        // A bare `NotAllowedError` name is not a permission decision;
        // Chromium uses the same DOMException for service failures,
        // document detach, permissions-policy blocking, and system denial.
        let error = clipboard_exception_error(
            &bound,
            &details("DOMException", "NotAllowedError: Document detached."),
        );
        assert_eq!(error.code, ErrorCode::InteractionFailed);
        assert!(!error.message.as_str().contains("denied"));
        assert!(!error.message.as_str().contains("permission"));

        // Stack content must not manufacture a known cause: the first
        // line names the real rejection.
        let error = clipboard_exception_error(
            &bound,
            &details(
                "TypeError",
                "TypeError: unexpected page state\n    at NotAllowedError: Read permission denied. (<anonymous>)",
            ),
        );
        assert_eq!(error.code, ErrorCode::InteractionFailed);
        assert!(!error.message.as_str().contains("denied"));
        assert!(!error.message.as_str().contains("permission"));

        // An exceptionDetails without `exception.description` (the only
        // description location in the current CDP shape) stays neutral.
        let error = clipboard_exception_error(
            &bound,
            &json!({"exceptionId": 1, "text": "Uncaught (in promise)"}),
        );
        assert_eq!(error.code, ErrorCode::InteractionFailed);
        assert!(!error.message.as_str().contains("denied"));
        assert!(!error.message.as_str().contains("permission"));

        let error = clipboard_exception_error(
            &bound,
            &details("TypeError", "TypeError: raw-exception-content-sentinel"),
        );
        assert_eq!(error.code, ErrorCode::InteractionFailed);
        assert!(!error.message.as_str().contains("denied"));
        assert!(!error.message.as_str().contains("permission"));
        assert!(
            !error
                .message
                .as_str()
                .contains("raw-exception-content-sentinel")
        );

        let error = clipboard_uninterpretable_error(&bound);
        assert_eq!(error.code, ErrorCode::InteractionFailed);
        assert!(!error.message.as_str().contains("denied"));
        assert!(error.recovery.is_some());

        let error = clipboard_dispatch_error(TransportError::CommandFailed, &bound);
        assert_eq!(error.code, ErrorCode::InteractionFailed);
        assert!(error.message.as_str().contains("script dispatch"));
        assert!(error.message.as_str().contains("command_failed"));
        assert!(!error.message.as_str().contains("denied"));
        assert!(error.recovery.is_some());
        let error = clipboard_dispatch_error(TransportError::Disconnected, &bound);
        assert_eq!(error.code, ErrorCode::BrowserDisconnected);
    }

    /// The #8 classification fence: a command timeout names the unsettled
    /// clipboard operation and the pending-permission/OS-focus possibility
    /// instead of claiming a transport error, and a browser command
    /// rejection classifies as a stale document/world.
    #[test]
    fn timeout_and_rejection_dispatch_deaths_classify_what_is_knowable() {
        let bound = bound(TargetVisibility::Visible);

        let timeout = clipboard_dispatch_error(TransportError::Timeout, &bound);
        assert_eq!(timeout.code, ErrorCode::InteractionFailed);
        assert!(timeout.message.as_str().contains("did not settle"));
        assert!(timeout.message.as_str().contains("permission decision"));
        assert!(
            timeout
                .message
                .as_str()
                .contains("not focused at the OS level")
        );
        assert!(timeout.message.as_str().contains("command_timeout"));
        assert!(!timeout.message.as_str().contains("transport error:"));
        assert!(!timeout.message.as_str().contains("script dispatch failed"));
        assert!(
            timeout
                .recovery
                .as_ref()
                .is_some_and(|recovery| recovery.as_str().contains("OS level"))
        );

        let rejected = clipboard_dispatch_error(TransportError::Protocol, &bound);
        assert_eq!(rejected.code, ErrorCode::StaleReference);
        assert!(rejected.message.as_str().contains("destroyed"));
    }

    /// Builds the last scripted response — the `Runtime.callFunctionOn`
    /// outcome production `send_raw` forwards: the unwrapped CDP command
    /// result with the remote object beside an optional top-level
    /// `exceptionDetails`.
    fn bridge_exception_response(class_name: &str, description: &str) -> serde_json::Value {
        json!({
            "result": {"type": "object", "subtype": "error", "className": class_name, "description": description},
            "exceptionDetails": {
                "exceptionId": 1,
                "text": "Uncaught (in promise)",
                "exception": {"type": "object", "subtype": "error", "className": class_name, "description": description},
            },
        })
    }

    /// The write path performs the same three setup calls as read before its
    /// single bridge dispatch, so one transport builder serves both.
    fn read_bridge_transport(bridge: serde_json::Value) -> ScriptedTransport {
        ScriptedTransport {
            calls: Mutex::new(Vec::new()),
            responses: Mutex::new(vec![
                json!({"frameTree": {"frame": {"id": "main-frame"}}}),
                json!({"executionContextId": 41}),
                json!({"result": {"objectId": "isolated-global"}}),
                bridge,
            ]),
        }
    }

    async fn read_with(bridge: serde_json::Value) -> Result<ClipboardRead> {
        control()
            .read_clipboard(
                &read_bridge_transport(bridge),
                &bound(TargetVisibility::Visible),
                ReadClipboardRequest::default(),
            )
            .await
    }

    async fn write_failure_with(bridge: serde_json::Value) -> KrometrailError {
        let request = WriteClipboardRequest {
            target: krometrail_core::PageSelection::Selected,
            text: "sentinel-value".into(),
        };
        control()
            .write_clipboard(
                &read_bridge_transport(bridge),
                &bound(TargetVisibility::Visible),
                &request,
            )
            .await
            .unwrap_err()
    }

    /// Successful reads decode the production command-result shape:
    /// `{"result": {"type": "string", "value": ...}}` without any nested
    /// envelope.
    #[tokio::test]
    async fn read_success_decodes_the_production_command_result_shape() {
        let read =
            read_with(json!({"result": {"type": "string", "value": "sentinel-clipboard-text"}}))
                .await
                .expect("production success shape decodes");
        assert_eq!(read.text, "sentinel-clipboard-text");
        assert_eq!(read.utf8_bytes, 23);
    }

    /// Regression: a secure-context rejection arrives with `exceptionDetails`
    /// at the top level of the command result. It must classify as
    /// unsupported with secure-context recovery, never as a generic
    /// permission denial.
    #[tokio::test]
    async fn secure_context_failure_classifies_from_the_transport_exception_shape() {
        let error = read_with(bridge_exception_response(
            "Error",
            "Error: secure_context_required\n    at async function",
        ))
        .await
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::Unsupported);
        assert!(error.message.as_str().contains("secure page context"));
        assert!(!error.message.as_str().contains("denied"));
        assert!(error.recovery.is_some_and(|recovery| {
            let recovery = recovery.as_str();
            recovery.contains("HTTPS") || recovery.contains("secure context")
        }));
    }

    /// Regression: the write path classifies the same transport-shaped
    /// focus rejection as a focus failure with focus recovery, not as a
    /// permission denial.
    #[tokio::test]
    async fn write_focus_failure_classifies_from_the_transport_exception_shape() {
        let error =
            write_failure_with(bridge_exception_response("Error", "Error: focus_required")).await;
        assert_eq!(error.code, ErrorCode::InteractionFailed);
        assert!(error.message.as_str().contains("focused"));
        assert!(!error.message.as_str().contains("denied"));
        assert!(
            error
                .recovery
                .is_some_and(|recovery| recovery.as_str().contains("focus"))
        );
    }

    /// Regression: the real-Chrome focus race — focus is lost between the
    /// bridge pre-check and the clipboard call — surfaces as
    /// `NotAllowedError: Document is not focused.` (verified in the
    /// reviewed Chromium clipboard promise source) and must classify as a
    /// focus failure, not a permission denial.
    #[tokio::test]
    async fn document_not_focus_race_classifies_as_focus_not_denial() {
        let error = read_with(bridge_exception_response(
            "DOMException",
            "NotAllowedError: Document is not focused.",
        ))
        .await
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::InteractionFailed);
        assert!(error.message.as_str().contains("focused"));
        assert!(!error.message.as_str().contains("denied"));
    }

    /// Regression: an unidentified exception must stay neutral — no
    /// confident permission diagnosis, and the raw exception content and
    /// any clipboard text stay out of the agent-facing error.
    #[tokio::test]
    async fn unknown_exception_stays_neutral_and_never_claims_permission_denial() {
        let error = read_with(bridge_exception_response(
            "TypeError",
            "TypeError: raw-exception-content-sentinel-9f41",
        ))
        .await
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::InteractionFailed);
        assert!(!error.message.as_str().contains("denied"));
        assert!(!error.message.as_str().contains("permission"));
        assert!(
            !error
                .message
                .as_str()
                .contains("raw-exception-content-sentinel-9f41")
        );
        let recovery = error
            .recovery
            .expect("neutral failure still advises recovery");
        assert!(
            !recovery
                .as_str()
                .contains("raw-exception-content-sentinel-9f41")
        );
    }

    /// Regression: `exceptionDetails` is inspected before any value is
    /// accepted, so a rejected bridge call can never surface a stray remote
    /// value as a successful read.
    #[tokio::test]
    async fn exception_details_are_never_accepted_as_a_successful_value() {
        let bridge = json!({
            "result": {"type": "string", "value": "clipboard-sentinel-leak-b52e"},
            "exceptionDetails": {
                "exceptionId": 1,
                "text": "Uncaught (in promise)",
                "exception": {"className": "Error", "description": "Error: focus_required"},
            },
        });
        let error = read_with(bridge).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::InteractionFailed);
        assert!(error.message.as_str().contains("focused"));
        assert!(
            !error
                .message
                .as_str()
                .contains("clipboard-sentinel-leak-b52e")
        );
    }

    /// Regression: a well-formed envelope with no exception and no value is
    /// an uninterpretable response, not evidence of a permission denial.
    #[tokio::test]
    async fn malformed_bridge_response_stays_neutral() {
        let error = read_with(json!({"result": {"type": "object", "objectId": "remote-only"}}))
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::InteractionFailed);
        assert!(!error.message.as_str().contains("denied"));
        assert!(!error.message.as_str().contains("permission"));
        assert!(error.recovery.is_some());
    }

    /// The successful read byte limit is preserved on the production shape.
    #[tokio::test]
    async fn read_rejects_text_beyond_the_byte_limit() {
        let oversized = "x".repeat(MAX_CLIPBOARD_TEXT_BYTES + 1);
        let error = read_with(json!({"result": {"type": "string", "value": oversized}}))
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::InteractionFailed);
        assert!(error.message.as_str().contains("65536-byte limit"));
    }

    /// Regression (parent review of the first-pass classifier): Chromium
    /// rejects with `NotAllowedError: Permission Service could not
    /// connect.` when the browser permission service is unreachable — not
    /// a permission decision — so this must stay neutral instead of
    /// instructing the caller to allow permissions.
    #[tokio::test]
    async fn permission_service_failure_stays_neutral() {
        let error = read_with(bridge_exception_response(
            "DOMException",
            "NotAllowedError: Permission Service could not connect.",
        ))
        .await
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::InteractionFailed);
        assert!(!error.message.as_str().contains("denied"));
        assert!(!error.message.as_str().contains("permission"));
        let recovery = error
            .recovery
            .expect("neutral failure still advises recovery");
        assert!(!recovery.as_str().contains("allow clipboard access"));
        assert!(!recovery.as_str().contains("permission"));
    }

    /// Regression (parent review): `NotAllowedError` also covers document
    /// detach, permissions-policy blocking, and unlisted causes, and a
    /// denial or bridge marker further down the description is stack
    /// content, not the rejection reason. The first `ClassName: message`
    /// line is the only evidence; neither operation may report a known
    /// cause from any of these.
    #[tokio::test]
    async fn unlisted_rejections_stay_neutral_across_operations() {
        #[derive(Clone, Copy)]
        enum Op {
            Read,
            Write,
        }
        let failure = |op: Op, description: &'static str| async move {
            let bridge = bridge_exception_response("DOMException", description);
            match op {
                Op::Read => read_with(bridge).await.unwrap_err(),
                Op::Write => write_failure_with(bridge).await,
            }
        };
        let op_name = |op: Op| match op {
            Op::Read => "read",
            Op::Write => "write",
        };
        let cases: &[(&str, &str)] = &[
            ("detach", "NotAllowedError: Document detached."),
            (
                "bare",
                "NotAllowedError: Clipboard access was blocked by an unlisted browser policy.",
            ),
            (
                "stack-poisoned",
                "TypeError: unexpected page state\n    at NotAllowedError: Read permission denied. (<anonymous>)\n    at Error: secure_context_required (<anonymous>)",
            ),
        ];
        for op in [Op::Read, Op::Write] {
            for (label, description) in cases {
                let error = failure(op, description).await;
                assert_eq!(
                    error.code,
                    ErrorCode::InteractionFailed,
                    "{}/{} must stay a neutral interaction failure",
                    op_name(op),
                    label
                );
                assert!(
                    !error.message.as_str().contains("denied"),
                    "{}/{} must not claim denial",
                    op_name(op),
                    label
                );
                assert!(
                    !error.message.as_str().contains("permission"),
                    "{}/{} must not claim a permission cause",
                    op_name(op),
                    label
                );
                let recovery = error.recovery.expect("neutral failure advises recovery");
                assert!(
                    !recovery.as_str().contains("allow clipboard access"),
                    "{}/{} must not give denial recovery",
                    op_name(op),
                    label
                );
                assert!(
                    !error.message.as_str().contains("secure page context"),
                    "{}/{} must not classify from stack content",
                    op_name(op),
                    label
                );
                assert!(
                    !error.message.as_str().contains(description),
                    "{}/{} must not echo the raw description",
                    op_name(op),
                    label
                );
            }
        }
    }

    /// Operation-level coverage: the unavailable-API sentinel classifies
    /// as unsupported through the full read operation, not only through
    /// the direct helper.
    #[tokio::test]
    async fn clipboard_unavailable_classifies_from_the_transport_shape() {
        let error = read_with(bridge_exception_response(
            "Error",
            "Error: clipboard_unavailable",
        ))
        .await
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::Unsupported);
        assert!(error.message.as_str().contains("unavailable"));
        assert!(!error.message.as_str().contains("denied"));
        assert!(
            error
                .recovery
                .is_some_and(|recovery| recovery.as_str().contains("supported secure Chromium"))
        );
    }

    /// Regression (parent review): a success envelope with a value of the
    /// wrong type for the operation — on either read or write — is an
    /// uninterpretable response, never a permission claim.
    #[tokio::test]
    async fn wrong_type_success_values_stay_neutral() {
        for bridge in [
            json!({"result": {"type": "number", "value": 42}}),
            json!({"result": {"type": "boolean", "value": true}}),
        ] {
            let error = read_with(bridge).await.unwrap_err();
            assert_eq!(error.code, ErrorCode::InteractionFailed);
            assert!(!error.message.as_str().contains("denied"));
            assert!(!error.message.as_str().contains("permission"));
            assert!(error.recovery.is_some());
        }
        for bridge in [
            json!({"result": {"type": "string", "value": "not-a-confirmation"}}),
            json!({"result": {"type": "object"}}),
        ] {
            let error = write_failure_with(bridge).await;
            assert_eq!(error.code, ErrorCode::InteractionFailed);
            assert!(!error.message.as_str().contains("denied"));
            assert!(!error.message.as_str().contains("permission"));
            assert!(error.recovery.is_some());
        }
    }

    /// Permission denial is claimed only from Chrome's source-grounded
    /// rejection messages — `Read permission denied.` on the read path and
    /// `Write permission denied.` on the write path — with recovery that
    /// names the real state change.
    #[tokio::test]
    async fn source_grounded_denial_messages_report_permission_denial() {
        let read_error = read_with(bridge_exception_response(
            "DOMException",
            "NotAllowedError: Read permission denied.",
        ))
        .await
        .unwrap_err();
        assert_eq!(read_error.code, ErrorCode::InteractionFailed);
        assert!(read_error.message.as_str().contains("permission denied"));
        assert!(
            read_error
                .recovery
                .is_some_and(|recovery| recovery.as_str().contains("allow clipboard access"))
        );

        let write_error = write_failure_with(bridge_exception_response(
            "DOMException",
            "NotAllowedError: Write permission denied.",
        ))
        .await;
        assert_eq!(write_error.code, ErrorCode::InteractionFailed);
        assert!(write_error.message.as_str().contains("permission denied"));
        assert!(
            write_error
                .recovery
                .is_some_and(|recovery| recovery.as_str().contains("allow clipboard access"))
        );
    }

    #[tokio::test]
    async fn missing_document_or_world_is_stale_and_never_dispatches_the_bridge() {
        for response in [
            vec![json!({"frameTree":{}})],
            vec![
                json!({"frameTree":{"frame":{"id":"main-frame"}}}),
                json!({}),
            ],
        ] {
            let transport = ScriptedTransport {
                calls: Mutex::new(Vec::new()),
                responses: Mutex::new(response),
            };
            let error = control()
                .read_clipboard(
                    &transport,
                    &bound(TargetVisibility::Visible),
                    ReadClipboardRequest::default(),
                )
                .await
                .unwrap_err();
            assert_eq!(error.code, ErrorCode::StaleReference);
            assert!(
                transport
                    .calls
                    .lock()
                    .unwrap()
                    .iter()
                    .all(|(method, _)| method != "Runtime.callFunctionOn")
            );
        }
    }
}
