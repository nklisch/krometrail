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
