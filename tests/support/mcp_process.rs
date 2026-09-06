//! Bounded actual-process MCP harness shared by protocol and live qualification.
#![allow(dead_code)]
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

pub struct McpProcess {
    child: Child,
    input: Option<ChildStdin>,
    messages: mpsc::Receiver<Value>,
    output: Option<std::thread::JoinHandle<()>>,
    errors: Option<std::thread::JoinHandle<String>>,
    pub root: tempfile::TempDir,
    output_gate: Option<mpsc::Sender<()>>,
    next: u64,
    pub modern: bool,
}
impl McpProcess {
    pub fn start() -> Self {
        Self::start_mode(false)
    }
    pub fn start_with_output_paused() -> Self {
        Self::start_mode(true)
    }
    fn start_mode(paused: bool) -> Self {
        let root = tempfile::tempdir().unwrap();
        let command: Vec<String> = std::env::var("KROMETRAIL_TEST_MCP_COMMAND")
            .ok()
            .map(|s| serde_json::from_str(&s).expect("test command must be a JSON string array"))
            .unwrap_or_else(|| vec![env!("CARGO_BIN_EXE_krometrail").into(), "mcp".into()]);
        let mut child = Command::new(&command[0])
            .args(&command[1..])
            .env("KROMETRAIL_DATA_DIR", root.path().join("data"))
            .env("KROMETRAIL_FFMPEG_PATH", root.path().join("missing-ffmpeg"))
            .env("HOME", root.path())
            .env("TMPDIR", root.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let (send, messages) = mpsc::channel();
        let (gate_send, gate_recv) = mpsc::channel();
        let output_gate = if paused {
            Some(gate_send)
        } else {
            drop(gate_send);
            None
        };
        let output = std::thread::spawn(move || {
            let mut stdout = BufReader::new(stdout);
            if paused {
                // Observe a small discovery response before pausing. This proves the
                // signal handler is installed even on a heavily loaded test runner.
                let mut first = String::new();
                stdout.read_line(&mut first).unwrap();
                send.send(serde_json::from_str(&first).unwrap()).unwrap();
                let _ = gate_recv.recv();
                // Forced shutdown of an unread pipe may interrupt a JSON line. This
                // mode tests process lifetime, not delivery of an unread response.
                let _ = std::io::copy(&mut stdout, &mut std::io::sink());
                return;
            }
            for line in stdout.lines() {
                let Ok(line) = line else {
                    break;
                };
                let value = serde_json::from_str(&line).expect("stdout must contain JSON-RPC only");
                if send.send(value).is_err() {
                    break;
                }
            }
        });
        let errors = std::thread::spawn(move || {
            let mut s = String::new();
            BufReader::new(stderr).read_to_string(&mut s).unwrap();
            s
        });
        Self {
            child,
            input,
            messages,
            output: Some(output),
            errors: Some(errors),
            root,
            output_gate,
            next: 10,
            modern: true,
        }
    }
    pub fn send(&mut self, message: Value) {
        let input = self.input.as_mut().expect("stdin open");
        writeln!(input, "{message}").unwrap();
        input.flush().unwrap();
    }
    pub fn recv(&self) -> Value {
        self.messages
            .recv_timeout(Duration::from_secs(20))
            .expect("bounded MCP response")
    }
    pub fn request(&mut self, method: &str, mut params: Value) -> Value {
        self.next += 1;
        if self.modern {
            params["_meta"] = metadata("2026-07-28");
        }
        let id = self.next;
        self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}));
        let result = self.recv();
        assert_eq!(result["id"], id);
        result
    }
    pub fn initialize(&mut self, version: &str) -> Value {
        self.modern = false;
        let result=self.request("initialize",json!({"protocolVersion":version,"capabilities":{},"clientInfo":{"name":"qualification","version":"1"}}));
        self.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
        result
    }
    pub fn tools(&mut self) -> Vec<Value> {
        let mut tools = Vec::new();
        let mut cursor = None;
        loop {
            let params = cursor
                .take()
                .map_or(json!({}), |c: Value| json!({"cursor":c}));
            let page = self.request("tools/list", params);
            let result = &page["result"];
            let entries = result["tools"].as_array().expect("tools page");
            if self.modern {
                assert!(serde_json::to_vec(result).unwrap().len() <= 192 * 1024);
                assert!(entries.len() <= 8);
                assert_eq!(result["resultType"], "complete");
                assert_eq!(result["ttlMs"], 60000);
                assert_eq!(result["cacheScope"], "private");
            } else {
                assert!(result.get("resultType").is_none());
                assert!(result.get("ttlMs").is_none());
                assert!(result.get("cacheScope").is_none());
                assert!(result.get("nextCursor").is_none());
            }
            tools.extend(entries.iter().cloned());
            cursor = result.get("nextCursor").cloned();
            if cursor.is_none() {
                break;
            }
        }
        assert!(
            tools
                .windows(2)
                .all(|p| p[0]["name"].as_str().unwrap() < p[1]["name"].as_str().unwrap())
        );
        tools
    }
    pub fn finish(&mut self, success: bool) -> String {
        self.input.take();
        let end = Instant::now() + Duration::from_secs(35);
        let status = loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                break status;
            }
            assert!(Instant::now() < end, "MCP process did not exit after EOF");
            // Test harness child supervision, not agent-job polling.
            std::thread::sleep(Duration::from_millis(10));
        };
        self.output_gate.take();
        self.output.take().unwrap().join().unwrap();
        let stderr = self.errors.take().unwrap().join().unwrap();
        assert_eq!(status.success(), success, "{stderr}");
        stderr
    }
    pub fn pid(&self) -> u32 {
        self.child.id()
    }
}
impl Drop for McpProcess {
    fn drop(&mut self) {
        self.input.take();
        self.output_gate.take();
        let deadline = Instant::now() + Duration::from_secs(35);
        while matches!(self.child.try_wait(), Ok(None)) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(t) = self.output.take() {
            let _ = t.join();
        }
        if let Some(t) = self.errors.take() {
            let _ = t.join();
        }
    }
}
pub fn metadata(version: &str) -> Value {
    json!({"io.modelcontextprotocol/protocolVersion":version,"io.modelcontextprotocol/clientCapabilities":{}})
}
