//! Stable, infrastructure-free domain contracts for browser recording.

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

pub use browser::{BrowserVersion, PageTarget, ProfileIdentity};
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
    AttachBrowser, BrowserCompatibility, BrowserConnectRequest, BrowserConnector,
    BrowserSessionPort, DomainSupport, IdSource, LaunchBrowser, MonotonicClock, PortFuture,
    RecordingSink, TimelineStore, WallClock,
};
pub use recording::{
    CaptureGap, CaptureGapReason, CaptureStatistics, CaptureWarning, CapturedFrame,
    DeviceScaleFactor, DiskBudgetBytes, EncodedFrame, ImageFormat, PixelDimensions,
    RecordingSession,
};
pub use time::{ObservedTime, SessionOrigin, SessionRange, SessionTime, SourceTime};
pub use timeline::{ObservationKind, ObservationPayloadRef, TimelineObservation};
