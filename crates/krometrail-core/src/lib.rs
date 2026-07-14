//! Stable, infrastructure-free domain contracts for browser recording.

macro_rules! define_stable_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $( $variant:ident => $stable_name:literal ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
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

pub mod artifacts;
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

pub use artifacts::{
    AnalysisScale, ArtifactCacheDisposition, ArtifactFailurePolicy, ArtifactGeneration,
    ArtifactGenerationContext, ArtifactGenerationRequest, ArtifactGenerationResult,
    ArtifactGeneratorRequest, ArtifactHandle, ArtifactLabelsRequest, ArtifactManifest,
    ArtifactMarker, ArtifactMarkerId, ArtifactOutcome, DifferenceMapRequest, FrameSelector,
    MotionHistoryRequest, NormalizationRequest, OutputLimitsRequest, RegionFilmstripRequest,
    StoryboardRequest, VisualEpoch,
};
pub use browser::{
    AcceptedLocator, AccessibleProperty, AccessibleValue, ActionCategory, ActionDefinition,
    ActionabilityRequirement, BROWSER_OPERATION_REGISTRY, BatchFailurePolicy, BatchOptions,
    BatchOutcome, BatchRequest, BatchResult, BatchSkipReason, BatchStepResult, BatchStepStatus,
    BrowserActionRequest, BrowserCompatibility, BrowserInstallation, BrowserInstallationSource,
    BrowserOperationDefinition, BrowserOperationKind, BrowserOperationRequest,
    BrowserOperationResult, BrowserOperationScope, BrowserOperationScopeKind, BrowserOwnership,
    BrowserProduct, BrowserProductVersion, BrowserSessionEvent, BrowserSessionState, BrowserStatus,
    BrowserStopOutcome, BrowserVersion, CapabilitySupport, ClickRequest, ClosePageRequest,
    CompletionKind, CoordinateSpace, CreatePageRequest, CssPoint, CssRect, CssSize,
    DEFAULT_MANAGED_PROFILE_NAME, DialogAction, DocumentReadiness, DragRequest, ElementLocator,
    ElementState, EncodedScreenshot, EvaluationResult, EvaluationValue, FillMode, FillRequest,
    GoBackRequest, GoForwardRequest, HandleDialogRequest, HoverRequest, InspectPageRequest,
    InteractionAnchor, InteractionLocator, InteractionOutcome, InteractionRecord,
    InteractionResult, InteractionTiming, KeyChord, KeySegment, ListPagesRequest, LiveObservation,
    LiveObservationRequest, LocatorKind, LocatorSummary, MAX_OPERATION_TIMEOUT,
    MAX_WAIT_POLL_INTERVAL, MIN_WAIT_POLL_INTERVAL, ManagedProfilePersistence, ManagedProfileRef,
    Modifier, Modifiers, MouseButton, NamedKey, NavigatePageRequest, NavigationState,
    NodeReference, ObservationContext, ObservationPart, OperationEvidence, OperationMutability,
    PageChange, PageOperationOutcome, PageOperationResult, PageSelection, PageSnapshot, PageState,
    PageStatus, PageTarget, PressKeysRequest, ProfileIdentity, ProfileRef,
    REQUIRED_RENDERER_CAPABILITIES, ReadOnlyEvaluationRequest, ReloadPageRequest,
    RendererCapability, SanitizedParameters, ScreenshotMetadata, ScreenshotRequest,
    ScreenshotTarget, ScrollDelta, ScrollRequest, SelectOptionRequest, SelectPageRequest,
    SelectValue, SnapshotGeneration, SnapshotNode, SnapshotNodeId, SnapshotPageRequest,
    SupervisedTarget, TargetVisibility, UploadFilesRequest, UrlMatch, ValidatedFilePath,
    ViewportState, WaitCondition, WaitOutcome, WaitPresence, WaitProbe, WaitRequest, WaitResult,
    WaitTextMatch, wait_timeout_error,
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
    ArtifactCacheKey, ArtifactCacheMetadata, ArtifactLookup, ArtifactPublication, ArtifactPublish,
    ArtifactSourceFingerprint, ArtifactStore, AttachBrowser, BrowserConnectRequest,
    BrowserConnector, BrowserFailureKind, BrowserOperationContext, BrowserPageTargets,
    BrowserSessionEvents, BrowserSessionPort, CancellationSignal, CaptureGapStore, FrameSource,
    IdSource, InteractionAnchorSource, InteractionEvidenceSink, InteractionRecordSource,
    LaunchBrowser, ManagedProfile, MonotonicClock, PortFuture, RecordingCatalog, RecordingSink,
    RetentionStore, StoredArtifact, TimelineAnchorSource, TimelineStore, WallClock,
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
    AnchorScope, CaptureGapPolicy, FrameAvailability, InteractionWindow, MAX_NATURAL_ANCHOR_WINDOW,
    ObservationKind, ObservationPayloadRef, RangeResolutionOptions, ResolvedRange, RetentionPolicy,
    RetentionWarning, TemporalQuery, TemporalQueryRequest, TemporalQueryService,
    TemporalRangeAnchor, TemporalRangeAnchorKind, TemporalRangeResolver, TimelineObservation,
};
