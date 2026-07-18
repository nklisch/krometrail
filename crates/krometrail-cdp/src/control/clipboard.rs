use krometrail_core::{
    ClipboardRead, ErrorCode, ErrorContext, KrometrailError, MAX_CLIPBOARD_TEXT_BYTES,
    NonEmptyText, ReadClipboardRequest, Result, RetryAdvice, TargetVisibility,
    WriteClipboardRequest,
};
use serde_json::json;

use super::{BoundTarget, PageControl, transport_error};
use crate::transport::{CdpTransport, CommandScope};

const READ_CLIPBOARD: &str = "async function(){if(!globalThis.isSecureContext)throw new Error('secure_context_required');if(document.visibilityState!=='visible'||!document.hasFocus())throw new Error('focus_required');if(!navigator.clipboard)throw new Error('clipboard_unavailable');return await navigator.clipboard.readText();}";
const WRITE_CLIPBOARD: &str = "async function(value){if(!globalThis.isSecureContext)throw new Error('secure_context_required');if(document.visibilityState!=='visible'||!document.hasFocus())throw new Error('focus_required');if(!navigator.clipboard)throw new Error('clipboard_unavailable');await navigator.clipboard.writeText(value);return true;}";

impl PageControl {
    pub(crate) async fn read_clipboard(
        &self,
        transport: &dyn CdpTransport,
        bound: &BoundTarget,
        _request: ReadClipboardRequest,
    ) -> Result<ClipboardRead> {
        require_visible(bound)?;
        let response = transport.send_raw(
            &CommandScope::Session(bound.transport_session.clone()),
            "Runtime.callFunctionOn",
            json!({"functionDeclaration": READ_CLIPBOARD, "awaitPromise": true, "returnByValue": true}),
        ).await.map_err(|error| transport_error(error, ErrorCode::InteractionFailed, bound.target_id))?;
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
        let response = transport.send_raw(
            &CommandScope::Session(bound.transport_session.clone()),
            "Runtime.callFunctionOn",
            json!({"functionDeclaration": WRITE_CLIPBOARD, "arguments": [{"value": request.text}], "awaitPromise": true, "returnByValue": true}),
        ).await.map_err(|error| transport_error(error, ErrorCode::InteractionFailed, bound.target_id))?;
        if result_value(&response).and_then(serde_json::Value::as_bool) != Some(true) {
            return Err(clipboard_response_error(bound, &response));
        }
        Ok(())
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
    message: &'static str,
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
            responses: Mutex::new(vec![json!({"result":{"result":{"value":true}}})]),
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
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "Runtime.callFunctionOn");
        assert_eq!(calls[0].1["arguments"][0]["value"], "sentinel-value");
        assert!(
            !calls[0].1["functionDeclaration"]
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
    }
}
