//! The exact cdpkit 0.4.0 implementation of the transport seam.
//!
//! This is the only production module that names cdpkit. The adapter owns one live connection and
//! multiplexes browser/session requests; reconnect and target reconstruction remain outside it.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use cdpkit::{CDP, CdpError, Sender};
use futures_util::StreamExt;

use super::{
    CdpTransport, CdpTransportFactory, CommandScope, NamedEvent, TransportClose, TransportEvents,
    TransportFuture, TransportSessionId,
};
use crate::LocalCdpEndpoint;
use crate::transport::error::TransportError;

type JsonStream = cdpkit::EventStream<serde_json::Value>;

#[derive(Clone, Debug, Default)]
pub struct CdpkitTransportFactory {
    command_timeout: Option<Duration>,
}

impl CdpkitTransportFactory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_command_timeout(mut self, timeout: Duration) -> Self {
        self.command_timeout = Some(timeout);
        self
    }
}

impl CdpTransportFactory for CdpkitTransportFactory {
    fn connect(
        &self,
        browser_websocket_url: &str,
    ) -> TransportFuture<'_, Result<Arc<dyn CdpTransport>, TransportError>> {
        self.connect_url(browser_websocket_url.to_owned())
    }

    fn connect_endpoint(
        &self,
        endpoint: &LocalCdpEndpoint,
    ) -> TransportFuture<'_, Result<Arc<dyn CdpTransport>, TransportError>> {
        let url = match endpoint.websocket_dial_url() {
            Ok(url) => url.to_string(),
            Err(error) => return Box::pin(std::future::ready(Err(map_endpoint_error(error)))),
        };
        self.connect_url(url)
    }
}

impl CdpkitTransportFactory {
    fn connect_url(
        &self,
        url: String,
    ) -> TransportFuture<'_, Result<Arc<dyn CdpTransport>, TransportError>> {
        let timeout = self.command_timeout;
        Box::pin(async move {
            let cdp = CDP::connect_ws(&url).await.map_err(|error| {
                tracing::debug!(error = ?error, "cdpkit connection failed");
                map_error(error)
            })?;
            if let Some(timeout) = timeout {
                cdp.set_command_timeout(timeout);
            }
            Ok(Arc::new(CdpkitTransport { cdp }) as Arc<dyn CdpTransport>)
        })
    }
}

pub struct CdpkitTransport {
    cdp: CDP,
}

impl CdpkitTransport {
    pub async fn attach_flat_page(
        &self,
        target_id: &str,
    ) -> Result<TransportSessionId, TransportError> {
        let response = self
            .cdp
            .send_cmd(
                cdpkit::target::methods::AttachToTarget::new(target_id.to_owned())
                    .with_flatten(true),
            )
            .await
            .map_err(|error| {
                tracing::debug!(error = ?error, "cdpkit typed attach failed");
                map_error(error)
            })?;
        TransportSessionId::new(response.session_id)
    }
}

impl CdpTransport for CdpkitTransport {
    fn send_raw(
        &self,
        scope: &CommandScope,
        method: &str,
        params: serde_json::Value,
    ) -> TransportFuture<'_, Result<serde_json::Value, TransportError>> {
        let invalid = method.trim().is_empty();
        let method = method.to_owned();
        let scope = scope.clone();
        Box::pin(async move {
            if invalid {
                return Err(TransportError::InvalidInput);
            }
            let result = match &scope {
                CommandScope::Browser => self.cdp.send_raw(&method, params).await,
                CommandScope::Session(session) => {
                    self.cdp
                        .owned_session(session.as_str())
                        .send_raw(&method, params)
                        .await
                }
            };
            result.map_err(|error| {
                tracing::debug!(error = ?error, method, "cdpkit raw command failed");
                map_error(error)
            })
        })
    }

    fn subscribe_named(
        &self,
        scope: &CommandScope,
        method: &str,
    ) -> TransportFuture<'_, Result<Box<dyn TransportEvents>, TransportError>> {
        let invalid = method.trim().is_empty();
        let name = method.to_owned();
        let stream = match scope {
            CommandScope::Browser => {
                (!invalid).then(|| self.cdp.event_stream::<serde_json::Value>(method))
            }
            CommandScope::Session(session) => (!invalid).then(|| {
                self.cdp
                    .owned_session(session.as_str())
                    .event_stream::<serde_json::Value>(method)
            }),
        };
        Box::pin(async move {
            let stream = stream.ok_or(TransportError::InvalidInput)?;
            Ok(Box::new(CdpkitEvents {
                method: name,
                stream,
            }) as Box<dyn TransportEvents>)
        })
    }

    fn close_reason(&self) -> Option<TransportClose> {
        self.cdp.close_reason().map(|reason| {
            let reason = match reason {
                cdpkit::CloseReason::Normal => "normal",
                cdpkit::CloseReason::Remote => "remote",
                cdpkit::CloseReason::Error(_) => "error",
                _ => "unknown",
            };
            TransportClose::new(reason)
        })
    }

    fn is_closed(&self) -> bool {
        self.cdp.is_closed()
    }
}

struct CdpkitEvents {
    method: String,
    stream: JsonStream,
}

impl TransportEvents for CdpkitEvents {
    fn next(&mut self) -> TransportFuture<'_, Result<Option<NamedEvent>, TransportError>> {
        Box::pin(async move {
            match self.stream.next().await {
                Some(params) => Ok(Some(NamedEvent {
                    method: self.method.clone(),
                    params,
                    received_at: Some(Instant::now()),
                })),
                None => Err(TransportError::SubscriptionClosed),
            }
        })
    }
}

fn map_endpoint_error(error: crate::EndpointError) -> TransportError {
    tracing::debug!(error = ?error, "validated CDP endpoint cannot be dialed");
    TransportError::ConnectFailed
}

fn map_error(error: CdpError) -> TransportError {
    match error {
        CdpError::ConnectionClosed | CdpError::ChannelClosed => TransportError::Disconnected,
        CdpError::WebSocket(_)
        | CdpError::Io(_)
        | CdpError::DiscoveryTimeout
        | CdpError::HandshakeTimeout
        | CdpError::HttpStatus(_)
        | CdpError::InvalidDiscoveryResponse(_) => TransportError::ConnectFailed,
        CdpError::Protocol { .. } | CdpError::Serialization(_) => TransportError::Protocol,
        // A timeout is not a command failure: the browser never answered, so
        // the command may still be pending (e.g. behind an unresolved
        // permission decision). Collapsing it into CommandFailed hid the #8
        // clipboard root cause.
        CdpError::Timeout => TransportError::Timeout,
        _ => TransportError::CommandFailed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn adapter_is_send_sync_and_does_not_advertise_reconnect() {
        assert_send_sync::<CdpkitTransport>();
        assert_send_sync::<CdpkitTransportFactory>();
    }

    /// The #8 root-cause fence: a cdpkit command timeout maps to the distinct
    /// timeout category, never to the answered-rejection CommandFailed class.
    #[test]
    fn command_timeout_keeps_its_own_transport_category() {
        assert_eq!(map_error(CdpError::Timeout), TransportError::Timeout);
        assert!(!TransportError::Timeout.is_retryable());
        assert_ne!(map_error(CdpError::Timeout), TransportError::CommandFailed);
    }
}
