use std::{path::PathBuf, sync::Arc};

use serde::{Deserialize, Serialize};

use crate::{
    browser::{
        BrowserInstallation, BrowserOperationRequest, BrowserOperationResult, BrowserSessionEvent,
        BrowserStatus, BrowserStopOutcome, PageTarget,
    },
    error::{Result, invalid},
    time::SessionOrigin,
    validation::{delegate_json_schema, deserialize_validated},
};

use super::PortFuture;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BrowserConnectRequest {
    Launch(LaunchBrowser),
    Attach(AttachBrowser),
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ManagedProfile {
    Reusable {
        name: crate::browser::ProfileIdentity,
    },
    Temporary,
}

impl Default for ManagedProfile {
    fn default() -> Self {
        Self::Reusable {
            name: crate::browser::ProfileIdentity::new(
                crate::browser::DEFAULT_MANAGED_PROFILE_NAME,
            )
            .expect("default managed profile name is valid"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AttachBrowser {
    pub endpoint: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct AttachBrowserWire {
    endpoint: String,
}

delegate_json_schema!(AttachBrowser => AttachBrowserWire);

impl AttachBrowser {
    pub fn new(endpoint: impl Into<String>) -> Result<Self> {
        let endpoint = endpoint.into();
        if endpoint.trim().is_empty() {
            return Err(invalid("browser endpoint must not be empty"));
        }
        Ok(Self { endpoint })
    }
}

impl<'de> Deserialize<'de> for AttachBrowser {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        deserialize_validated(deserializer, |wire: AttachBrowserWire| {
            Self::new(wire.endpoint)
        })
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

pub trait CancellationSignal: Send + Sync {
    fn is_cancelled(&self) -> bool;
    fn cancelled(&self) -> PortFuture<'_, ()>;
}

#[derive(Clone, Default)]
pub struct BrowserOperationContext {
    cancellation: Option<Arc<dyn CancellationSignal>>,
}

impl BrowserOperationContext {
    pub fn with_cancellation(cancellation: Arc<dyn CancellationSignal>) -> Self {
        Self {
            cancellation: Some(cancellation),
        }
    }

    pub fn cancellation(&self) -> Option<&Arc<dyn CancellationSignal>> {
        self.cancellation.as_ref()
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancellation
            .as_ref()
            .is_some_and(|signal| signal.is_cancelled())
    }
}

impl std::fmt::Debug for BrowserOperationContext {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BrowserOperationContext")
            .field("cancellable", &self.cancellation.is_some())
            .field("cancelled", &self.is_cancelled())
            .finish()
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
    fn session_origin(&self) -> SessionOrigin;
    fn status(&self) -> PortFuture<'_, Result<BrowserStatus>>;
    fn subscribe(&self) -> PortFuture<'_, Result<Box<dyn BrowserSessionEvents>>>;
    fn execute(
        &self,
        request: BrowserOperationRequest,
        context: BrowserOperationContext,
    ) -> PortFuture<'_, Result<BrowserOperationResult>>;
    fn stop(&self) -> PortFuture<'_, Result<BrowserStopOutcome>>;
}

// Kept as a named contract for adapter code that still deals in the raw page
// projection while target supervision is being assembled.
pub type BrowserPageTargets = Vec<PageTarget>;
