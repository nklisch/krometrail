//! Target discovery, attachment, lifecycle, and reconnection supervision.

mod model;
mod reducer;

#[cfg(feature = "cdpkit-transport")]
pub(crate) mod supervisor;

pub use model::{
    CaptureBinding, CaptureEffectContext, ReconnectedSnapshot, ReconnectedTarget, Reduction,
    ShutdownCause, SupervisorEffect, SupervisorInput, SupervisorState, SupervisorTargetState,
    TransportTargetInfo, ViewportEffectContext,
};
pub use reducer::reduce;

#[cfg(feature = "cdpkit-transport")]
pub use supervisor::{
    DEFAULT_RECONNECT_ATTACH_CONCURRENCY, DEFAULT_RECONNECT_TARGET_LIMIT, ReconnectPolicy,
    SubscriberLag, SupervisorConfig,
};
