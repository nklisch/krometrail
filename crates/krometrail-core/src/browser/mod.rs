//! Browser and page-target domain contracts.

mod session;
mod target;

pub use session::{
    BrowserCompatibility, BrowserOwnership, BrowserSessionEvent, BrowserSessionState,
    BrowserStopOutcome, CapabilitySupport, REQUIRED_RENDERER_CAPABILITIES, RendererCapability,
    SupervisedTarget, TargetVisibility, renderer_capability_is_required,
};
pub use target::{
    BrowserInstallation, BrowserInstallationSource, BrowserProduct, BrowserProductVersion,
    BrowserVersion, PageTarget, ProfileIdentity, ProfileRef,
};
