use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::evidence::TransportGateId;

/// Named phases make a timeout actionable instead of collapsing the whole gate into one
/// undifferentiated deadline. This is spike infrastructure state, not a production lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QualificationStage {
    Initializing,
    CandidateContract,
    CandidateConnect,
    CandidateRouting,
    CandidateDrift,
    CandidateScreencast,
    CandidateLifecycle,
    CandidateRebuild,
    ChromeStartup,
    BrowserConnect,
    TargetSetup,
    TypedProbe,
    RoutingSubscriptions,
    ProtocolDrift,
    ScreencastStart,
    ScreencastFrameReceive,
    ScreencastAck,
    Disconnect,
    Rebuild,
    Evidence,
}

impl std::fmt::Display for QualificationStage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

#[derive(Clone, Debug)]
pub struct StageTracker {
    current: Arc<Mutex<QualificationStage>>,
}

impl StageTracker {
    pub fn new(initial: QualificationStage) -> Self {
        Self {
            current: Arc::new(Mutex::new(initial)),
        }
    }

    pub fn set(&self, stage: QualificationStage) {
        *self
            .current
            .lock()
            .expect("qualification stage mutex poisoned") = stage;
    }

    pub fn current(&self) -> QualificationStage {
        *self
            .current
            .lock()
            .expect("qualification stage mutex poisoned")
    }
}

/// Errors deliberately exposed by the disposable qualification boundary.
#[derive(Clone, Debug, Eq, PartialEq, Error, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[error("{code:?} [{stage:?}]: {message}")]
pub struct SpikeError {
    pub code: SpikeErrorCode,
    pub message: String,
    pub gate: Option<TransportGateId>,
    pub retryable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<QualificationStage>,
}

impl SpikeError {
    pub fn new(code: SpikeErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            gate: None,
            retryable: false,
            stage: None,
        }
    }

    pub fn at_stage(mut self, stage: QualificationStage) -> Self {
        self.stage = Some(stage);
        self
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
