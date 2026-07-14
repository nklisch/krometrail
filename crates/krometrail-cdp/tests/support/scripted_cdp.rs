#![allow(dead_code)]

use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use krometrail_cdp::{
    CdpTransport, CommandScope, NamedEvent, TransportError, TransportEvents, TransportFuture,
};
use serde_json::{Value, json};
use tokio::sync::Notify;

#[derive(Clone, Debug)]
pub struct ScriptedCdp {
    state: Arc<Mutex<State>>,
    closed: Arc<AtomicBool>,
    disconnect_notify: Arc<Notify>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CommandCall {
    pub method: String,
    pub session: Option<String>,
    pub params: Value,
}

#[derive(Debug)]
struct State {
    product: String,
    user_agent: String,
    missing: HashSet<String>,
    malformed: HashSet<String>,
    commands: Vec<(String, Option<String>)>,
    command_calls: Vec<CommandCall>,
    subscriptions: Vec<(String, Option<String>)>,
    events: HashMap<String, Vec<Value>>,
    responses: HashMap<String, VecDeque<Result<Value, TransportError>>>,
    hold_events_open: bool,
    held_methods: HashSet<String>,
    command_notify: Arc<Notify>,
}

impl ScriptedCdp {
    pub fn capable(product: &str, user_agent: &str) -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                product: product.to_owned(),
                user_agent: user_agent.to_owned(),
                missing: HashSet::new(),
                malformed: HashSet::new(),
                commands: Vec::new(),
                command_calls: Vec::new(),
                subscriptions: Vec::new(),
                events: HashMap::new(),
                responses: HashMap::new(),
                hold_events_open: false,
                held_methods: HashSet::new(),
                command_notify: Arc::new(Notify::new()),
            })),
            closed: Arc::new(AtomicBool::new(false)),
            disconnect_notify: Arc::new(Notify::new()),
        }
    }

    pub fn chrome() -> Self {
        Self::capable("Chrome/149.0.0.0", "Mozilla/5.0 Chrome/149.0.0.0")
    }

    pub fn missing(&self, method: &str) {
        self.state.lock().unwrap().missing.insert(method.to_owned());
    }

    #[allow(dead_code)]
    pub fn malformed(&self, method: &str) {
        self.state
            .lock()
            .unwrap()
            .malformed
            .insert(method.to_owned());
    }

    #[allow(dead_code)]
    pub fn commands(&self) -> Vec<(String, Option<String>)> {
        self.state.lock().unwrap().commands.clone()
    }

    #[allow(dead_code)]
    pub fn command_calls(&self) -> Vec<CommandCall> {
        self.state.lock().unwrap().command_calls.clone()
    }

    #[allow(dead_code)]
    pub fn push_response(&self, method: &str, response: Value) {
        self.state
            .lock()
            .unwrap()
            .responses
            .entry(method.to_owned())
            .or_default()
            .push_back(Ok(response));
    }

    #[allow(dead_code)]
    pub fn push_failure(&self, method: &str, error: TransportError) {
        self.state
            .lock()
            .unwrap()
            .responses
            .entry(method.to_owned())
            .or_default()
            .push_back(Err(error));
    }

    #[allow(dead_code)]
    pub fn hold_events_open(&self) {
        self.state.lock().unwrap().hold_events_open = true;
    }

    pub fn hold_method(&self, method: &str) {
        self.state
            .lock()
            .unwrap()
            .held_methods
            .insert(method.to_owned());
    }

    pub async fn wait_for_command(&self, method: &str) {
        self.wait_for_command_count(method, 1).await;
    }

    pub async fn wait_for_command_count(&self, method: &str, count: usize) {
        loop {
            let notified = {
                let state = self.state.lock().unwrap();
                if state
                    .commands
                    .iter()
                    .filter(|(called, _)| called == method)
                    .count()
                    >= count
                {
                    return;
                }
                Arc::clone(&state.command_notify).notified_owned()
            };
            notified.await;
        }
    }

    pub fn disconnect(&self) {
        self.closed.store(true, Ordering::Release);
        self.disconnect_notify.notify_waiters();
    }

    #[allow(dead_code)]
    pub fn subscriptions(&self) -> Vec<(String, Option<String>)> {
        self.state.lock().unwrap().subscriptions.clone()
    }

    #[allow(dead_code)]
    pub fn push_event(&self, method: &str, params: Value) {
        self.state
            .lock()
            .unwrap()
            .events
            .entry(method.to_owned())
            .or_default()
            .push(params);
    }

    fn response(
        &self,
        scope: &CommandScope,
        method: &str,
        params: Value,
    ) -> Result<Value, TransportError> {
        let mut state = self.state.lock().unwrap();
        let session = match scope {
            CommandScope::Browser => None,
            CommandScope::Session(session) => Some(session.as_str().to_owned()),
        };
        state.commands.push((method.to_owned(), session.clone()));
        state.command_calls.push(CommandCall {
            method: method.to_owned(),
            session: session.clone(),
            params: params.clone(),
        });
        state.command_notify.notify_waiters();
        if state.missing.contains(method) {
            return Err(TransportError::CommandFailed);
        }
        if state.malformed.contains(method) {
            return Ok(json!("malformed-response"));
        }
        if let Some(response) = state
            .responses
            .get_mut(method)
            .and_then(VecDeque::pop_front)
        {
            return response;
        }
        Ok(match method {
            "Browser.getVersion" => json!({
                "protocolVersion": "1.3",
                "product": state.product,
                "revision": "@fixture",
                "userAgent": state.user_agent,
                "jsVersion": "12.0"
            }),
            "Target.getTargets" => {
                json!({"targetInfos": [{"targetId":"target-a","type":"page","url":"http://fixture/","title":"fixture"}]})
            }
            "Target.attachToTarget" => json!({"sessionId":"session-a"}),
            "Runtime.evaluate"
                if params.get("expression").and_then(Value::as_str)
                    == Some("document.visibilityState") =>
            {
                json!({"result":{"type":"string","value":"visible"}})
            }
            "Schema.getDomains" => {
                json!({"domains":[{"name":"Page","commands":[{"name":"startScreencast"}]}]})
            }
            _ => Value::Object(Default::default()),
        })
    }
}

impl CdpTransport for ScriptedCdp {
    fn send_raw(
        &self,
        scope: &CommandScope,
        method: &str,
        params: Value,
    ) -> TransportFuture<'_, Result<Value, TransportError>> {
        let held = self.state.lock().unwrap().held_methods.contains(method);
        let result = self.response(scope, method, params);
        Box::pin(async move {
            if held {
                std::future::pending::<Result<Value, TransportError>>().await
            } else {
                result
            }
        })
    }

    fn subscribe_named(
        &self,
        scope: &CommandScope,
        method: &str,
    ) -> TransportFuture<'_, Result<Box<dyn TransportEvents>, TransportError>> {
        let session = match scope {
            CommandScope::Browser => None,
            CommandScope::Session(session) => Some(session.as_str().to_owned()),
        };
        let result = if method.trim().is_empty() {
            Err(TransportError::InvalidInput)
        } else {
            self.state
                .lock()
                .unwrap()
                .subscriptions
                .push((method.to_owned(), session));
            let (params, hold_open) = {
                let mut state = self.state.lock().unwrap();
                (
                    state.events.remove(method).unwrap_or_default(),
                    state.hold_events_open,
                )
            };
            Ok(Box::new(ScriptedEvents {
                method: method.to_owned(),
                params,
                index: 0,
                hold_open,
                closed: Arc::clone(&self.closed),
                disconnect_notify: Arc::clone(&self.disconnect_notify),
            }) as Box<dyn TransportEvents>)
        };
        Box::pin(async move { result })
    }

    fn close_reason(&self) -> Option<krometrail_cdp::TransportClose> {
        self.closed
            .load(Ordering::Acquire)
            .then(|| krometrail_cdp::TransportClose {
                reason: krometrail_core::NonEmptyText::new("scripted disconnect").unwrap(),
            })
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }
}

struct ScriptedEvents {
    method: String,
    params: Vec<Value>,
    index: usize,
    hold_open: bool,
    closed: Arc<AtomicBool>,
    disconnect_notify: Arc<Notify>,
}

impl TransportEvents for ScriptedEvents {
    fn next(&mut self) -> TransportFuture<'_, Result<Option<NamedEvent>, TransportError>> {
        let event = self.params.get(self.index).cloned();
        self.index += usize::from(event.is_some());
        let method = self.method.clone();
        let hold_open = self.hold_open;
        let closed = Arc::clone(&self.closed);
        let disconnect_notify = Arc::clone(&self.disconnect_notify);
        Box::pin(async move {
            if let Some(params) = event {
                return Ok(Some(NamedEvent { method, params }));
            }
            if hold_open {
                loop {
                    if closed.load(Ordering::Acquire) {
                        return Err(TransportError::Disconnected);
                    }
                    let notified = disconnect_notify.notified();
                    if closed.load(Ordering::Acquire) {
                        return Err(TransportError::Disconnected);
                    }
                    notified.await;
                }
            }
            Ok(None)
        })
    }
}
