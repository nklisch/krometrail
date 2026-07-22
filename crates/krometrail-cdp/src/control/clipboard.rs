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
        let text = result_value(&response)
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| clipboard_response_error(bound, &response))?;
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
        if result_value(&response).and_then(serde_json::Value::as_bool) != Some(true) {
            return Err(clipboard_response_error(bound, &response));
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
    let root = frame_tree.get("frameTree").unwrap_or(&frame_tree);
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
        .or_else(|| world.pointer("/result/executionContextId"))
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
        .or_else(|| global.pointer("/result/result/objectId"))
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

fn result_value(value: &serde_json::Value) -> Option<&serde_json::Value> {
    value
        .pointer("/result/result/value")
        .or_else(|| value.pointer("/result/value"))
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

fn clipboard_response_error(bound: &BoundTarget, response: &serde_json::Value) -> KrometrailError {
    let description = response
        .pointer("/result/exceptionDetails/exception/description")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let (code, message, recovery) = if description.contains("secure_context_required") {
        (
            ErrorCode::Unsupported,
            "clipboard access requires a secure page context",
            "navigate the managed page to HTTPS or another secure context and retry",
        )
    } else if description.contains("clipboard_unavailable") {
        (
            ErrorCode::Unsupported,
            "the page clipboard API is unavailable",
            "use a supported secure Chromium page and retry",
        )
    } else if description.contains("focus_required") {
        (
            ErrorCode::InteractionFailed,
            "clipboard access requires a visible focused page",
            "focus the managed browser page and retry; Krometrail will not steal focus",
        )
    } else {
        (
            ErrorCode::InteractionFailed,
            "browser clipboard permission denied the explicit request",
            "focus the managed page, allow clipboard access in Chrome, and retry",
        )
    };
    clipboard_failure(code, bound, message, recovery)
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
                json!({"result":{"result":{"value":true}}}),
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

    #[test]
    fn response_failures_have_stable_supported_and_interaction_codes() {
        let bound = bound(TargetVisibility::Visible);
        for description in ["secure_context_required", "clipboard_unavailable"] {
            let error = clipboard_response_error(
                &bound,
                &json!({"result":{"exceptionDetails":{"exception":{"description":description}}}}),
            );
            assert_eq!(error.code, ErrorCode::Unsupported);
        }
        for description in ["focus_required", "NotAllowedError"] {
            let error = clipboard_response_error(
                &bound,
                &json!({"result":{"exceptionDetails":{"exception":{"description":description}}}}),
            );
            assert_eq!(error.code, ErrorCode::InteractionFailed);
        }
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
