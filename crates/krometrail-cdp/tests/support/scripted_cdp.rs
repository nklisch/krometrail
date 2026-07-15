#![allow(dead_code)]

use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use krometrail_cdp::{
    CdpTransport, CdpTransportFactory, CommandScope, NamedEvent, TransportError, TransportEvents,
    TransportFuture,
};
use serde_json::{Value, json};
use tokio::sync::{Notify, mpsc};

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScriptedActivity {
    Command {
        method: String,
        session: Option<String>,
    },
    Subscription {
        method: String,
        session: Option<String>,
    },
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
    activity: Vec<ScriptedActivity>,
    events: HashMap<(String, Option<String>), Vec<Value>>,
    live_events: HashMap<(String, Option<String>), Vec<mpsc::UnboundedSender<Value>>>,
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
                activity: Vec::new(),
                events: HashMap::new(),
                live_events: HashMap::new(),
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
    pub fn activity(&self) -> Vec<ScriptedActivity> {
        self.state.lock().unwrap().activity.clone()
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
        self.push_scoped_event(method, None, params);
    }

    #[allow(dead_code)]
    pub fn push_scoped_event(&self, method: &str, session: Option<&str>, params: Value) {
        let mut state = self.state.lock().unwrap();
        let session = session.map(str::to_owned);
        let mut delivered = false;
        for ((live_method, live_session), senders) in &mut state.live_events {
            if live_method == method && (session.is_none() || *live_session == session) {
                senders.retain(|sender| sender.send(params.clone()).is_ok());
                delivered |= !senders.is_empty();
            }
        }
        if !delivered {
            state
                .events
                .entry((method.to_owned(), session))
                .or_default()
                .push(params);
        }
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
        state.activity.push(ScriptedActivity::Command {
            method: method.to_owned(),
            session: session.clone(),
        });
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

/// A deterministic physical-connection seam. Each connect consumes one pre-scripted transport;
/// callers retain the handles so a test can sever exactly one generation without touching the
/// replacement connection.
#[derive(Clone, Debug, Default)]
pub struct ScriptedCdpFactory {
    connections: Arc<Mutex<VecDeque<Arc<ScriptedCdp>>>>,
}

impl ScriptedCdpFactory {
    pub fn new(connections: impl IntoIterator<Item = Arc<ScriptedCdp>>) -> Self {
        Self {
            connections: Arc::new(Mutex::new(connections.into_iter().collect())),
        }
    }
}

impl CdpTransportFactory for ScriptedCdpFactory {
    fn connect(
        &self,
        _browser_websocket_url: &str,
    ) -> TransportFuture<'_, Result<Arc<dyn CdpTransport>, TransportError>> {
        let connection = self
            .connections
            .lock()
            .unwrap()
            .pop_front()
            .ok_or(TransportError::ConnectFailed);
        Box::pin(async move { connection.map(|connection| connection as Arc<dyn CdpTransport>) })
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
            {
                let mut state = self.state.lock().unwrap();
                state
                    .subscriptions
                    .push((method.to_owned(), session.clone()));
                state.activity.push(ScriptedActivity::Subscription {
                    method: method.to_owned(),
                    session: session.clone(),
                });
            }
            let (params, hold_open, live_receiver) = {
                let mut state = self.state.lock().unwrap();
                let exact = (method.to_owned(), session.clone());
                let fallback = (method.to_owned(), None);
                let params = state
                    .events
                    .remove(&exact)
                    .or_else(|| {
                        (exact != fallback)
                            .then(|| state.events.remove(&fallback))
                            .flatten()
                    })
                    .unwrap_or_default();
                let hold_open = state.hold_events_open;
                let live_receiver = hold_open.then(|| {
                    let (sender, receiver) = mpsc::unbounded_channel();
                    for params in &params {
                        let _ = sender.send(params.clone());
                    }
                    state.live_events.entry(exact).or_default().push(sender);
                    receiver
                });
                (params, hold_open, live_receiver)
            };
            Ok(Box::new(ScriptedEvents {
                method: method.to_owned(),
                params: if live_receiver.is_none() {
                    params
                } else {
                    Vec::new()
                },
                index: 0,
                hold_open,
                live_receiver,
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
    live_receiver: Option<mpsc::UnboundedReceiver<Value>>,
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
        let live_receiver = self.live_receiver.as_mut();
        Box::pin(async move {
            if let Some(params) = event {
                return Ok(Some(NamedEvent { method, params }));
            }
            if let Some(receiver) = live_receiver {
                tokio::select! {
                    params = receiver.recv() => match params {
                        Some(params) => Ok(Some(NamedEvent { method, params })),
                        None => Ok(None),
                    },
                    _ = disconnect_notify.notified() => Err(TransportError::Disconnected),
                }
            } else if hold_open {
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
            } else {
                Ok(None)
            }
        })
    }
}
