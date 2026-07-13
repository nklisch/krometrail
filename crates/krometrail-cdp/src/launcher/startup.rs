//! Launch orchestration and endpoint readiness.

use super::{
    ChromeLauncher,
    discovery::discover_installations,
    process::ManagedChromeProcess,
    profile::{ProfileError, ProfileLease, ProfileLeaseKind},
};
use crate::LocalCdpEndpoint;
use krometrail_core::{BrowserInstallation, LaunchBrowser};
use std::{
    net::TcpListener,
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use thiserror::Error;

#[derive(Clone, Debug)]
pub struct LauncherConfig {
    pub profile_root: PathBuf,
    pub startup_timeout: Duration,
    pub shutdown_timeout: Duration,
}

impl LauncherConfig {
    pub fn new(profile_root: impl Into<PathBuf>) -> Self {
        Self {
            profile_root: profile_root.into(),
            startup_timeout: Duration::from_secs(15),
            shutdown_timeout: Duration::from_secs(3),
        }
    }
}

impl Default for LauncherConfig {
    fn default() -> Self {
        let root = std::env::var_os("KROMETRAIL_PROFILE_ROOT")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("XDG_CACHE_HOME")
                    .map(|path| PathBuf::from(path).join("krometrail"))
            })
            .unwrap_or_else(|| std::env::temp_dir().join("krometrail"));
        Self::new(root)
    }
}

#[derive(Debug, Error)]
pub enum LaunchError {
    #[error("no supported browser installation was found")]
    BrowserNotFound,
    #[error("browser executable is unavailable")]
    ExecutableUnavailable,
    #[error("browser launch failed")]
    SpawnFailed,
    #[error("managed profile is already in use")]
    ProfileInUse,
    #[error("managed profile is invalid")]
    InvalidProfile,
    #[error("browser endpoint did not become ready")]
    StartupTimeout,
    #[error("browser process terminated during startup")]
    ProcessTerminated,
    #[error("browser endpoint is invalid or unavailable")]
    EndpointUnavailable,
    #[error("browser shutdown was incomplete")]
    ShutdownIncomplete,
    #[error("operation was cancelled")]
    Cancelled,
}

impl LaunchError {
    pub fn stable_code(&self) -> krometrail_core::ErrorCode {
        match self {
            Self::BrowserNotFound | Self::ExecutableUnavailable => {
                krometrail_core::ErrorCode::BrowserNotFound
            }
            Self::ProfileInUse => krometrail_core::ErrorCode::ProfileInUse,
            Self::InvalidProfile => krometrail_core::ErrorCode::InvalidInput,
            Self::ProcessTerminated => krometrail_core::ErrorCode::BrowserProcessTerminated,
            Self::ShutdownIncomplete => krometrail_core::ErrorCode::ShutdownIncomplete,
            Self::Cancelled => krometrail_core::ErrorCode::Cancelled,
            Self::SpawnFailed | Self::StartupTimeout | Self::EndpointUnavailable => {
                krometrail_core::ErrorCode::BrowserLaunchFailed
            }
        }
    }
}

/// Values transferred together into session supervision. `Drop` explicitly terminates the child
/// before dropping the profile lease; field declaration order must not be able to reverse that
/// safety rule during a future refactor.
pub struct LaunchedChrome {
    pub endpoint: LocalCdpEndpoint,
    pub profile: ProfileLease,
    pub process: ManagedChromeProcess,
    shutdown_timeout: Duration,
}

impl LaunchedChrome {
    pub fn profile_kind(&self) -> ProfileLeaseKind {
        self.profile.kind()
    }

    pub async fn shutdown(&mut self) -> Result<(), LaunchError> {
        let started = Instant::now();
        let result = self.process.terminate(self.shutdown_timeout).await;
        match result {
            Ok(_) => {
                tracing::info!(
                    event = "browser.shutdown.completed",
                    disposition = "managed_browser_closed",
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    forced_termination = false,
                    unfinished_task_count = 0_u64,
                );
                Ok(())
            }
            Err(_) => {
                tracing::warn!(
                    event = "browser.shutdown.incomplete",
                    disposition = "managed_browser_close_incomplete",
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    forced_termination = true,
                    unfinished_task_count = 0_u64,
                );
                Err(LaunchError::ShutdownIncomplete)
            }
        }
    }
}

impl Drop for LaunchedChrome {
    fn drop(&mut self) {
        // `force_kill_now` is cancellation-safe and does not wait. The profile remains locked until
        // this Drop returns, so another launch cannot observe a half-cleaned browser tree.
        self.process.force_kill_now();
    }
}

pub struct SystemChromeLauncher {
    config: LauncherConfig,
}

impl SystemChromeLauncher {
    pub fn new(config: LauncherConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> &LauncherConfig {
        &self.config
    }

    pub async fn launch_owned(
        &self,
        request: &LaunchBrowser,
    ) -> Result<LaunchedChrome, LaunchError> {
        let started = Instant::now();
        let installations = discover_installations(request.executable.as_deref());
        let executable = if let Some(requested) = request.executable.as_deref() {
            match installations
                .iter()
                .find(|installation| {
                    installation.executable
                        == requested
                            .canonicalize()
                            .unwrap_or_else(|_| requested.to_owned())
                })
                .map(|installation| installation.executable.clone())
            {
                Some(executable) => executable,
                None => {
                    emit_launch_failed("requested_executable_unavailable");
                    return Err(LaunchError::ExecutableUnavailable);
                }
            }
        } else {
            match installations
                .first()
                .map(|installation| installation.executable.clone())
            {
                Some(executable) => executable,
                None => {
                    emit_launch_failed("browser_not_found");
                    return Err(LaunchError::BrowserNotFound);
                }
            }
        };
        let profile = match ProfileLease::acquire(&self.config.profile_root, &request.profile) {
            Ok(profile) => profile,
            Err(error) => {
                emit_launch_failed("profile_unavailable");
                return Err(profile_error(error));
            }
        };
        let port = match free_loopback_port() {
            Ok(port) => port,
            Err(_) => {
                emit_launch_failed("port_unavailable");
                return Err(LaunchError::SpawnFailed);
            }
        };
        let mut command = Command::new(&executable);
        command
            .arg("--remote-debugging-address=127.0.0.1")
            .arg(format!("--remote-debugging-port={port}"))
            .arg("--no-first-run")
            .arg("--no-default-browser-check")
            .arg(format!("--user-data-dir={}", profile.path().display()))
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some(url) = request.initial_url.as_deref() {
            command.arg(url);
        }
        // The process guard and profile lease both exist before the first await below.
        let mut process = match ManagedChromeProcess::spawn(&mut command) {
            Ok(process) => process,
            Err(_) => {
                emit_launch_failed("process_spawn_failed");
                return Err(LaunchError::SpawnFailed);
            }
        };
        tracing::info!(
            event = "browser.launch.started",
            ownership = "managed",
            executable_kind = installations
                .first()
                .map(|installation| installation.product.as_str())
                .unwrap_or("unknown"),
            profile_kind = profile.kind_string(),
            child_id = process.child_id(),
            elapsed_ms = started.elapsed().as_millis() as u64,
        );
        let endpoint = match tokio::time::timeout(
            self.config.startup_timeout,
            wait_for_endpoint(port, &mut process),
        )
        .await
        {
            Ok(Ok(endpoint)) => endpoint,
            Ok(Err(error)) => {
                process.force_kill_now();
                emit_launch_failed(match &error {
                    LaunchError::ProcessTerminated => "process_terminated",
                    _ => "endpoint_unavailable",
                });
                return Err(error);
            }
            Err(_) => {
                process.force_kill_now();
                emit_launch_failed("startup_timeout");
                return Err(LaunchError::StartupTimeout);
            }
        };
        tracing::info!(
            event = "browser.launch.ready",
            ownership = "managed",
            profile_kind = profile.kind_string(),
            child_id = process.child_id(),
            elapsed_ms = started.elapsed().as_millis() as u64,
        );
        Ok(LaunchedChrome {
            endpoint,
            profile,
            process,
            shutdown_timeout: self.config.shutdown_timeout,
        })
    }
}

impl ChromeLauncher for SystemChromeLauncher {
    fn installations(
        &self,
    ) -> super::LauncherFuture<'_, Result<Vec<BrowserInstallation>, LaunchError>> {
        Box::pin(async { Ok(discover_installations(None)) })
    }

    fn launch(
        &self,
        request: &LaunchBrowser,
    ) -> super::LauncherFuture<'_, Result<LaunchedChrome, LaunchError>> {
        let request = request.clone();
        Box::pin(async move { self.launch_owned(&request).await })
    }
}

pub async fn attach_endpoint(input: impl AsRef<str>) -> Result<LocalCdpEndpoint, LaunchError> {
    LocalCdpEndpoint::resolve(input.as_ref())
        .await
        .map_err(|_| LaunchError::EndpointUnavailable)
}

async fn wait_for_endpoint(
    port: u16,
    process: &mut ManagedChromeProcess,
) -> Result<LocalCdpEndpoint, LaunchError> {
    let input = format!("http://127.0.0.1:{port}");
    loop {
        if !process.is_alive() {
            return Err(LaunchError::ProcessTerminated);
        }
        if let Ok(endpoint) = LocalCdpEndpoint::resolve(&input).await {
            return Ok(endpoint);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

fn free_loopback_port() -> std::io::Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    Ok(listener.local_addr()?.port())
}

fn emit_launch_failed(reason: &'static str) {
    tracing::warn!(event = "browser.launch.failed", reason);
}

fn profile_error(error: ProfileError) -> LaunchError {
    match error {
        ProfileError::InUse => LaunchError::ProfileInUse,
        ProfileError::InvalidName => LaunchError::InvalidProfile,
        ProfileError::Root | ProfileError::Prepare => LaunchError::SpawnFailed,
    }
}

// Kept private so logging cannot accidentally expose a path-bearing representation.
trait ProfileLeaseLogging {
    fn kind_string(&self) -> &'static str;
}
impl ProfileLeaseLogging for ProfileLease {
    fn kind_string(&self) -> &'static str {
        match self.kind() {
            ProfileLeaseKind::Reusable => "reusable",
            ProfileLeaseKind::Temporary => "temporary",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krometrail_core::{ManagedProfile, ProfileIdentity};

    #[tokio::test]
    async fn attach_validation_has_no_managed_side_effects() {
        let endpoint = attach_endpoint("ws://127.0.0.1:9222/devtools/browser/test")
            .await
            .unwrap();
        assert_eq!(endpoint.redacted_label(), "127.0.0.1:9222");
    }

    #[test]
    fn launcher_config_does_not_make_headless_or_gpu_a_product_default() {
        let launcher = SystemChromeLauncher::new(LauncherConfig::default());
        assert!(launcher.config().startup_timeout > Duration::ZERO);
        let _ = ManagedProfile::Reusable {
            name: ProfileIdentity::new("profile").unwrap(),
        };
    }
}
