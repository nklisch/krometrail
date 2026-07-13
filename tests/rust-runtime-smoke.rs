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
    assert!(!stdout.to_ascii_lowercase().contains("dap"));
    assert!(!stdout.to_ascii_lowercase().contains("typescript"));
}

#[test]
fn doctor_reports_missing_browser_installation() {
    let output = run(&["doctor"]);
    let stderr = text(&output.stderr);
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr.contains("error[browser_not_found]"),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("no supported browser installation was found"),
        "stderr: {stderr}"
    );
    assert!(!stderr.contains("error[unsupported]"), "stderr: {stderr}");
    assert!(
        !stderr.contains("browser transport is not available"),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("recovery:"), "stderr: {stderr}");
    assert!(!stderr.to_ascii_lowercase().contains("bun"));
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}
