//! Stable transport-private error categories.
//!
//! cdpkit's source error is intentionally not stored in these values. It is available to a
//! `debug` span at the adapter boundary, while callers receive only a safe category and message.

use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq, Error)]
pub enum TransportError {
    #[error("transport input is invalid")]
    InvalidInput,
    #[error("transport connection failed")]
    ConnectFailed,
    #[error("transport command failed")]
    CommandFailed,
    #[error("transport protocol response is invalid")]
    Protocol,
    #[error("transport disconnected")]
    Disconnected,
    #[error("transport event subscription closed")]
    SubscriptionClosed,
    #[error("transport is closed")]
    Closed,
}

impl TransportError {
    pub const fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::ConnectFailed | Self::Disconnected | Self::SubscriptionClosed | Self::Closed
        )
    }
}
