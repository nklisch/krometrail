use std::{collections::BTreeSet, future::Future, pin::Pin};

use futures_util::Stream;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use super::{
    error::{SpikeError, SpikeErrorCode},
    evidence::CandidateIdentity,
};

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

/// Results that can be proved from the scripted WebSocket observations alone.
///
/// Keeping these fields in a wire-specific type prevents adapter convenience APIs from being
/// accidentally presented as protocol evidence. Values are projected from recorded envelopes,
/// not from scenario expectations.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CandidateWireResults {
    pub drift_fixtures: u64,
    pub drift_methods: Vec<String>,
    pub connection_survived: bool,
    pub routing_commands: u64,
    pub routing_events: u64,
    pub routing_cross_delivery: u64,
    pub event_before_response: bool,
    pub detach_during_pending: bool,
    pub socket_closed: bool,
    pub reconnect_connections: u64,
    pub sessions_rebuilt: u64,
}

/// Assertions exposed by the candidate runtime after the socket closes. These are deliberately
/// classified separately because a cdpkit close-status API is not a wire observation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CandidateRuntimeAssertions {
    pub pending_calls_closed: bool,
    pub subscriptions_closed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CandidateContractResults {
    pub wire: CandidateWireResults,
    pub runtime: CandidateRuntimeAssertions,
}

/// The small, stable fixture projection retained with decisive evidence.
///
/// The projection intentionally stores parsed method and params rather than filesystem paths or
/// raw runner output. Its ordered serialization is the fixture digest input, so validation can
/// recompute both the claims and the digest from the report itself.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CanonicalProtocolFixture {
    pub name: String,
    pub method: String,
    pub params: Value,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CanonicalWireObservationKind {
    Command,
    Response,
    Event,
    ConnectionClosed,
}

/// A sanitized wire projection, not an unbounded raw log. Sequence and connection numbers are
/// retained because ordering and reconnect assertions cannot be reconstructed from summaries.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CanonicalWireObservation {
    pub sequence: u64,
    pub connection: u64,
    pub kind: CanonicalWireObservationKind,
    pub request_id: Option<u64>,
    pub method: Option<String>,
    pub session_id: Option<String>,
    pub params: Value,
}

/// Canonical material from which every candidate-contract summary is derived.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CandidateContractTrace {
    pub fixtures: Vec<CanonicalProtocolFixture>,
    pub observations: Vec<CanonicalWireObservation>,
    pub runtime_assertions: CandidateRuntimeAssertions,
}

/// Results and the exact observed wire projection are inseparable: a report that uses scripted
/// candidate evidence must carry the material needed to independently recompute every summary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CandidateContractEvidence {
    pub fixture_sha256: String,
    pub trace_sha256: String,
    pub trace_observations: u64,
    pub results: CandidateContractResults,
    pub trace: CandidateContractTrace,
}

impl CandidateContractEvidence {
    pub fn from_trace(trace: CandidateContractTrace) -> Result<Self, SpikeError> {
        validate_candidate_contract_trace(&trace)?;
        Ok(Self {
            fixture_sha256: canonical_fixture_digest(&trace.fixtures)?,
            trace_sha256: canonical_trace_digest(&trace)?,
            trace_observations: trace.observations.len() as u64,
            results: recompute_candidate_contract_results(&trace)?,
            trace,
        })
    }
}

pub const MAX_CANDIDATE_TRACE_FIXTURES: usize = 16;
pub const MAX_CANDIDATE_TRACE_OBSERVATIONS: usize = 4_096;
const MAX_CANONICAL_VALUE_BYTES: usize = 16 * 1024;
const MAX_CANONICAL_TRACE_BYTES: usize = 2 * 1024 * 1024;
const MAX_CANONICAL_STRING_BYTES: usize = 512;

pub fn canonical_fixture_digest(
    fixtures: &[CanonicalProtocolFixture],
) -> Result<String, SpikeError> {
    canonical_digest(fixtures)
}

pub fn canonical_trace_digest(trace: &CandidateContractTrace) -> Result<String, SpikeError> {
    canonical_digest(trace)
}

/// Derive the candidate result object exclusively from the retained canonical projection.
pub fn recompute_candidate_contract_results(
    trace: &CandidateContractTrace,
) -> Result<CandidateContractResults, SpikeError> {
    validate_candidate_contract_trace(trace)?;

    let mut drift_methods = Vec::new();
    for observation in &trace.observations {
        if observation.kind == CanonicalWireObservationKind::Event
            && observation.session_id.as_deref() == Some("session-a")
            && trace.fixtures.iter().any(|fixture| {
                observation.method.as_deref() == Some(fixture.method.as_str())
                    && observation.params == fixture.params
            })
        {
            if let Some(method) = observation.method.clone() {
                drift_methods.push(method);
            }
        }
    }

    let last_drift = trace
        .observations
        .iter()
        .filter(|observation| {
            observation.kind == CanonicalWireObservationKind::Event
                && trace.fixtures.iter().any(|fixture| {
                    observation.method.as_deref() == Some(fixture.method.as_str())
                        && observation.params == fixture.params
                })
        })
        .max_by_key(|observation| observation.sequence);
    let connection_survived = last_drift.is_some_and(|last| {
        trace.observations.iter().any(|observation| {
            observation.connection == last.connection
                && observation.sequence > last.sequence
                && matches!(
                    observation.kind,
                    CanonicalWireObservationKind::Command | CanonicalWireObservationKind::Response
                )
        })
    });

    let mut commands = BTreeSet::new();
    let mut events = BTreeSet::new();
    let mut cross_delivery = 0_u64;
    for observation in &trace.observations {
        match observation.kind {
            CanonicalWireObservationKind::Command
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
            CanonicalWireObservationKind::Event
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

    let event_before_response = trace
        .observations
        .iter()
        .filter(|observation| {
            observation.kind == CanonicalWireObservationKind::Command
                && observation.method.as_deref() == Some("Runtime.evaluate")
                && observation.params.get("phase").and_then(Value::as_str)
                    == Some("event-before-response")
        })
        .any(|command| {
            trace.observations.iter().any(|response| {
                response.kind == CanonicalWireObservationKind::Response
                    && response.connection == command.connection
                    && response.request_id == command.request_id
                    && trace.observations.iter().any(|event| {
                        event.kind == CanonicalWireObservationKind::Event
                            && event.connection == command.connection
                            && event.sequence > command.sequence
                            && event.sequence < response.sequence
                    })
            })
        });

    let detach_during_pending = trace.observations.iter().any(|command| {
        command.kind == CanonicalWireObservationKind::Command
            && command.method.as_deref() == Some("Runtime.evaluate")
            && command.params.get("phase").and_then(Value::as_str) == Some("detach-during-pending")
            && trace.observations.iter().any(|event| {
                event.kind == CanonicalWireObservationKind::Event
                    && event.connection == command.connection
                    && event.method.as_deref() == Some("Target.detachedFromTarget")
                    && event.sequence > command.sequence
            })
    });

    let first_connection = trace
        .observations
        .iter()
        .map(|observation| observation.connection)
        .min()
        .unwrap_or(0);
    let sessions_rebuilt = trace
        .observations
        .iter()
        .filter(|observation| {
            observation.kind == CanonicalWireObservationKind::Command
                && observation.method.as_deref() == Some("Target.attachToTarget")
                && observation.connection > first_connection
                && observation.params.get("targetId").is_some()
        })
        .filter_map(|observation| {
            observation
                .params
                .get("targetId")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .collect::<BTreeSet<_>>()
        .len() as u64;

    Ok(CandidateContractResults {
        wire: CandidateWireResults {
            drift_fixtures: drift_methods.len() as u64,
            drift_methods,
            connection_survived,
            routing_commands: correlated,
            routing_events: correlated,
            routing_cross_delivery: cross_delivery,
            event_before_response,
            detach_during_pending,
            socket_closed: trace.observations.iter().any(|observation| {
                observation.kind == CanonicalWireObservationKind::ConnectionClosed
            }),
            reconnect_connections: trace
                .observations
                .iter()
                .map(|observation| observation.connection)
                .collect::<BTreeSet<_>>()
                .len() as u64,
            sessions_rebuilt,
        },
        runtime: trace.runtime_assertions.clone(),
    })
}

pub fn validate_candidate_contract_trace(trace: &CandidateContractTrace) -> Result<(), SpikeError> {
    if trace.fixtures.is_empty() || trace.fixtures.len() > MAX_CANDIDATE_TRACE_FIXTURES {
        return Err(trace_error(
            "candidate trace fixture projection is out of bounds",
        ));
    }
    if trace.observations.is_empty() || trace.observations.len() > MAX_CANDIDATE_TRACE_OBSERVATIONS
    {
        return Err(trace_error(
            "candidate trace observation projection is out of bounds",
        ));
    }
    let mut previous_sequence = 0;
    for fixture in &trace.fixtures {
        validate_string(&fixture.name, "fixture name")?;
        validate_string(&fixture.method, "fixture method")?;
        validate_value(&fixture.params, "fixture params")?;
    }
    for observation in &trace.observations {
        if observation.sequence == 0 || observation.sequence <= previous_sequence {
            return Err(trace_error(
                "candidate trace sequence is not strictly ordered",
            ));
        }
        if observation.connection == 0 {
            return Err(trace_error("candidate trace connection number is zero"));
        }
        previous_sequence = observation.sequence;
        if let Some(method) = &observation.method {
            validate_string(method, "wire method")?;
        }
        if let Some(session_id) = &observation.session_id {
            validate_string(session_id, "wire session")?;
        }
        validate_value(&observation.params, "wire params")?;
    }
    let encoded = canonical_json_bytes(trace)?;
    if encoded.len() > MAX_CANONICAL_TRACE_BYTES {
        return Err(trace_error(
            "candidate trace exceeds its bounded serialized size",
        ));
    }
    Ok(())
}

fn validate_string(value: &str, label: &str) -> Result<(), SpikeError> {
    if value.is_empty() || value.len() > MAX_CANONICAL_STRING_BYTES {
        return Err(trace_error(format!(
            "{label} is empty or exceeds its bounded size"
        )));
    }
    Ok(())
}

fn validate_value(value: &Value, label: &str) -> Result<(), SpikeError> {
    fn depth(value: &Value, current: usize) -> bool {
        if current > 16 {
            return false;
        }
        match value {
            Value::Array(values) => values.iter().all(|value| depth(value, current + 1)),
            Value::Object(values) => values.values().all(|value| depth(value, current + 1)),
            _ => true,
        }
    }
    if !depth(value, 0) {
        return Err(trace_error(format!(
            "{label} exceeds its maximum JSON depth"
        )));
    }
    let encoded = canonical_json_bytes(value)?;
    if encoded.len() > MAX_CANONICAL_VALUE_BYTES {
        return Err(trace_error(format!(
            "{label} exceeds its bounded JSON size"
        )));
    }
    Ok(())
}

fn canonical_digest<T: Serialize + ?Sized>(value: &T) -> Result<String, SpikeError> {
    Ok(format!(
        "sha256:{:x}",
        Sha256::digest(canonical_json_bytes(value)?)
    ))
}

fn canonical_json_bytes<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>, SpikeError> {
    let value = serde_json::to_value(value)
        .map_err(|error| trace_error(format!("cannot serialize canonical material: {error}")))?;
    let canonical = canonicalize_value(value);
    serde_json::to_vec(&canonical)
        .map_err(|error| trace_error(format!("cannot encode canonical material: {error}")))
}

fn canonicalize_value(value: Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.into_iter().map(canonicalize_value).collect()),
        Value::Object(values) => {
            let mut sorted = values
                .into_iter()
                .map(|(key, value)| (key, canonicalize_value(value)))
                .collect::<Vec<_>>();
            sorted.sort_by(|left, right| left.0.cmp(&right.0));
            let mut object = Map::new();
            for (key, value) in sorted {
                object.insert(key, value);
            }
            Value::Object(object)
        }
        other => other,
    }
}

fn trace_error(message: impl Into<String>) -> SpikeError {
    SpikeError::new(SpikeErrorCode::Evidence, message)
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
