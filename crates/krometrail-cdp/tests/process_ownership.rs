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
