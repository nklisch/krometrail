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
    CandidateContractEvidence, CandidateContractResults, CandidateContractTrace,
    CandidateRuntimeAssertions, CandidateWireResults, CanonicalProtocolFixture,
    CanonicalWireObservation, CanonicalWireObservationKind, EventStream, NamedEventParams,
    SpikeFuture, SpikeTransport, SpikeTransportFactory, TransportScope, canonical_fixture_digest,
    canonical_trace_digest, recompute_candidate_contract_results,
    validate_candidate_contract_trace,
};
pub use error::{QualificationStage, SpikeError, SpikeErrorCode, StageTracker};
pub use evidence::{
    BrowserEvidence, CandidateIdentity, FixtureEvidence, GateConfiguration, GateProvenance,
    GateResult, GateStatus, PlatformEvidence, RSS_SAMPLE_INTERVAL_SECONDS, RSS_WARMUP_SECONDS,
    SanitizedEnvironment, SourceAttestation, SourceFileAttestation, SourceIdentity,
    TransportDecision, TransportDecisionV1, TransportDecisionV2, TransportEvidenceV1,
    TransportEvidenceV2, TransportGateId, attest_relevant_source, attest_relevant_source_at,
    configuration_digest, decide, decide_from_files, decide_from_files_at, is_git_revision,
    resolve_repository_root, rss_measurements_are_valid, sanitize_evidence,
    validate_decisive_report, validate_decisive_report_at, validate_evidence,
    validate_source_attestation, write_json_schema,
};
pub use fake::{FakeTransport, FakeTransportFactory};
#[cfg(feature = "cdp-spike-cdpkit")]
pub use scenarios::run_candidate_wire_contract;
pub use scenarios::{ScenarioEvidence, run_transport_scenarios};
pub use scripted_peer::{
    ProtocolDriftFixture, RoutingMeasurements, ScriptedCdpPeer, ScriptedCdpServer, WireObservation,
    WireObservationKind, committed_protocol_fixtures, ordered_protocol_fixture_digest,
};
