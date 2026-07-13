//! Chrome DevTools Protocol adapter boundary.
//!
//! Chrome DevTools Protocol adapter boundary.
//!
//! Production transport and compatibility probing are replaceable adapter modules. Qualification
//! code remains disposable and is only compiled when an explicit spike feature is requested.

pub mod endpoint;
pub mod launcher;

#[cfg(feature = "cdpkit-transport")]
pub mod compatibility;
#[cfg(feature = "cdpkit-transport")]
pub mod transport;

#[cfg(feature = "cdp-spike")]
#[doc(hidden)]
pub mod spike;

#[cfg(feature = "cdpkit-transport")]
pub use compatibility::{
    CompatibilityProbeError, EndpointKind, RENDERER_CAPABILITY_PROBES, RendererCapabilityProbe,
    probe_compatibility,
};
pub use endpoint::{EndpointError, LocalCdpEndpoint};
pub use launcher::{
    ChromeLauncher, DiscoveryCandidate, DiscoveryInputs, LaunchError, LaunchedChrome,
    LauncherConfig, ManagedChromeProcess, ProcessError, ProcessTermination, ProfileError,
    ProfileLease, ProfileLeaseKind, SanitizedProcessExit, SystemChromeLauncher,
    discover_installations, discover_installations_with,
};
#[cfg(feature = "cdpkit-transport")]
pub use transport::{
    CdpTransport, CdpTransportFactory, CommandScope, NamedEvent, TransportClose, TransportError,
    TransportEvents, TransportFuture, TransportSessionId,
};
