#![allow(dead_code)]

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use krometrail_cdp::{
    CdpTransport, CommandScope, NamedEvent, TransportError, TransportEvents, TransportFuture,
};
use serde_json::{Value, json};

#[derive(Clone, Debug)]
pub struct ScriptedCdp {
    state: Arc<Mutex<State>>,
}

#[derive(Debug)]
struct State {
    product: String,
    user_agent: String,
    missing: HashSet<String>,
    malformed: HashSet<String>,
    commands: Vec<(String, Option<String>)>,
    subscriptions: Vec<(String, Option<String>)>,
    events: HashMap<String, Vec<Value>>,
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
                subscriptions: Vec::new(),
                events: HashMap::new(),
            })),
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

    fn response(&self, scope: &CommandScope, method: &str) -> Result<Value, TransportError> {
        let mut state = self.state.lock().unwrap();
        let session = match scope {
            CommandScope::Browser => None,
            CommandScope::Session(session) => Some(session.as_str().to_owned()),
        };
        state.commands.push((method.to_owned(), session.clone()));
        if state.missing.contains(method) {
            return Err(TransportError::CommandFailed);
        }
        if state.malformed.contains(method) {
            return Ok(json!("malformed-response"));
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
        _params: Value,
    ) -> TransportFuture<'_, Result<Value, TransportError>> {
        let result = self.response(scope, method);
        Box::pin(async move { result })
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
            let params = self
                .state
                .lock()
                .unwrap()
                .events
                .remove(method)
                .unwrap_or_default();
            Ok(Box::new(ScriptedEvents {
                method: method.to_owned(),
                params,
                index: 0,
            }) as Box<dyn TransportEvents>)
        };
        Box::pin(async move { result })
    }

    fn close_reason(&self) -> Option<krometrail_cdp::TransportClose> {
        None
    }

    fn is_closed(&self) -> bool {
        false
    }
}

struct ScriptedEvents {
    method: String,
    params: Vec<Value>,
    index: usize,
}

impl TransportEvents for ScriptedEvents {
    fn next(&mut self) -> TransportFuture<'_, Result<Option<NamedEvent>, TransportError>> {
        let event = self.params.get(self.index).cloned();
        self.index += usize::from(event.is_some());
        let method = self.method.clone();
        Box::pin(async move { Ok(event.map(|params| NamedEvent { method, params })) })
    }
}
