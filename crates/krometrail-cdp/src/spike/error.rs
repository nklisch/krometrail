use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::evidence::TransportGateId;

/// Errors deliberately exposed by the disposable qualification boundary.
#[derive(Clone, Debug, Eq, PartialEq, Error, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[error("{code:?}: {message}")]
pub struct SpikeError {
    pub code: SpikeErrorCode,
    pub message: String,
    pub gate: Option<TransportGateId>,
    pub retryable: bool,
}

impl SpikeError {
    pub fn new(code: SpikeErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            gate: None,
            retryable: false,
        }
    }

    pub fn for_gate(
        code: SpikeErrorCode,
        gate: TransportGateId,
        message: impl Into<String>,
    ) -> Self {
        Self {
            gate: Some(gate),
            ..Self::new(code, message)
        }
    }

    pub fn retryable(mut self) -> Self {
        self.retryable = true;
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SpikeErrorCode {
    Connect,
    Command,
    Protocol,
    Routing,
    SubscriptionClosed,
    Disconnected,
    Deadline,
    Invariant,
    Io,
    Evidence,
}
