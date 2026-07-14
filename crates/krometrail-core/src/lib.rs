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
    AccessibleProperty, AccessibleValue, BROWSER_OPERATION_REGISTRY, BrowserCompatibility,
    BrowserInstallation, BrowserInstallationSource, BrowserOperationDefinition,
    BrowserOperationKind, BrowserOperationRequest, BrowserOperationResult, BrowserOperationScope,
    BrowserOperationScopeKind, BrowserOwnership, BrowserProduct, BrowserProductVersion,
    BrowserSessionEvent, BrowserSessionState, BrowserStatus, BrowserStopOutcome, BrowserVersion,
    CapabilitySupport, ClosePageRequest, CoordinateSpace, CreatePageRequest, CssPoint, CssRect,
    CssSize, DEFAULT_MANAGED_PROFILE_NAME, DocumentReadiness, ElementLocator, EncodedScreenshot,
    EvaluationResult, EvaluationValue, GoBackRequest, GoForwardRequest, InspectPageRequest,
    InteractionAnchor, InteractionTiming, ListPagesRequest, LiveObservation,
    LiveObservationRequest, ManagedProfilePersistence, ManagedProfileRef, NavigatePageRequest,
    NavigationState, NodeReference, ObservationContext, ObservationPart, OperationEvidence,
    OperationMutability, PageChange, PageOperationOutcome, PageOperationResult, PageSelection,
    PageSnapshot, PageState, PageStatus, PageTarget, ProfileIdentity, ProfileRef,
    REQUIRED_RENDERER_CAPABILITIES, ReadOnlyEvaluationRequest, ReloadPageRequest,
    RendererCapability, ScreenshotMetadata, ScreenshotRequest, ScreenshotTarget, SelectPageRequest,
    SnapshotGeneration, SnapshotNode, SnapshotNodeId, SnapshotPageRequest, SupervisedTarget,
    TargetVisibility, ViewportState,
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
    BrowserSessionEvents, BrowserSessionPort, CaptureGapStore, FrameSource, IdSource,
    LaunchBrowser, ManagedProfile, MonotonicClock, PortFuture, RecordingCatalog, RecordingSink,
    TimelineStore, WallClock,
};
pub use recording::{
    ByteOffset, CaptureGap, CaptureGapReason, CaptureOrdinal, CaptureStatistics,
    CaptureStreamState, CaptureTimingSummary, CaptureWarning, CapturedFrame, DeviceScaleFactor,
    DiskBudgetBytes, EncodedFrame, FrameAddress, ImageFormat, PixelDimensions, RecordingSession,
    TargetCaptureStatus,
};
pub use time::{ObservedTime, SessionOrigin, SessionRange, SessionTime, SourceTime};
pub use timeline::{ObservationKind, ObservationPayloadRef, TimelineObservation};
