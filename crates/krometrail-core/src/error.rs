//! Stable, source-safe failures shared by domain contracts and infrastructure ports.
//!
//! Adapters map their private error chains into this vocabulary at the boundary. The
//! public value deliberately carries no source error, debug representation, path, or
//! credential-bearing detail.

use std::fmt;

use serde::{Deserialize, Serialize, de::Error as _};

use crate::{
    ids::{InteractionId, SessionId, TargetId},
    time::SessionRange,
};

define_stable_enum! {
    /// The stable categories exposed at the core boundary.
    pub enum ErrorCode {
        InvalidInput => "invalid_input",
        InvalidLifecycleTransition => "invalid_lifecycle_transition",
        InvalidTime => "invalid_time",
        NotFound => "not_found",
        Unsupported => "unsupported",
        BrowserDisconnected => "browser_disconnected",
        BrowserNotFound => "browser_not_found",
        BrowserLaunchFailed => "browser_launch_failed",
        BrowserProcessTerminated => "browser_process_terminated",
        BrowserCompatibilityFailed => "browser_compatibility_failed",
        ProfileInUse => "profile_in_use",
        TargetFailed => "target_failed",
        ReconnectExhausted => "reconnect_exhausted",
        Cancelled => "cancelled",
        ShutdownIncomplete => "shutdown_incomplete",
        CaptureRejected => "capture_rejected",
        PersistenceFailed => "persistence_failed",
        BudgetExhausted => "budget_exhausted",
        Internal => "internal",
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetryAdvice {
    #[default]
    Never,
    Safe,
    AfterRecovery,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ErrorContext {
    pub session_id: Option<SessionId>,
    pub target_id: Option<TargetId>,
    pub interaction_id: Option<InteractionId>,
    pub range: Option<SessionRange>,
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[error("text must not be empty or whitespace-only")]
pub struct EmptyTextError;

/// A validated user-facing string. It prevents an error boundary from emitting
/// an empty explanation or recovery action while preserving exact serde shape.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct NonEmptyText(Box<str>);

impl NonEmptyText {
    pub fn new(value: impl Into<String>) -> std::result::Result<Self, EmptyTextError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(EmptyTextError);
        }
        Ok(Self(value.into_boxed_str()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for NonEmptyText {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(|_| D::Error::custom("text must not be empty or whitespace-only"))
    }
}

impl fmt::Display for NonEmptyText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A stable, serializable boundary failure.
///
/// `KrometrailError` intentionally has no source-error field. Infrastructure
/// adapters can retain their private cause in local logs, but cannot accidentally
/// serialize arbitrary implementation details to callers.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error, Serialize, Deserialize)]
#[error("{code}: {message}")]
pub struct KrometrailError {
    pub code: ErrorCode,
    pub message: NonEmptyText,
    #[serde(default)]
    pub context: ErrorContext,
    #[serde(default)]
    pub retry: RetryAdvice,
    #[serde(default)]
    pub recovery: Option<NonEmptyText>,
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl ErrorCode {
    pub const BROWSER_SESSION_CODES: &'static [Self] = &[
        Self::BrowserNotFound,
        Self::BrowserLaunchFailed,
        Self::BrowserProcessTerminated,
        Self::BrowserCompatibilityFailed,
        Self::ProfileInUse,
        Self::TargetFailed,
        Self::ReconnectExhausted,
        Self::Cancelled,
        Self::ShutdownIncomplete,
    ];

    pub const fn is_browser_session_failure(self) -> bool {
        matches!(
            self,
            Self::BrowserNotFound
                | Self::BrowserLaunchFailed
                | Self::BrowserProcessTerminated
                | Self::BrowserCompatibilityFailed
                | Self::ProfileInUse
                | Self::TargetFailed
                | Self::ReconnectExhausted
                | Self::Cancelled
                | Self::ShutdownIncomplete
        )
    }

    pub const fn default_retry(self) -> RetryAdvice {
        match self {
            Self::BrowserNotFound
            | Self::BrowserLaunchFailed
            | Self::BrowserCompatibilityFailed
            | Self::ProfileInUse
            | Self::ReconnectExhausted => RetryAdvice::AfterRecovery,
            Self::TargetFailed => RetryAdvice::Safe,
            _ => RetryAdvice::Never,
        }
    }

    pub const fn default_recovery(self) -> Option<&'static str> {
        match self {
            Self::BrowserNotFound => Some("install Chrome or Chromium, then retry"),
            Self::BrowserLaunchFailed => {
                Some("check the browser installation and profile, then retry")
            }
            Self::BrowserProcessTerminated => Some("start a new browser session"),
            Self::BrowserCompatibilityFailed => Some("use a compatible Chrome renderer and retry"),
            Self::ProfileInUse => Some("close the other session using this profile, then retry"),
            Self::TargetFailed => Some("refresh the target or choose another page"),
            Self::ReconnectExhausted => Some("check the browser and start a new session"),
            Self::Cancelled => Some("start the operation again if it is still needed"),
            Self::ShutdownIncomplete => {
                Some("inspect the browser process before starting another session")
            }
            _ => None,
        }
    }
}

impl KrometrailError {
    pub fn new(code: ErrorCode, message: NonEmptyText) -> Self {
        Self {
            code,
            message,
            context: ErrorContext::default(),
            retry: RetryAdvice::Never,
            recovery: None,
        }
    }

    pub fn from_browser_failure(code: ErrorCode, message: NonEmptyText) -> Self {
        let mut error = Self::new(code, message).with_retry(code.default_retry());
        if let Some(recovery) = code.default_recovery() {
            error = error.with_recovery(safe_text(recovery));
        }
        error
    }

    pub fn with_context(mut self, context: ErrorContext) -> Self {
        self.context = context;
        self
    }

    pub fn with_retry(mut self, retry: RetryAdvice) -> Self {
        self.retry = retry;
        self
    }

    pub fn with_recovery(mut self, recovery: NonEmptyText) -> Self {
        self.recovery = Some(recovery);
        self
    }
}

pub type Result<T, E = KrometrailError> = std::result::Result<T, E>;

pub(crate) fn invalid(message: impl Into<String>) -> KrometrailError {
    KrometrailError::new(ErrorCode::InvalidInput, safe_text(message))
}

pub(crate) fn invalid_time(message: impl Into<String>) -> KrometrailError {
    KrometrailError::new(ErrorCode::InvalidTime, safe_text(message))
}

pub(crate) fn invalid_transition(message: impl Into<String>) -> KrometrailError {
    KrometrailError::new(ErrorCode::InvalidLifecycleTransition, safe_text(message))
}

fn safe_text(message: impl Into<String>) -> NonEmptyText {
    // These helpers are only used with programmer-authored validation messages.
    // Keep the invariant centralized rather than allowing an unchecked String
    // into the public error type.
    NonEmptyText::new(message).expect("core validation errors must have a message")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ids::SessionId,
        time::{SessionRange, SessionTime},
    };

    const UUID: &str = "123e4567-e89b-12d3-a456-426614174000";

    #[test]
    fn structured_errors_round_trip_without_source_details() {
        let session_id = SessionId::from_uuid(UUID.parse().unwrap());
        let range = SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(10)).unwrap();
        let error = KrometrailError::new(
            ErrorCode::PersistenceFailed,
            NonEmptyText::new("recording could not be saved").unwrap(),
        )
        .with_context(ErrorContext {
            session_id: Some(session_id),
            target_id: None,
            interaction_id: None,
            range: Some(range),
        })
        .with_retry(RetryAdvice::AfterRecovery)
        .with_recovery(NonEmptyText::new("check the recording budget and retry").unwrap());

        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("persistence_failed"));
        assert!(!json.contains("source"));
        assert!(!json.contains("debug"));
        assert_eq!(
            serde_json::from_str::<KrometrailError>(&json).unwrap(),
            error
        );

        let legacy = r#"{"code":"invalid_input","message":"legacy validation"}"#;
        let decoded = serde_json::from_str::<KrometrailError>(legacy).unwrap();
        assert_eq!(decoded.message.as_str(), "legacy validation");
        assert_eq!(decoded.retry, RetryAdvice::Never);
    }

    #[test]
    fn every_error_code_round_trips_with_its_stable_name() {
        for code in ErrorCode::ALL {
            let encoded = serde_json::to_string(code).unwrap();
            assert_eq!(encoded, format!("\"{}\"", code.as_str()));
            assert_eq!(serde_json::from_str::<ErrorCode>(&encoded).unwrap(), *code);

            let error = KrometrailError::new(*code, NonEmptyText::new("boundary failure").unwrap());
            let error_json = serde_json::to_string(&error).unwrap();
            assert!(error_json.contains(code.as_str()));
            assert_eq!(
                serde_json::from_str::<KrometrailError>(&error_json).unwrap(),
                error
            );
        }
    }

    #[test]
    fn browser_error_registry_maps_every_stable_code_with_guidance() {
        assert_eq!(ErrorCode::BROWSER_SESSION_CODES.len(), 9);
        for code in ErrorCode::BROWSER_SESSION_CODES {
            assert!(code.is_browser_session_failure());
            let error = KrometrailError::from_browser_failure(
                *code,
                NonEmptyText::new("safe adapter summary").unwrap(),
            );
            assert_eq!(error.code, *code);
            assert!(error.recovery.is_some());
            let json = serde_json::to_string(&error).unwrap();
            assert!(json.contains(code.as_str()));
            assert!(!json.contains("source"));
        }
    }

    #[test]
    fn empty_boundary_text_is_rejected() {
        assert!(NonEmptyText::new("").is_err());
        assert!(NonEmptyText::new(" \n\t ").is_err());
        assert!(NonEmptyText::new("recover").is_ok());
        assert!(serde_json::from_str::<NonEmptyText>("\"\"").is_err());
    }
}
