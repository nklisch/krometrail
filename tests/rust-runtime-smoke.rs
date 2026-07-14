use std::process::{Command, Output};

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

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}
