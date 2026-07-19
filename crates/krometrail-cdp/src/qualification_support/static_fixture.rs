use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
};

pub const INDEX_HTML: &str =
    include_str!("../../../../tests/fixtures/browser/cdp-transport-gate/index.html");
pub const ANIMATION_JS: &str =
    include_str!("../../../../tests/fixtures/browser/cdp-transport-gate/animation.js");
pub const TEMPORAL_BENCHMARK_INDEX: &str =
    include_str!("../../../../tests/fixtures/browser/temporal-benchmark/index.html");
pub const TEMPORAL_BENCHMARK_CSS: &str =
    include_str!("../../../../tests/fixtures/browser/temporal-benchmark/benchmark.css");
pub const TEMPORAL_BENCHMARK_JS: &str =
    include_str!("../../../../tests/fixtures/browser/temporal-benchmark/benchmark.js");
pub const TEMPORAL_BENCHMARK_README: &str =
    include_str!("../../../../tests/fixtures/browser/temporal-benchmark/README.md");
pub const BROWSER_CONTEXTS_INDEX: &str =
    include_str!("../../../../tests/fixtures/browser/browser-contexts/index.html");
pub const BROWSER_CONTEXTS_CHILD_NEXT: &str =
    include_str!("../../../../tests/fixtures/browser/browser-contexts/child-next.html");
pub const BROWSER_CONTEXTS_CROSS_ORIGIN: &str =
    include_str!("../../../../tests/fixtures/browser/browser-contexts/cross-origin.html");
pub const BROWSER_CONTEXTS_STYLE: &str =
    include_str!("../../../../tests/fixtures/browser/browser-contexts/style.css");

pub fn contains_stable_fixture_markers() -> bool {
    INDEX_HTML.contains("CDP") && !ANIMATION_JS.trim().is_empty()
}

/// A loopback-only static server for opted-in browser tests. Binding and thread readiness are
/// observable, so callers do not need a sleep-based startup guess.
pub struct FixtureServer {
    address: SocketAddr,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl FixtureServer {
    pub fn start() -> std::io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let (ready_sender, ready_receiver) = std::sync::mpsc::sync_channel(0);
        let thread = thread::spawn(move || {
            let _ = ready_sender.send(());
            while !thread_stop.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => serve_fixture(stream),
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::yield_now();
                    }
                    Err(_) => break,
                }
            }
        });
        ready_receiver
            .recv()
            .map_err(|_| std::io::Error::other("fixture server readiness failed"))?;
        Ok(Self {
            address,
            stop,
            thread: Some(thread),
        })
    }

    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}/index.html", self.address.port())
    }

    /// Returns the exact benchmark route; case and duration come from the canonical matrix.
    pub fn temporal_benchmark_url(&self, case_id: &str, duration_ms: u16) -> String {
        format!(
            "http://127.0.0.1:{}/temporal-benchmark/index.html?case={case_id}&duration_ms={duration_ms}",
            self.address.port()
        )
    }

    pub fn browser_contexts_url(&self) -> String {
        format!(
            "http://127.0.0.1:{}/browser-contexts/index.html",
            self.address.port()
        )
    }

    pub fn address(&self) -> SocketAddr {
        self.address
    }

    pub fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for FixtureServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn serve_fixture(mut stream: TcpStream) {
    let mut request = [0_u8; 2048];
    let Ok(size) = stream.read(&mut request) else {
        return;
    };
    let request = String::from_utf8_lossy(&request[..size]);
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|target| {
            target
                .split_once('?')
                .map_or(Some(target), |(path, _)| Some(path))
        })
        .unwrap_or("/");
    let (status, content_type, body) = match path {
        "/" | "/index.html" => ("200 OK", "text/html; charset=utf-8", INDEX_HTML.as_bytes()),
        "/animation.js" => (
            "200 OK",
            "text/javascript; charset=utf-8",
            ANIMATION_JS.as_bytes(),
        ),
        "/temporal-benchmark" | "/temporal-benchmark/index.html" => (
            "200 OK",
            "text/html; charset=utf-8",
            TEMPORAL_BENCHMARK_INDEX.as_bytes(),
        ),
        "/temporal-benchmark/benchmark.css" => (
            "200 OK",
            "text/css; charset=utf-8",
            TEMPORAL_BENCHMARK_CSS.as_bytes(),
        ),
        "/temporal-benchmark/benchmark.js" => (
            "200 OK",
            "text/javascript; charset=utf-8",
            TEMPORAL_BENCHMARK_JS.as_bytes(),
        ),
        "/temporal-benchmark/README.md" => (
            "200 OK",
            "text/markdown; charset=utf-8",
            TEMPORAL_BENCHMARK_README.as_bytes(),
        ),
        "/browser-contexts" | "/browser-contexts/index.html" => (
            "200 OK",
            "text/html; charset=utf-8",
            BROWSER_CONTEXTS_INDEX.as_bytes(),
        ),
        "/browser-contexts/child-next.html" => (
            "200 OK",
            "text/html; charset=utf-8",
            BROWSER_CONTEXTS_CHILD_NEXT.as_bytes(),
        ),
        "/browser-contexts/cross-origin.html" => (
            "200 OK",
            "text/html; charset=utf-8",
            BROWSER_CONTEXTS_CROSS_ORIGIN.as_bytes(),
        ),
        "/browser-contexts/style.css" => (
            "200 OK",
            "text/css; charset=utf-8",
            BROWSER_CONTEXTS_STYLE.as_bytes(),
        ),
        path if path.starts_with("/browser-contexts/asset-") && path.ends_with(".json") => {
            ("200 OK", "application/json", b"{}" as &[u8])
        }
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
