use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex},
};

use futures_util::stream;

use super::{
    contract::{
        EventStream, ScreencastFrame, SpikeFuture, SpikeTransport, SpikeTransportFactory,
        TransportScope, TypedProbeEvidence,
    },
    error::{SpikeError, SpikeErrorCode},
    evidence::CandidateIdentity,
};

#[derive(Debug)]
struct FakeState {
    connected: bool,
    sessions: BTreeMap<String, String>,
    events: BTreeMap<(TransportScope, String), VecDeque<super::contract::NamedEventParams>>,
    frames: BTreeMap<String, i64>,
    last_frame: BTreeMap<String, i64>,
    started: BTreeMap<String, bool>,
    acks: Vec<(String, i64)>,
    disconnect: Option<super::contract::DisconnectEvidence>,
}

/// Deterministic in-memory candidate used to prove the shared harness itself.
#[derive(Clone, Debug)]
pub struct FakeTransport {
    state: Arc<Mutex<FakeState>>,
}

impl FakeTransport {
    pub fn new() -> Self {
        let mut events = BTreeMap::new();
        for session_id in ["session-a", "session-b"] {
            let scope = TransportScope::session(session_id);
            let mut queue = VecDeque::new();
            for token in 0..100 {
                queue.push_back(super::contract::NamedEventParams {
                    method: "Runtime.consoleAPICalled".into(),
                    scope: scope.clone(),
                    params: serde_json::json!({ "token": format!("{session_id}-{token}") }),
                });
            }
            for (method, params) in [
                (
                    "Protocol.unknownEvent",
                    serde_json::json!({ "kind": "unknown" }),
                ),
                (
                    "Runtime.additiveField",
                    serde_json::json!({ "known": true, "new_field": 7 }),
                ),
                (
                    "Runtime.unknownEnum",
                    serde_json::json!({ "value": "future-value" }),
                ),
            ] {
                queue.push_back(super::contract::NamedEventParams {
                    method: method.into(),
                    scope: scope.clone(),
                    params,
                });
            }
            events.insert((scope, "Runtime.consoleAPICalled".into()), queue);
        }
        Self {
            state: Arc::new(Mutex::new(FakeState {
                connected: true,
                sessions: BTreeMap::new(),
                events,
                frames: BTreeMap::new(),
                last_frame: BTreeMap::new(),
                started: BTreeMap::new(),
                acks: Vec::new(),
                disconnect: None,
            })),
        }
    }

    pub fn disconnect(&self, reason: impl Into<String>) {
        let mut state = self.state.lock().expect("fake state mutex poisoned");
        state.connected = false;
        state.disconnect = Some(super::contract::DisconnectEvidence {
            reason: reason.into(),
            pending_calls_closed: true,
            subscriptions_closed: true,
        });
    }

    pub fn ack_log(&self) -> Vec<(String, i64)> {
        self.state
            .lock()
            .expect("fake state mutex poisoned")
            .acks
            .clone()
    }

    pub fn rebuild(&self) {
        let mut state = self.state.lock().expect("fake state mutex poisoned");
        state.connected = true;
        state.disconnect = None;
        state.sessions.clear();
        state.frames.clear();
        state.last_frame.clear();
        state.started.clear();
    }

    fn ensure_connected(state: &FakeState) -> Result<(), SpikeError> {
        if state.connected {
            Ok(())
        } else {
            Err(
                SpikeError::new(SpikeErrorCode::Disconnected, "fake peer is disconnected")
                    .retryable(),
            )
        }
    }
}

impl Default for FakeTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl SpikeTransport for FakeTransport {
    fn send_raw<'a>(
        &'a self,
        scope: &'a TransportScope,
        method: &'a str,
        _params: serde_json::Value,
    ) -> SpikeFuture<'a, Result<serde_json::Value, SpikeError>> {
        Box::pin(async move {
            let state = self.state.lock().expect("fake state mutex poisoned");
            Self::ensure_connected(&state)?;
            if let TransportScope::Session { session_id } = scope {
                if !state.sessions.contains_key(session_id) {
                    return Err(SpikeError::new(
                        SpikeErrorCode::Routing,
                        "unknown flat session",
                    ));
                }
            }
            Ok(serde_json::json!({
                "method": method,
                "scope": scope,
                "token": scope.session_id().unwrap_or("browser"),
            }))
        })
    }

    fn subscribe_named<'a>(
        &'a self,
        scope: &'a TransportScope,
        method: &'a str,
    ) -> SpikeFuture<'a, Result<EventStream, SpikeError>> {
        Box::pin(async move {
            let mut state = self.state.lock().expect("fake state mutex poisoned");
            Self::ensure_connected(&state)?;
            let queue = state
                .events
                .remove(&(scope.clone(), method.to_owned()))
                .unwrap_or_else(|| {
                    let params = match method {
                        "Protocol.unknownEvent" => serde_json::json!({ "kind": "unknown" }),
                        "Runtime.additiveField" => {
                            serde_json::json!({ "known": true, "new_field": 7 })
                        }
                        "Runtime.unknownEnum" => serde_json::json!({ "value": "future-value" }),
                        _ => return VecDeque::new(),
                    };
                    VecDeque::from([super::contract::NamedEventParams {
                        method: method.to_owned(),
                        scope: scope.clone(),
                        params,
                    }])
                });
            Ok(Box::pin(stream::iter(queue.into_iter().map(Ok))) as EventStream)
        })
    }

    fn run_typed_probe<'a>(
        &'a self,
        session: &'a TransportScope,
    ) -> SpikeFuture<'a, Result<TypedProbeEvidence, SpikeError>> {
        Box::pin(async move {
            let state = self.state.lock().expect("fake state mutex poisoned");
            Self::ensure_connected(&state)?;
            if !matches!(session, TransportScope::Session { .. }) {
                return Err(SpikeError::new(
                    SpikeErrorCode::Routing,
                    "typed probe requires a page session",
                ));
            }
            Ok(TypedProbeEvidence {
                browser_version_observed: true,
                page_enable_observed: true,
                runtime_evaluate_observed: true,
                accessibility_observed: true,
                input_observed: true,
            })
        })
    }

    fn attach_flat_page<'a>(
        &'a self,
        target_id: &'a str,
    ) -> SpikeFuture<'a, Result<TransportScope, SpikeError>> {
        Box::pin(async move {
            let mut state = self.state.lock().expect("fake state mutex poisoned");
            Self::ensure_connected(&state)?;
            let session_id = match target_id {
                "target-a" => "session-a",
                "target-b" => "session-b",
                other => {
                    return Err(SpikeError::new(
                        SpikeErrorCode::Routing,
                        format!("unknown target {other}"),
                    ));
                }
            };
            state.sessions.insert(session_id.into(), target_id.into());
            Ok(TransportScope::session(session_id))
        })
    }

    fn start_screencast<'a>(
        &'a self,
        session: &'a TransportScope,
    ) -> SpikeFuture<'a, Result<(), SpikeError>> {
        Box::pin(async move {
            let mut state = self.state.lock().expect("fake state mutex poisoned");
            Self::ensure_connected(&state)?;
            let session_id = session.session_id().ok_or_else(|| {
                SpikeError::new(
                    SpikeErrorCode::Routing,
                    "screencast requires a page session",
                )
            })?;
            if !state.sessions.contains_key(session_id) {
                return Err(SpikeError::new(
                    SpikeErrorCode::Routing,
                    "unknown flat session",
                ));
            }
            state.started.insert(session_id.into(), true);
            state.frames.insert(session_id.into(), 0);
            Ok(())
        })
    }

    fn next_screencast_frame<'a>(
        &'a self,
        session: &'a TransportScope,
    ) -> SpikeFuture<'a, Result<ScreencastFrame, SpikeError>> {
        Box::pin(async move {
            let mut state = self.state.lock().expect("fake state mutex poisoned");
            Self::ensure_connected(&state)?;
            let session_id = session.session_id().ok_or_else(|| {
                SpikeError::new(
                    SpikeErrorCode::Routing,
                    "screencast requires a page session",
                )
            })?;
            if !state.started.get(session_id).copied().unwrap_or(false) {
                return Err(SpikeError::new(
                    SpikeErrorCode::Invariant,
                    "screencast was not started",
                ));
            }
            let sequence = {
                let next = state.frames.entry(session_id.into()).or_insert(0);
                *next += 1;
                *next
            };
            state.last_frame.insert(session_id.into(), sequence);
            Ok(ScreencastFrame {
                scope: session.clone(),
                sequence,
                data: format!("frame-{session_id}-{sequence}"),
                metadata: serde_json::json!({ "width": 1, "height": 1 }),
            })
        })
    }

    fn ack_screencast<'a>(
        &'a self,
        session: &'a TransportScope,
        sequence: i64,
    ) -> SpikeFuture<'a, Result<(), SpikeError>> {
        Box::pin(async move {
            let mut state = self.state.lock().expect("fake state mutex poisoned");
            Self::ensure_connected(&state)?;
            let session_id = session.session_id().ok_or_else(|| {
                SpikeError::new(
                    SpikeErrorCode::Routing,
                    "screencast requires a page session",
                )
            })?;
            if state.last_frame.get(session_id) != Some(&sequence) {
                return Err(SpikeError::new(
                    SpikeErrorCode::Invariant,
                    "acknowledged frame is not the most recent frame",
                ));
            }
            state.acks.push((session_id.into(), sequence));
            Ok(())
        })
    }

    fn close_reason(&self) -> Option<super::contract::DisconnectEvidence> {
        self.state
            .lock()
            .expect("fake state mutex poisoned")
            .disconnect
            .clone()
    }
}

#[derive(Clone, Debug, Default)]
pub struct FakeTransportFactory;

impl SpikeTransportFactory for FakeTransportFactory {
    fn candidate(&self) -> CandidateIdentity {
        CandidateIdentity::fake()
    }

    fn connect<'a>(
        &'a self,
        _browser_ws_url: &'a str,
    ) -> SpikeFuture<'a, Result<Box<dyn SpikeTransport>, SpikeError>> {
        Box::pin(async { Ok(Box::new(FakeTransport::new()) as Box<dyn SpikeTransport>) })
    }
}
