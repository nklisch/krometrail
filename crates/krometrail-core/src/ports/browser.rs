use std::{path::PathBuf, sync::Arc};

use crate::{
    browser::{
        BrowserCompatibility, BrowserInstallation, BrowserOwnership, BrowserSessionEvent,
        BrowserSessionState, BrowserStopOutcome, PageTarget, ProfileRef, SupervisedTarget,
    },
    error::{Result, invalid},
    ids::SessionId,
    recording::TargetCaptureStatus,
    time::SessionOrigin,
};

use super::PortFuture;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BrowserConnectRequest {
    Launch(LaunchBrowser),
    Attach(AttachBrowser),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaunchBrowser {
    pub executable: Option<PathBuf>,
    pub profile: ManagedProfile,
    pub initial_url: Option<String>,
}

impl LaunchBrowser {
    pub fn new(profile: ManagedProfile) -> Self {
        Self {
            executable: None,
            profile,
            initial_url: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ManagedProfile {
    Reusable {
        name: crate::browser::ProfileIdentity,
    },
    Temporary,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttachBrowser {
    pub endpoint: String,
}

impl AttachBrowser {
    pub fn new(endpoint: impl Into<String>) -> Result<Self> {
        let endpoint = endpoint.into();
        if endpoint.trim().is_empty() {
            return Err(invalid("browser endpoint must not be empty"));
        }
        Ok(Self { endpoint })
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserFailureKind {
    NotFound,
    LaunchFailed,
    ProcessTerminated,
    CompatibilityFailed,
    ProfileInUse,
    TargetFailed,
    ReconnectExhausted,
    Cancelled,
    ShutdownIncomplete,
}

impl BrowserFailureKind {
    pub const ALL: &'static [Self] = &[
        Self::NotFound,
        Self::LaunchFailed,
        Self::ProcessTerminated,
        Self::CompatibilityFailed,
        Self::ProfileInUse,
        Self::TargetFailed,
        Self::ReconnectExhausted,
        Self::Cancelled,
        Self::ShutdownIncomplete,
    ];

    pub const fn error_code(self) -> crate::ErrorCode {
        match self {
            Self::NotFound => crate::ErrorCode::BrowserNotFound,
            Self::LaunchFailed => crate::ErrorCode::BrowserLaunchFailed,
            Self::ProcessTerminated => crate::ErrorCode::BrowserProcessTerminated,
            Self::CompatibilityFailed => crate::ErrorCode::BrowserCompatibilityFailed,
            Self::ProfileInUse => crate::ErrorCode::ProfileInUse,
            Self::TargetFailed => crate::ErrorCode::TargetFailed,
            Self::ReconnectExhausted => crate::ErrorCode::ReconnectExhausted,
            Self::Cancelled => crate::ErrorCode::Cancelled,
            Self::ShutdownIncomplete => crate::ErrorCode::ShutdownIncomplete,
        }
    }

    pub fn into_error(self, message: crate::NonEmptyText) -> crate::KrometrailError {
        crate::KrometrailError::from_browser_failure(self.error_code(), message)
    }
}

pub trait BrowserSessionEvents: Send {
    fn next(&mut self) -> PortFuture<'_, Result<Option<BrowserSessionEvent>>>;
}

pub trait BrowserConnector: Send + Sync {
    fn installations(&self) -> PortFuture<'_, Result<Vec<BrowserInstallation>>>;
    fn connect(
        &self,
        request: BrowserConnectRequest,
    ) -> PortFuture<'_, Result<Arc<dyn BrowserSessionPort>>>;
}

pub trait BrowserSessionPort: Send + Sync {
    fn session_id(&self) -> SessionId;
    fn session_origin(&self) -> SessionOrigin;
    fn compatibility(&self) -> &BrowserCompatibility;
    fn ownership(&self) -> BrowserOwnership;
    fn profile(&self) -> &ProfileRef;
    fn state(&self) -> BrowserSessionState;
    fn targets(&self) -> PortFuture<'_, Result<Vec<SupervisedTarget>>>;
    fn subscribe(&self) -> PortFuture<'_, Result<Box<dyn BrowserSessionEvents>>>;
    fn capture_statuses(&self) -> PortFuture<'_, Result<Vec<TargetCaptureStatus>>>;
    fn stop(&self) -> PortFuture<'_, Result<BrowserStopOutcome>>;
}

// Kept as a named contract for adapter code that still deals in the raw page
// projection while target supervision is being assembled.
pub type BrowserPageTargets = Vec<PageTarget>;
