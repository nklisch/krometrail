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
    /// The PGID captured after spawning an isolated child. `None` means the child was supplied
    /// externally (or process-group setup was unavailable), so only its direct `Child` may be
    /// signaled.
    process_group: Option<u32>,
}

impl ManagedChromeProcess {
    pub fn spawn(command: &mut Command) -> Result<Self, ProcessError> {
        Self::configure_process_group(command);
        let child = command.spawn().map_err(|_| ProcessError::SpawnFailed)?;
        let pid = child.id();
        let process_group = owned_process_group(pid);
        Ok(Self {
            child: Some(child),
            pid,
            process_group,
        })
    }

    #[cfg(test)]
    pub(crate) fn from_child(child: Child) -> Self {
        let pid = child.id();
        Self {
            child: Some(child),
            pid,
            process_group: None,
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

    /// Takes an already-observed child exit without awaiting or exposing the platform status.
    #[cfg(feature = "cdpkit-transport")]
    pub(crate) fn termination_if_exited(&mut self) -> Option<ProcessTermination> {
        let child = self.child.as_mut()?;
        match child.try_wait() {
            Ok(Some(status)) => {
                // A browser can leave helpers behind even on natural exit. Reuse the same
                // ownership-checked force path before removing the process guard from supervision.
                self.force_kill_now();
                Some(ProcessTermination {
                    exit: sanitize_exit(status),
                })
            }
            _ => None,
        }
    }

    /// Waits for the managed child without exposing raw ExitStatus or source details.
    pub async fn wait_for_termination(&mut self) -> Result<ProcessTermination, ProcessError> {
        loop {
            let Some(child) = self.child.as_mut() else {
                return Err(ProcessError::TerminationFailed);
            };
            match child.try_wait() {
                Ok(Some(status)) => {
                    self.force_kill_now();
                    return Ok(ProcessTermination {
                        exit: sanitize_exit(status),
                    });
                }
                Ok(None) => tokio::time::sleep(Duration::from_millis(10)).await,
                Err(_) => return Err(ProcessError::TerminationFailed),
            }
        }
    }

    /// Gracefully asks the isolated process group to exit, then escalates after the bounded grace
    /// period. A browser leader can exit before helpers do, so direct-child reaping is deliberately
    /// followed by group cleanup and a positive no-members check.
    pub async fn terminate(&mut self, grace: Duration) -> Result<ProcessTermination, ProcessError> {
        if self.child.is_none() {
            return Ok(ProcessTermination {
                exit: SanitizedProcessExit::Unknown,
            });
        }

        let deadline = tokio::time::Instant::now() + grace;
        let direct_alive = self.child_is_alive()?;
        if !self.signal_group(false, direct_alive) {
            self.kill_direct_if_alive();
        }

        let status = loop {
            let result = self
                .child
                .as_mut()
                .ok_or(ProcessError::TerminationFailed)?
                .try_wait();
            match result {
                Ok(Some(status)) => break status,
                Ok(None) if tokio::time::Instant::now() < deadline => {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                Ok(None) => {
                    // The direct child ignored TERM (or a helper kept the group alive). Force the
                    // owned target before waiting, then reap the direct child below.
                    if !self.signal_group(true, true) {
                        self.kill_direct_if_alive();
                    }
                    break self
                        .child
                        .as_mut()
                        .ok_or(ProcessError::TerminationFailed)?
                        .wait()
                        .map_err(|_| ProcessError::TerminationFailed)?;
                }
                Err(_) => return Err(ProcessError::TerminationFailed),
            }
        };
        self.child.take();

        self.finish_group_termination(deadline, grace).await?;
        Ok(ProcessTermination {
            exit: sanitize_exit(status),
        })
    }

    /// Cancellation/drop cannot await, but it must still reap the direct child. The group is
    /// force-killed only while the captured PGID is demonstrably ours; after the leader exits,
    /// `/proc` membership prevents a recycled PID/PGID from becoming an unrelated signal target.
    pub(crate) fn force_kill_now(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        let direct_alive = matches!(child.try_wait(), Ok(None));
        let group_signaled = self.signal_group(true, direct_alive);
        if !group_signaled && direct_alive && matches!(child.try_wait(), Ok(None)) {
            let _ = child.kill();
        }
        let _ = child.wait();

        // Drop has no async budget, but do not release a profile while a known group member is
        // still live. SIGKILL is reliable for ordinary userspace processes; the bound prevents
        // cancellation cleanup from hanging forever on an uninterruptible kernel task.
        if group_signaled {
            let deadline = std::time::Instant::now() + Duration::from_secs(1);
            while self.group_has_members() && std::time::Instant::now() < deadline {
                let _ = self.signal_group(true, false);
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        self.process_group = None;
    }

    async fn finish_group_termination(
        &mut self,
        deadline: tokio::time::Instant,
        grace: Duration,
    ) -> Result<(), ProcessError> {
        while self.group_has_members() && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        if self.group_has_members() {
            if !self.signal_group(true, false) {
                return Err(ProcessError::TerminationFailed);
            }
            let force_deadline = tokio::time::Instant::now()
                + grace
                    .max(Duration::from_millis(100))
                    .min(Duration::from_secs(1));
            while self.group_has_members() && tokio::time::Instant::now() < force_deadline {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            if self.group_has_members() {
                return Err(ProcessError::Timeout);
            }
        }
        self.process_group = None;
        Ok(())
    }

    fn group_has_members(&self) -> bool {
        self.process_group.is_some_and(process_group_has_members)
    }

    fn signal_group(&self, force: bool, direct_child_alive: bool) -> bool {
        let Some(pgid) = self.process_group else {
            return false;
        };
        if !process_group_is_owned(self.pid, pgid, direct_child_alive) {
            return false;
        }
        signal_process_group(pgid, force)
    }

    fn child_is_alive(&mut self) -> Result<bool, ProcessError> {
        self.child
            .as_mut()
            .ok_or(ProcessError::TerminationFailed)?
            .try_wait()
            .map(|status| status.is_none())
            .map_err(|_| ProcessError::TerminationFailed)
    }

    fn kill_direct_if_alive(&mut self) {
        if let Some(child) = self.child.as_mut()
            && matches!(child.try_wait(), Ok(None))
        {
            let _ = child.kill();
        }
    }

    pub(crate) fn configure_process_group(command: &mut Command) {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            // The standard API performs setpgid in the child before exec, avoiding a parent-side
            // race where a fast browser could exit before its group was isolated.
            command.process_group(0);
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

#[cfg(unix)]
fn owned_process_group(pid: u32) -> Option<u32> {
    let pid = libc::pid_t::try_from(pid).ok()?;
    let pgid = unsafe { libc::getpgid(pid) };
    if pgid > 0 && pgid == pid {
        return Some(pgid as u32);
    }
    // A very short-lived leader may exit between spawn and this validation while its helper is
    // still alive. The expected PGID is safe to retain only when a member currently proves that
    // the isolated group still exists; an empty/recycled PID is never adopted as ownership.
    process_group_has_members(pid as u32).then_some(pid as u32)
}

#[cfg(not(unix))]
fn owned_process_group(_pid: u32) -> Option<u32> {
    None
}

#[cfg(unix)]
fn process_group_is_owned(child_pid: u32, pgid: u32, direct_child_alive: bool) -> bool {
    let Ok(child_pid) = libc::pid_t::try_from(child_pid) else {
        return false;
    };
    let Ok(pgid) = libc::pid_t::try_from(pgid) else {
        return false;
    };
    if pgid <= 0 {
        return false;
    }
    // While the leader exists, getpgid proves that the PID still names our child and that it
    // remains in the PGID captured at spawn. Once it exits, the leader PID may be reused, so only
    // a currently populated captured group can be signaled. An empty group is never signaled.
    if direct_child_alive {
        (unsafe { libc::getpgid(child_pid) == pgid }) && process_group_has_members(pgid as u32)
    } else {
        process_group_has_members(pgid as u32)
    }
}

#[cfg(not(unix))]
fn process_group_is_owned(_child_pid: u32, _pgid: u32, _direct_child_alive: bool) -> bool {
    false
}

#[cfg(unix)]
fn signal_process_group(pgid: u32, force: bool) -> bool {
    let Ok(pgid) = libc::pid_t::try_from(pgid) else {
        return false;
    };
    let signal = if force { libc::SIGKILL } else { libc::SIGTERM };
    pgid > 0 && unsafe { libc::kill(-pgid, signal) == 0 }
}

#[cfg(not(unix))]
fn signal_process_group(_pgid: u32, _force: bool) -> bool {
    false
}

#[cfg(target_os = "linux")]
fn process_group_has_members(pgid: u32) -> bool {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return false;
    };
    entries.flatten().any(|entry| {
        if entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
            .is_none()
        {
            return false;
        }
        let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) else {
            return false;
        };
        let Some((_, fields)) = stat.rsplit_once(") ") else {
            return false;
        };
        let mut fields = fields.split_whitespace();
        let _state = fields.next();
        let _parent_pid = fields.next();
        fields
            .next()
            .and_then(|group| group.parse::<u32>().ok())
            .is_some_and(|group| group == pgid)
    })
}

#[cfg(all(unix, not(target_os = "linux")))]
fn process_group_has_members(pgid: u32) -> bool {
    let Ok(pgid) = libc::pid_t::try_from(pgid) else {
        return false;
    };
    if pgid <= 0 {
        return false;
    }
    let result = unsafe { libc::kill(-pgid, 0) };
    result == 0
        || (result == -1 && std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM))
}

#[cfg(not(unix))]
fn process_group_has_members(_pgid: u32) -> bool {
    false
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
