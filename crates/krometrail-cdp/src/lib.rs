//! Chrome DevTools Protocol adapter boundary.
//!
//! Chrome DevTools Protocol adapter boundary.
//!
//! Production transport and compatibility probing are replaceable adapter modules. Qualification
//! code remains disposable and is only compiled when an explicit spike feature is requested.

pub mod endpoint;
pub mod launcher;
pub mod targets;
#[cfg(feature = "cdpkit-transport")]
pub use targets::{ReconnectPolicy, SubscriberLag, SupervisorConfig};
pub use targets::{
    ReconnectedSnapshot, ReconnectedTarget, Reduction, ShutdownCause, SupervisorEffect,
    SupervisorInput, SupervisorState, SupervisorTargetState, TransportTargetInfo, reduce,
};

#[cfg(feature = "cdpkit-transport")]
pub mod compatibility;
pub mod transport;

#[cfg(feature = "cdp-spike")]
#[doc(hidden)]
pub mod spike;

#[cfg(feature = "cdpkit-transport")]
pub use compatibility::{
    CompatibilityProbeError, EndpointKind, RENDERER_CAPABILITY_PROBES, RendererCapabilityProbe,
    probe_compatibility,
};
pub use endpoint::{
    EndpointError, EndpointResolver, LocalCdpEndpoint, LocalCdpEndpointKind, SystemEndpointResolver,
};
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

pub use transport::{
    CdpTransport, CdpTransportFactory, CommandScope, NamedEvent, TransportClose, TransportError,
    TransportEvents, TransportFuture, TransportSessionId,
};
