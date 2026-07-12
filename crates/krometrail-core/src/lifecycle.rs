//! Exhaustive lifecycle transition tables.

use serde::{Deserialize, Serialize};

use crate::error::{Result, invalid_transition};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionLifecycle {
    Starting,
    Recording,
    Reconnecting,
    Stopping,
    Ended,
}

impl SessionLifecycle {
    pub fn transition(self, next: Self) -> Result<Self> {
        let valid = matches!(
            (self, next),
            (
                Self::Starting,
                Self::Recording | Self::Reconnecting | Self::Stopping
            ) | (Self::Recording, Self::Reconnecting | Self::Stopping)
                | (Self::Reconnecting, Self::Recording | Self::Stopping)
                | (Self::Stopping, Self::Ended)
        );
        if valid {
            Ok(next)
        } else {
            Err(invalid_transition(format!(
                "cannot transition session from {self:?} to {next:?}"
            )))
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetLifecycle {
    Discovered,
    Attached,
    Recording,
    Hidden,
    Closed,
    Failed,
}

impl TargetLifecycle {
    pub fn transition(self, next: Self) -> Result<Self> {
        let valid = matches!(
            (self, next),
            (
                Self::Discovered,
                Self::Attached | Self::Closed | Self::Failed
            ) | (
                Self::Attached,
                Self::Recording | Self::Hidden | Self::Closed | Self::Failed
            ) | (Self::Recording, Self::Hidden | Self::Closed | Self::Failed)
                | (Self::Hidden, Self::Recording | Self::Closed | Self::Failed)
        );
        if valid {
            Ok(next)
        } else {
            Err(invalid_transition(format!(
                "cannot transition target from {self:?} to {next:?}"
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ErrorCode;

    #[test]
    fn accepts_valid_session_paths_and_rejects_terminal_reentry() {
        assert_eq!(
            SessionLifecycle::Starting
                .transition(SessionLifecycle::Recording)
                .unwrap(),
            SessionLifecycle::Recording
        );
        assert_eq!(
            SessionLifecycle::Recording
                .transition(SessionLifecycle::Reconnecting)
                .unwrap(),
            SessionLifecycle::Reconnecting
        );
        assert_eq!(
            SessionLifecycle::Reconnecting
                .transition(SessionLifecycle::Recording)
                .unwrap(),
            SessionLifecycle::Recording
        );
        assert_eq!(
            SessionLifecycle::Recording
                .transition(SessionLifecycle::Stopping)
                .unwrap(),
            SessionLifecycle::Stopping
        );
        assert_eq!(
            SessionLifecycle::Stopping
                .transition(SessionLifecycle::Ended)
                .unwrap(),
            SessionLifecycle::Ended
        );
        let error = SessionLifecycle::Ended
            .transition(SessionLifecycle::Recording)
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidLifecycleTransition);
    }

    #[test]
    fn accepts_target_visibility_cycle_and_rejects_closed_target() {
        assert!(
            TargetLifecycle::Discovered
                .transition(TargetLifecycle::Attached)
                .is_ok()
        );
        assert!(
            TargetLifecycle::Attached
                .transition(TargetLifecycle::Recording)
                .is_ok()
        );
        assert!(
            TargetLifecycle::Recording
                .transition(TargetLifecycle::Hidden)
                .is_ok()
        );
        assert!(
            TargetLifecycle::Hidden
                .transition(TargetLifecycle::Recording)
                .is_ok()
        );
        assert!(
            TargetLifecycle::Closed
                .transition(TargetLifecycle::Recording)
                .is_err()
        );
    }
}
