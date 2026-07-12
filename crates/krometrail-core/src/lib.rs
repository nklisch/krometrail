//! Stable, infrastructure-free domain contracts for browser recording.

pub mod browser;
pub mod capabilities;
pub mod error;
pub mod ids;
pub mod lifecycle;
pub mod recording;
pub mod time;
pub mod timeline;

pub use browser::{BrowserVersion, PageTarget, ProfileIdentity};
pub use capabilities::{
    CAPABILITY_REGISTRY, CapabilityDefault, CapabilityDefinition, CapabilityId, RecordingSubsystem,
    capability, validate_capability_selection,
};
pub use error::{ErrorCode, KrometrailError, Result};
pub use ids::{
    ArtifactId, FrameId, GapId, IdValue, InteractionId, MarkerId, NavigationId, SegmentId,
    SessionId, TargetId,
};
pub use lifecycle::{SessionLifecycle, TargetLifecycle};
pub use recording::{
    CaptureGap, CaptureGapReason, CaptureStatistics, CaptureWarning, CapturedFrame,
    DeviceScaleFactor, DiskBudgetBytes, EncodedFrame, ImageFormat, PixelDimensions,
    RecordingSession,
};
pub use time::{ObservedTime, SessionOrigin, SessionRange, SessionTime, SourceTime};
pub use timeline::{ObservationKind, ObservationPayloadRef, TimelineObservation};
