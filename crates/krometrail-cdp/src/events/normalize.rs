use std::{collections::HashMap, sync::Arc};

use krometrail_core::{
    BrowserEventClass, BrowserEventKind, BrowserEventPayload, BrowserSourceTimestamp, ConsoleEvent,
    ConsoleEventSource, DialogClosedEvent, DialogOpenedEvent, ExceptionEvent, HttpStatus, IdSource,
    NavigationEvent, NavigationFrameScope, NavigationTransition, NetworkRequestFailed,
    NetworkRequestFinished, NetworkRequestId, NetworkRequestStarted, NetworkResourceType,
    NetworkResponseReceived, PageLifecycleEvent,
};
use serde_json::Value;

use super::{
    network::{NetworkActivity, NetworkActivityKind, NetworkRequestKey},
    privacy,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SourceDomain {
    Page,
    Runtime,
    Log,
    Network,
}

pub(super) struct SemanticSourceDefinition {
    pub(super) method: &'static str,
    pub(super) domain: SourceDomain,
    pub(super) class: BrowserEventClass,
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) kinds: &'static [BrowserEventKind],
}

pub(super) static SEMANTIC_SOURCE_REGISTRY: &[SemanticSourceDefinition] = &[
    SemanticSourceDefinition {
        method: "Runtime.consoleAPICalled",
        domain: SourceDomain::Runtime,
        class: BrowserEventClass::Console,
        kinds: &[BrowserEventKind::ConsoleMessage],
    },
    SemanticSourceDefinition {
        method: "Runtime.exceptionThrown",
        domain: SourceDomain::Runtime,
        class: BrowserEventClass::Exception,
        kinds: &[BrowserEventKind::JavascriptException],
    },
    SemanticSourceDefinition {
        method: "Log.entryAdded",
        domain: SourceDomain::Log,
        class: BrowserEventClass::Console,
        kinds: &[BrowserEventKind::ConsoleMessage],
    },
    SemanticSourceDefinition {
        method: "Network.requestWillBeSent",
        domain: SourceDomain::Network,
        class: BrowserEventClass::Network,
        kinds: &[
            BrowserEventKind::NetworkResponseReceived,
            BrowserEventKind::NetworkRequestStarted,
        ],
    },
    SemanticSourceDefinition {
        method: "Network.responseReceived",
        domain: SourceDomain::Network,
        class: BrowserEventClass::Network,
        kinds: &[BrowserEventKind::NetworkResponseReceived],
    },
    SemanticSourceDefinition {
        method: "Network.loadingFinished",
        domain: SourceDomain::Network,
        class: BrowserEventClass::Network,
        kinds: &[BrowserEventKind::NetworkRequestFinished],
    },
    SemanticSourceDefinition {
        method: "Network.loadingFailed",
        domain: SourceDomain::Network,
        class: BrowserEventClass::Network,
        kinds: &[BrowserEventKind::NetworkRequestFailed],
    },
    SemanticSourceDefinition {
        method: "Page.frameNavigated",
        domain: SourceDomain::Page,
        class: BrowserEventClass::Navigation,
        kinds: &[BrowserEventKind::Navigation],
    },
    SemanticSourceDefinition {
        method: "Page.navigatedWithinDocument",
        domain: SourceDomain::Page,
        class: BrowserEventClass::Navigation,
        kinds: &[BrowserEventKind::Navigation],
    },
    SemanticSourceDefinition {
        method: "Page.lifecycleEvent",
        domain: SourceDomain::Page,
        class: BrowserEventClass::Lifecycle,
        kinds: &[BrowserEventKind::PageLifecycle],
    },
    SemanticSourceDefinition {
        method: "Page.javascriptDialogOpening",
        domain: SourceDomain::Page,
        class: BrowserEventClass::Dialog,
        kinds: &[BrowserEventKind::DialogOpened],
    },
    SemanticSourceDefinition {
        method: "Page.javascriptDialogClosed",
        domain: SourceDomain::Page,
        class: BrowserEventClass::Dialog,
        kinds: &[BrowserEventKind::DialogClosed],
    },
];

#[derive(Clone, Debug)]
pub(super) struct NormalizedEvent {
    pub(super) source_time: Option<BrowserSourceTimestamp>,
    pub(super) payload: BrowserEventPayload,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum NormalizeError {
    InvalidPayload,
    RequestLimit,
}

impl NormalizeError {
    pub(super) const fn gap_reason(self) -> krometrail_core::BrowserEventGapReason {
        match self {
            Self::InvalidPayload => krometrail_core::BrowserEventGapReason::InvalidPayload,
            Self::RequestLimit => krometrail_core::BrowserEventGapReason::QueueSaturated,
        }
    }
}

#[derive(Clone)]
struct RequestContext {
    id: NetworkRequestId,
    method: Option<krometrail_core::HttpMethod>,
    resource_type: Option<NetworkResourceType>,
    url: Option<krometrail_core::SanitizedUrl>,
    long_lived: bool,
}

#[derive(Default)]
struct NormalizerState {
    requests: HashMap<NetworkRequestKey, RequestContext>,
    main_frame: Option<String>,
    open_dialog: Option<krometrail_core::BrowserDialogType>,
}

pub(super) struct EventNormalizer {
    ids: Arc<dyn IdSource>,
    request_limit: usize,
    state: std::sync::Mutex<NormalizerState>,
}

impl EventNormalizer {
    pub(super) fn new(ids: Arc<dyn IdSource>, request_limit: usize) -> Self {
        Self {
            ids,
            request_limit,
            state: std::sync::Mutex::new(NormalizerState::default()),
        }
    }

    pub(super) fn normalize_non_network(
        &self,
        method: &str,
        params: &Value,
    ) -> Result<Vec<NormalizedEvent>, NormalizeError> {
        match method {
            "Runtime.consoleAPICalled" => self.runtime_console(params),
            "Runtime.exceptionThrown" => self.exception(params),
            "Log.entryAdded" => self.log_entry(params),
            "Page.frameNavigated" => self.frame_navigated(params),
            "Page.navigatedWithinDocument" => self.within_document(params),
            "Page.lifecycleEvent" => self.lifecycle(params),
            "Page.javascriptDialogOpening" => self.dialog_opened(params),
            "Page.javascriptDialogClosed" => self.dialog_closed(params),
            _ => Ok(Vec::new()),
        }
    }

    pub(super) fn normalize_network(
        &self,
        method: &str,
        params: &Value,
    ) -> Result<NetworkActivity, NormalizeError> {
        let raw_key = params
            .get("requestId")
            .and_then(Value::as_str)
            .and_then(NetworkRequestKey::new)
            .ok_or(NormalizeError::InvalidPayload)?;
        match method {
            "Network.requestWillBeSent" => self.request_started(raw_key, params),
            "Network.responseReceived" => self.response_received(raw_key, params),
            "Network.loadingFinished" => self.request_finished(raw_key, params),
            "Network.loadingFailed" => self.request_failed(raw_key, params),
            _ => Err(NormalizeError::InvalidPayload),
        }
    }

    fn runtime_console(&self, params: &Value) -> Result<Vec<NormalizedEvent>, NormalizeError> {
        let level = privacy::console_level(params.get("type"));
        let event = ConsoleEvent::new(
            ConsoleEventSource::Runtime,
            level,
            privacy::console_method(params.get("type")),
            privacy::console_argument_types(params.get("args")),
            privacy::console_preview(params.get("args")),
            privacy::stack_frames(
                params.get("stackTrace"),
                krometrail_core::MAX_EVENT_STACK_FRAMES,
            ),
        );
        Ok(vec![NormalizedEvent {
            source_time: privacy::source_epoch_millis(params.get("timestamp")),
            payload: BrowserEventPayload::ConsoleMessage(event),
        }])
    }

    fn log_entry(&self, params: &Value) -> Result<Vec<NormalizedEvent>, NormalizeError> {
        let entry = params
            .get("entry")
            .filter(|entry| entry.is_object())
            .ok_or(NormalizeError::InvalidPayload)?;
        let level = privacy::console_level(entry.get("level"));
        let preview = entry
            .get("text")
            .and_then(Value::as_str)
            .map(|text| krometrail_core::EventRedactor.text(text));
        let event = ConsoleEvent::new(
            ConsoleEventSource::Log,
            level,
            privacy::console_method(entry.get("level")),
            Vec::new(),
            preview,
            privacy::stack_frames(
                entry.get("stackTrace"),
                krometrail_core::MAX_EVENT_STACK_FRAMES,
            ),
        );
        Ok(vec![NormalizedEvent {
            source_time: privacy::source_epoch_millis(entry.get("timestamp")),
            payload: BrowserEventPayload::ConsoleMessage(event),
        }])
    }

    fn exception(&self, params: &Value) -> Result<Vec<NormalizedEvent>, NormalizeError> {
        let details = params
            .get("exceptionDetails")
            .filter(|details| details.is_object())
            .ok_or(NormalizeError::InvalidPayload)?;
        let name = details
            .pointer("/exception/className")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(|value| krometrail_core::EventRedactor.name(value));
        let text = details
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or("JavaScript exception");
        let event = ExceptionEvent::new(
            name,
            krometrail_core::EventRedactor.text(text),
            privacy::stack_frames(
                details.get("stackTrace"),
                krometrail_core::MAX_EVENT_STACK_FRAMES,
            ),
        )
        .map_err(|_| NormalizeError::InvalidPayload)?;
        Ok(vec![NormalizedEvent {
            source_time: privacy::source_epoch_millis(details.get("timestamp")),
            payload: BrowserEventPayload::JavascriptException(event),
        }])
    }

    fn frame_navigated(&self, params: &Value) -> Result<Vec<NormalizedEvent>, NormalizeError> {
        let frame = params
            .get("frame")
            .filter(|frame| frame.is_object())
            .ok_or(NormalizeError::InvalidPayload)?;
        let frame_id = frame
            .get("id")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or(NormalizeError::InvalidPayload)?;
        let main = frame.get("parentId").is_none();
        if main {
            self.state.lock().expect("event normalizer lock").main_frame =
                Some(frame_id.to_owned());
        }
        let event = NavigationEvent::new(
            if main {
                NavigationFrameScope::Main
            } else {
                NavigationFrameScope::Child
            },
            navigation_transition(frame.get("transitionType")),
            privacy::sanitized_url(frame.get("url"))?,
        );
        Ok(vec![NormalizedEvent {
            source_time: None,
            payload: BrowserEventPayload::Navigation(event),
        }])
    }

    fn within_document(&self, params: &Value) -> Result<Vec<NormalizedEvent>, NormalizeError> {
        let frame_id = params
            .get("frameId")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or(NormalizeError::InvalidPayload)?;
        let main = self
            .state
            .lock()
            .expect("event normalizer lock")
            .main_frame
            .as_deref()
            == Some(frame_id);
        let event = NavigationEvent::new(
            if main {
                NavigationFrameScope::Main
            } else {
                NavigationFrameScope::Child
            },
            NavigationTransition::SameDocument,
            privacy::sanitized_url(params.get("url"))?,
        );
        Ok(vec![NormalizedEvent {
            source_time: None,
            payload: BrowserEventPayload::Navigation(event),
        }])
    }

    fn lifecycle(&self, params: &Value) -> Result<Vec<NormalizedEvent>, NormalizeError> {
        let frame_id = params
            .get("frameId")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or(NormalizeError::InvalidPayload)?;
        let main = self
            .state
            .lock()
            .expect("event normalizer lock")
            .main_frame
            .as_deref()
            == Some(frame_id);
        Ok(vec![NormalizedEvent {
            source_time: privacy::source_seconds(params.get("timestamp")),
            payload: BrowserEventPayload::PageLifecycle(PageLifecycleEvent::new(
                if main {
                    NavigationFrameScope::Main
                } else {
                    NavigationFrameScope::Child
                },
                privacy::lifecycle_name(params.get("name")),
            )),
        }])
    }

    fn dialog_opened(&self, params: &Value) -> Result<Vec<NormalizedEvent>, NormalizeError> {
        let dialog_type = privacy::dialog_type(params.get("type"));
        self.state
            .lock()
            .expect("event normalizer lock")
            .open_dialog = Some(dialog_type);
        Ok(vec![NormalizedEvent {
            source_time: None,
            payload: BrowserEventPayload::DialogOpened(DialogOpenedEvent::new(
                dialog_type,
                params
                    .get("message")
                    .and_then(Value::as_str)
                    .is_some_and(|message| !message.is_empty()),
                params
                    .get("defaultPrompt")
                    .and_then(Value::as_str)
                    .is_some_and(|prompt| !prompt.is_empty()),
            )),
        }])
    }

    fn dialog_closed(&self, params: &Value) -> Result<Vec<NormalizedEvent>, NormalizeError> {
        let dialog_type = self
            .state
            .lock()
            .expect("event normalizer lock")
            .open_dialog
            .take()
            .unwrap_or(krometrail_core::BrowserDialogType::Other);
        Ok(vec![NormalizedEvent {
            source_time: None,
            payload: BrowserEventPayload::DialogClosed(DialogClosedEvent::new(
                dialog_type,
                params.get("result").and_then(Value::as_bool) == Some(true),
                params
                    .get("userInput")
                    .and_then(Value::as_str)
                    .is_some_and(|input| !input.is_empty()),
            )),
        }])
    }

    fn request_started(
        &self,
        key: NetworkRequestKey,
        params: &Value,
    ) -> Result<NetworkActivity, NormalizeError> {
        let request = params
            .get("request")
            .filter(|request| request.is_object())
            .ok_or(NormalizeError::InvalidPayload)?;
        let method =
            privacy::http_method(request.get("method"))?.ok_or(NormalizeError::InvalidPayload)?;
        let url =
            privacy::sanitized_url(request.get("url"))?.ok_or(NormalizeError::InvalidPayload)?;
        let resource_type =
            privacy::resource_type(params.get("type")).unwrap_or(NetworkResourceType::Other);
        let long_lived = is_long_lived(Some(resource_type));
        let source_time = privacy::source_seconds(params.get("timestamp"));
        let mut state = self.state.lock().expect("event normalizer lock");
        let id = if let Some(existing) = state.requests.get(&key) {
            existing.id
        } else {
            if state.requests.len() >= self.request_limit {
                return Err(NormalizeError::RequestLimit);
            }
            self.next_request_id()
        };
        let mut normalized = Vec::with_capacity(2);
        if let Some(redirect) = params.get("redirectResponse")
            && let Some(previous) = state.requests.get(&key)
        {
            normalized.push(NormalizedEvent {
                source_time: source_time.clone(),
                payload: BrowserEventPayload::NetworkResponseReceived(response_payload(
                    previous, redirect,
                )?),
            });
        }
        let context = RequestContext {
            id,
            method: Some(method.clone()),
            resource_type: Some(resource_type),
            url: Some(url.clone()),
            long_lived,
        };
        state.requests.insert(key.clone(), context);
        drop(state);
        normalized.push(NormalizedEvent {
            source_time,
            payload: BrowserEventPayload::NetworkRequestStarted(
                NetworkRequestStarted::new(
                    id,
                    method,
                    resource_type,
                    url,
                    privacy::network_initiator(params.get("initiator")),
                )
                .map_err(|_| NormalizeError::InvalidPayload)?,
            ),
        });
        Ok(NetworkActivity::new(
            key,
            NetworkActivityKind::Started,
            long_lived,
            normalized,
        ))
    }

    fn response_received(
        &self,
        key: NetworkRequestKey,
        params: &Value,
    ) -> Result<NetworkActivity, NormalizeError> {
        let response = params
            .get("response")
            .filter(|response| response.is_object())
            .ok_or(NormalizeError::InvalidPayload)?;
        let mut state = self.state.lock().expect("event normalizer lock");
        let context = if let Some(context) = state.requests.get(&key).cloned() {
            context
        } else {
            if state.requests.len() >= self.request_limit {
                return Err(NormalizeError::RequestLimit);
            }
            let resource_type = privacy::resource_type(params.get("type"));
            let context = RequestContext {
                id: self.next_request_id(),
                method: None,
                resource_type,
                url: privacy::sanitized_url(response.get("url"))?,
                long_lived: is_long_lived(resource_type),
            };
            state.requests.insert(key.clone(), context.clone());
            context
        };
        drop(state);
        let payload = response_payload(&context, response)?;
        Ok(NetworkActivity::new(
            key,
            NetworkActivityKind::Response,
            context.long_lived,
            vec![NormalizedEvent {
                source_time: privacy::source_seconds(params.get("timestamp")),
                payload: BrowserEventPayload::NetworkResponseReceived(payload),
            }],
        ))
    }

    fn request_finished(
        &self,
        key: NetworkRequestKey,
        params: &Value,
    ) -> Result<NetworkActivity, NormalizeError> {
        let context = self
            .state
            .lock()
            .expect("event normalizer lock")
            .requests
            .remove(&key)
            .unwrap_or_else(|| RequestContext {
                id: self.next_request_id(),
                method: None,
                resource_type: None,
                url: None,
                long_lived: false,
            });
        Ok(NetworkActivity::new(
            key,
            NetworkActivityKind::Finished,
            context.long_lived,
            vec![NormalizedEvent {
                source_time: privacy::source_seconds(params.get("timestamp")),
                payload: BrowserEventPayload::NetworkRequestFinished(
                    NetworkRequestFinished::new(context.id)
                        .map_err(|_| NormalizeError::InvalidPayload)?,
                ),
            }],
        ))
    }

    fn request_failed(
        &self,
        key: NetworkRequestKey,
        params: &Value,
    ) -> Result<NetworkActivity, NormalizeError> {
        let context = self
            .state
            .lock()
            .expect("event normalizer lock")
            .requests
            .remove(&key)
            .unwrap_or_else(|| {
                let resource_type = privacy::resource_type(params.get("type"));
                RequestContext {
                    id: self.next_request_id(),
                    method: None,
                    resource_type,
                    url: None,
                    long_lived: is_long_lived(resource_type),
                }
            });
        Ok(NetworkActivity::new(
            key,
            NetworkActivityKind::Failed,
            context.long_lived,
            vec![NormalizedEvent {
                source_time: privacy::source_seconds(params.get("timestamp")),
                payload: BrowserEventPayload::NetworkRequestFailed(
                    NetworkRequestFailed::new(
                        context.id,
                        context.method,
                        context.resource_type,
                        context.url,
                        privacy::failure_kind(params),
                    )
                    .map_err(|_| NormalizeError::InvalidPayload)?,
                ),
            }],
        ))
    }

    fn next_request_id(&self) -> NetworkRequestId {
        NetworkRequestId::from_uuid(*self.ids.next().as_uuid())
    }
}

fn is_long_lived(resource_type: Option<NetworkResourceType>) -> bool {
    matches!(
        resource_type,
        Some(NetworkResourceType::WebSocket | NetworkResourceType::EventSource)
    )
}

fn response_payload(
    context: &RequestContext,
    response: &Value,
) -> Result<NetworkResponseReceived, NormalizeError> {
    let status = response
        .get("status")
        .and_then(Value::as_f64)
        .filter(|status| status.is_finite() && status.fract() == 0.0 && *status >= 0.0)
        .and_then(|status| u16::try_from(status as u64).ok())
        .and_then(|status| HttpStatus::new(status).ok())
        .ok_or(NormalizeError::InvalidPayload)?;
    NetworkResponseReceived::new(
        context.id,
        context.method.clone(),
        context.resource_type,
        context.url.clone(),
        status,
        response.get("fromDiskCache").and_then(Value::as_bool) == Some(true),
        response.get("fromServiceWorker").and_then(Value::as_bool) == Some(true),
    )
    .map_err(|_| NormalizeError::InvalidPayload)
}

fn navigation_transition(value: Option<&Value>) -> NavigationTransition {
    match value
        .and_then(Value::as_str)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("link") => NavigationTransition::Link,
        Some("typed") => NavigationTransition::Typed,
        Some("autobookmark") => NavigationTransition::AutoBookmark,
        Some("autosubframe") => NavigationTransition::AutoSubframe,
        Some("manualsubframe") => NavigationTransition::ManualSubframe,
        Some("generated") => NavigationTransition::Generated,
        Some("startpage") => NavigationTransition::StartPage,
        Some("formsubmit") => NavigationTransition::FormSubmit,
        Some("reload") => NavigationTransition::Reload,
        Some("keyword") => NavigationTransition::Keyword,
        Some("keywordgenerated") => NavigationTransition::KeywordGenerated,
        _ => NavigationTransition::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krometrail_core::IdValue;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct TestIds(AtomicU64);

    impl IdSource for TestIds {
        fn next(&self) -> IdValue {
            IdValue::from_uuid(uuid::Uuid::from_u128(
                u128::from(self.0.fetch_add(1, Ordering::Relaxed)) + 1,
            ))
        }
    }

    fn normalizer(limit: usize) -> EventNormalizer {
        EventNormalizer::new(Arc::new(TestIds(AtomicU64::new(0))), limit)
    }

    #[test]
    fn source_registry_is_unique_and_references_core_kinds() {
        let mut methods = std::collections::HashSet::new();
        for source in SEMANTIC_SOURCE_REGISTRY {
            assert!(methods.insert(source.method));
            assert!(!source.kinds.is_empty());
            assert!(source.kinds.iter().all(|kind| {
                krometrail_core::BROWSER_EVENT_REGISTRY
                    .iter()
                    .any(|definition| definition.kind == *kind)
            }));
        }
    }

    #[test]
    fn redirects_and_out_of_order_network_events_keep_typed_private_correlations() {
        let normalizer = normalizer(4);
        let first = normalizer
            .normalize_network(
                "Network.requestWillBeSent",
                &serde_json::json!({
                    "requestId":"raw-private-id",
                    "timestamp":1.0,
                    "type":"Document",
                    "request":{"method":"GET","url":"https://user:secret@example.test/private?q=secret#f"},
                    "initiator":{"type":"parser"}
                }),
            )
            .unwrap();
        assert_eq!(first.normalized.len(), 1);
        let redirect = normalizer
            .normalize_network(
                "Network.requestWillBeSent",
                &serde_json::json!({
                    "requestId":"raw-private-id",
                    "timestamp":2.0,
                    "type":"Document",
                    "redirectResponse":{"status":302.0},
                    "request":{"method":"GET","url":"https://example.test/next"},
                    "initiator":{"type":"parser"}
                }),
            )
            .unwrap();
        assert_eq!(redirect.normalized.len(), 2);
        let encoded = serde_json::to_string(&redirect.normalized[0].payload).unwrap();
        assert!(!encoded.contains("raw-private-id"));
        assert!(!encoded.contains("secret"));
        assert!(!encoded.contains("private"));

        let orphan = normalizer
            .normalize_network(
                "Network.loadingFailed",
                &serde_json::json!({
                    "requestId":"orphan-private-id",
                    "timestamp":3.0,
                    "errorText":"net::ERR_CONNECTION_REFUSED"
                }),
            )
            .unwrap();
        assert_eq!(orphan.kind(), NetworkActivityKind::Failed);
    }

    #[test]
    fn source_clocks_and_dialog_values_reduce_to_allowlisted_semantics() {
        let normalizer = normalizer(4);
        let console = normalizer
            .normalize_non_network(
                "Runtime.consoleAPICalled",
                &serde_json::json!({
                    "type":"log",
                    "timestamp":1720000000123.5,
                    "args":[{"type":"string","value":"safe preview"}]
                }),
            )
            .unwrap();
        assert_eq!(
            console[0].source_time.as_ref().map(|time| time.clock()),
            Some(krometrail_core::BrowserSourceClock::UnixEpoch)
        );

        let opened = normalizer
            .normalize_non_network(
                "Page.javascriptDialogOpening",
                &serde_json::json!({
                    "type":"prompt",
                    "message":"private dialog value",
                    "defaultPrompt":"private default value"
                }),
            )
            .unwrap();
        let closed = normalizer
            .normalize_non_network(
                "Page.javascriptDialogClosed",
                &serde_json::json!({
                    "result":true,
                    "userInput":"private submitted value"
                }),
            )
            .unwrap();
        let encoded = serde_json::to_string(&[&opened[0].payload, &closed[0].payload]).unwrap();
        assert!(!encoded.contains("private dialog value"));
        assert!(!encoded.contains("private default value"));
        assert!(!encoded.contains("private submitted value"));
        assert!(encoded.contains("had_message"));
        assert!(encoded.contains("had_user_input"));
    }
}
