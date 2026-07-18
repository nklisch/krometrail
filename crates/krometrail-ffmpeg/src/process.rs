use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::{ExitStatus, Stdio},
    time::{Duration, Instant},
};

use sha2::{Digest, Sha256};
use tokio::{io::AsyncReadExt, process::Command};

use crate::{
    error::{AdapterFailure, AdapterFailureKind, AdapterFailureStage},
    policy::FFMPEG_TERMINATION_GRACE,
};

pub(crate) struct ProcessLimits {
    pub(crate) deadline: Instant,
    pub(crate) cpu_seconds: u64,
    pub(crate) stdout_bytes: usize,
    pub(crate) stderr_bytes: usize,
}

pub(crate) enum FfmpegInvocation<'a> {
    VersionProbe,
    Encode {
        arguments: &'a [OsString],
        working_directory: &'a Path,
        output_path: PathBuf,
        output_limit: u64,
    },
}

pub(crate) struct SanitizedProcessOutcome {
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
    pub(crate) diagnostic_sha256: [u8; 32],
}

impl std::fmt::Debug for SanitizedProcessOutcome {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SanitizedProcessOutcome")
            .field("stdout_bytes", &self.stdout.len())
            .field("stderr_bytes", &self.stderr.len())
            .field("diagnostic_sha256", &HexDigest(self.diagnostic_sha256))
            .finish()
    }
}

struct HexDigest([u8; 32]);

impl std::fmt::Debug for HexDigest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

pub(crate) struct ManagedFfmpegProcess {
    child: Option<tokio::process::Child>,
    tree: ProcessTreeGuard,
    stdout: Option<tokio::process::ChildStdout>,
    stderr: Option<tokio::process::ChildStderr>,
    output_watch: Option<(PathBuf, u64)>,
}

impl ManagedFfmpegProcess {
    pub(crate) async fn spawn(
        executable: &Path,
        invocation: FfmpegInvocation<'_>,
        limits: ProcessLimits,
    ) -> Result<Self, AdapterFailure> {
        let mut command = Command::new(executable);
        let output_watch = match invocation {
            FfmpegInvocation::VersionProbe => {
                command.args(["-nostdin", "-version"]);
                None
            }
            FfmpegInvocation::Encode {
                arguments,
                working_directory,
                output_path,
                output_limit,
            } => {
                command.args(arguments).current_dir(working_directory);
                Some((output_path, output_limit))
            }
        };
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .env_clear();
        preserve_platform_environment(&mut command);
        configure_tree_before_spawn(&mut command, limits.cpu_seconds)?;

        let mut child = command.spawn().map_err(|_| {
            AdapterFailure::new(AdapterFailureStage::Spawn, AdapterFailureKind::Spawn)
        })?;
        let pid = child.id().ok_or_else(|| {
            AdapterFailure::new(AdapterFailureStage::Spawn, AdapterFailureKind::Spawn)
        })?;
        let tree = match ProcessTreeGuard::after_spawn(pid, limits.cpu_seconds) {
            Ok(tree) => tree,
            Err(failure) => {
                let _ = child.start_kill();
                let _ = child.wait().await;
                return Err(failure);
            }
        };
        let stdout = child.stdout.take().ok_or_else(|| {
            AdapterFailure::new(AdapterFailureStage::Spawn, AdapterFailureKind::ProcessIo)
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            AdapterFailure::new(AdapterFailureStage::Spawn, AdapterFailureKind::ProcessIo)
        })?;
        Ok(Self {
            child: Some(child),
            tree,
            stdout: Some(stdout),
            stderr: Some(stderr),
            output_watch,
        })
    }

    pub(crate) async fn wait_or_cancel(
        &mut self,
        cancellation: &dyn krometrail_core::CancellationSignal,
        limits: ProcessLimits,
    ) -> Result<SanitizedProcessOutcome, AdapterFailure> {
        let stdout = self.stdout.take().ok_or_else(process_io_failure)?;
        let stderr = self.stderr.take().ok_or_else(process_io_failure)?;
        enum WaitResult {
            Execution(Result<(ExitStatus, Vec<u8>, Vec<u8>), AdapterFailure>),
            Cancelled,
            Deadline,
            OutputOverflow(AdapterFailure),
        }

        let result = {
            let child = self.child.as_mut().ok_or_else(process_io_failure)?;
            let execution = async {
                tokio::try_join!(
                    async {
                        child.wait().await.map_err(|_| {
                            AdapterFailure::new(
                                AdapterFailureStage::ProcessWait,
                                AdapterFailureKind::ProcessIo,
                            )
                        })
                    },
                    read_bounded(stdout, limits.stdout_bytes, false),
                    read_bounded(stderr, limits.stderr_bytes, true),
                )
            };
            tokio::pin!(execution);
            let output_watch = self.output_watch.clone();
            let output_monitor = monitor_output(output_watch);
            tokio::pin!(output_monitor);
            tokio::select! {
                biased;
                _ = cancellation.cancelled() => WaitResult::Cancelled,
                _ = tokio::time::sleep_until(limits.deadline.into()) => WaitResult::Deadline,
                failure = &mut output_monitor => WaitResult::OutputOverflow(failure),
                result = &mut execution => WaitResult::Execution(result),
            }
        };

        match result {
            WaitResult::Cancelled => {
                self.terminate_and_reap().await?;
                Err(AdapterFailure::new(
                    AdapterFailureStage::ProcessWait,
                    AdapterFailureKind::Cancelled,
                ))
            }
            WaitResult::Deadline => {
                self.terminate_and_reap().await?;
                Err(AdapterFailure::new(
                    AdapterFailureStage::ProcessWait,
                    AdapterFailureKind::Deadline,
                ))
            }
            WaitResult::OutputOverflow(failure) => {
                self.terminate_and_reap().await?;
                Err(failure)
            }
            WaitResult::Execution(Err(failure)) => {
                self.terminate_and_reap().await?;
                Err(failure)
            }
            WaitResult::Execution(Ok((status, stdout, stderr))) => {
                self.finish_after_exit().await?;
                if !status.success() {
                    return Err(AdapterFailure::new(
                        AdapterFailureStage::ProcessWait,
                        AdapterFailureKind::ProcessExit,
                    )
                    .with_observed_bytes(stderr.len() as u64));
                }
                if let Some((path, limit)) = &self.output_watch {
                    let length = tokio::fs::metadata(path)
                        .await
                        .map(|metadata| metadata.len())
                        .unwrap_or(0);
                    if length > *limit {
                        return Err(AdapterFailure::new(
                            AdapterFailureStage::ProcessWait,
                            AdapterFailureKind::OutputOverflow,
                        )
                        .with_observed_bytes(length));
                    }
                }
                let diagnostic_sha256 = Sha256::digest(&stderr).into();
                Ok(SanitizedProcessOutcome {
                    stdout,
                    stderr,
                    diagnostic_sha256,
                })
            }
        }
    }

    pub(crate) async fn terminate_and_reap(&mut self) -> Result<(), AdapterFailure> {
        if self.child.is_none() {
            return self.verify_tree_empty().await;
        }
        self.tree.terminate(false);
        let grace_deadline = Instant::now() + FFMPEG_TERMINATION_GRACE;
        loop {
            let status = self
                .child
                .as_mut()
                .ok_or_else(process_cleanup_failure)?
                .try_wait()
                .map_err(|_| process_cleanup_failure())?;
            if status.is_some() {
                self.child.take();
                break;
            }
            if Instant::now() >= grace_deadline {
                self.tree.terminate(true);
                if let Some(child) = self.child.as_mut() {
                    let _ = child.start_kill();
                    child.wait().await.map_err(|_| process_cleanup_failure())?;
                }
                self.child.take();
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        self.tree.terminate(true);
        self.verify_tree_empty().await
    }

    async fn finish_after_exit(&mut self) -> Result<(), AdapterFailure> {
        self.child.take();
        if self.tree.has_members() {
            self.tree.terminate(true);
        }
        self.verify_tree_empty().await
    }

    async fn verify_tree_empty(&mut self) -> Result<(), AdapterFailure> {
        let deadline = Instant::now() + Duration::from_secs(1);
        while self.tree.has_members() && Instant::now() < deadline {
            self.tree.terminate(true);
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        if self.tree.has_members() {
            Err(process_cleanup_failure())
        } else {
            self.tree.release();
            Ok(())
        }
    }
}

impl Drop for ManagedFfmpegProcess {
    fn drop(&mut self) {
        self.tree.terminate(true);
        if let Some(child) = self.child.as_mut() {
            let _ = child.start_kill();
        }
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            let child_done = self
                .child
                .as_mut()
                .is_none_or(|child| child.try_wait().ok().flatten().is_some());
            if child_done && !self.tree.has_members() {
                break;
            }
            self.tree.terminate(true);
            std::thread::sleep(Duration::from_millis(10));
        }
        self.child.take();
        self.tree.release();
    }
}

async fn read_bounded<R: tokio::io::AsyncRead + Unpin>(
    mut reader: R,
    limit: usize,
    diagnostic: bool,
) -> Result<Vec<u8>, AdapterFailure> {
    let mut bytes = Vec::with_capacity(limit.min(8 * 1024));
    let mut chunk = [0_u8; 8 * 1024];
    loop {
        let read = reader
            .read(&mut chunk)
            .await
            .map_err(|_| process_io_failure())?;
        if read == 0 {
            return Ok(bytes);
        }
        if bytes.len().saturating_add(read) > limit {
            let remaining = limit.saturating_sub(bytes.len());
            bytes.extend_from_slice(&chunk[..remaining]);
            return Err(AdapterFailure::new(
                AdapterFailureStage::ProcessWait,
                if diagnostic {
                    AdapterFailureKind::DiagnosticOverflow
                } else {
                    AdapterFailureKind::StdoutOverflow
                },
            )
            .with_bytes(&bytes));
        }
        bytes.extend_from_slice(&chunk[..read]);
    }
}

async fn monitor_output(output: Option<(PathBuf, u64)>) -> AdapterFailure {
    let Some((path, limit)) = output else {
        return std::future::pending().await;
    };
    loop {
        tokio::time::sleep(Duration::from_millis(10)).await;
        if let Ok(metadata) = tokio::fs::metadata(&path).await
            && metadata.len() > limit
        {
            return AdapterFailure::new(
                AdapterFailureStage::ProcessWait,
                AdapterFailureKind::OutputOverflow,
            )
            .with_observed_bytes(metadata.len());
        }
    }
}

fn process_io_failure() -> AdapterFailure {
    AdapterFailure::new(
        AdapterFailureStage::ProcessWait,
        AdapterFailureKind::ProcessIo,
    )
}

fn process_cleanup_failure() -> AdapterFailure {
    AdapterFailure::new(
        AdapterFailureStage::ProcessCleanup,
        AdapterFailureKind::ProcessCleanup,
    )
}

#[cfg(unix)]
fn configure_tree_before_spawn(
    command: &mut Command,
    cpu_seconds: u64,
) -> Result<(), AdapterFailure> {
    use std::os::unix::process::CommandExt;

    let cpu_seconds = cpu_seconds.max(1);
    command.as_std_mut().process_group(0);
    unsafe {
        command.as_std_mut().pre_exec(move || {
            libc::umask(0o077);
            let limit = libc::rlimit {
                rlim_cur: cpu_seconds as libc::rlim_t,
                rlim_max: cpu_seconds.saturating_add(1) as libc::rlim_t,
            };
            if libc::setrlimit(libc::RLIMIT_CPU, &limit) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    Ok(())
}

#[cfg(windows)]
fn configure_tree_before_spawn(
    _command: &mut Command,
    _cpu_seconds: u64,
) -> Result<(), AdapterFailure> {
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn configure_tree_before_spawn(
    _command: &mut Command,
    _cpu_seconds: u64,
) -> Result<(), AdapterFailure> {
    Err(process_cleanup_failure())
}

#[cfg(windows)]
fn preserve_platform_environment(command: &mut Command) {
    for key in ["SystemRoot", "WINDIR"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
}

#[cfg(not(windows))]
fn preserve_platform_environment(_command: &mut Command) {}

#[cfg(unix)]
struct ProcessTreeGuard {
    process_group: Option<u32>,
}

#[cfg(unix)]
impl ProcessTreeGuard {
    fn after_spawn(pid: u32, _cpu_seconds: u64) -> Result<Self, AdapterFailure> {
        let native_pid = libc::pid_t::try_from(pid).map_err(|_| process_cleanup_failure())?;
        let group = unsafe { libc::getpgid(native_pid) };
        if group <= 0 || group != native_pid {
            return Err(process_cleanup_failure());
        }
        Ok(Self {
            process_group: Some(pid),
        })
    }

    fn terminate(&self, force: bool) {
        let Some(group) = self.process_group else {
            return;
        };
        let Ok(group) = libc::pid_t::try_from(group) else {
            return;
        };
        let signal = if force { libc::SIGKILL } else { libc::SIGTERM };
        if group > 0 {
            unsafe {
                libc::kill(-group, signal);
            }
        }
    }

    fn has_members(&self) -> bool {
        self.process_group.is_some_and(process_group_has_members)
    }

    fn release(&mut self) {
        self.process_group = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use tokio::sync::Notify;

    mod support {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/mod.rs"));
    }

    struct NeverCancelled;

    impl krometrail_core::CancellationSignal for NeverCancelled {
        fn is_cancelled(&self) -> bool {
            false
        }

        fn cancelled(&self) -> krometrail_core::PortFuture<'_, ()> {
            Box::pin(std::future::pending())
        }
    }

    struct ManualCancellation {
        cancelled: AtomicBool,
        notify: Notify,
    }

    impl ManualCancellation {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                cancelled: AtomicBool::new(false),
                notify: Notify::new(),
            })
        }

        fn cancel(&self) {
            self.cancelled.store(true, Ordering::SeqCst);
            self.notify.notify_waiters();
        }
    }

    impl krometrail_core::CancellationSignal for ManualCancellation {
        fn is_cancelled(&self) -> bool {
            self.cancelled.load(Ordering::SeqCst)
        }

        fn cancelled(&self) -> krometrail_core::PortFuture<'_, ()> {
            Box::pin(async move {
                while !self.is_cancelled() {
                    self.notify.notified().await;
                }
            })
        }
    }

    fn limits(deadline: Instant) -> ProcessLimits {
        ProcessLimits {
            deadline,
            cpu_seconds: 2,
            stdout_bytes: 64 * 1024,
            stderr_bytes: 64 * 1024,
        }
    }

    async fn fake_process(
        mode: &str,
        deadline: Instant,
    ) -> (
        support::FixtureExecutable,
        tempfile::TempDir,
        ManagedFfmpegProcess,
    ) {
        let fixture = support::FixtureExecutable::new(mode);
        let workspace = tempfile::tempdir().unwrap();
        let arguments: Vec<OsString> = [
            "-nostdin",
            "-safe",
            "1",
            "-c:v",
            "libx264",
            "-an",
            "-sn",
            "output.partial.mp4",
        ]
        .into_iter()
        .map(Into::into)
        .collect();
        let process = ManagedFfmpegProcess::spawn(
            fixture.path(),
            FfmpegInvocation::Encode {
                arguments: &arguments,
                working_directory: workspace.path(),
                output_path: workspace.path().join("output.partial.mp4"),
                output_limit: 1_000_000,
            },
            limits(deadline),
        )
        .await
        .unwrap();
        (fixture, workspace, process)
    }

    #[tokio::test]
    async fn cancellation_terminates_and_reaps_the_owned_process() {
        let deadline = Instant::now() + Duration::from_secs(5);
        let (_fixture, _workspace, mut process) = fake_process("hang", deadline).await;
        let cancellation = ManualCancellation::new();
        let trigger = Arc::clone(&cancellation);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            trigger.cancel();
        });
        let failure = process
            .wait_or_cancel(cancellation.as_ref(), limits(deadline))
            .await
            .unwrap_err();
        assert_eq!(failure.kind, AdapterFailureKind::Cancelled);
        assert!(!process.tree.has_members());
    }

    #[tokio::test]
    async fn deadline_terminates_and_reaps_the_owned_process() {
        let deadline = Instant::now() + Duration::from_millis(40);
        let (_fixture, _workspace, mut process) = fake_process("hang", deadline).await;
        let failure = process
            .wait_or_cancel(&NeverCancelled, limits(deadline))
            .await
            .unwrap_err();
        assert_eq!(failure.kind, AdapterFailureKind::Deadline);
        assert!(!process.tree.has_members());
    }

    #[tokio::test]
    async fn diagnostic_and_output_overflow_fail_closed() {
        for (mode, expected) in [
            ("stderr-overflow", AdapterFailureKind::DiagnosticOverflow),
            ("output-overflow", AdapterFailureKind::OutputOverflow),
        ] {
            let deadline = Instant::now() + Duration::from_secs(5);
            let (_fixture, _workspace, mut process) = fake_process(mode, deadline).await;
            let failure = process
                .wait_or_cancel(&NeverCancelled, limits(deadline))
                .await
                .unwrap_err();
            assert_eq!(failure.kind, expected);
            assert!(!process.tree.has_members());
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn drop_force_kills_the_compiled_descendant_tree() {
        let deadline = Instant::now() + Duration::from_secs(5);
        let (_fixture, _workspace, process) = fake_process("descendant", deadline).await;
        let group = process.tree.process_group.unwrap();
        tokio::time::sleep(Duration::from_millis(40)).await;
        drop(process);
        assert!(!process_group_has_members(group));
    }
}

#[cfg(target_os = "linux")]
fn process_group_has_members(group: u32) -> bool {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return true;
    };
    entries.flatten().any(|entry| {
        if entry.file_name().to_string_lossy().parse::<u32>().is_err() {
            return false;
        }
        let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) else {
            return false;
        };
        let Some((_, fields)) = stat.rsplit_once(") ") else {
            return false;
        };
        let mut fields = fields.split_whitespace();
        let state = fields.next();
        let _parent = fields.next();
        let process_group = fields.next().and_then(|value| value.parse::<u32>().ok());
        process_group == Some(group) && state != Some("Z")
    })
}

#[cfg(all(unix, not(target_os = "linux")))]
fn process_group_has_members(group: u32) -> bool {
    let Ok(group) = libc::pid_t::try_from(group) else {
        return true;
    };
    let result = unsafe { libc::kill(-group, 0) };
    result == 0
        || (result == -1 && std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM))
}

#[cfg(windows)]
struct ProcessTreeGuard {
    job: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
impl ProcessTreeGuard {
    fn after_spawn(pid: u32, cpu_seconds: u64) -> Result<Self, AdapterFailure> {
        use windows_sys::Win32::{
            Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
            System::{
                JobObjects::{
                    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_ACTIVE_PROCESS,
                    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOB_OBJECT_LIMIT_PROCESS_TIME,
                    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
                    SetInformationJobObject,
                },
                Threading::{
                    OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_QUOTA,
                    PROCESS_TERMINATE,
                },
            },
        };
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() || job == INVALID_HANDLE_VALUE {
                return Err(process_cleanup_failure());
            }
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_ACTIVE_PROCESS
                | JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
                | JOB_OBJECT_LIMIT_PROCESS_TIME;
            limits.BasicLimitInformation.ActiveProcessLimit = 1;
            limits.BasicLimitInformation.PerProcessUserTimeLimit =
                cpu_seconds.max(1).saturating_mul(10_000_000) as i64;
            if SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                std::ptr::addr_of!(limits).cast(),
                std::mem::size_of_val(&limits) as u32,
            ) == 0
            {
                CloseHandle(job);
                return Err(process_cleanup_failure());
            }
            let process = OpenProcess(
                PROCESS_SET_QUOTA | PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION,
                0,
                pid,
            );
            if process.is_null() || process == INVALID_HANDLE_VALUE {
                CloseHandle(job);
                return Err(process_cleanup_failure());
            }
            let assigned = AssignProcessToJobObject(job, process);
            CloseHandle(process);
            if assigned == 0 {
                CloseHandle(job);
                return Err(process_cleanup_failure());
            }
            Ok(Self { job })
        }
    }

    fn terminate(&self, _force: bool) {
        unsafe {
            windows_sys::Win32::System::JobObjects::TerminateJobObject(self.job, 1);
        }
    }

    fn has_members(&self) -> bool {
        use windows_sys::Win32::System::JobObjects::{
            JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JobObjectBasicAccountingInformation,
            QueryInformationJobObject,
        };
        unsafe {
            let mut accounting: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = std::mem::zeroed();
            QueryInformationJobObject(
                self.job,
                JobObjectBasicAccountingInformation,
                std::ptr::addr_of_mut!(accounting).cast(),
                std::mem::size_of_val(&accounting) as u32,
                std::ptr::null_mut(),
            ) == 0
                || accounting.TotalActiveProcesses != 0
        }
    }

    fn release(&mut self) {
        if !self.job.is_null() {
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(self.job);
            }
            self.job = std::ptr::null_mut();
        }
    }
}

#[cfg(not(any(unix, windows)))]
struct ProcessTreeGuard;

#[cfg(not(any(unix, windows)))]
impl ProcessTreeGuard {
    fn after_spawn(_pid: u32, _cpu_seconds: u64) -> Result<Self, AdapterFailure> {
        Err(process_cleanup_failure())
    }
    fn terminate(&self, _force: bool) {}
    fn has_members(&self) -> bool {
        true
    }
    fn release(&mut self) {}
}
