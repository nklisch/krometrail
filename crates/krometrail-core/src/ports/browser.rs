use std::{path::PathBuf, sync::Arc};

use serde::{Deserialize, Serialize};

use crate::{
    browser::{
        BrowserInstallation, BrowserOperationRequest, BrowserOperationResult, BrowserSessionEvent,
        BrowserStatus, BrowserStopOutcome, CssRect, NodeReference, PageTarget,
    },
    error::{Result, invalid},
    ids::{SessionId, TargetId},
    time::{ObservedTime, SessionOrigin, SessionTime},
    validation::{delegate_json_schema, deserialize_validated},
};

use super::PortFuture;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BrowserConnectRequest {
    Launch(LaunchBrowser),
    Attach(AttachBrowser),
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
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

/// A current-only snapshot reference lookup. It makes no historical or tracking claim.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct CurrentReferenceGeometryRequest {
    pub session_id: SessionId,
    pub reference: NodeReference,
}

impl CurrentReferenceGeometryRequest {
    pub fn new(session_id: SessionId, reference: NodeReference) -> Result<Self> {
        if session_id.as_uuid().is_nil() || reference.target_id.as_uuid().is_nil() {
            return Err(invalid(
                "current-reference geometry requires non-nil session and target ids",
            ));
        }
        Ok(Self {
            session_id,
            reference,
        })
    }
}

impl<'de> Deserialize<'de> for CurrentReferenceGeometryRequest {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            session_id: SessionId,
            reference: NodeReference,
        }
        deserialize_validated(deserializer, |wire: Wire| {
            Self::new(wire.session_id, wire.reference)
        })
    }
}

/// Geometry sampled once from the one currently active snapshot generation.
///
/// This is current browser provenance only: it carries no source-frame identity and makes no
/// historical or element-tracking claim. `observed_at` and normalized `resolved_at` preserve the
/// two current clocks without implying that either time belongs to a retained frame.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ResolvedReferenceGeometry {
    pub session_id: SessionId,
    pub target_id: TargetId,
    pub reference: NodeReference,
    pub attachment_generation: u64,
    pub observed_at: ObservedTime,
    pub resolved_at: SessionTime,
    pub viewport_css_rect: CssRect,
}

impl ResolvedReferenceGeometry {
    pub fn new(
        request: CurrentReferenceGeometryRequest,
        target_id: TargetId,
        attachment_generation: u64,
        observed_at: ObservedTime,
        resolved_at: SessionTime,
        viewport_css_rect: CssRect,
    ) -> Result<Self> {
        if target_id != request.reference.target_id {
            return Err(invalid(
                "resolved current geometry target must match its exact reference",
            ));
        }
        if attachment_generation == 0 {
            return Err(invalid(
                "resolved current geometry requires an attached target generation",
            ));
        }
        CssRect::new(viewport_css_rect.origin, viewport_css_rect.size)?;
        if !viewport_css_rect.right().is_finite() || !viewport_css_rect.bottom().is_finite() {
            return Err(invalid(
                "resolved current geometry bounds must remain finite",
            ));
        }
        Ok(Self {
            session_id: request.session_id,
            target_id,
            reference: request.reference,
            attachment_generation,
            observed_at,
            resolved_at,
            viewport_css_rect,
        })
    }
}

impl<'de> Deserialize<'de> for ResolvedReferenceGeometry {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            session_id: SessionId,
            target_id: TargetId,
            reference: NodeReference,
            attachment_generation: u64,
            observed_at: ObservedTime,
            resolved_at: SessionTime,
            viewport_css_rect: CssRect,
        }
        deserialize_validated(deserializer, |wire: Wire| {
            Self::new(
                CurrentReferenceGeometryRequest::new(wire.session_id, wire.reference)?,
                wire.target_id,
                wire.attachment_generation,
                wire.observed_at,
                wire.resolved_at,
                wire.viewport_css_rect,
            )
        })
    }
}

/// Narrow inward port supplied only by the active browser-session owner.
pub trait CurrentReferenceGeometry: Send + Sync {
    fn current_reference_geometry(
        &self,
        request: CurrentReferenceGeometryRequest,
    ) -> PortFuture<'_, Result<ResolvedReferenceGeometry>>;
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

    /// Adapter seam used by the narrow [`CurrentReferenceGeometry`] view.
    ///
    /// Session implementations that do not own a live snapshot registry remain explicitly
    /// unavailable; this is not a browser-operation registry entry.
    fn resolve_current_reference_geometry(
        &self,
        _request: CurrentReferenceGeometryRequest,
    ) -> PortFuture<'_, Result<ResolvedReferenceGeometry>> {
        Box::pin(std::future::ready(Err(crate::KrometrailError::new(
            crate::ErrorCode::InvalidLifecycleTransition,
            crate::NonEmptyText::new(
                "current-reference geometry is unavailable for this browser session",
            )
            .expect("static current-reference error is non-empty"),
        )
        .with_recovery(
            crate::NonEmptyText::new(
                "request a structured snapshot from an active supervised browser session",
            )
            .expect("static current-reference recovery is non-empty"),
        ))))
    }

    fn status(&self) -> PortFuture<'_, Result<BrowserStatus>>;
    fn subscribe(&self) -> PortFuture<'_, Result<Box<dyn BrowserSessionEvents>>>;
    fn execute(
        &self,
        request: BrowserOperationRequest,
        context: BrowserOperationContext,
    ) -> PortFuture<'_, Result<BrowserOperationResult>>;
    fn stop(&self) -> PortFuture<'_, Result<BrowserStopOutcome>>;
}

impl<T> CurrentReferenceGeometry for T
where
    T: BrowserSessionPort + ?Sized,
{
    fn current_reference_geometry(
        &self,
        request: CurrentReferenceGeometryRequest,
    ) -> PortFuture<'_, Result<ResolvedReferenceGeometry>> {
        self.resolve_current_reference_geometry(request)
    }
}

// Kept as a named contract for adapter code that still deals in the raw page
// projection while target supervision is being assembled.
pub type BrowserPageTargets = Vec<PageTarget>;
