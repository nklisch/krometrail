//! Validated, non-owning local CDP endpoints.
//!
//! Endpoint validation is deliberately separate from connection establishment. This prevents a
//! malformed or remote endpoint from reaching a socket operation, and gives launch/supervision a
//! value that carries no credentials or mutable process state.

use std::{fmt, net::ToSocketAddrs};

use krometrail_core::NonEmptyText;
use thiserror::Error;
use url::Url;

#[cfg(feature = "cdpkit-transport")]
const MAX_DISCOVERY_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug, Eq, PartialEq, Error)]
pub enum EndpointError {
    #[error("CDP endpoint URL is invalid")]
    InvalidUrl,
    #[error("CDP endpoint uses an unsupported scheme")]
    UnsupportedScheme,
    #[error("CDP endpoint must not contain credentials")]
    Credentials,
    #[error("CDP endpoint must not contain a query or fragment")]
    QueryOrFragment,
    #[error("CDP endpoint must use an explicit port")]
    MissingPort,
    #[error("CDP endpoint host is not loopback")]
    NotLoopback,
    #[error("CDP discovery returned an invalid response")]
    InvalidDiscovery,
    #[error("CDP discovery failed")]
    DiscoveryFailed,
    #[error("CDP endpoint label could not be represented")]
    InvalidLabel,
}

/// An explicitly validated loopback CDP endpoint.
///
/// The HTTP origin is retained for status and future rediscovery; the WebSocket URL is the only
/// value passed to the transport. Both are private so callers cannot manufacture an unvalidated
/// endpoint or accidentally log a credential-bearing URL.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalCdpEndpoint {
    http_origin: Url,
    browser_websocket_url: Url,
    redacted_label: NonEmptyText,
}

impl LocalCdpEndpoint {
    /// Resolve an HTTP debugging endpoint through `/json/version`, or validate a direct WebSocket
    /// endpoint. All URL checks happen before the first network operation.
    #[cfg(feature = "cdpkit-transport")]
    pub async fn resolve(input: impl AsRef<str>) -> Result<Self, EndpointError> {
        let url = parse_and_validate(input.as_ref())?;
        match url.scheme() {
            "ws" => Self::from_validated_websocket(url),
            "http" => Self::resolve_http(url).await,
            _ => Err(EndpointError::UnsupportedScheme),
        }
    }

    /// Validate a direct WebSocket endpoint without opening a connection.
    pub fn from_websocket_url(input: impl AsRef<str>) -> Result<Self, EndpointError> {
        let url = parse_and_validate(input.as_ref())?;
        if url.scheme() != "ws" {
            return Err(EndpointError::UnsupportedScheme);
        }
        Self::from_validated_websocket(url)
    }

    /// Alias emphasizing that this constructor performs no readiness probe.
    pub fn validate_websocket(input: impl AsRef<str>) -> Result<Self, EndpointError> {
        Self::from_websocket_url(input)
    }

    pub fn http_origin(&self) -> &Url {
        &self.http_origin
    }

    pub fn browser_websocket_url(&self) -> &Url {
        &self.browser_websocket_url
    }

    pub fn redacted_label(&self) -> &str {
        self.redacted_label.as_str()
    }

    fn from_validated_websocket(url: Url) -> Result<Self, EndpointError> {
        ensure_loopback_resolves(&url)?;
        let http_origin = origin_for(&url, "http")?;
        let redacted_label = label_for(&url)?;
        Ok(Self {
            http_origin,
            browser_websocket_url: url,
            redacted_label,
        })
    }

    #[cfg(feature = "cdpkit-transport")]
    async fn resolve_http(url: Url) -> Result<Self, EndpointError> {
        ensure_loopback_resolves(&url)?;
        let response = fetch_version(&url).await?;
        let websocket = response
            .get("webSocketDebuggerUrl")
            .and_then(|value| value.as_str())
            .ok_or(EndpointError::InvalidDiscovery)?;
        let websocket = parse_and_validate(websocket)?;
        if websocket.scheme() != "ws" {
            return Err(EndpointError::UnsupportedScheme);
        }
        ensure_loopback_resolves(&websocket)?;
        let redacted_label = label_for(&url)?;
        Ok(Self {
            http_origin: origin_for(&url, "http")?,
            browser_websocket_url: websocket,
            redacted_label,
        })
    }
}

impl fmt::Display for LocalCdpEndpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.redacted_label())
    }
}

fn parse_and_validate(input: &str) -> Result<Url, EndpointError> {
    let url = Url::parse(input).map_err(|_| EndpointError::InvalidUrl)?;
    if !matches!(url.scheme(), "http" | "ws") {
        return Err(EndpointError::UnsupportedScheme);
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(EndpointError::Credentials);
    }
    if url.fragment().is_some() || url.query().is_some() {
        return Err(EndpointError::QueryOrFragment);
    }
    if url.host_str().is_none() {
        return Err(EndpointError::InvalidUrl);
    }
    if url.port().is_none() {
        return Err(EndpointError::MissingPort);
    }
    Ok(url)
}

fn ensure_loopback_resolves(url: &Url) -> Result<(), EndpointError> {
    let host = url.host_str().ok_or(EndpointError::InvalidUrl)?;
    let port = url.port().ok_or(EndpointError::MissingPort)?;
    if host.eq_ignore_ascii_case("localhost") {
        let addresses = (host, port)
            .to_socket_addrs()
            .map_err(|_| EndpointError::NotLoopback)?;
        if addresses
            .into_iter()
            .any(|address| address.ip().is_loopback())
        {
            return Ok(());
        }
        return Err(EndpointError::NotLoopback);
    }
    let ip = url.host().ok_or(EndpointError::InvalidUrl)?;
    match ip {
        url::Host::Ipv4(ip) if ip.is_loopback() => Ok(()),
        url::Host::Ipv6(ip) if ip.is_loopback() => Ok(()),
        _ => Err(EndpointError::NotLoopback),
    }
}

fn origin_for(url: &Url, scheme: &str) -> Result<Url, EndpointError> {
    let mut origin = url.clone();
    origin
        .set_scheme(scheme)
        .map_err(|_| EndpointError::InvalidUrl)?;
    origin.set_path("");
    origin.set_query(None);
    origin.set_fragment(None);
    Ok(origin)
}

fn label_for(url: &Url) -> Result<NonEmptyText, EndpointError> {
    let host = url.host_str().ok_or(EndpointError::InvalidUrl)?;
    let port = url.port().ok_or(EndpointError::MissingPort)?;
    let label = match url.host() {
        Some(url::Host::Ipv6(_)) => format!("[{host}]:{port}"),
        _ => format!("{host}:{port}"),
    };
    NonEmptyText::new(label).map_err(|_| EndpointError::InvalidLabel)
}

#[cfg(feature = "cdpkit-transport")]
async fn fetch_version(url: &Url) -> Result<serde_json::Value, EndpointError> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    let host = url.host_str().ok_or(EndpointError::InvalidUrl)?;
    let port = url.port().ok_or(EndpointError::MissingPort)?;
    let mut stream = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        TcpStream::connect((host, port)),
    )
    .await
    .map_err(|_| EndpointError::DiscoveryFailed)?
    .map_err(|_| EndpointError::DiscoveryFailed)?;
    let authority = match url.host() {
        Some(url::Host::Ipv6(_)) => format!("[{host}]:{port}"),
        _ => format!("{host}:{port}"),
    };
    let request =
        format!("GET /json/version HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\n\r\n");
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        stream.write_all(request.as_bytes()),
    )
    .await
    .map_err(|_| EndpointError::DiscoveryFailed)?
    .map_err(|_| EndpointError::DiscoveryFailed)?;
    let mut bytes = Vec::new();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let mut buffer = [0_u8; 8192];
            let count = stream.read(&mut buffer).await?;
            if count == 0 {
                break Ok::<(), std::io::Error>(());
            }
            bytes.extend_from_slice(&buffer[..count]);
            if bytes.len() > MAX_DISCOVERY_BYTES {
                break Err(std::io::Error::other("response too large"));
            }
        }
    })
    .await
    .map_err(|_| EndpointError::DiscoveryFailed)?
    .map_err(|_| EndpointError::InvalidDiscovery)?;
    let split = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or(EndpointError::InvalidDiscovery)?;
    let headers =
        std::str::from_utf8(&bytes[..split]).map_err(|_| EndpointError::InvalidDiscovery)?;
    if !headers.starts_with("HTTP/1.1 200") && !headers.starts_with("HTTP/1.0 200") {
        return Err(EndpointError::InvalidDiscovery);
    }
    serde_json::from_slice(&bytes[split + 4..]).map_err(|_| EndpointError::InvalidDiscovery)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_only_loopback_websocket_origins() {
        let endpoint =
            LocalCdpEndpoint::from_websocket_url("ws://127.0.0.1:9222/devtools/browser/id")
                .unwrap();
        assert_eq!(endpoint.redacted_label(), "127.0.0.1:9222");
        assert!(LocalCdpEndpoint::from_websocket_url("ws://8.8.8.8:9222/id").is_err());
        assert!(LocalCdpEndpoint::from_websocket_url("wss://127.0.0.1:9222/id").is_err());
        assert!(
            LocalCdpEndpoint::from_websocket_url("ws://user:secret@127.0.0.1:9222/id").is_err()
        );
    }
}
