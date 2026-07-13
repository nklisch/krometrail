//! Owned child/process-group lifecycle.

use std::{
    process::{Child, Command},
    time::Duration,
};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SanitizedProcessExit {
    Code(i32),
    Signaled,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessTermination {
    pub exit: SanitizedProcessExit,
}

#[derive(Debug, Error)]
pub enum ProcessError {
    #[error("process spawn failed")]
    SpawnFailed,
    #[error("process termination failed")]
    TerminationFailed,
    #[error("process termination timed out")]
    Timeout,
}

/// The sole authority for terminating a managed browser tree.
pub struct ManagedChromeProcess {
    child: Option<Child>,
    pid: u32,
    process_group: bool,
}

impl ManagedChromeProcess {
    pub fn spawn(command: &mut Command) -> Result<Self, ProcessError> {
        Self::configure_process_group(command);
        let child = command.spawn().map_err(|_| ProcessError::SpawnFailed)?;
        Ok(Self::from_child(child))
    }

    pub(crate) fn from_child(child: Child) -> Self {
        let pid = child.id();
        Self {
            child: Some(child),
            pid,
            process_group: cfg!(unix),
        }
    }

    pub fn child_id(&self) -> u32 {
        self.pid
    }

    pub fn is_alive(&mut self) -> bool {
        self.child
            .as_mut()
            .is_some_and(|child| child.try_wait().ok().flatten().is_none())
    }

    /// Waits for the managed child without exposing raw ExitStatus or source details.
    pub async fn wait_for_termination(&mut self) -> Result<ProcessTermination, ProcessError> {
        loop {
            let Some(child) = self.child.as_mut() else {
                return Err(ProcessError::TerminationFailed);
            };
            match child.try_wait() {
                Ok(Some(status)) => {
                    self.child.take();
                    return Ok(ProcessTermination {
                        exit: sanitize_exit(status),
                    });
                }
                Ok(None) => tokio::time::sleep(Duration::from_millis(10)).await,
                Err(_) => return Err(ProcessError::TerminationFailed),
            }
        }
    }

    /// Gracefully asks the process group to exit, then escalates after the bounded grace period.
    pub async fn terminate(&mut self, grace: Duration) -> Result<ProcessTermination, ProcessError> {
        let Some(child) = self.child.as_mut() else {
            return Ok(ProcessTermination {
                exit: SanitizedProcessExit::Unknown,
            });
        };
        send_signal(self.pid, self.process_group, false);
        let deadline = tokio::time::Instant::now() + grace;
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    self.child.take();
                    return Ok(ProcessTermination {
                        exit: sanitize_exit(status),
                    });
                }
                Ok(None) if tokio::time::Instant::now() < deadline => {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                Ok(None) => {
                    send_signal(self.pid, self.process_group, true);
                    let status = child.wait().map_err(|_| ProcessError::TerminationFailed)?;
                    self.child.take();
                    return Ok(ProcessTermination {
                        exit: sanitize_exit(status),
                    });
                }
                Err(_) => return Err(ProcessError::TerminationFailed),
            }
        }
    }

    /// Cancellation/drop cannot await. It still kills only the captured PID/process group.
    pub(crate) fn force_kill_now(&mut self) {
        if let Some(child) = self.child.as_mut() {
            send_signal(self.pid, self.process_group, true);
            let _ = child.kill();
        }
        self.child.take();
    }

    pub(crate) fn configure_process_group(command: &mut Command) {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            // SAFETY: setpgid is called in the child before exec and does not touch Rust state.
            unsafe {
                command.pre_exec(|| {
                    if libc::setpgid(0, 0) == -1 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
        }
    }
}

impl Drop for ManagedChromeProcess {
    fn drop(&mut self) {
        self.force_kill_now();
    }
}

fn sanitize_exit(status: std::process::ExitStatus) -> SanitizedProcessExit {
    if let Some(code) = status.code() {
        SanitizedProcessExit::Code(code)
    } else if status.success() {
        SanitizedProcessExit::Code(0)
    } else {
        SanitizedProcessExit::Signaled
    }
}

fn send_signal(pid: u32, process_group: bool, force: bool) {
    #[cfg(unix)]
    {
        let signal = if force { libc::SIGKILL } else { libc::SIGTERM };
        let target = if process_group {
            -(pid as libc::pid_t)
        } else {
            pid as libc::pid_t
        };
        // A disappearing child is already clean from the ownership perspective.
        unsafe {
            libc::kill(target, signal);
        }
    }
    #[cfg(windows)]
    {
        if force {
            let _ = std::process::Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .status();
        }
        let _ = process_group;
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (pid, process_group, force);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Stdio;

    #[tokio::test]
    async fn child_exit_is_sanitized_without_a_source_string() {
        let mut command = if cfg!(windows) {
            let mut command = Command::new("cmd");
            command.args(["/C", "exit 7"]);
            command
        } else {
            let mut command = Command::new("sh");
            command.args(["-c", "exit 7"]);
            command
        };
        command.stdout(Stdio::null()).stderr(Stdio::null());
        let child = command.spawn().unwrap();
        let mut process = ManagedChromeProcess::from_child(child);
        assert_eq!(
            process.wait_for_termination().await.unwrap().exit,
            SanitizedProcessExit::Code(7)
        );
    }
}
