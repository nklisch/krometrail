//! Validated, non-owning local CDP endpoints.
//!
//! Endpoint validation is deliberately separate from connection establishment. This prevents a
//! malformed or remote endpoint from reaching a socket operation, and gives launch/supervision a
//! value that carries no credentials or mutable process state.

use std::{
    fmt,
    net::{SocketAddr, ToSocketAddrs},
    sync::Arc,
};

use krometrail_core::NonEmptyText;
use thiserror::Error;
use url::Url;

const MAX_DISCOVERY_BYTES: usize = 256 * 1024;
const DISCOVERY_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(500);

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
    #[error("CDP HTTP rediscovery is only available for an HTTP endpoint")]
    NotHttpOrigin,
}

/// The protocol form used to create a local endpoint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalCdpEndpointKind {
    /// An HTTP debugging origin discovered through `/json/version`.
    Http,
    /// A browser WebSocket URL supplied directly by the caller.
    WebSocket,
}

/// Resolves one endpoint hostname to its candidate socket addresses.
///
/// The resolver is deliberately a small synchronous port: endpoint construction is the only place
/// where name resolution is allowed, and deterministic tests can supply a fixed result without
/// changing the network code. Implementations must return all candidates observed for the name;
/// [`LocalCdpEndpoint`] rejects an empty set and every set containing a non-loopback address.
pub trait EndpointResolver: Send + Sync {
    fn resolve(&self, host: &str, port: u16) -> std::io::Result<Vec<SocketAddr>>;
}

impl<F> EndpointResolver for F
where
    F: Fn(&str, u16) -> std::io::Result<Vec<SocketAddr>> + Send + Sync,
{
    fn resolve(&self, host: &str, port: u16) -> std::io::Result<Vec<SocketAddr>> {
        self(host, port)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemEndpointResolver;

impl EndpointResolver for SystemEndpointResolver {
    fn resolve(&self, host: &str, port: u16) -> std::io::Result<Vec<SocketAddr>> {
        (host, port).to_socket_addrs().map(Iterator::collect)
    }
}

/// An explicitly validated loopback CDP endpoint.
///
/// The endpoint keeps the original protocol authorities for HTTP `Host` headers and the public
/// URI accessors, while separately retaining the socket addresses selected during validation. The
/// transport uses those pinned addresses for dialing; it never asks the operating system to
/// resolve the endpoint hostname again. Both URL fields and the resolver are private so callers
/// cannot manufacture an unvalidated endpoint or accidentally log a credential-bearing URL.
#[derive(Clone)]
pub struct LocalCdpEndpoint {
    kind: LocalCdpEndpointKind,
    http_origin: Url,
    browser_websocket_url: Url,
    http_address: SocketAddr,
    websocket_address: SocketAddr,
    redacted_label: NonEmptyText,
    resolver: Arc<dyn EndpointResolver>,
}

impl fmt::Debug for LocalCdpEndpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LocalCdpEndpoint")
            .field("kind", &self.kind)
            .field("http_origin", &self.http_origin)
            .field("browser_websocket_url", &self.browser_websocket_url)
            .field("http_address", &self.http_address)
            .field("websocket_address", &self.websocket_address)
            .field("redacted_label", &self.redacted_label)
            .finish_non_exhaustive()
    }
}

impl PartialEq for LocalCdpEndpoint {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
            && self.http_origin == other.http_origin
            && self.browser_websocket_url == other.browser_websocket_url
            && self.http_address == other.http_address
            && self.websocket_address == other.websocket_address
            && self.redacted_label == other.redacted_label
    }
}

impl Eq for LocalCdpEndpoint {}

impl LocalCdpEndpoint {
    /// Resolve an HTTP debugging endpoint through `/json/version`, or validate a direct WebSocket
    /// endpoint. All URL checks and hostname resolution happen before the first network operation.
    pub async fn resolve(input: impl AsRef<str>) -> Result<Self, EndpointError> {
        Self::resolve_with_resolver(input, Arc::new(SystemEndpointResolver)).await
    }

    /// Resolve an endpoint using an injectable hostname resolver.
    pub async fn resolve_with_resolver(
        input: impl AsRef<str>,
        resolver: Arc<dyn EndpointResolver>,
    ) -> Result<Self, EndpointError> {
        let url = parse_and_validate(input.as_ref())?;
        match url.scheme() {
            "ws" => Self::from_validated_websocket(url, resolver),
            "http" => Self::resolve_http(url, resolver).await,
            _ => Err(EndpointError::UnsupportedScheme),
        }
    }

    /// Generic convenience form of [`Self::resolve_with_resolver`].
    pub async fn resolve_with<R>(input: impl AsRef<str>, resolver: R) -> Result<Self, EndpointError>
    where
        R: EndpointResolver + 'static,
    {
        Self::resolve_with_resolver(input, Arc::new(resolver)).await
    }

    /// Validate a direct WebSocket endpoint without opening a connection.
    pub fn from_websocket_url(input: impl AsRef<str>) -> Result<Self, EndpointError> {
        Self::from_websocket_url_with_resolver(input, Arc::new(SystemEndpointResolver))
    }

    /// Validate a direct WebSocket endpoint with an injectable hostname resolver.
    pub fn from_websocket_url_with_resolver(
        input: impl AsRef<str>,
        resolver: Arc<dyn EndpointResolver>,
    ) -> Result<Self, EndpointError> {
        let url = parse_and_validate(input.as_ref())?;
        if url.scheme() != "ws" {
            return Err(EndpointError::UnsupportedScheme);
        }
        Self::from_validated_websocket(url, resolver)
    }

    /// Generic convenience form of [`Self::from_websocket_url_with_resolver`].
    pub fn from_websocket_url_with<R>(
        input: impl AsRef<str>,
        resolver: R,
    ) -> Result<Self, EndpointError>
    where
        R: EndpointResolver + 'static,
    {
        Self::from_websocket_url_with_resolver(input, Arc::new(resolver))
    }

    /// Alias emphasizing that this constructor performs no readiness probe.
    pub fn validate_websocket(input: impl AsRef<str>) -> Result<Self, EndpointError> {
        Self::from_websocket_url(input)
    }

    pub fn kind(&self) -> LocalCdpEndpointKind {
        self.kind
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

    /// Refresh the browser WebSocket URL for an HTTP-origin endpoint.
    ///
    /// The HTTP socket address selected during initial validation is reused. If discovery returns
    /// the same WebSocket authority, its already validated address is reused too; only a genuinely
    /// new authority is resolved. This is what makes reconnect safe from hostname rebinding while
    /// still allowing Chrome to rotate its browser WebSocket path.
    pub async fn refresh_http(&self) -> Result<Self, EndpointError> {
        if self.kind != LocalCdpEndpointKind::Http {
            return Err(EndpointError::NotHttpOrigin);
        }
        let response = fetch_version(&self.http_origin, self.http_address).await?;
        self.with_discovered_websocket(response)
    }

    fn from_validated_websocket(
        url: Url,
        resolver: Arc<dyn EndpointResolver>,
    ) -> Result<Self, EndpointError> {
        let websocket_address = pin_loopback(&url, resolver.as_ref())?;
        let http_origin = origin_for(&url, "http")?;
        let redacted_label = label_for(&url)?;
        Ok(Self {
            kind: LocalCdpEndpointKind::WebSocket,
            http_origin,
            browser_websocket_url: url,
            http_address: websocket_address,
            websocket_address,
            redacted_label,
            resolver,
        })
    }

    async fn resolve_http(
        url: Url,
        resolver: Arc<dyn EndpointResolver>,
    ) -> Result<Self, EndpointError> {
        let http_address = pin_loopback(&url, resolver.as_ref())?;
        let response = fetch_version(&url, http_address).await?;
        let websocket = discovered_websocket(&response)?;
        let websocket_address = pin_loopback(&websocket, resolver.as_ref())?;
        let redacted_label = label_for(&url)?;
        Ok(Self {
            kind: LocalCdpEndpointKind::Http,
            http_origin: origin_for(&url, "http")?,
            browser_websocket_url: websocket,
            http_address,
            websocket_address,
            redacted_label,
            resolver,
        })
    }

    fn with_discovered_websocket(
        &self,
        response: serde_json::Value,
    ) -> Result<Self, EndpointError> {
        let websocket = discovered_websocket(&response)?;
        let websocket_address = if same_authority(&self.browser_websocket_url, &websocket) {
            self.websocket_address
        } else {
            pin_loopback(&websocket, self.resolver.as_ref())?
        };
        Ok(Self {
            kind: self.kind,
            http_origin: self.http_origin.clone(),
            browser_websocket_url: websocket,
            http_address: self.http_address,
            websocket_address,
            redacted_label: self.redacted_label.clone(),
            resolver: Arc::clone(&self.resolver),
        })
    }

    // These are crate-private because the public endpoint contract intentionally exposes the
    // original protocol URLs, not a second caller-manufacturable URL representation.
    pub(crate) fn websocket_dial_url(&self) -> Result<Url, EndpointError> {
        dial_url(&self.browser_websocket_url, self.websocket_address)
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

fn pin_loopback(url: &Url, resolver: &dyn EndpointResolver) -> Result<SocketAddr, EndpointError> {
    let host = url.host_str().ok_or(EndpointError::InvalidUrl)?;
    let port = url.port().ok_or(EndpointError::MissingPort)?;
    let addresses = resolver
        .resolve(host, port)
        .map_err(|_| EndpointError::NotLoopback)?;
    let Some(address) = addresses.first().copied() else {
        return Err(EndpointError::NotLoopback);
    };
    if addresses.iter().any(|address| !address.ip().is_loopback()) {
        return Err(EndpointError::NotLoopback);
    }
    Ok(address)
}

fn discovered_websocket(response: &serde_json::Value) -> Result<Url, EndpointError> {
    let websocket = response
        .get("webSocketDebuggerUrl")
        .and_then(|value| value.as_str())
        .ok_or(EndpointError::InvalidDiscovery)?;
    let websocket = parse_and_validate(websocket)?;
    if websocket.scheme() != "ws" {
        return Err(EndpointError::UnsupportedScheme);
    }
    Ok(websocket)
}

fn same_authority(left: &Url, right: &Url) -> bool {
    left.host_str()
        .zip(right.host_str())
        .is_some_and(|(left_host, right_host)| {
            left_host.eq_ignore_ascii_case(right_host) && left.port() == right.port()
        })
}

fn dial_url(url: &Url, address: SocketAddr) -> Result<Url, EndpointError> {
    let mut dial = url.clone();
    let host = address.ip().to_string();
    dial.set_host(Some(&host))
        .map_err(|_| EndpointError::InvalidUrl)?;
    Ok(dial)
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

async fn fetch_version(url: &Url, address: SocketAddr) -> Result<serde_json::Value, EndpointError> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    let host = url.host_str().ok_or(EndpointError::InvalidUrl)?;
    let port = url.port().ok_or(EndpointError::MissingPort)?;
    let mut stream = tokio::time::timeout(DISCOVERY_TIMEOUT, TcpStream::connect(address))
        .await
        .map_err(|_| EndpointError::DiscoveryFailed)?
        .map_err(|_| EndpointError::DiscoveryFailed)?;
    let authority = match url.host() {
        Some(url::Host::Ipv6(_)) => format!("[{host}]:{port}"),
        _ => format!("{host}:{port}"),
    };
    let request =
        format!("GET /json/version HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\n\r\n");
    tokio::time::timeout(DISCOVERY_TIMEOUT, stream.write_all(request.as_bytes()))
        .await
        .map_err(|_| EndpointError::DiscoveryFailed)?
        .map_err(|_| EndpointError::DiscoveryFailed)?;
    let mut bytes = Vec::new();
    tokio::time::timeout(DISCOVERY_TIMEOUT, async {
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
            if response_body_complete(&bytes) {
                break Ok(());
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

fn response_body_complete(bytes: &[u8]) -> bool {
    let Some(split) = bytes.windows(4).position(|window| window == b"\r\n\r\n") else {
        return false;
    };
    let headers = String::from_utf8_lossy(&bytes[..split]);
    let Some(length) = headers.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("content-length")
            .then(|| value.trim().parse::<usize>().ok())
            .flatten()
    }) else {
        return false;
    };
    bytes.len() >= split + 4 + length
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        net::{IpAddr, Ipv4Addr},
        sync::atomic::{AtomicUsize, Ordering},
    };

    #[test]
    fn validates_only_loopback_websocket_origins() {
        let endpoint =
            LocalCdpEndpoint::from_websocket_url("ws://127.0.0.1:9222/devtools/browser/id")
                .unwrap();
        assert_eq!(endpoint.redacted_label(), "127.0.0.1:9222");
        assert_eq!(endpoint.kind(), LocalCdpEndpointKind::WebSocket);
        assert!(LocalCdpEndpoint::from_websocket_url("ws://8.8.8.8:9222/id").is_err());
        assert!(LocalCdpEndpoint::from_websocket_url("wss://127.0.0.1:9222/id").is_err());
        assert!(
            LocalCdpEndpoint::from_websocket_url("ws://user:secret@127.0.0.1:9222/id").is_err()
        );
    }

    #[test]
    fn rejects_empty_and_mixed_resolver_results_before_network_use() {
        let empty = Arc::new(|_: &str, _: u16| Ok(Vec::new()));
        assert_eq!(
            LocalCdpEndpoint::from_websocket_url_with_resolver("ws://test.invalid:9222/id", empty,)
                .unwrap_err(),
            EndpointError::NotLoopback
        );

        let mixed = Arc::new(|_: &str, _: u16| {
            Ok(vec![
                SocketAddr::from(([127, 0, 0, 1], 9222)),
                SocketAddr::from(([8, 8, 8, 8], 9222)),
            ])
        });
        assert_eq!(
            LocalCdpEndpoint::from_websocket_url_with_resolver("ws://test.invalid:9222/id", mixed,)
                .unwrap_err(),
            EndpointError::NotLoopback
        );
    }

    #[test]
    fn pins_the_first_validated_address_without_re_resolving_for_dial() {
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_for_resolver = Arc::clone(&calls);
        let resolver = move |_: &str, port: u16| {
            calls_for_resolver.fetch_add(1, Ordering::AcqRel);
            Ok(vec![SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port)])
        };
        let endpoint = LocalCdpEndpoint::from_websocket_url_with(
            "ws://rebinding.invalid:9222/devtools/browser/id",
            resolver,
        )
        .unwrap();
        assert_eq!(calls.load(Ordering::Acquire), 1);
        assert_eq!(
            endpoint.websocket_dial_url().unwrap().as_str(),
            "ws://127.0.0.1:9222/devtools/browser/id"
        );
        assert_eq!(
            endpoint.browser_websocket_url().host_str(),
            Some("rebinding.invalid")
        );
        assert_eq!(
            endpoint.browser_websocket_url().path(),
            "/devtools/browser/id"
        );
        assert_eq!(calls.load(Ordering::Acquire), 1);
    }

    #[tokio::test]
    async fn http_discovery_preserves_host_and_refreshes_path_without_rebinding() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let resolver_calls = Arc::new(AtomicUsize::new(0));
        let resolver_calls_for_resolver = Arc::clone(&resolver_calls);
        let resolver = move |host: &str, port: u16| {
            resolver_calls_for_resolver.fetch_add(1, Ordering::AcqRel);
            assert_eq!(port, address.port());
            match host {
                "origin.invalid" | "ws.invalid" => Ok(vec![address]),
                _ => panic!("unexpected host {host}"),
            }
        };
        let server = tokio::spawn(async move {
            for path in ["/devtools/browser/initial", "/devtools/browser/rotated"] {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                loop {
                    let mut chunk = [0_u8; 512];
                    let count = stream.read(&mut chunk).await.unwrap();
                    request.extend_from_slice(&chunk[..count]);
                    if request.windows(4).any(|window| window == b"\r\n\r\n") {
                        break;
                    }
                }
                let request = String::from_utf8(request).unwrap();
                assert!(request.contains(&format!("Host: origin.invalid:{}", address.port())));
                let body = serde_json::json!({
                    "webSocketDebuggerUrl": format!("ws://ws.invalid:{}{path}", address.port()),
                })
                .to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        });

        let endpoint = LocalCdpEndpoint::resolve_with(
            format!("http://origin.invalid:{}", address.port()),
            resolver,
        )
        .await
        .unwrap();
        assert_eq!(endpoint.kind(), LocalCdpEndpointKind::Http);
        assert_eq!(endpoint.http_origin().host_str(), Some("origin.invalid"));
        assert_eq!(
            endpoint.browser_websocket_url().host_str(),
            Some("ws.invalid")
        );
        assert_eq!(
            endpoint.browser_websocket_url().path(),
            "/devtools/browser/initial"
        );
        assert_eq!(resolver_calls.load(Ordering::Acquire), 2);

        let refreshed = endpoint.refresh_http().await.unwrap();
        assert_eq!(
            refreshed.browser_websocket_url().path(),
            "/devtools/browser/rotated"
        );
        assert_eq!(resolver_calls.load(Ordering::Acquire), 2);
        server.await.unwrap();
    }

    #[test]
    fn content_length_ends_discovery_without_waiting_for_connection_close() {
        let body = br#"{"ok":true}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            String::from_utf8_lossy(body)
        );
        assert!(response_body_complete(response.as_bytes()));
    }
}
