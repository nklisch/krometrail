use std::{
    ffi::OsString,
    io::{Seek as _, SeekFrom, Write as _},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    time::Duration,
};

use krometrail_store::{IndexStoreConfig, SqliteIndex};

/// Removes the scratch directory on scope exit, including during assertion
/// unwinds: a red doctor test must not leak its state directory or fake
/// executable into the shared temporary directory.
struct ScratchGuard(PathBuf);

impl ScratchGuard {
    fn new(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }
}

impl AsRef<Path> for ScratchGuard {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl Drop for ScratchGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A deterministic discovery candidate: doctor probes `--version` on this script,
/// which prints a parseable Chrome version without any real browser involvement.
#[cfg(unix)]
fn fixture_browser(root: &Path) -> PathBuf {
    let path = root.join("fixture-chrome");
    std::fs::write(&path, "#!/bin/sh\necho 'Google Chrome 123.4.5.6'\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).unwrap();
    }
    path
}

#[cfg(unix)]
fn run_doctor(data: &Path, environment: &[(&str, OsString)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_krometrail"));
    command.arg("doctor").env("KROMETRAIL_DATA_DIR", data);
    for (key, value) in environment {
        command.env(key, value);
    }
    command
        .output()
        .expect("krometrail binary should be executable")
}

/// Points discovery at the deterministic fixture so the doctor outcome does not
/// depend on which browsers happen to be installed on the test host.
#[cfg(unix)]
fn with_fixture_browser(root: &Path) -> (PathBuf, Vec<(&'static str, OsString)>) {
    let executable = fixture_browser(root);
    (
        executable.clone(),
        vec![("KROMETRAIL_CHROME", executable.into_os_string())],
    )
}

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

/// Doctor is a browser-discovery diagnostic, so it must leave every member of the
/// data root alone: the abandoned recording cache it finds, plus profiles,
/// configuration, downloads, and anything it does not recognize. The abandoned
/// instance root is named as a UUID, exactly the shape startup reclamation scans
/// for, so the previous doctor-through-runtime composition deleted it here.
#[test]
#[cfg(unix)]
fn doctor_preserves_abandoned_cache_and_unrelated_data_root_members_byte_for_byte() {
    let scratch = ScratchGuard::new(std::env::temp_dir().join(format!(
        "krometrail-doctor-preserve-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )));
    let data = scratch.as_ref().join("data");
    let abandoned = data.join("instances/00000000-0000-4000-8000-000000000001");
    std::fs::create_dir_all(abandoned.join("segments")).unwrap();
    std::fs::write(abandoned.join("index.sqlite3"), b"abandoned-index").unwrap();
    std::fs::write(
        abandoned.join("segments/frame-0001.bin"),
        b"abandoned-frame",
    )
    .unwrap();
    std::fs::create_dir_all(data.join("browser-profiles/default")).unwrap();
    std::fs::write(
        data.join("browser-profiles/default/profile"),
        b"preserve-profile",
    )
    .unwrap();
    std::fs::write(data.join("config.toml"), b"preserve-configuration").unwrap();
    std::fs::create_dir_all(data.join("browser-downloads")).unwrap();
    std::fs::write(
        data.join("browser-downloads/receipt.txt"),
        b"preserve-download",
    )
    .unwrap();
    std::fs::write(data.join("unknown-member.bin"), b"preserve-unknown").unwrap();
    let (_fixture, environment) = with_fixture_browser(scratch.as_ref());

    let output = run_doctor(&data, &environment);

    let stdout = text(&output.stdout);
    let stderr = text(&output.stderr);
    assert!(
        output.status.success(),
        "doctor must not be blocked by storage it does not need: {stderr}"
    );
    assert!(stdout.contains("browser available: "), "stdout: {stdout}");
    assert!(!stderr.contains("error["), "stderr: {stderr}");

    let root = abandoned;
    assert_eq!(
        std::fs::read(root.join("index.sqlite3")).unwrap(),
        b"abandoned-index",
        "the abandoned recording index was modified or removed by doctor"
    );
    assert_eq!(
        std::fs::read(root.join("segments/frame-0001.bin")).unwrap(),
        b"abandoned-frame",
        "abandoned recording segments were modified or removed by doctor"
    );
    assert!(
        !root.join(".owner.lock").exists(),
        "doctor claimed the abandoned instance root even though it must never own storage"
    );
    let roots: Vec<_> = std::fs::read_dir(data.join("instances"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(
        roots,
        vec![std::ffi::OsString::from(
            "00000000-0000-4000-8000-000000000001"
        )],
        "doctor must not claim a fresh instance root"
    );
    assert_eq!(
        std::fs::read(data.join("browser-profiles/default/profile")).unwrap(),
        b"preserve-profile"
    );
    assert_eq!(
        std::fs::read(data.join("config.toml")).unwrap(),
        b"preserve-configuration"
    );
    assert_eq!(
        std::fs::read(data.join("browser-downloads/receipt.txt")).unwrap(),
        b"preserve-download"
    );
    assert_eq!(
        std::fs::read(data.join("unknown-member.bin")).unwrap(),
        b"preserve-unknown"
    );
}

/// A storage root that cannot host recordings must not stop the browser
/// discovery answer. The data path is a regular file, so instance ownership and
/// every storage member below it are structurally impossible — a stronger
/// obstacle than a permission bit, and one that root cannot bypass.
#[test]
#[cfg(unix)]
fn doctor_succeeds_when_the_storage_root_is_structurally_unusable() {
    let scratch = ScratchGuard::new(std::env::temp_dir().join(format!(
        "krometrail-doctor-unusable-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )));
    std::fs::create_dir_all(scratch.as_ref()).unwrap();
    let data = scratch.as_ref().join("data");
    std::fs::write(&data, b"not a data directory").unwrap();
    let (_fixture, environment) = with_fixture_browser(scratch.as_ref());

    let output = run_doctor(&data, &environment);

    let stdout = text(&output.stdout);
    let stderr = text(&output.stderr);
    assert!(
        output.status.success(),
        "unusable recording storage must not fail doctor: {stderr}"
    );
    assert!(stdout.contains("browser available: "), "stdout: {stdout}");
    assert!(!stderr.contains("error["), "stderr: {stderr}");
}

/// A fresh data root must stay free of recording setup: no instance root, no
/// index, no segments, no profile or download scaffolding. Best-effort
/// diagnostic logging is the one documented data-root side effect doctor has.
#[test]
#[cfg(unix)]
fn doctor_never_creates_recording_setup_for_a_fresh_data_root() {
    let scratch = ScratchGuard::new(std::env::temp_dir().join(format!(
        "krometrail-doctor-fresh-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )));
    std::fs::create_dir_all(scratch.as_ref()).unwrap();
    let data = scratch.as_ref().join("data");
    let (_fixture, environment) = with_fixture_browser(scratch.as_ref());

    let output = run_doctor(&data, &environment);

    let stdout = text(&output.stdout);
    let stderr = text(&output.stderr);
    assert!(output.status.success(), "stderr: {stderr}");
    assert!(stdout.contains("browser available: "), "stdout: {stdout}");
    assert!(!stderr.contains("error["), "stderr: {stderr}");
    let mut members: Vec<String> = std::fs::read_dir(&data)
        .expect("doctor should only have created diagnostics state")
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    members.sort();
    assert_eq!(
        members,
        vec!["diagnostics"],
        "doctor created recording setup"
    );
    assert!(!data.join("instances").exists());
    assert!(!data.join("index.sqlite3").exists());
    assert!(!data.join("segments").exists());
    assert!(!data.join("browser-profiles").exists());
    assert!(!data.join("browser-downloads").exists());
}

/// Recording-only settings are not doctor's inputs. An invalid disk budget and
/// an invalid retention age must be ignored instead of failing a discovery
/// diagnostic that never opens storage.
#[test]
#[cfg(unix)]
fn doctor_ignores_invalid_recording_only_configuration() {
    let scratch = ScratchGuard::new(std::env::temp_dir().join(format!(
        "krometrail-doctor-config-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )));
    std::fs::create_dir_all(scratch.as_ref()).unwrap();
    let data = scratch.as_ref().join("data");
    let (_fixture, mut environment) = with_fixture_browser(scratch.as_ref());
    environment.push(("KROMETRAIL_DISK_BUDGET_BYTES", OsString::from("0")));
    environment.push((
        "KROMETRAIL_RETENTION_MAX_AGE_SECS",
        OsString::from("not-a-number"),
    ));

    let output = run_doctor(&data, &environment);

    let stdout = text(&output.stdout);
    let stderr = text(&output.stderr);
    assert!(
        output.status.success(),
        "invalid recording-only configuration must not fail doctor: {stderr}"
    );
    assert!(stdout.contains("browser available: "), "stdout: {stdout}");
    assert!(stderr.is_empty(), "stderr: {stderr}");
}

/// The guard must clean up during unwinds: a red doctor test panics before its
/// cleanup statements run, and its scratch state must still be removed. This
/// pins the Drop behavior directly instead of trusting happy-path cleanup.
#[test]
fn scratch_guard_removes_state_when_the_test_panics() {
    let scratch = std::env::temp_dir().join(format!(
        "krometrail-doctor-guard-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let result = std::panic::catch_unwind(|| {
        let _guard = ScratchGuard::new(&scratch);
        std::fs::create_dir_all(scratch.join("data")).unwrap();
        std::fs::write(scratch.join("data/state"), b"scratch").unwrap();
        panic!("simulated assertion failure before cleanup");
    });

    assert_eq!(
        result
            .expect_err("the simulated failure should unwind")
            .downcast_ref::<&str>()
            .copied(),
        Some("simulated assertion failure before cleanup")
    );
    assert!(
        !scratch.exists(),
        "the guard must remove scratch state during unwinding, not only on success"
    );
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
        13
    );

    // Managed browser profiles are not recording cache and must survive.
    assert_eq!(
        std::fs::read(data.join("browser-profiles/default/profile")).unwrap(),
        b"preserve"
    );
    std::fs::remove_dir_all(data).unwrap();
}

#[path = "support/mcp_process.rs"]
mod mcp_process;

#[test]
fn mcp_binary_initializes_lists_json_rpc_and_keeps_stderr_separate() {
    let mut process = mcp_process::McpProcess::start();
    assert_eq!(
        process.initialize("2025-06-18")["result"]["protocolVersion"],
        "2025-06-18"
    );
    let tools = process.tools();
    let expected = 6
        + krometrail_core::BROWSER_OPERATION_REGISTRY.len()
        + 1
        + krometrail_core::PROGRESSIVE_EVIDENCE_REGISTRY
            .iter()
            .filter(|d| d.exposure == krometrail_core::OperationExposure::Tool)
            .count()
        + krometrail_core::TEMPORAL_CONTEXT_OPERATION_REGISTRY.len();
    assert_eq!(tools.len(), expected);
    assert!(process.finish(true).is_empty());
}

#[test]
fn mcp_without_qualified_ffmpeg_keeps_the_still_surface_and_omits_video() {
    let mut process = mcp_process::McpProcess::start();
    process.initialize("2025-06-18");
    let tools = process.tools();
    assert!(tools.iter().any(|t| t["name"] == "temporal_debug_bundle"));
    assert!(!tools.iter().any(|t| t["name"] == "generate_temporal_video"));
    let templates = process.request("resources/templates/list", serde_json::json!({}));
    let names = templates["result"]["resourceTemplates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        vec![
            "temporal-artifact",
            "temporal-artifact-manifest",
            "temporal-source-frame",
            "managed-download"
        ]
    );
    assert!(process.finish(true).is_empty());
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
    // Ownership is claimed before storage opens. Wait for the files whose
    // survival this test checks, not just the earlier ownership-lock signal.
    for _ in 0..200 {
        if index.is_file() && root.join("segments").is_dir() {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(index.is_file(), "startup should create an index");
    assert!(
        root.join("segments").is_dir(),
        "startup should create its segments directory"
    );

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
