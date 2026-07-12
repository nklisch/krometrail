use std::{
    collections::{BTreeSet, HashSet},
    sync::{Arc, Mutex},
};

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::net::TcpListener;
use tokio::sync::Notify;
use tokio_tungstenite::{WebSocketStream, accept_async, tungstenite::Message};

use super::error::{SpikeError, SpikeErrorCode};

/// The in-memory socket pair is a real WebSocket framing boundary over Tokio's deterministic
/// duplex stream. It lets candidate adapters be exercised without binding a machine port.
pub type ScriptedWebSocket = tokio_tungstenite::WebSocketStream<tokio::io::DuplexStream>;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum WireObservationKind {
    Command,
    Response,
    Event,
    ConnectionClosed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WireObservation {
    pub sequence: u64,
    pub connection: u64,
    pub kind: WireObservationKind,
    pub request_id: Option<u64>,
    pub method: Option<String>,
    pub session_id: Option<String>,
    pub params: Value,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RoutingMeasurements {
    pub commands: u64,
    pub events: u64,
    pub cross_delivery: u64,
}

#[derive(Debug, Default)]
struct WireState {
    next_connection: u64,
    next_sequence: u64,
    observations: Vec<WireObservation>,
}

/// A wire-connected controller. Unlike the former expected-message deque, this controller is
/// passive until the candidate sends a command and derives its assertions from recorded wire
/// observations. The server and the test inspect this same state.
#[derive(Clone, Debug, Default)]
pub struct ScriptedCdpPeer {
    state: Arc<Mutex<WireState>>,
    command_notify: Arc<Notify>,
}

impl ScriptedCdpPeer {
    pub fn empty() -> Self {
        Self::default()
    }

    pub async fn websocket_pair(capacity: usize) -> (ScriptedWebSocket, ScriptedWebSocket) {
        let (left, right) = tokio::io::duplex(capacity);
        (
            tokio_tungstenite::WebSocketStream::from_raw_socket(
                left,
                tokio_tungstenite::tungstenite::protocol::Role::Client,
                None,
            )
            .await,
            tokio_tungstenite::WebSocketStream::from_raw_socket(
                right,
                tokio_tungstenite::tungstenite::protocol::Role::Server,
                None,
            )
            .await,
        )
    }

    pub fn observations(&self) -> Vec<WireObservation> {
        self.state
            .lock()
            .expect("scripted peer mutex poisoned")
            .observations
            .clone()
    }

    pub fn connection_count(&self) -> u64 {
        self.state
            .lock()
            .expect("scripted peer mutex poisoned")
            .observations
            .iter()
            .filter(|observation| observation.kind == WireObservationKind::Command)
            .map(|observation| observation.connection)
            .max()
            .unwrap_or(0)
    }

    pub fn observed_routing(&self) -> RoutingMeasurements {
        let observations = self.observations();
        let mut commands = BTreeSet::new();
        let mut events = BTreeSet::new();
        let mut cross_delivery = 0;
        for observation in &observations {
            match observation.kind {
                WireObservationKind::Command
                    if observation.method.as_deref() == Some("Runtime.evaluate")
                        && observation.params.get("phase").is_none() =>
                {
                    if let (Some(session), Some(token)) = (
                        observation.session_id.as_deref(),
                        observation.params.get("token").and_then(Value::as_u64),
                    ) {
                        commands.insert((session.to_owned(), token));
                    }
                }
                WireObservationKind::Event
                    if observation.method.as_deref() == Some("Runtime.consoleAPICalled") =>
                {
                    if let (Some(envelope_session), Some((token_session, token))) = (
                        observation.session_id.as_deref(),
                        observation
                            .params
                            .get("token")
                            .and_then(Value::as_str)
                            .and_then(|token| token.rsplit_once('-'))
                            .and_then(|(session, token)| {
                                token.parse::<u64>().ok().map(|token| (session, token))
                            }),
                    ) {
                        if token_session != envelope_session {
                            cross_delivery += 1;
                        }
                        events.insert((token_session.to_owned(), token));
                    }
                }
                _ => {}
            }
        }
        let correlated = commands.intersection(&events).count() as u64;
        RoutingMeasurements {
            commands: correlated,
            events: correlated,
            cross_delivery,
        }
    }

    pub fn drift_methods_observed(&self) -> HashSet<String> {
        self.observations()
            .into_iter()
            .filter_map(|observation| {
                (observation.kind == WireObservationKind::Event
                    && matches!(
                        observation.method.as_deref(),
                        Some("Protocol.unknownEvent")
                            | Some("Runtime.additiveField")
                            | Some("Runtime.unknownEnum")
                    ))
                .then_some(observation.method)
                .flatten()
            })
            .collect()
    }

    pub fn event_before_response(&self, method: &str) -> bool {
        let observations = self.observations();
        let Some(command) = observations.iter().find(|observation| {
            observation.kind == WireObservationKind::Command
                && observation.method.as_deref() == Some(method)
                && observation.params.get("phase").and_then(Value::as_str)
                    == Some("event-before-response")
        }) else {
            return false;
        };
        let Some(response) = observations.iter().find(|observation| {
            observation.kind == WireObservationKind::Response
                && observation.connection == command.connection
                && observation.request_id == command.request_id
        }) else {
            return false;
        };
        observations.iter().any(|observation| {
            observation.kind == WireObservationKind::Event
                && observation.connection == command.connection
                && observation.sequence > command.sequence
                && observation.sequence < response.sequence
        })
    }

    pub async fn wait_for_command(&self, method: &str, phase: &str) -> Result<(), SpikeError> {
        loop {
            // Create the notification future before checking the snapshot. Otherwise a command
            // observed between the check and `notified().await` can lose its wake-up forever.
            let notified = self.command_notify.notified();
            if self.observations().iter().any(|observation| {
                observation.kind == WireObservationKind::Command
                    && observation.method.as_deref() == Some(method)
                    && observation.params.get("phase").and_then(Value::as_str) == Some(phase)
            }) {
                return Ok(());
            }
            notified.await;
        }
    }

    fn observe_command(&self, connection: u64, command: &Value) {
        let mut state = self.state.lock().expect("scripted peer mutex poisoned");
        let sequence = state.next_sequence();
        state.observations.push(WireObservation {
            sequence,
            connection,
            kind: WireObservationKind::Command,
            request_id: command.get("id").and_then(Value::as_u64),
            method: command
                .get("method")
                .and_then(Value::as_str)
                .map(str::to_owned),
            session_id: command
                .get("sessionId")
                .and_then(Value::as_str)
                .map(str::to_owned),
            params: command
                .get("params")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({})),
        });
        self.command_notify.notify_waiters();
    }

    fn observe_response(&self, connection: u64, request_id: u64) {
        let mut state = self.state.lock().expect("scripted peer mutex poisoned");
        let sequence = state.next_sequence();
        state.observations.push(WireObservation {
            sequence,
            connection,
            kind: WireObservationKind::Response,
            request_id: Some(request_id),
            method: None,
            session_id: None,
            params: serde_json::json!({}),
        });
    }

    fn observe_event(&self, connection: u64, method: &str, session_id: &str, params: &Value) {
        let mut state = self.state.lock().expect("scripted peer mutex poisoned");
        let sequence = state.next_sequence();
        state.observations.push(WireObservation {
            sequence,
            connection,
            kind: WireObservationKind::Event,
            request_id: None,
            method: Some(method.into()),
            session_id: Some(session_id.into()),
            params: params.clone(),
        });
    }

    fn observe_connection(&self, connection: u64) {
        let mut state = self.state.lock().expect("scripted peer mutex poisoned");
        let sequence = state.next_sequence();
        state.observations.push(WireObservation {
            sequence,
            connection,
            kind: WireObservationKind::ConnectionClosed,
            request_id: None,
            method: None,
            session_id: None,
            params: serde_json::json!({}),
        });
    }

    fn allocate_connection(&self) -> u64 {
        let mut state = self.state.lock().expect("scripted peer mutex poisoned");
        state.next_connection += 1;
        state.next_connection
    }
}

impl WireState {
    fn next_sequence(&mut self) -> u64 {
        self.next_sequence += 1;
        self.next_sequence
    }
}

/// A real loopback WebSocket server backed by the same observed controller used by the test.
pub struct ScriptedCdpServer {
    pub ws_url: String,
    controller: ScriptedCdpPeer,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    task: Option<tokio::task::JoinHandle<()>>,
    connections: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>>,
}

impl ScriptedCdpServer {
    pub async fn start() -> Result<Self, SpikeError> {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .map_err(io_error)?;
        let address = listener.local_addr().map_err(io_error)?;
        let controller = ScriptedCdpPeer::empty();
        let server_controller = controller.clone();
        let connections = Arc::new(Mutex::new(Vec::new()));
        let server_connections = Arc::clone(&connections);
        let (shutdown, mut stop) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            loop {
                let accepted = tokio::select! {
                    result = listener.accept() => result,
                    _ = &mut stop => return,
                };
                let Ok((stream, _)) = accepted else { return };
                let Ok(socket) = accept_async(stream).await else {
                    continue;
                };
                let connection = server_controller.allocate_connection();
                let connection_controller = server_controller.clone();
                let connection_task = tokio::spawn(async move {
                    serve_connection(socket, connection_controller, connection).await;
                });
                server_connections
                    .lock()
                    .expect("scripted connection mutex poisoned")
                    .push(connection_task);
            }
        });
        Ok(Self {
            ws_url: format!(
                "ws://127.0.0.1:{}/devtools/browser/scripted",
                address.port()
            ),
            controller,
            shutdown: Some(shutdown),
            task: Some(task),
            connections,
        })
    }

    pub fn controller(&self) -> ScriptedCdpPeer {
        self.controller.clone()
    }

    pub async fn shutdown(mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
        let connections = std::mem::take(
            &mut *self
                .connections
                .lock()
                .expect("scripted connection mutex poisoned"),
        );
        for connection in &connections {
            connection.abort();
        }
        for connection in connections {
            let _ = connection.await;
        }
    }
}

impl Drop for ScriptedCdpServer {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(task) = self.task.take() {
            task.abort();
        }
        let connections = self
            .connections
            .lock()
            .expect("scripted connection mutex poisoned");
        for connection in connections.iter() {
            connection.abort();
        }
    }
}

async fn serve_connection<S>(mut socket: WebSocketStream<S>, peer: ScriptedCdpPeer, connection: u64)
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    while let Some(Ok(Message::Text(text))) = socket.next().await {
        let Ok(command) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        peer.observe_command(connection, &command);
        let id = command.get("id").and_then(Value::as_u64).unwrap_or(0);
        let method = command.get("method").and_then(Value::as_str).unwrap_or("");
        let session_id = command.get("sessionId").and_then(Value::as_str);
        let params = command
            .get("params")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));

        if method == "Runtime.evaluate" {
            if let Some(session_id) = session_id {
                let event_params = serde_json::json!({
                    "type": "log",
                    "args": [],
                    "executionContextId": 1,
                    "timestamp": 0,
                    "token": format!("{}-{}", session_id, params.get("token").and_then(Value::as_u64).unwrap_or(0)),
                });
                if send_event(
                    &mut socket,
                    &peer,
                    connection,
                    "Runtime.consoleAPICalled",
                    session_id,
                    event_params,
                )
                .await
                .is_err()
                {
                    break;
                }
            }
            if params.get("phase").and_then(Value::as_str) == Some("detach-during-pending") {
                if let Some(session_id) = session_id {
                    let event_params = serde_json::json!({
                        "sessionId": session_id,
                        "targetId": "target-b",
                        "reason": "replaced",
                    });
                    let _ = send_event(
                        &mut socket,
                        &peer,
                        connection,
                        "Target.detachedFromTarget",
                        session_id,
                        event_params,
                    )
                    .await;
                }
                let _ = socket.close(None).await;
                break;
            }
        }

        if method == "Browser.getVersion" {
            if let Some(drift) = params.get("scripted_drift").and_then(Value::as_str) {
                let drift_params = match drift {
                    "Protocol.unknownEvent" => serde_json::json!({"kind":"unknown"}),
                    "Runtime.additiveField" => serde_json::json!({"known":true,"new_field":7}),
                    "Runtime.unknownEnum" => serde_json::json!({"value":"future-value"}),
                    _ => serde_json::json!({}),
                };
                let target = session_id.unwrap_or("session-a");
                if send_event(&mut socket, &peer, connection, drift, target, drift_params)
                    .await
                    .is_err()
                {
                    break;
                }
            }
        }

        let result = match method {
            "Target.createTarget" => {
                let target = params.get("url").and_then(Value::as_str).unwrap_or("");
                serde_json::json!({"targetId": if target.ends_with("/b") { "target-b" } else { "target-a" }})
            }
            "Target.attachToTarget" => {
                let target = params
                    .get("targetId")
                    .and_then(Value::as_str)
                    .unwrap_or("target");
                serde_json::json!({"sessionId": if target == "target-a" { "session-a" } else { "session-b" }})
            }
            "Browser.getVersion" => serde_json::json!({
                "scope":{"scope":"browser"},
                "protocolVersion":"1.3",
                "product":"Chrome/qualification",
                "revision":"r-qualification",
                "userAgent":"qualification",
                "jsVersion":"qualification"
            }),
            "Accessibility.getFullAXTree" => serde_json::json!({"nodes": []}),
            "Runtime.evaluate" => serde_json::json!({
                "token":session_id.unwrap_or("browser"),
                "result":{"type":"number","value":2,"description":"2"}
            }),
            "Page.screencastFrameAck" => {
                if let Some(session_id) = session_id {
                    let sequence = params.get("sessionId").and_then(Value::as_i64).unwrap_or(1);
                    if send_frame(&mut socket, &peer, connection, session_id, sequence)
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                serde_json::json!({})
            }
            "Page.startScreencast" => {
                if let Some(session_id) = session_id {
                    if send_frame(&mut socket, &peer, connection, session_id, 1)
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                serde_json::json!({})
            }
            _ => serde_json::json!({}),
        };
        let response = serde_json::json!({"id": id, "result": result});
        if socket
            .send(Message::Text(response.to_string().into()))
            .await
            .is_err()
        {
            break;
        }
        peer.observe_response(connection, id);
    }
    peer.observe_connection(connection);
}

async fn send_event<S>(
    socket: &mut WebSocketStream<S>,
    peer: &ScriptedCdpPeer,
    connection: u64,
    method: &str,
    session_id: &str,
    params: Value,
) -> Result<(), SpikeError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    socket
        .send(Message::Text(
            serde_json::json!({"method":method,"sessionId":session_id,"params":params})
                .to_string()
                .into(),
        ))
        .await
        .map_err(ws_error)?;
    peer.observe_event(connection, method, session_id, &params);
    Ok(())
}

async fn send_frame<S>(
    socket: &mut WebSocketStream<S>,
    peer: &ScriptedCdpPeer,
    connection: u64,
    session_id: &str,
    sequence: i64,
) -> Result<(), SpikeError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    send_event(
        socket,
        peer,
        connection,
        "Page.screencastFrame",
        session_id,
        serde_json::json!({
            "data":"Zg==",
            "metadata":{"pageScaleFactor":1,"offsetTop":0,"deviceWidth":1,"deviceHeight":1,"scrollOffsetX":0,"scrollOffsetY":0,"timestamp":0},
            "sessionId":sequence
        }),
    )
    .await
}

fn io_error(error: std::io::Error) -> SpikeError {
    SpikeError::new(SpikeErrorCode::Io, error.to_string())
}

fn ws_error(error: tokio_tungstenite::tungstenite::Error) -> SpikeError {
    SpikeError::new(SpikeErrorCode::Io, error.to_string())
}
