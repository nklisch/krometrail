use std::collections::VecDeque;

use super::error::{SpikeError, SpikeErrorCode};

/// The in-memory socket pair is a real WebSocket framing boundary over Tokio's deterministic
/// duplex stream. It lets candidate adapters be exercised without binding a machine port.
pub type ScriptedWebSocket = tokio_tungstenite::WebSocketStream<tokio::io::DuplexStream>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScriptedMessage {
    Command { scope: String, method: String },
    Response { request_id: u64 },
    Event { scope: String, method: String },
    Detach { session_id: String },
    Disconnect,
    Reconnect,
}

/// A local deterministic peer model. It advances only when the harness explicitly consumes
/// the next scripted message; no wall-clock ordering is involved.
#[derive(Debug, Default)]
pub struct ScriptedCdpPeer {
    messages: VecDeque<ScriptedMessage>,
    consumed: Vec<ScriptedMessage>,
}

impl ScriptedCdpPeer {
    pub fn new(messages: impl IntoIterator<Item = ScriptedMessage>) -> Self {
        Self {
            messages: messages.into_iter().collect(),
            consumed: Vec::new(),
        }
    }

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

    pub fn push(&mut self, message: ScriptedMessage) {
        self.messages.push_back(message);
    }

    pub fn advance(&mut self) -> Result<ScriptedMessage, SpikeError> {
        let message = self
            .messages
            .pop_front()
            .ok_or_else(|| SpikeError::new(SpikeErrorCode::Invariant, "scripted peer exhausted"))?;
        self.consumed.push(message.clone());
        Ok(message)
    }

    pub fn expect(&mut self, expected: &ScriptedMessage) -> Result<(), SpikeError> {
        let actual = self.advance()?;
        if &actual != expected {
            return Err(SpikeError::new(
                SpikeErrorCode::Routing,
                format!("scripted peer expected {expected:?}, received {actual:?}"),
            ));
        }
        Ok(())
    }

    pub fn consumed(&self) -> &[ScriptedMessage] {
        &self.consumed
    }

    pub fn is_exhausted(&self) -> bool {
        self.messages.is_empty()
    }
}
