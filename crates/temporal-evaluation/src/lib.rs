//! Canonical, browser-agnostic contracts for Krometrail's temporal benchmark corpus.
//!
//! This crate contains committed benchmark definitions only. It does not launch a browser,
//! capture frames, invoke models, or read the filesystem; those are later adapters.

mod canonical;
mod conditions;
mod corpus;
mod error;
mod matrix;
mod prompts;
mod vocabulary;

pub use canonical::{canonical_json, sha256_prefixed};
pub use conditions::{
    ArtifactContract, ArtifactKind, ConditionId, ConditionInput, EvidenceCondition,
    RetrievalBudget, SourceIntervalPolicy, canonical_conditions,
};
pub use corpus::{
    BENCHMARK_ID, BENCHMARK_SCHEMA_VERSION, BenchmarkDefinition, CaseDefinition, CaseFamily,
    CaseIntent, DEVICE_SCALE_FACTOR_MILLI, DURATIONS_MS, DurationMode, FIXTURE_NAME, FIXTURE_ROOT,
    FixtureFile, FixtureIdentity, InputIdentities, PhaseBoundary, PhaseDefinition, Rect,
    TimeInterval, TimingDefinition, VIEWPORT_HEIGHT, VIEWPORT_WIDTH,
};
pub use error::{ContractError, Result};
pub use matrix::{
    CAPTURE_REPETITIONS, CaptureTrial, EvaluationStatus, INTERPRETATION_REPETITIONS,
    InterpretationTrial, MATRIX_SEED, MatrixDefinition, MatrixOrder, StatusRules,
};
pub use prompts::{
    AnswerKind, AnswerRegion, AnswerTruth, AnswerValidationContext, DebuggingAnswer,
    InterpretationAnswer, Judgment, MotionBehavior, PromptId, PromptSet, PromptTemplate,
    StateLabel, UncertaintyReason, parse_interpretation_answer, validate_debugging_answer,
    validate_interpretation_answer,
};
pub use vocabulary::{ScoringDimension, ScoringDimensionId, ScoringVocabulary};

/// Returns the generated JSON Schema for the one current benchmark definition contract.
pub fn benchmark_definition_schema() -> schemars::Schema {
    schemars::schema_for!(BenchmarkDefinition)
}
