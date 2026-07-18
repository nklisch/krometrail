use std::{
    io::{BufRead as _, BufReader, Read as _, Write as _},
    process::{Command, Output, Stdio},
};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_krometrail"))
        .args(args)
        .output()
        .expect("krometrail binary should be executable")
}

#[test]
fn version_is_cargo_version_and_succeeds() {
    let output = run(&["--version"]);
    assert!(output.status.success(), "stderr: {}", text(&output.stderr));
    assert_eq!(
        text(&output.stdout).trim(),
        format!("krometrail {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn help_is_truthful_and_succeeds() {
    let output = run(&["--help"]);
    let stdout = text(&output.stdout);
    assert!(output.status.success(), "stderr: {}", text(&output.stderr));
    assert!(stdout.contains("Usage: krometrail"));
    assert!(stdout.contains("doctor"));
    assert!(stdout.contains("mcp"));
    assert!(!stdout.to_ascii_lowercase().contains("dap"));
    assert!(!stdout.to_ascii_lowercase().contains("typescript"));
}

#[test]
fn doctor_reports_only_the_production_discovery_outcomes() {
    let output = run(&["doctor"]);
    let stdout = text(&output.stdout);
    let stderr = text(&output.stderr);
    assert!(!stdout.to_ascii_lowercase().contains("unsupported"));
    assert!(!stderr.contains("error[unsupported]"), "stderr: {stderr}");
    assert!(
        !stderr.contains("browser transport is not available"),
        "stderr: {stderr}"
    );
    assert!(!stderr.to_ascii_lowercase().contains("bun"));

    if output.status.success() {
        assert!(stdout.contains("browser available:"), "stdout: {stdout}");
        assert!(stderr.is_empty(), "stderr: {stderr}");
    } else {
        assert_eq!(output.status.code(), Some(1));
        assert!(
            stderr.contains("error[browser_not_found]"),
            "stderr: {stderr}"
        );
        assert!(
            stderr.contains("no supported browser installation was found"),
            "stderr: {stderr}"
        );
        assert!(stderr.contains("recovery:"), "stderr: {stderr}");
    }
}

#[test]
fn mcp_eof_exits_cleanly_without_non_protocol_output() {
    let data = std::env::temp_dir().join(format!(
        "krometrail-mcp-eof-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let output = Command::new(env!("CARGO_BIN_EXE_krometrail"))
        .arg("mcp")
        .env("KROMETRAIL_DATA_DIR", &data)
        .output()
        .expect("MCP binary should exit after stdin EOF");
    assert!(output.status.success(), "stderr: {}", text(&output.stderr));
    assert!(output.stdout.is_empty(), "stdout: {}", text(&output.stdout));
    assert!(output.stderr.is_empty(), "stderr: {}", text(&output.stderr));
    std::fs::remove_dir_all(data).unwrap();
}

#[test]
fn mcp_binary_initializes_lists_json_rpc_and_keeps_stderr_separate() {
    let data = std::env::temp_dir().join(format!(
        "krometrail-mcp-protocol-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut child = Command::new(env!("CARGO_BIN_EXE_krometrail"))
        .arg("mcp")
        .env("KROMETRAIL_DATA_DIR", &data)
        .env("KROMETRAIL_FFMPEG_PATH", data.join("missing-ffmpeg"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("MCP binary should spawn");
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut first = String::new();
    {
        let stdin = child.stdin.as_mut().unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({
                "jsonrpc":"2.0","id":1,"method":"initialize","params":{
                    "protocolVersion":"2025-06-18","capabilities":{},
                    "clientInfo":{"name":"binary-smoke","version":"1"}
                }
            })
        )
        .unwrap();
        stdin.flush().unwrap();
    }
    stdout.read_line(&mut first).unwrap();
    let initialized: serde_json::Value = serde_json::from_str(first.trim()).unwrap();
    assert_eq!(initialized["id"], 1);
    assert_eq!(initialized["result"]["protocolVersion"], "2025-06-18");

    {
        let stdin = child.stdin.as_mut().unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({
                "jsonrpc":"2.0","method":"notifications/initialized"
            })
        )
        .unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({
                "jsonrpc":"2.0","id":2,"method":"tools/list","params":{}
            })
        )
        .unwrap();
        stdin.flush().unwrap();
    }
    let mut second = String::new();
    stdout.read_line(&mut second).unwrap();
    let listed: serde_json::Value = serde_json::from_str(second.trim()).unwrap();
    assert_eq!(listed["id"], 2);
    let expected_tools = 4
        + krometrail_core::BROWSER_OPERATION_REGISTRY.len()
        + 1
        + krometrail_core::PROGRESSIVE_EVIDENCE_REGISTRY
            .iter()
            .filter(|definition| definition.exposure == krometrail_core::OperationExposure::Tool)
            .count()
        + krometrail_core::TEMPORAL_CONTEXT_OPERATION_REGISTRY.len();
    assert_eq!(
        listed["result"]["tools"].as_array().unwrap().len(),
        expected_tools
    );

    drop(child.stdin.take());
    let mut trailing_stdout = String::new();
    stdout.read_to_string(&mut trailing_stdout).unwrap();
    let status = child.wait().unwrap();
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    assert!(status.success(), "stderr: {stderr}");
    assert!(stderr.is_empty(), "stderr: {stderr}");
    assert!(
        trailing_stdout.trim().is_empty(),
        "stdout: {trailing_stdout}"
    );
    std::fs::remove_dir_all(data).unwrap();
}

#[test]
fn mcp_without_qualified_ffmpeg_keeps_the_still_surface_and_omits_video() {
    let data = std::env::temp_dir().join(format!(
        "krometrail-mcp-no-ffmpeg-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut child = Command::new(env!("CARGO_BIN_EXE_krometrail"))
        .arg("mcp")
        .env("KROMETRAIL_DATA_DIR", &data)
        .env("KROMETRAIL_FFMPEG_PATH", data.join("missing-ffmpeg"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("MCP binary should spawn without FFmpeg");
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let stdin = child.stdin.as_mut().unwrap();
    for request in [
        serde_json::json!({
            "jsonrpc":"2.0","id":1,"method":"initialize","params":{
                "protocolVersion":"2025-06-18","capabilities":{},
                "clientInfo":{"name":"no-ffmpeg-smoke","version":"1"}
            }
        }),
        serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
        serde_json::json!({
            "jsonrpc":"2.0","id":3,"method":"resources/templates/list","params":{}
        }),
    ] {
        writeln!(stdin, "{request}").unwrap();
    }
    stdin.flush().unwrap();

    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(line.trim()).unwrap()["id"],
        1
    );
    line.clear();
    stdout.read_line(&mut line).unwrap();
    let listed: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
    let tool_names = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(tool_names.contains(&"temporal_debug_bundle"));
    assert!(!tool_names.contains(&"generate_temporal_video"));
    line.clear();
    stdout.read_line(&mut line).unwrap();
    let templates: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
    let template_names = templates["result"]["resourceTemplates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|template| template["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        template_names,
        vec![
            "temporal-artifact",
            "temporal-artifact-manifest",
            "temporal-source-frame"
        ]
    );

    drop(child.stdin.take());
    let status = child.wait().unwrap();
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    assert!(status.success(), "stderr: {stderr}");
    assert!(stderr.is_empty(), "stderr: {stderr}");
    std::fs::remove_dir_all(data).unwrap();
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}
