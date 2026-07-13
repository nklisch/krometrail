//! Stable, infrastructure-free domain contracts for browser recording.

macro_rules! define_stable_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $( $variant:ident => $stable_name:literal ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
        $vis enum $name {
            $( #[serde(rename = $stable_name)] $variant ),+
        }

        impl $name {
            /// The complete variant registry and the serialized boundary name for each value
            /// are generated from the same declaration as the enum.
            pub const ALL: &'static [Self] = &[
                $(Self::$variant),+
            ];

            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $stable_name),+
                }
            }
        }
    };
}

pub mod browser;
pub mod capabilities;
pub mod error;
pub mod ids;
pub mod lifecycle;
pub mod ports;
pub mod recording;
pub mod time;
pub mod timeline;

mod validation;

pub use browser::{
    BrowserCompatibility, BrowserInstallation, BrowserInstallationSource, BrowserOwnership,
    BrowserProduct, BrowserProductVersion, BrowserSessionEvent, BrowserSessionState,
    BrowserStopOutcome, BrowserVersion, CapabilitySupport, PageTarget, ProfileIdentity, ProfileRef,
    REQUIRED_RENDERER_CAPABILITIES, RendererCapability, SupervisedTarget, TargetVisibility,
};
pub use capabilities::{
    CAPABILITY_REGISTRY, CapabilityDefault, CapabilityDefinition, CapabilityId, RecordingSubsystem,
    capability, validate_capability_selection,
};
pub use error::{
    EmptyTextError, ErrorCode, ErrorContext, KrometrailError, NonEmptyText, Result, RetryAdvice,
};
pub use ids::{
    ArtifactId, FrameId, GapId, IdValue, InteractionId, MarkerId, NavigationId, SegmentId,
    SessionId, TargetId,
};
pub use lifecycle::{SessionLifecycle, TargetLifecycle};
pub use ports::{
    AttachBrowser, BrowserConnectRequest, BrowserConnector, BrowserFailureKind, BrowserPageTargets,
    BrowserSessionEvents, BrowserSessionPort, IdSource, LaunchBrowser, ManagedProfile,
    MonotonicClock, PortFuture, RecordingSink, TimelineStore, WallClock,
};
pub use recording::{
    CaptureGap, CaptureGapReason, CaptureStatistics, CaptureStreamState, CaptureTimingSummary,
    CaptureWarning, CapturedFrame, DeviceScaleFactor, DiskBudgetBytes, EncodedFrame, ImageFormat,
    PixelDimensions, RecordingSession, TargetCaptureStatus,
};
pub use time::{ObservedTime, SessionOrigin, SessionRange, SessionTime, SourceTime};
pub use timeline::{ObservationKind, ObservationPayloadRef, TimelineObservation};
