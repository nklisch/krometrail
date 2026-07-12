//! Disposable CDP qualification laboratory. Nothing in this module is a production adapter.

#[cfg(feature = "cdp-spike-cdpkit")]
pub mod cdpkit_adapter;
#[cfg(feature = "cdp-spike-cdpkit")]
pub mod chrome_harness;
pub mod contract;
pub mod error;
pub mod evidence;
pub mod fake;
#[cfg(feature = "cdp-spike-cdpkit")]
pub mod fixture_server;
pub mod scenarios;
pub mod scripted_peer;

pub use contract::{
    EventStream, NamedEventParams, SpikeFuture, SpikeTransport, SpikeTransportFactory,
    TransportScope,
};
pub use error::{SpikeError, SpikeErrorCode};
pub use evidence::{
    BrowserEvidence, CandidateIdentity, EvidenceDigest, FixtureEvidence, GateConfiguration,
    GateResult, GateStatus, RSS_SAMPLE_INTERVAL_SECONDS, RSS_WARMUP_SECONDS, SanitizedEnvironment,
    SourceIdentity, TransportDecision, TransportDecisionV1, TransportEvidenceV1, TransportGateId,
    decide, rss_measurements_are_valid, sanitize_evidence, validate_evidence, write_json_schema,
};
pub use fake::{FakeTransport, FakeTransportFactory};
pub use scenarios::{ScenarioEvidence, run_transport_scenarios};
pub use scripted_peer::{ScriptedCdpPeer, ScriptedMessage};
