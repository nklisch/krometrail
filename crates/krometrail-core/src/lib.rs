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
    AcceptedLocator, AccessibleProperty, AccessibleValue, ActionCategory, ActionDefinition,
    ActionabilityRequirement, BROWSER_OPERATION_REGISTRY, BrowserActionRequest,
    BrowserCompatibility, BrowserInstallation, BrowserInstallationSource,
    BrowserOperationDefinition, BrowserOperationKind, BrowserOperationRequest,
    BrowserOperationResult, BrowserOperationScope, BrowserOperationScopeKind, BrowserOwnership,
    BrowserProduct, BrowserProductVersion, BrowserSessionEvent, BrowserSessionState, BrowserStatus,
    BrowserStopOutcome, BrowserVersion, CapabilitySupport, ClickRequest, ClosePageRequest,
    CompletionKind, CoordinateSpace, CreatePageRequest, CssPoint, CssRect, CssSize,
    DEFAULT_MANAGED_PROFILE_NAME, DialogAction, DocumentReadiness, DragRequest, ElementLocator,
    EncodedScreenshot, EvaluationResult, EvaluationValue, FillMode, FillRequest, GoBackRequest,
    GoForwardRequest, HandleDialogRequest, HoverRequest, InspectPageRequest, InteractionAnchor,
    InteractionLocator, InteractionOutcome, InteractionRecord, InteractionResult,
    InteractionTiming, KeyChord, KeySegment, ListPagesRequest, LiveObservation,
    LiveObservationRequest, LocatorKind, LocatorSummary, ManagedProfilePersistence,
    ManagedProfileRef, Modifier, Modifiers, MouseButton, NamedKey, NavigatePageRequest,
    NavigationState, NodeReference, ObservationContext, ObservationPart, OperationEvidence,
    OperationMutability, PageChange, PageOperationOutcome, PageOperationResult, PageSelection,
    PageSnapshot, PageState, PageStatus, PageTarget, PressKeysRequest, ProfileIdentity, ProfileRef,
    REQUIRED_RENDERER_CAPABILITIES, ReadOnlyEvaluationRequest, ReloadPageRequest,
    RendererCapability, SanitizedParameters, ScreenshotMetadata, ScreenshotRequest,
    ScreenshotTarget, ScrollDelta, ScrollRequest, SelectOptionRequest, SelectPageRequest,
    SelectValue, SnapshotGeneration, SnapshotNode, SnapshotNodeId, SnapshotPageRequest,
    SupervisedTarget, TargetVisibility, UploadFilesRequest, ValidatedFilePath, ViewportState,
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
    InteractionAnchorSource, LaunchBrowser, ManagedProfile, MonotonicClock, PortFuture,
    RecordingCatalog, RecordingSink, RetentionStore, TimelineAnchorSource, TimelineStore,
    WallClock,
};
pub use recording::{
    ByteOffset, CaptureGap, CaptureGapReason, CaptureOrdinal, CaptureStatistics,
    CaptureStreamState, CaptureTimingSummary, CaptureWarning, CapturedFrame,
    DEFAULT_DISK_BUDGET_BYTES, DeviceScaleFactor, DiskBudgetBytes, EncodedFrame, FrameAddress,
    ImageFormat, PinChange, PixelDimensions, RecordingBudgetState, RecordingSession, RetainedPoint,
    RetentionRange, RetentionStatus, SessionDeletion, StorageUsage, TargetCaptureStatus,
};
pub use time::{ObservedTime, SessionOrigin, SessionRange, SessionTime, SourceTime};
pub use timeline::{
    AnchorScope, CaptureGapPolicy, InteractionWindow, ObservationKind, ObservationPayloadRef,
    RangeResolutionOptions, ResolvedRange, RetentionPolicy, RetentionWarning, TemporalRangeAnchor,
    TemporalRangeAnchorKind, TimelineObservation,
};
