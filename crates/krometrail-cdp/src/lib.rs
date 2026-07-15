//! Chrome DevTools Protocol adapter boundary.
//!
//! Chrome DevTools Protocol adapter boundary.
//!
//! Production transport and compatibility probing are replaceable adapter modules. Qualification
//! code remains disposable and is only compiled when an explicit spike feature is requested.

pub mod endpoint;
pub mod launcher;
pub mod targets;
pub use targets::{
    CaptureBinding, CaptureEffectContext, ReconnectedSnapshot, ReconnectedTarget, Reduction,
    ShutdownCause, SupervisorEffect, SupervisorInput, SupervisorState, SupervisorTargetState,
    TransportTargetInfo, reduce,
};
#[cfg(feature = "cdpkit-transport")]
pub use targets::{
    DEFAULT_RECONNECT_ATTACH_CONCURRENCY, DEFAULT_RECONNECT_TARGET_LIMIT, ReconnectPolicy,
    SubscriberLag, SupervisorConfig,
};

#[cfg(feature = "cdpkit-transport")]
pub mod capture;
#[cfg(feature = "cdpkit-transport")]
pub mod compatibility;
#[cfg(feature = "cdpkit-transport")]
mod control;
#[cfg(feature = "cdpkit-transport")]
pub mod events;
pub mod transport;

#[cfg(feature = "cdp-spike")]
#[doc(hidden)]
pub mod spike;

#[cfg(feature = "cdpkit-transport")]
pub use capture::CaptureConfig;
#[cfg(feature = "cdpkit-transport")]
pub use compatibility::{
    BrowserEventSupport, CompatibilityProbeError, EndpointKind, RENDERER_CAPABILITY_PROBES,
    RendererCapabilityProbe, probe_compatibility,
};
pub use endpoint::{
    EndpointError, EndpointResolveFuture, EndpointResolver, LocalCdpEndpoint, LocalCdpEndpointKind,
    SystemEndpointResolver,
};
#[cfg(feature = "cdpkit-transport")]
pub use events::{BrowserEventConfig, BrowserEventStatus};
pub use launcher::{
    ChromeLauncher, DiscoveryCandidate, DiscoveryInputs, LaunchError, LaunchedChrome,
    LauncherConfig, ManagedChromeProcess, ProcessError, ProcessTermination, ProfileError,
    ProfileLease, ProfileLeaseKind, SanitizedProcessExit, SystemChromeLauncher,
    discover_installations, discover_installations_with,
};
#[cfg(feature = "cdpkit-transport")]
pub mod session;
#[cfg(feature = "cdpkit-transport")]
pub use session::ProductionBrowserConnector;

/// Test-only real-browser qualification support. This is deliberately absent from default builds.
#[cfg(feature = "qualification-support")]
pub mod qualification_support;

pub use transport::{
    CdpTransport, CdpTransportFactory, CommandScope, NamedEvent, TransportClose, TransportError,
    TransportEvents, TransportFuture, TransportSessionId,
};
