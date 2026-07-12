use std::{future::Future, pin::Pin};

use futures_util::Stream;
use serde::{Deserialize, Serialize};

use super::{error::SpikeError, evidence::CandidateIdentity};

pub type SpikeFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
pub type EventStream = Pin<Box<dyn Stream<Item = Result<NamedEventParams, SpikeError>> + Send>>;

#[derive(
    Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case", tag = "scope")]
pub enum TransportScope {
    Browser,
    Session { session_id: String },
}

impl TransportScope {
    pub fn session(session_id: impl Into<String>) -> Self {
        Self::Session {
            session_id: session_id.into(),
        }
    }

    pub fn session_id(&self) -> Option<&str> {
        match self {
            Self::Browser => None,
            Self::Session { session_id } => Some(session_id),
        }
    }
}

/// The honest raw event boundary: a named event's parameters, not a wildcard or full envelope.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NamedEventParams {
    pub method: String,
    pub scope: TransportScope,
    pub params: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScreencastFrame {
    pub scope: TransportScope,
    pub sequence: i64,
    pub data: String,
    pub metadata: serde_json::Value,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TypedProbeEvidence {
    pub browser_version_observed: bool,
    pub page_enable_observed: bool,
    pub runtime_evaluate_observed: bool,
    pub accessibility_observed: bool,
    pub input_observed: bool,
}

/// Evidence produced by the candidate-only wire contract. It is deliberately separate from
/// real-Chrome measurements: Chrome cannot be instructed to emit unknown future protocol fields.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CandidateContractResults {
    pub drift_fixtures: u64,
    pub connection_survived: bool,
    pub routing_commands: u64,
    pub routing_events: u64,
    pub routing_cross_delivery: u64,
    pub event_before_response: bool,
    pub detach_during_pending: bool,
    pub pending_calls_closed: bool,
    pub subscriptions_closed: bool,
    pub socket_closed: bool,
    pub reconnect_connections: u64,
    pub sessions_rebuilt: u64,
}

/// Results and the exact observed wire trace are inseparable: a report that uses scripted
/// candidate evidence must carry both the digest and the measurements derived from that trace.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CandidateContractEvidence {
    pub trace_sha256: String,
    pub trace_observations: u64,
    pub results: CandidateContractResults,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DisconnectEvidence {
    pub reason: String,
    pub pending_calls_closed: bool,
    pub subscriptions_closed: bool,
}

pub trait SpikeTransportFactory: Send + Sync {
    fn candidate(&self) -> CandidateIdentity;
    fn connect<'a>(
        &'a self,
        browser_ws_url: &'a str,
    ) -> SpikeFuture<'a, Result<Box<dyn SpikeTransport>, SpikeError>>;
}

/// Candidate-neutral transport seam used only by the qualification harness.
pub trait SpikeTransport: Send + Sync {
    fn send_raw<'a>(
        &'a self,
        scope: &'a TransportScope,
        method: &'a str,
        params: serde_json::Value,
    ) -> SpikeFuture<'a, Result<serde_json::Value, SpikeError>>;
    fn subscribe_named<'a>(
        &'a self,
        scope: &'a TransportScope,
        method: &'a str,
    ) -> SpikeFuture<'a, Result<EventStream, SpikeError>>;
    fn run_typed_probe<'a>(
        &'a self,
        session: &'a TransportScope,
    ) -> SpikeFuture<'a, Result<TypedProbeEvidence, SpikeError>>;
    fn attach_flat_page<'a>(
        &'a self,
        target_id: &'a str,
    ) -> SpikeFuture<'a, Result<TransportScope, SpikeError>>;
    fn start_screencast<'a>(
        &'a self,
        session: &'a TransportScope,
    ) -> SpikeFuture<'a, Result<(), SpikeError>>;
    fn next_screencast_frame<'a>(
        &'a self,
        session: &'a TransportScope,
    ) -> SpikeFuture<'a, Result<ScreencastFrame, SpikeError>>;
    fn ack_screencast<'a>(
        &'a self,
        session: &'a TransportScope,
        sequence: i64,
    ) -> SpikeFuture<'a, Result<(), SpikeError>>;
    fn close_reason(&self) -> Option<DisconnectEvidence>;
}
