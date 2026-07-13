//! Target discovery, attachment, lifecycle, and reconnection supervision.

mod model;
mod reducer;

#[cfg(feature = "cdpkit-transport")]
pub(crate) mod supervisor;

pub use model::{
    ReconnectedSnapshot, ReconnectedTarget, Reduction, ShutdownCause, SupervisorEffect,
    SupervisorInput, SupervisorState, SupervisorTargetState, TransportTargetInfo,
};
pub use reducer::reduce;

#[cfg(feature = "cdpkit-transport")]
pub use supervisor::{ReconnectPolicy, SubscriberLag, SupervisorConfig};
