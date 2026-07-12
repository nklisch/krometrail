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

use super::error::{SpikeError, SpikeErrorCode};

pub use super::scripted_peer::ScriptedCdpServer;

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

fn io_error(error: std::io::Error) -> SpikeError {
    SpikeError::new(SpikeErrorCode::Io, error.to_string())
}
