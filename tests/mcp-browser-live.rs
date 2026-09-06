#![cfg(all(feature = "qualification-support", unix))]
#[path = "support/mcp_process.rs"]
mod process;
use base64::Engine as _;
use process::McpProcess;
use serde_json::{Value, json};
use std::{
    os::unix::fs::PermissionsExt,
    path::Path,
    time::{Duration, Instant},
};

fn call(p: &mut McpProcess, name: &str, args: Value) -> Value {
    let result = p.request("tools/call", json!({"name":name,"arguments":args}));
    assert_ne!(result["result"]["isError"], true, "{name}: {result}");
    assert!(result.get("error").is_none(), "{name}: {result}");
    result
}
fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
fn wrapper(root: &Path) -> std::path::PathBuf {
    let chrome = std::env::var("CHROME_BIN").unwrap_or_else(|_| {
        if cfg!(target_os = "macos") {
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome".into()
        } else {
            "/usr/bin/google-chrome".into()
        }
    });
    assert!(
        Path::new(&chrome).is_file(),
        "required Chrome missing: {chrome}"
    );
    let path = root.join("qualification-chrome");
    std::fs::write(&path,format!("#!/bin/sh\nif [ \"$1\" != --version ]; then echo $$ > {}; fi\nexec {} --headless=new --no-sandbox --disable-gpu \"$@\"\n",quote(root.join("browser.pid").to_str().unwrap()),quote(&chrome))).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    path
}
fn image(result: &Value) -> Vec<u8> {
    let content = result["result"]["content"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["type"] == "image")
        .expect("inline real image");
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(content["data"].as_str().unwrap())
        .unwrap();
    let decoded = image::load_from_memory(&bytes).unwrap();
    assert!(decoded.width() > 0 && decoded.height() > 0);
    bytes
}
#[test]
fn required_browser_evidence_cancellation_and_managed_eof_cleanup() {
    if std::env::var("KROMETRAIL_REAL_CHROME_TESTS").as_deref() != Ok("1") {
        eprintln!("real Chrome qualification not requested");
        return;
    }
    let mut p = McpProcess::start();
    let executable = wrapper(p.root.path());
    let fixture = p.root.path().join("fixture.html");
    std::fs::write(&fixture,"<!doctype html><title>MCP qualification</title><button onclick=\"document.querySelector('output').textContent='clicked'\">Change</button><output>ready</output><div id='clock'></div><script>let i=0;setInterval(()=>{document.getElementById('clock').textContent=++i},30)</script>").unwrap();
    let discovery = p.request("server/discover", json!({}));
    let tools = p.tools();
    let start = call(
        &mut p,
        "start_browser",
        json!({"executable":executable,"profile":"temporary","initial_url":format!("file://{}",fixture.display())}),
    );
    let status = &start["result"]["structuredContent"]["result"];
    let session = status["session_id"].clone();
    let target = status["selected_target_id"].clone();
    assert!(session.is_string() && target.is_string(), "{start}");
    let browser_pid: i32 = std::fs::read_to_string(p.root.path().join("browser.pid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    call(&mut p, "list_pages", json!({}));
    let screenshot = call(
        &mut p,
        "take_screenshot",
        json!({"target":{"kind":"viewport"},"format":"png"}),
    );
    let screenshot_bytes = image(&screenshot);
    let silent = call(
        &mut p,
        "take_screenshot",
        json!({"target":{"kind":"viewport"},"format":"png","response":{"inline_images":false}}),
    );
    assert!(
        !silent["result"]["content"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["type"] == "image")
    );
    // An actual browser wait is cancelled; a later operation must still work.
    let wait = json!({"condition":{"condition":"elapsed","value":{"duration":10000}},"timeout":20000,"poll_interval":25});
    p.send(json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"_meta":process::metadata("2026-07-28"),"name":"wait","arguments":wait}}));
    std::thread::sleep(Duration::from_millis(100));
    p.send(json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":2}}));
    call(&mut p, "browser_status", json!({}));
    call(
        &mut p,
        "click",
        json!({"locator":{"kind":"element","value":{"kind":"css_selector","value":"button"}}}),
    );
    let evaluated = call(
        &mut p,
        "evaluate_page",
        json!({"expression":"document.querySelector('output').textContent","await_promise":false}),
    );
    assert!(evaluated.to_string().contains("clicked"));
    std::thread::sleep(Duration::from_millis(350));
    let anchor = json!({"anchor":"latest_interaction","session_id":session,"target_id":target,"window":null});
    let query = json!({"anchor":anchor,"retention":"allow_partial","capture_gaps":"include"});
    let resolved = call(&mut p, "resolve_temporal_range", query.clone());
    assert!(resolved["result"]["structuredContent"]["range_handle"].is_string());
    let mut args = query;
    args["caller_markers"] = json!([]);
    args["orientation"] = json!("include");
    let bundle = call(&mut p, "temporal_debug_bundle", args);
    image(&bundle);
    let resources = bundle["result"]["structuredContent"]["resources"]
        .as_array()
        .unwrap();
    let uri = resources
        .iter()
        .find(|r| r["mime_type"] == "image/png")
        .expect("canonical artifact resource")["uri"]
        .as_str()
        .unwrap()
        .to_owned();
    let before = p.request("resources/read", json!({"uri":uri}));
    assert_eq!(before["result"]["ttlMs"], 0);
    assert_eq!(before["result"]["cacheScope"], "private");
    let content = &before["result"]["contents"][0];
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(content["blob"].as_str().unwrap())
        .unwrap();
    image::load_from_memory(&bytes).unwrap();
    call(&mut p, "stop_browser", json!({}));
    let after = p.request("resources/read", json!({"uri":uri}));
    assert_eq!(before["result"]["contents"], after["result"]["contents"]);
    // Start again and let EOF, not stop_browser, exercise managed cleanup.
    call(
        &mut p,
        "start_browser",
        json!({"executable":executable,"profile":"temporary"}),
    );
    let eof_pid: i32 = std::fs::read_to_string(p.root.path().join("browser.pid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let began = Instant::now();
    p.finish(true);
    assert!(began.elapsed() < Duration::from_secs(35));
    for pid in [browser_pid, eof_pid] {
        assert_eq!(
            unsafe { libc::kill(pid, 0) },
            -1,
            "managed Chrome remained alive after cleanup"
        );
    }
    if let Ok(dir) = std::env::var("KROMETRAIL_MCP_EVIDENCE_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        let revision = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .output()
            .unwrap();
        let receipt = json!({"revision":String::from_utf8(revision.stdout).unwrap().trim(),"platform":std::env::consts::OS,"server":discovery["result"]["_meta"]["io.modelcontextprotocol/serverInfo"],"protocol":"2026-07-28","tool_count":tools.len(),"screenshot_bytes":screenshot_bytes.len(),"artifact_bytes":bytes.len(),"retained_after_stop":true,"managed_eof_cleanup":true});
        std::fs::write(
            Path::new(&dir).join(format!("mcp-{}.json", std::env::consts::OS)),
            serde_json::to_vec_pretty(&receipt).unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn attached_browser_survives_mcp_eof() {
    if std::env::var("KROMETRAIL_REAL_CHROME_TESTS").as_deref() != Ok("1") {
        eprintln!("real Chrome qualification not requested");
        return;
    }
    struct External(std::process::Child);
    impl Drop for External {
        fn drop(&mut self) {
            unsafe {
                libc::kill(self.0.id() as i32, libc::SIGTERM);
            }
            let deadline = Instant::now() + Duration::from_secs(5);
            while matches!(self.0.try_wait(), Ok(None)) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut p = McpProcess::start();
    let executable = wrapper(p.root.path());
    let profile = p.root.path().join("external-profile");
    std::fs::create_dir_all(&profile).unwrap();
    let mut external = External(
        std::process::Command::new(executable)
            .arg(format!("--user-data-dir={}", profile.display()))
            .args([
                "--remote-debugging-port=0",
                "--no-first-run",
                "--no-default-browser-check",
                "about:blank",
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    );
    let port_file = profile.join("DevToolsActivePort");
    let deadline = Instant::now() + Duration::from_secs(15);
    while !port_file.is_file() {
        assert!(
            external.0.try_wait().unwrap().is_none(),
            "external Chrome exited"
        );
        assert!(
            Instant::now() < deadline,
            "external Chrome did not publish its port"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    let port: u16 = std::fs::read_to_string(port_file)
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    p.initialize("2025-11-25");
    let result = call(
        &mut p,
        "attach_browser",
        json!({"endpoint":format!("http://127.0.0.1:{port}"), "response":{"detail":"full"}}),
    );
    assert_eq!(
        result["result"]["structuredContent"]["result"]["ownership"],
        "attached"
    );
    image(&call(
        &mut p,
        "take_screenshot",
        json!({"target":{"kind":"viewport"},"format":"png"}),
    ));
    p.finish(true);
    assert!(
        external.0.try_wait().unwrap().is_none(),
        "MCP closed externally owned Chrome"
    );
}
