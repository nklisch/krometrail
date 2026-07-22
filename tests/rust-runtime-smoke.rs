use std::{
    io::{BufRead as _, BufReader, Read as _, Seek as _, SeekFrom, Write as _},
    process::{Command, Output, Stdio},
    time::Duration,
};

use krometrail_store::{IndexStoreConfig, SqliteIndex};

fn run(args: &[&str]) -> Output {
    let data = std::env::temp_dir().join(format!(
        "krometrail-smoke-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let output = Command::new(env!("CARGO_BIN_EXE_krometrail"))
        .args(args)
        .env("KROMETRAIL_DATA_DIR", &data)
        .output()
        .expect("krometrail binary should be executable");
    let _ = std::fs::remove_dir_all(data);
    output
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

/// Startup moves to an owned instance root and clears the pre-isolation flat
/// store, while leaving everything that is not recording cache alone.
///
/// The managed browser-profile assertion is the important one: profiles live in
/// the same data directory as the recording cache, are expensive to recreate,
/// and a wrongly scoped clear would silently destroy them.
#[test]
fn mcp_startup_clears_the_legacy_flat_store_and_owns_an_instance_root() {
    let data = std::env::temp_dir().join(format!(
        "krometrail-mcp-stale-cache-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let database_path = data.join("index.sqlite3");
    let segments_directory = data.join("segments");
    drop(
        SqliteIndex::open(IndexStoreConfig {
            database_path: database_path.clone(),
            segments_directory: segments_directory.clone(),
            busy_timeout: Duration::from_millis(250),
        })
        .unwrap(),
    );

    let mut database = std::fs::OpenOptions::new()
        .write(true)
        .open(&database_path)
        .unwrap();
    database.seek(SeekFrom::Start(60)).unwrap();
    database.write_all(&6_u32.to_be_bytes()).unwrap();
    database.sync_all().unwrap();
    drop(database);
    std::fs::write(segments_directory.join("stale.segment"), b"stale").unwrap();
    std::fs::create_dir_all(data.join("browser-profiles/default")).unwrap();
    std::fs::write(data.join("browser-profiles/default/profile"), b"preserve").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_krometrail"))
        .arg("mcp")
        .env("KROMETRAIL_DATA_DIR", &data)
        .output()
        .expect("MCP binary should replace stale cache and exit after stdin EOF");

    assert!(output.status.success(), "stderr: {}", text(&output.stderr));
    assert!(output.stdout.is_empty(), "stdout: {}", text(&output.stdout));
    assert!(output.stderr.is_empty(), "stderr: {}", text(&output.stderr));

    // The legacy flat store is gone rather than upgraded in place.
    assert!(!database_path.exists());
    assert!(!segments_directory.exists());

    // Exactly one instance root was claimed, carrying the current schema.
    let roots: Vec<_> = std::fs::read_dir(data.join("instances"))
        .expect("startup should create an instance directory")
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_dir())
        .collect();
    assert_eq!(roots.len(), 1, "expected one instance root, got {roots:?}");
    let instance_database = std::fs::read(roots[0].join("index.sqlite3")).unwrap();
    assert_eq!(
        u32::from_be_bytes(instance_database[60..64].try_into().unwrap()),
        9
    );

    // Managed browser profiles are not recording cache and must survive.
    assert_eq!(
        std::fs::read(data.join("browser-profiles/default/profile")).unwrap(),
        b"preserve"
    );
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
    let expected_tools = 6
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
            "temporal-source-frame",
            "managed-download"
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

/// A running process must hold its instance root's lock for as long as it lives.
///
/// The store-level tests bind the ownership guard to a live local, so they prove
/// the primitive and never the application. This is the assertion that was
/// missing: with the guard dropped at the end of bootstrap, a live root reads as
/// abandoned, and the next process to start reclaims it — deleting the running
/// process's index and segments while it keeps writing to the deleted inodes.
#[test]
fn mcp_holds_its_instance_lock_for_the_life_of_the_process() {
    if !krometrail_store::OWNERSHIP_IS_ENFORCED {
        // Without provable ownership every root reads as live, so there is no
        // claim to make in either direction and nothing here is meaningful.
        return;
    }
    let data = std::env::temp_dir().join(format!(
        "krometrail-instance-lock-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut child = Command::new(env!("CARGO_BIN_EXE_krometrail"))
        .arg("mcp")
        .env("KROMETRAIL_DATA_DIR", &data)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("MCP binary should spawn");

    let root = wait_for_instance_root(&data.join("instances"));

    // While the process runs, its root is not claimable by anyone else.
    let claim = krometrail_store::InstanceOwnership::acquire_existing(&root)
        .expect("claiming a live instance root should not error");
    assert!(
        claim.is_none(),
        "a running process's instance root was claimable, so any second process \
         would reclaim it and delete this one's evidence: {root:?}"
    );
    drop(claim);

    // Exit releases it, so an abandoned root stays reclaimable.
    drop(child.stdin.take());
    let status = child.wait().unwrap();
    assert!(status.success());
    let claim = krometrail_store::InstanceOwnership::acquire_existing(&root)
        .expect("claiming an abandoned instance root should not error");
    assert!(
        claim.is_some(),
        "an exited process's instance root stayed locked: {root:?}"
    );
    drop(claim);
    let _ = std::fs::remove_dir_all(data);
}

fn wait_for_instance_root(instances: &std::path::Path) -> std::path::PathBuf {
    for _ in 0..200 {
        if let Ok(entries) = std::fs::read_dir(instances) {
            let roots: Vec<_> = entries
                .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                .filter(|path| path.is_dir())
                .collect();
            // Wait for the lock file too: the root exists for a moment before it
            // is claimed, and reading it in that window would test nothing.
            if let Some(root) = roots.first()
                && root.join(".owner.lock").is_file()
            {
                return root.clone();
            }
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!("startup never created a locked instance root under {instances:?}");
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// A second process must not disturb a running one's capture.
///
/// This is `docs/SPEC.md`'s isolation guarantee stated as a test. The lock test
/// above proves the mechanism; this proves the consequence, which is the thing
/// that actually broke: three throwaway starts against a live instance's data
/// directory used to delete that instance's index and segments, leaving it
/// writing to deleted inodes while reporting healthy.
#[test]
fn a_second_process_leaves_a_running_instance_store_intact() {
    if !krometrail_store::OWNERSHIP_IS_ENFORCED {
        return;
    }
    let data = std::env::temp_dir().join(format!(
        "krometrail-instance-isolation-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut victim = Command::new(env!("CARGO_BIN_EXE_krometrail"))
        .arg("mcp")
        .env("KROMETRAIL_DATA_DIR", &data)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("MCP binary should spawn");
    let root = wait_for_instance_root(&data.join("instances"));
    let index = root.join("index.sqlite3");
    assert!(index.is_file(), "startup should create an index");

    // Exactly what destroyed the store before: other processes running their
    // ordinary startup, including abandoned-root reclamation.
    for _ in 0..3 {
        let output = Command::new(env!("CARGO_BIN_EXE_krometrail"))
            .arg("mcp")
            .env("KROMETRAIL_DATA_DIR", &data)
            .output()
            .expect("a second MCP process should run and exit");
        assert!(output.status.success(), "stderr: {}", text(&output.stderr));
    }

    assert!(
        index.is_file(),
        "a running instance's index was deleted by another process: {index:?}"
    );
    assert!(
        root.join("segments").is_dir(),
        "a running instance's segments were deleted by another process"
    );

    drop(victim.stdin.take());
    assert!(victim.wait().unwrap().success());
    let _ = std::fs::remove_dir_all(data);
}
