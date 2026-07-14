//! Canonical, browser-agnostic contracts for Krometrail's temporal benchmark corpus.
//!
//! This crate contains committed benchmark definitions only. It does not launch a browser,
//! capture frames, invoke models, or read the filesystem; those are later adapters.

mod canonical;
mod corpus;
mod error;

pub use canonical::{canonical_json, sha256_prefixed};
pub use corpus::{
    BENCHMARK_ID, BENCHMARK_SCHEMA_VERSION, BenchmarkDefinition, CaseDefinition, CaseFamily,
    CaseIntent, DEVICE_SCALE_FACTOR_MILLI, DURATIONS_MS, DurationMode, FIXTURE_NAME, FIXTURE_ROOT,
    FixtureFile, FixtureIdentity, PhaseBoundary, PhaseDefinition, Rect, TimeInterval,
    TimingDefinition, VIEWPORT_HEIGHT, VIEWPORT_WIDTH,
};
pub use error::{ContractError, Result};

/// Returns the generated JSON Schema for the one current benchmark definition contract.
pub fn benchmark_definition_schema() -> schemars::Schema {
    schemars::schema_for!(BenchmarkDefinition)
}
