//! Replaceable CDP transport seam.
//!
//! The seam deliberately exposes raw JSON commands and named event parameters. It does not
//! expose cdpkit handles, target lifecycle, reconnection, or screencast control.

use std::{future::Future, pin::Pin, sync::Arc};

use krometrail_core::NonEmptyText;

use crate::LocalCdpEndpoint;

#[cfg(feature = "cdpkit-transport")]
pub mod cdpkit;
pub mod error;

#[cfg(feature = "cdpkit-transport")]
pub use cdpkit::CdpkitTransportFactory;
pub use error::TransportError;

pub type TransportFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct TransportSessionId(String);

impl TransportSessionId {
    pub fn new(value: impl Into<String>) -> Result<Self, TransportError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(TransportError::InvalidInput);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommandScope {
    Browser,
    Session(TransportSessionId),
}

impl CommandScope {
    pub fn session(value: impl Into<String>) -> Result<Self, TransportError> {
        Ok(Self::Session(TransportSessionId::new(value)?))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NamedEvent {
    pub method: String,
    pub params: serde_json::Value,
    /// Receipt time at the production transport boundary. Test and alternate
    /// transports may leave this absent; consumers then use their local pump
    /// clock as the documented fallback.
    pub received_at: Option<std::time::Instant>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransportClose {
    pub reason: NonEmptyText,
}

impl TransportClose {
    #[cfg(feature = "cdpkit-transport")]
    fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: NonEmptyText::new(reason).expect("adapter close reasons are non-empty"),
        }
    }
}

pub trait TransportEvents: Send {
    fn next(&mut self) -> TransportFuture<'_, Result<Option<NamedEvent>, TransportError>>;
}

pub trait CdpTransport: Send + Sync {
    fn send_raw(
        &self,
        scope: &CommandScope,
        method: &str,
        params: serde_json::Value,
    ) -> TransportFuture<'_, Result<serde_json::Value, TransportError>>;
    fn subscribe_named(
        &self,
        scope: &CommandScope,
        method: &str,
    ) -> TransportFuture<'_, Result<Box<dyn TransportEvents>, TransportError>>;
    fn close_reason(&self) -> Option<TransportClose>;
    fn is_closed(&self) -> bool;
}

pub trait CdpTransportFactory: Send + Sync {
    fn connect(
        &self,
        browser_websocket_url: &str,
    ) -> TransportFuture<'_, Result<Arc<dyn CdpTransport>, TransportError>>;

    /// Connect a validated endpoint using its pinned socket address.
    ///
    /// Keeping this as a default method preserves the narrow test/adapter contract: alternate
    /// transports can continue to accept a protocol URL, while the production cdpkit adapter
    /// overrides it to avoid resolving the endpoint hostname again.
    fn connect_endpoint(
        &self,
        endpoint: &LocalCdpEndpoint,
    ) -> TransportFuture<'_, Result<Arc<dyn CdpTransport>, TransportError>> {
        self.connect(endpoint.browser_websocket_url().as_str())
    }
}
