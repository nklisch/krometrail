//! Disposable loopback servers used only by the qualification harness.

use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
};

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::net::TcpListener as TokioTcpListener;
use tokio_tungstenite::{WebSocketStream, accept_async, tungstenite::Message};

use super::error::{SpikeError, SpikeErrorCode};

pub struct StaticFixtureServer {
    pub base_url: String,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl StaticFixtureServer {
    pub fn start(root: &Path) -> Result<Self, SpikeError> {
        let index = std::fs::read(root.join("index.html")).map_err(io_error)?;
        let animation = std::fs::read(root.join("animation.js")).map_err(io_error)?;
        let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(io_error)?;
        listener.set_nonblocking(true).map_err(io_error)?;
        let address = listener.local_addr().map_err(io_error)?;
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let thread = thread::spawn(move || {
            while !thread_stop.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => serve_http(stream, &index, &animation),
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::yield_now();
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            base_url: format!("http://127.0.0.1:{}", address.port()),
            stop,
            thread: Some(thread),
        })
    }
}

impl Drop for StaticFixtureServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn serve_http(mut stream: TcpStream, index: &[u8], animation: &[u8]) {
    let mut request = [0_u8; 2048];
    let Ok(size) = stream.read(&mut request) else {
        return;
    };
    let request = String::from_utf8_lossy(&request[..size]);
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");
    let (status, content_type, body) = match path {
        "/" | "/index.html" => ("200 OK", "text/html; charset=utf-8", index),
        "/animation.js" => ("200 OK", "text/javascript; charset=utf-8", animation),
        _ => (
            "404 Not Found",
            "text/plain; charset=utf-8",
            b"not found" as &[u8],
        ),
    };
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: {content_type}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(body);
}

/// A tiny deterministic CDP peer. It is a real WebSocket server, but its behavior is fully
/// scripted and contains no timing-based ordering assumptions.
pub struct ScriptedCdpServer {
    pub ws_url: String,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    _task: tokio::task::JoinHandle<()>,
}

impl ScriptedCdpServer {
    pub async fn start() -> Result<Self, SpikeError> {
        let listener = TokioTcpListener::bind(("127.0.0.1", 0))
            .await
            .map_err(io_error)?;
        let address = listener.local_addr().map_err(io_error)?;
        let (shutdown, mut stop) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let Ok((stream, _)) = (tokio::select! {
                result = listener.accept() => result,
                _ = &mut stop => return,
            }) else {
                return;
            };
            let Ok(mut socket) = accept_async(stream).await else {
                return;
            };
            while let Some(Ok(message)) = socket.next().await {
                let Message::Text(text) = message else {
                    continue;
                };
                let Ok(command) = serde_json::from_str::<Value>(&text) else {
                    continue;
                };
                if handle_command(&mut socket, &command).await.is_err() {
                    break;
                }
            }
        });
        Ok(Self {
            ws_url: format!(
                "ws://127.0.0.1:{}/devtools/browser/scripted",
                address.port()
            ),
            shutdown: Some(shutdown),
            _task: task,
        })
    }
}

impl Drop for ScriptedCdpServer {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
    }
}

async fn handle_command<S>(
    socket: &mut WebSocketStream<S>,
    command: &Value,
) -> Result<(), SpikeError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let id = command.get("id").and_then(Value::as_u64).unwrap_or(0);
    let method = command.get("method").and_then(Value::as_str).unwrap_or("");
    let session_id = command.get("sessionId").and_then(Value::as_str);
    let params = command
        .get("params")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    if method == "Runtime.evaluate" {
        if let Some(session_id) = session_id {
            send_event(socket, "Runtime.consoleAPICalled", session_id, serde_json::json!({
				"type": "log", "args": [], "executionContextId": 1, "timestamp": 0,
				"token": format!("{}-{}", session_id, params.get("token").and_then(Value::as_u64).unwrap_or(0)), 
			})).await?;
        }
    }
    if method == "Browser.getVersion" {
        for (name, params) in [
            (
                "Protocol.unknownEvent",
                serde_json::json!({"kind":"unknown"}),
            ),
            (
                "Runtime.additiveField",
                serde_json::json!({"known":true,"new_field":7}),
            ),
            (
                "Runtime.unknownEnum",
                serde_json::json!({"value":"future-value"}),
            ),
        ] {
            let target = session_id.unwrap_or("session-a");
            send_event(socket, name, target, params).await?;
        }
    }
    let result = match method {
        "Target.attachToTarget" => {
            let target = params
                .get("targetId")
                .and_then(Value::as_str)
                .unwrap_or("target");
            serde_json::json!({"sessionId": if target == "target-a" { "session-a" } else { "session-b" }})
        }
        "Browser.getVersion" => {
            serde_json::json!({"scope":{"scope":"browser"},"protocolVersion":"1.3","product":"Chrome/qualification","revision":"r-qualification","userAgent":"qualification","jsVersion":"qualification"})
        }
        "Accessibility.getFullAXTree" => serde_json::json!({"nodes": []}),
        "Runtime.evaluate" => {
            serde_json::json!({"token":session_id.unwrap_or("browser"),"result":{"type":"number","value":2,"description":"2"}})
        }
        "Page.screencastFrameAck" => {
            if let Some(session_id) = session_id {
                send_frame(
                    socket,
                    session_id,
                    params.get("sessionId").and_then(Value::as_i64).unwrap_or(1),
                )
                .await?;
            }
            serde_json::json!({})
        }
        "Page.startScreencast" => {
            if let Some(session_id) = session_id {
                send_frame(socket, session_id, 1).await?;
            }
            serde_json::json!({})
        }
        _ => serde_json::json!({}),
    };
    socket
        .send(Message::Text(
            serde_json::json!({"id": id, "result": result})
                .to_string()
                .into(),
        ))
        .await
        .map_err(ws_error)
}

async fn send_event<S>(
    socket: &mut WebSocketStream<S>,
    method: &str,
    session_id: &str,
    params: Value,
) -> Result<(), SpikeError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    socket
        .send(Message::Text(
            serde_json::json!({"method":method,"sessionId":session_id,"params":params})
                .to_string()
                .into(),
        ))
        .await
        .map_err(ws_error)
}

async fn send_frame<S>(
    socket: &mut WebSocketStream<S>,
    session_id: &str,
    sequence: i64,
) -> Result<(), SpikeError>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    send_event(socket, "Page.screencastFrame", session_id, serde_json::json!({"data":"Zg==","metadata":{"pageScaleFactor":1,"offsetTop":0,"deviceWidth":1,"deviceHeight":1,"scrollOffsetX":0,"scrollOffsetY":0,"timestamp":0},"sessionId":sequence})).await
}

fn io_error(error: std::io::Error) -> SpikeError {
    SpikeError::new(SpikeErrorCode::Io, error.to_string())
}
fn ws_error(error: tokio_tungstenite::tungstenite::Error) -> SpikeError {
    SpikeError::new(SpikeErrorCode::Io, error.to_string())
}
