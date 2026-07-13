#![allow(dead_code)]

use std::{
    net::SocketAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use futures_util::{Sink, SinkExt, Stream, StreamExt};
use krometrail_cdp::LocalCdpEndpoint;
use serde_json::json;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{Notify, oneshot},
    task::{JoinHandle, JoinSet},
};
use tokio_tungstenite::{accept_async, connect_async, tungstenite::Message};

const PROXY_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(1);
const HTTP_REQUEST_LIMIT: usize = 16 * 1024;

/// A loopback CDP boundary that forwards both discovery and WebSocket frames to a real Chrome
/// endpoint. It deliberately owns only the proxy listener and connections; Chrome remains owned by
/// the caller so an active transport can be severed without turning a browser failure into a
/// process failure.
pub struct CdpFaultProxy {
    address: SocketAddr,
    control: Arc<ProxyControl>,
    stop: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<()>>,
}

struct ProxyControl {
    upstream_websocket_url: String,
    proxy_websocket_url: String,
    connection_count: AtomicUsize,
    version_request_count: AtomicUsize,
    connection_changed: Notify,
    active: Mutex<Option<Arc<ActiveConnection>>>,
}

struct ActiveConnection {
    sever_requested: AtomicBool,
    sever: Notify,
}

impl CdpFaultProxy {
    pub async fn start(upstream: &LocalCdpEndpoint) -> std::io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let address = listener.local_addr()?;
        let upstream_websocket_url = upstream.browser_websocket_url().to_string();
        let path = upstream.browser_websocket_url().path();
        let proxy_websocket_url = format!("ws://{address}{path}");
        let control = Arc::new(ProxyControl {
            upstream_websocket_url,
            proxy_websocket_url,
            connection_count: AtomicUsize::new(0),
            version_request_count: AtomicUsize::new(0),
            connection_changed: Notify::new(),
            active: Mutex::new(None),
        });
        let (stop, stop_receiver) = oneshot::channel();
        let task_control = Arc::clone(&control);
        let task = tokio::spawn(run_proxy(listener, task_control, stop_receiver));
        Ok(Self {
            address,
            control,
            stop: Some(stop),
            task: Some(task),
        })
    }

    pub fn http_endpoint(&self) -> String {
        format!("http://{}", self.address)
    }

    pub fn websocket_url(&self) -> &str {
        &self.control.proxy_websocket_url
    }

    pub fn connection_count(&self) -> usize {
        self.control.connection_count.load(Ordering::Acquire)
    }

    pub fn version_request_count(&self) -> usize {
        self.control.version_request_count.load(Ordering::Acquire)
    }

    /// Wait for a physical proxy-to-Chrome WebSocket connection without polling or sleeping.
    pub async fn wait_for_connections(&self, minimum: usize, timeout: Duration) -> bool {
        let wait = async {
            loop {
                if self.connection_count() >= minimum {
                    return;
                }
                let changed = self.control.connection_changed.notified();
                if self.connection_count() >= minimum {
                    return;
                }
                changed.await;
            }
        };
        tokio::time::timeout(timeout, wait).await.is_ok()
    }

    /// Close only the currently active client/upstream WebSocket pair. The listener and Chrome
    /// endpoint remain available, so the production supervisor must reconnect through a new pair.
    pub fn sever_active_transport(&self) -> bool {
        let active = self
            .control
            .active
            .lock()
            .expect("proxy active connection lock")
            .clone();
        let Some(active) = active else {
            return false;
        };
        active.sever_requested.store(true, Ordering::Release);
        active.sever.notify_waiters();
        true
    }

    /// Gracefully stop the listener and join all proxy connection tasks. `Drop` retains an aborting
    /// fallback for panic paths where async cleanup cannot be awaited.
    pub async fn shutdown(&mut self) {
        self.stop.take().map(|stop| stop.send(()));
        if let Some(mut task) = self.task.take() {
            if tokio::time::timeout(PROXY_SHUTDOWN_TIMEOUT, &mut task)
                .await
                .is_err()
            {
                task.abort();
                let _ = task.await;
            }
        }
    }
}

impl Drop for CdpFaultProxy {
    fn drop(&mut self) {
        self.stop.take().map(|stop| stop.send(()));
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

async fn run_proxy(
    listener: TcpListener,
    control: Arc<ProxyControl>,
    mut stop: oneshot::Receiver<()>,
) {
    let mut connections = JoinSet::new();
    loop {
        tokio::select! {
            _ = &mut stop => break,
            accepted = listener.accept() => {
                let Ok((stream, _peer)) = accepted else { break };
                let control = Arc::clone(&control);
                connections.spawn(async move {
                    handle_connection(stream, control).await;
                });
            }
            joined = connections.join_next(), if !connections.is_empty() => {
                let _ = joined;
            }
        }
    }
    connections.abort_all();
    while connections.join_next().await.is_some() {}
}

async fn handle_connection(mut stream: TcpStream, control: Arc<ProxyControl>) {
    let mut prefix = [0_u8; 64];
    let Ok(count) = stream.peek(&mut prefix).await else {
        return;
    };
    if prefix[..count].starts_with(b"GET /json/version") {
        serve_version(&mut stream, &control).await;
        return;
    }

    let Ok(client) = accept_async(stream).await else {
        return;
    };
    let Ok((upstream, _response)) = connect_async(&control.upstream_websocket_url).await else {
        return;
    };
    let active = Arc::new(ActiveConnection {
        sever_requested: AtomicBool::new(false),
        sever: Notify::new(),
    });
    {
        *control.active.lock().expect("proxy active connection lock") = Some(Arc::clone(&active));
    }
    control.connection_count.fetch_add(1, Ordering::AcqRel);
    control.connection_changed.notify_waiters();

    let (client_sink, client_stream) = client.split();
    let (upstream_sink, upstream_stream) = upstream.split();
    let client_to_upstream = relay(client_stream, upstream_sink);
    let upstream_to_client = relay(upstream_stream, client_sink);
    tokio::select! {
        _ = client_to_upstream => {},
        _ = upstream_to_client => {},
        _ = wait_for_sever(Arc::clone(&active)) => {},
    }
    let mut current = control.active.lock().expect("proxy active connection lock");
    if current
        .as_ref()
        .is_some_and(|candidate| Arc::ptr_eq(candidate, &active))
    {
        *current = None;
    }
}

async fn wait_for_sever(active: Arc<ActiveConnection>) {
    if active.sever_requested.load(Ordering::Acquire) {
        return;
    }
    active.sever.notified().await;
}

async fn relay<R, S>(mut reader: R, mut writer: S)
where
    R: Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
    S: Sink<Message> + Unpin,
    S::Error: std::fmt::Debug,
{
    while let Some(Ok(message)) = reader.next().await {
        if writer.send(message).await.is_err() {
            break;
        }
    }
}

async fn serve_version(stream: &mut TcpStream, control: &ProxyControl) {
    let mut request = Vec::with_capacity(1024);
    let mut buffer = [0_u8; 1024];
    loop {
        let Ok(count) =
            tokio::time::timeout(Duration::from_secs(1), stream.read(&mut buffer)).await
        else {
            return;
        };
        let Ok(count) = count else {
            return;
        };
        if count == 0 {
            return;
        }
        request.extend_from_slice(&buffer[..count]);
        if request.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
        if request.len() > HTTP_REQUEST_LIMIT {
            return;
        }
    }
    control.version_request_count.fetch_add(1, Ordering::AcqRel);
    let body = json!({
        "Browser": "Chrome/real-test-proxy",
        "Protocol-Version": "1.3",
        "User-Agent": "Chrome/real-test-proxy",
        "V8-Version": "real-test-proxy",
        "webSocketDebuggerUrl": control.proxy_websocket_url,
    })
    .to_string();
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}
