//! Small, domain-owned error foundation shared by validated core contracts.
//!
//! The structured error contract is extended by the ports story. Keeping the
//! result alias and error type here now means domain constructors do not grow a
//! second, incompatible error vocabulary later.

use std::fmt;

use serde::{Deserialize, Serialize};

/// The stable categories needed by the core domain before infrastructure
/// errors and retry advice are introduced.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidInput,
    InvalidLifecycleTransition,
    InvalidTime,
    Unsupported,
}

/// A domain validation failure. Infrastructure-specific context is added by
/// the ports layer without changing the domain constructors' result type.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct KrometrailError {
    pub code: ErrorCode,
    pub message: String,
}

impl KrometrailError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for KrometrailError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message)
    }
}

impl std::error::Error for KrometrailError {}

pub type Result<T, E = KrometrailError> = std::result::Result<T, E>;

pub(crate) fn invalid(message: impl Into<String>) -> KrometrailError {
    KrometrailError::new(ErrorCode::InvalidInput, message)
}

pub(crate) fn invalid_time(message: impl Into<String>) -> KrometrailError {
    KrometrailError::new(ErrorCode::InvalidTime, message)
}

pub(crate) fn invalid_transition(message: impl Into<String>) -> KrometrailError {
    KrometrailError::new(ErrorCode::InvalidLifecycleTransition, message)
}
