use std::{num::NonZeroU8, path::PathBuf, sync::Arc};

use serde::{Deserialize, Serialize};
use url::Url;

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

pub const MIN_EVERY_NTH_FRAME: u8 = 1;
pub const MAX_EVERY_NTH_FRAME: u8 = 60;

/// A validated relative capture stride requested when a browser session starts.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct EveryNthFrame(NonZeroU8);

// Schemars' range metadata is field-oriented; this local override keeps the public tuple
// newtype transparent while adding the same generated contract constraints to its primitive schema.
impl schemars::JsonSchema for EveryNthFrame {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("EveryNthFrame")
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        let mut schema = <u8 as schemars::JsonSchema>::json_schema(generator);
        let object = schema.ensure_object();
        object.insert("minimum".into(), serde_json::json!(MIN_EVERY_NTH_FRAME));
        object.insert("maximum".into(), serde_json::json!(MAX_EVERY_NTH_FRAME));
        object.insert("default".into(), serde_json::json!(MIN_EVERY_NTH_FRAME));
        schema
    }
}

impl EveryNthFrame {
    pub fn new(value: u8) -> Result<Self> {
        NonZeroU8::new(value)
            .filter(|value| value.get() <= MAX_EVERY_NTH_FRAME)
            .map(Self)
            .ok_or_else(|| invalid("every_nth_frame must be between 1 and 60"))
    }

    pub const fn get(self) -> u8 {
        self.0.get()
    }
}

impl Default for EveryNthFrame {
    fn default() -> Self {
        Self::new(MIN_EVERY_NTH_FRAME).expect("default capture stride is valid")
    }
}

impl<'de> Deserialize<'de> for EveryNthFrame {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        deserialize_validated(deserializer, Self::new)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct LaunchBrowser {
    pub executable: Option<PathBuf>,
    pub profile: ManagedProfile,
    pub initial_url: Option<String>,
    pub every_nth_frame: EveryNthFrame,
}

#[derive(Default, Deserialize, schemars::JsonSchema)]
#[serde(default)]
struct LaunchBrowserWire {
    executable: Option<PathBuf>,
    profile: ManagedProfile,
    initial_url: Option<String>,
    #[serde(default)]
    #[schemars(default)]
    every_nth_frame: EveryNthFrame,
}

delegate_json_schema!(LaunchBrowser => LaunchBrowserWire);

impl<'de> Deserialize<'de> for LaunchBrowser {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        deserialize_validated(deserializer, |wire: LaunchBrowserWire| {
            let request = Self {
                executable: wire.executable,
                profile: wire.profile,
                initial_url: wire.initial_url,
                every_nth_frame: wire.every_nth_frame,
            };
            request.validate()?;
            Ok(request)
        })
    }
}

impl LaunchBrowser {
    pub fn new(profile: ManagedProfile) -> Self {
        Self {
            executable: None,
            profile,
            initial_url: None,
            every_nth_frame: EveryNthFrame::default(),
        }
    }

    /// Validates launch input before it reaches a browser adapter. The public
    /// fields remain available to trusted in-process callers, while external
    /// JSON uses this same constructor through `Deserialize`.
    pub fn validate(&self) -> Result<()> {
        if let Some(url) = self.initial_url.as_deref() {
            validate_initial_browser_url(url)?;
        }
        Ok(())
    }
}

const MAX_INITIAL_BROWSER_URL_BYTES: usize = 2 * 1024 * 1024;

fn validate_initial_browser_url(value: &str) -> Result<()> {
    if value.is_empty() || value.len() > MAX_INITIAL_BROWSER_URL_BYTES {
        return Err(invalid(
            "initial browser URL is empty or exceeds its input limit",
        ));
    }
    if value != value.trim() || value.starts_with('-') {
        return Err(invalid(
            "initial browser URL must be a trimmed URL, not a browser switch",
        ));
    }
    if value.chars().any(char::is_control) {
        return Err(invalid(
            "initial browser URL must not contain control characters",
        ));
    }
    let parsed =
        Url::parse(value).map_err(|_| invalid("initial browser URL must use an absolute URL"))?;
    match parsed.scheme() {
        // These schemes cover remote pages plus the local fixture/tooling forms. `about` and
        // `data` intentionally remain available because they are useful for local smoke tests
        // and do not provide a way to reinterpret the argument as a Chrome switch.
        "http" | "https" if parsed.host_str().is_some() => Ok(()),
        "file" | "about" | "data" => Ok(()),
        "http" | "https" => Err(invalid("initial browser network URL must include a host")),
        _ => Err(invalid(
            "initial browser URL scheme is not supported; use http, https, file, about, or data",
        )),
    }
}

#[cfg(test)]
mod initial_url_tests {
    use super::*;

    #[test]
    fn launch_url_validation_preserves_local_tool_schemes_and_rejects_switches() {
        for value in [
            "http://127.0.0.1:4173/fixture",
            "https://example.test/",
            "file:///tmp/fixture/index.html",
            "about:blank",
            "data:text/html,%3Ch1%3Elocal%3C%2Fh1%3E",
        ] {
            let request = serde_json::from_value::<LaunchBrowser>(serde_json::json!({
                "initial_url": value,
            }));
            assert!(request.is_ok(), "expected URL to be accepted: {value}");
        }

        for value in [
            "--no-sandbox",
            "-remote-debugging-port=9222",
            "javascript:alert(1)",
            "chrome://settings",
            "relative/path",
            " https://example.test/",
        ] {
            let request = serde_json::from_value::<LaunchBrowser>(serde_json::json!({
                "initial_url": value,
            }));
            assert!(request.is_err(), "expected URL to be rejected: {value}");
        }
    }

    #[test]
    fn launch_schema_keeps_the_generated_initial_url_contract() {
        let schema = serde_json::to_value(schemars::schema_for!(LaunchBrowser)).unwrap();
        let types = schema["properties"]["initial_url"]["type"]
            .as_array()
            .expect("optional URL schema should contain nullable types");
        assert!(types.iter().any(|value| value == "string"));
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
    pub every_nth_frame: EveryNthFrame,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct AttachBrowserWire {
    endpoint: String,
    #[serde(default)]
    #[schemars(default)]
    every_nth_frame: EveryNthFrame,
}

delegate_json_schema!(AttachBrowser => AttachBrowserWire);

impl AttachBrowser {
    pub fn new(endpoint: impl Into<String>) -> Result<Self> {
        let endpoint = endpoint.into();
        if endpoint.trim().is_empty() {
            return Err(invalid("browser endpoint must not be empty"));
        }
        Ok(Self {
            endpoint,
            every_nth_frame: EveryNthFrame::default(),
        })
    }

    pub fn with_every_nth_frame(mut self, every_nth_frame: EveryNthFrame) -> Self {
        self.every_nth_frame = every_nth_frame;
        self
    }
}

impl<'de> Deserialize<'de> for AttachBrowser {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        deserialize_validated(deserializer, |wire: AttachBrowserWire| {
            Self::new(wire.endpoint)
                .map(|request| request.with_every_nth_frame(wire.every_nth_frame))
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

    /// Current in-memory retained-capture health. This view performs no storage I/O and must not
    /// make current-state browser control depend on recording persistence.
    fn capture_statuses(&self) -> Vec<crate::TargetCaptureStatus> {
        Vec::new()
    }

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
