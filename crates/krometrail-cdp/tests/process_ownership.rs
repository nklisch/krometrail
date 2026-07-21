use krometrail_cdp::{ManagedChromeProcess, ProcessTermination, SanitizedProcessExit};
use std::{
    process::{Command, Stdio},
    time::Duration,
};

fn long_running_command() -> Command {
    if cfg!(windows) {
        let mut command = Command::new("cmd");
        command.args(["/C", "ping 127.0.0.1 -n 30 > NUL"]);
        command
    } else {
        let mut command = Command::new("sh");
        command.args(["-c", "trap '' TERM; sleep 30"]);
        command
    }
}

#[tokio::test]
async fn managed_process_owns_termination_and_reports_sanitized_exit() {
    let mut command = long_running_command();
    command.stdout(Stdio::null()).stderr(Stdio::null());
    let mut process = ManagedChromeProcess::spawn(&mut command).unwrap();
    assert!(process.is_alive());
    let termination = process.terminate(Duration::from_millis(20)).await.unwrap();
    assert!(matches!(
        termination,
        ProcessTermination {
            exit: SanitizedProcessExit::Signaled | SanitizedProcessExit::Code(_)
        }
    ));
    assert!(!process.is_alive());
}

#[cfg(unix)]
fn leader_exits_but_descendant_ignores_term_command() -> Command {
    let mut command = Command::new("sh");
    command.args([
        "-c",
        "(trap '' TERM; sleep 30) & helper=$!; trap 'exit 0' TERM; wait $helper",
    ]);
    command
}

#[cfg(unix)]
fn process_group_exists(pgid: u32) -> bool {
    let Ok(pgid) = libc::pid_t::try_from(pgid) else {
        return false;
    };
    let result = unsafe { libc::kill(-pgid, 0) };
    result == 0
        || (result == -1 && std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM))
}

#[cfg(unix)]
#[tokio::test]
async fn terminate_reaps_descendants_after_group_leader_exits() {
    let mut command = leader_exits_but_descendant_ignores_term_command();
    command.stdout(Stdio::null()).stderr(Stdio::null());
    let mut process = ManagedChromeProcess::spawn(&mut command).unwrap();
    let pgid = process.child_id();
    tokio::time::sleep(Duration::from_millis(25)).await;

    process.terminate(Duration::from_millis(50)).await.unwrap();

    assert!(
        !process_group_exists(pgid),
        "managed descendant group survived terminate"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn drop_reaps_descendants_after_group_leader_exits() {
    let mut command = leader_exits_but_descendant_ignores_term_command();
    command.stdout(Stdio::null()).stderr(Stdio::null());
    let process = ManagedChromeProcess::spawn(&mut command).unwrap();
    let pgid = process.child_id();
    tokio::time::sleep(Duration::from_millis(25)).await;
    drop(process);

    assert!(
        !process_group_exists(pgid),
        "managed descendant group survived drop"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn natural_leader_exit_cleans_descendants_before_reporting_completion() {
    let mut command = Command::new("sh");
    command.args(["-c", "(trap '' TERM; sleep 30) & exit 0"]);
    command.stdout(Stdio::null()).stderr(Stdio::null());
    let mut process = ManagedChromeProcess::spawn(&mut command).unwrap();
    let pgid = process.child_id();

    process.wait_for_termination().await.unwrap();

    assert!(
        !process_group_exists(pgid),
        "natural leader exit reported completion while a helper remained"
    );
}

/// `Z` for a reaped-but-unwaited zombie, `None` once the pid is gone entirely. A plain
/// `kill(pid, 0)` cannot be used here: the leaked guard below never waits, so a dead child
/// lingers as a zombie that still answers signal-zero probes.
#[cfg(target_os = "linux")]
fn process_state(pid: u32) -> Option<char> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // `comm` is parenthesized and may itself contain spaces, so fields are read after its close.
    let tail = stat.rsplit_once(')')?.1;
    tail.split_whitespace().next()?.chars().next()
}

/// The evaluation harness leaked Chrome processes for days because a SIGKILLed launcher runs no
/// `Drop`, no `terminate`, and no process-group kill. PDEATHSIG is delivered on death of the
/// *forking thread*, so a dedicated thread that spawns and then exits while leaking the guard
/// reproduces that path exactly: teardown code never executes, and only the kernel can still
/// reap the browser.
#[cfg(target_os = "linux")]
#[test]
fn managed_child_dies_when_its_launcher_never_runs_teardown() {
    let pid = std::thread::spawn(|| {
        let mut command = long_running_command();
        command.stdout(Stdio::null()).stderr(Stdio::null());
        let process = ManagedChromeProcess::spawn(&mut command).unwrap();
        let pid = process.child_id();
        // Settle past fork/exec, then require a live child. Without this the test could pass
        // vacuously on a child that never started or exited for an unrelated reason.
        std::thread::sleep(Duration::from_millis(100));
        assert!(
            matches!(process_state(pid), Some(state) if state != 'Z'),
            "child must be alive while its launcher thread still is"
        );
        // Leak the guard: no Drop, no kill, no group signal — the orphan path.
        std::mem::forget(process);
        pid
    })
    .join()
    .unwrap();

    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        // The command traps SIGTERM and sleeps 30s, so anything but SIGKILL leaves it running.
        if matches!(process_state(pid), Some('Z') | None) {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("browser survived a launcher that ran no teardown; parent-death signal not armed");
}

#[tokio::test]
async fn natural_child_death_is_distinct_from_transport_data() {
    let mut command = if cfg!(windows) {
        let mut command = Command::new("cmd");
        command.args(["/C", "exit 9"]);
        command
    } else {
        let mut command = Command::new("sh");
        command.args(["-c", "exit 9"]);
        command
    };
    let mut process = ManagedChromeProcess::spawn(&mut command).unwrap();
    let termination = process.wait_for_termination().await.unwrap();
    assert_eq!(termination.exit, SanitizedProcessExit::Code(9));
}
