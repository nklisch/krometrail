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
pub mod debug_bundle;
pub mod error;
pub mod ids;
pub mod lifecycle;
pub mod ports;
pub mod progressive;
pub mod recording;
pub mod time;
pub mod timeline;
pub mod video;

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
    ActionabilityRequirement, BROWSER_EVENT_REGISTRY, BROWSER_OPERATION_REGISTRY,
    BatchFailurePolicy, BatchOptions, BatchOutcome, BatchRequest, BatchResult, BatchSkipReason,
    BatchStepResult, BatchStepStatus, BrowserActionRequest, BrowserCompatibility,
    BrowserDialogType, BrowserEvent, BrowserEventBatch, BrowserEventClass,
    BrowserEventCollectionGap, BrowserEventCollectionState, BrowserEventCollectionStatus,
    BrowserEventDefinition, BrowserEventGapReason, BrowserEventKind, BrowserEventOrdinal,
    BrowserEventPayload, BrowserEventSeverity, BrowserInstallation, BrowserInstallationSource,
    BrowserOperationDefinition, BrowserOperationKind, BrowserOperationRequest,
    BrowserOperationResult, BrowserOperationScope, BrowserOperationScopeKind, BrowserOwnership,
    BrowserProduct, BrowserProductVersion, BrowserSessionEvent, BrowserSessionState,
    BrowserSourceClock, BrowserSourceTimestamp, BrowserStatus, BrowserStopOutcome, BrowserVersion,
    CapabilitySupport, ClickRequest, ClosePageRequest, CompletionKind, ConsoleArgumentType,
    ConsoleEvent, ConsoleEventSource, ConsoleLevel, ConsoleMethod, CoordinateSpace,
    CreatePageRequest, CssPoint, CssRect, CssSize, DEFAULT_MANAGED_PROFILE_NAME, DialogAction,
    DialogClosedEvent, DialogOpenedEvent, DocumentReadiness, DragRequest, EffectiveViewport,
    ElementLocator, ElementState, EncodedScreenshot, EvaluationResult, EvaluationValue,
    EventRedactor, ExceptionEvent, FillMode, FillRequest, GoBackRequest, GoForwardRequest,
    HandleDialogRequest, HoverRequest, HttpMethod, HttpStatus, InspectPageRequest,
    InteractionAnchor, InteractionLocator, InteractionOutcome, InteractionRecord,
    InteractionResult, InteractionTiming, KeyChord, KeySegment, ListPagesRequest, LiveObservation,
    LiveObservationRequest, LocatorKind, LocatorSummary, MAX_BROWSER_EVENT_BATCH_BYTES,
    MAX_BROWSER_EVENT_BATCH_ROWS, MAX_BROWSER_EVENT_PAYLOAD_BYTES, MAX_CONSOLE_ARGUMENT_TYPES,
    MAX_EVENT_STACK_FRAMES, MAX_NETWORK_INITIATOR_STACK_FRAMES, MAX_OPERATION_TIMEOUT,
    MAX_REDACTED_FUNCTION_BYTES, MAX_REDACTED_NAME_BYTES, MAX_REDACTED_TEXT_BYTES,
    MAX_VIEWPORT_DEVICE_SCALE, MAX_VIEWPORT_DIMENSION, MAX_WAIT_POLL_INTERVAL,
    MIN_WAIT_POLL_INTERVAL, ManagedProfilePersistence, ManagedProfileRef, Modifier, Modifiers,
    MouseButton, NamedKey, NavigatePageRequest, NavigationEvent, NavigationFrameScope,
    NavigationState, NavigationTransition, NetworkFailureKind, NetworkInitiator,
    NetworkInitiatorKind, NetworkRequestFailed, NetworkRequestFinished, NetworkRequestStarted,
    NetworkResourceType, NetworkResponseReceived, NodeReference, ObservationContext,
    ObservationPart, OperationEvidence, OperationMutability, PageChange, PageLifecycleEvent,
    PageLifecycleName, PageOperationOutcome, PageOperationResult, PageSelection, PageSnapshot,
    PageState, PageStatus, PageTarget, PressKeysRequest, ProfileIdentity, ProfileRef,
    REQUIRED_RENDERER_CAPABILITIES, ReadOnlyEvaluationRequest, RedactedText, ReloadPageRequest,
    RendererCapability, SanitizedParameters, SanitizedStackFrame, SanitizedUrl, SanitizedUrlScheme,
    ScreenshotMetadata, ScreenshotRequest, ScreenshotTarget, ScrollDelta, ScrollRequest,
    SelectOptionRequest, SelectPageRequest, SelectValue, SetViewportRequest, SnapshotGeneration,
    SnapshotNode, SnapshotNodeId, SnapshotPageRequest, SupervisedTarget, TargetLifecycleEvent,
    TargetVisibility, TargetVisibilityEvent, UploadFilesRequest, UrlMatch, ValidatedFilePath,
    ViewportMetrics, ViewportOperationResult, ViewportOverride, ViewportState, WaitCondition,
    WaitOutcome, WaitPresence, WaitProbe, WaitRequest, WaitResult, WaitTextMatch,
    wait_timeout_error,
};
pub use capabilities::{
    CAPABILITY_REGISTRY, CapabilityDefault, CapabilityDefinition, CapabilityId, CapabilitySnapshot,
    CapabilityState, RecordingSubsystem, capability, validate_capability_selection,
};
pub use debug_bundle::{
    BundleArtifactEvidence, BundleContextEvidence, BundleDegradation, BundleEpochVisualSummary,
    BundleWarning, EffectiveBundlePolicy, EvidencePosture, MAX_BUNDLE_ARTIFACT_MARKERS,
    MAX_BUNDLE_CALLER_MARKERS, MAX_BUNDLE_HEADER_BYTES, MAX_BUNDLE_TIMELINE_ROWS,
    OrientationPolicy, TEMPORAL_DEBUG_BUNDLE_OPERATION, TEMPORAL_DEBUG_BUNDLE_POLICY_VERSION,
    TemporalDebugBundle, TemporalDebugBundleContext, TemporalDebugBundleOperationDefinition,
    TemporalDebugBundleRequest, TemporalDebugBundles, TemporalDebugHeader,
};
pub use error::{
    EmptyTextError, ErrorCode, ErrorContext, KrometrailError, NonEmptyText, Result, RetryAdvice,
};
pub use ids::{
    ArtifactId, BrowserEventId, FrameId, GapId, IdValue, InteractionId, MarkerId, NavigationId,
    NetworkRequestId, SegmentId, SessionId, TargetId,
};
pub use lifecycle::{SessionLifecycle, TargetLifecycle};
pub use ports::{
    ArtifactCacheKey, ArtifactCacheMetadata, ArtifactLookup, ArtifactPublication, ArtifactPublish,
    ArtifactReadLookup, ArtifactSourceFingerprint, ArtifactStore, AttachBrowser,
    BrowserConnectRequest, BrowserConnector, BrowserEventCursor, BrowserEventSelector,
    BrowserEventSink, BrowserEventSource, BrowserEventUnavailableRange,
    BrowserEventUnavailableReason, BrowserFailureKind, BrowserFocusPolicy, BrowserOperationContext,
    BrowserPageTargets, BrowserSessionEvents, BrowserSessionPort, CancellationSignal,
    CaptureGapStore, CaptureStatusSamples, CurrentReferenceGeometry,
    CurrentReferenceGeometryRequest, DEFAULT_EVENT_PAGE_ROWS, EventCandidateLimit, EventPageLimit,
    EveryNthFrame, FrameSource, IdSource, InteractionAnchorSource, InteractionEvidenceSink,
    InteractionRecordSource, LaunchBrowser, MAX_CAPTURE_STATUS_SAMPLES, MAX_EVENT_CANDIDATE_ROWS,
    MAX_EVENT_PAGE_ROWS, MAX_EVENT_UNAVAILABLE_RANGES, MAX_EVERY_NTH_FRAME,
    MAX_TIMELINE_RANGE_ROWS, MAX_VIDEO_ENCODER_LABEL_BYTES, MIN_EVERY_NTH_FRAME, ManagedProfile,
    MonotonicClock, PortFuture, ProgressiveEvidenceStore, RecordingCatalog, RecordingSink,
    ResolvedReferenceGeometry, RetentionStore, StoredArtifact, StoredVideoArtifact,
    TEMPORAL_VIDEO_GENERATOR_NAME, TEMPORAL_VIDEO_GENERATOR_VERSION, TemporalVideoEncoder,
    TimelineAnchorSource, TimelineRangeQuery, TimelineRangeSlice, TimelineStore,
    VideoArtifactLookup, VideoArtifactPublication, VideoArtifactPublish, VideoArtifactReadLookup,
    VideoEncodeFrame, VideoEncodeRequest, VideoEncodedClip, VideoEncoderIdentity,
    VideoEncodingContext, VideoEncodingProfile, WallClock,
};
pub use progressive::{
    ArtifactEvidenceHandle, ArtifactRead, CallerRegionShape, EvidenceScope,
    GenerateArtifactsRequest, OperationExposure, PROGRESSIVE_EVIDENCE_REGISTRY,
    PinChange as ProgressivePinChange, PinProtectionScope, PinState, ProgressiveEvidence,
    ProgressiveEvidenceContext, ProgressiveEvidenceOperationDefinition,
    ProgressiveEvidenceOperationKind, ProgressiveEvidenceRequest, ProgressiveEvidenceResult,
    ProgressiveRegion, ProtectedSegment, RangeEvidenceAvailability, RegionFilmstripEvidence,
    RegionFilmstripEvidenceRequest, ResolvedProgressiveRegion, ResolvedRangeEvidenceRequest,
    RetentionPinRequest, RetrieveArtifactRequest, RetrieveSourceFrameRequest, Sha256Digest,
    SourceFrameBatch, SourceFrameHandle, SourceFrameList, SourceFrameRead, SourceFrameSelection,
    SourceFramesRequest, SourceReadLimitsRequest,
};
pub use recording::{
    ByteOffset, CaptureFailureStage, CaptureGap, CaptureGapReason, CaptureOrdinal,
    CaptureStatistics, CaptureStreamState, CaptureTimingSummary, CaptureWarning, CapturedFrame,
    DEFAULT_DISK_BUDGET_BYTES, DeviceScaleFactor, DiskBudgetBytes, EncodedFrame, FrameAddress,
    ImageFormat, PinChange, PixelDimensions, RecordingBudgetState, RecordingSession, RetainedPoint,
    RetentionRange, RetentionStatus, SessionDeletion, StorageUsage, TargetCaptureStatus,
};
pub use time::{ObservedTime, SessionOrigin, SessionRange, SessionTime, SourceTime};
pub use timeline::{
    AnchorScope, BrowserEventContext, BrowserEventDetailRequest, BrowserEventFilter,
    BrowserEventSelection, BrowserEventSelectionReason, CadenceSummary, CaptureGapPolicy,
    CaptureGapSummary, CaptureQuality, CaptureQualityWarning, CaptureStatusEvidence,
    CaptureStatusPoint, CaptureWarningSummary, DEFAULT_CHRONOLOGICAL_EVENT_LIMIT,
    DEFAULT_COMPACT_EVENT_LIMIT, EventCompactLimit, EventQueryWarning, FrameAvailability,
    FramePoint, InteractionWindow, MAX_CAPTURE_QUALITY_FRAMES, MAX_COMPACT_EVENT_LIMIT,
    MAX_FOCUS_TIMES, MAX_NATURAL_ANCHOR_WINDOW, ObservationKind, ObservationPayloadRef,
    RangeResolutionOptions, ResolvedAnchor, ResolvedAnchorReference, ResolvedRange,
    RetentionPolicy, RetentionWarning, SelectedBrowserEvent, TEMPORAL_CONTEXT_OPERATION_REGISTRY,
    TemporalContext, TemporalContextOperationDefinition, TemporalContextOperationKind,
    TemporalContextQuery, TemporalContextRequest, TemporalContextService, TemporalQuery,
    TemporalQueryRequest, TemporalQueryService, TemporalRangeAnchor, TemporalRangeAnchorKind,
    TemporalRangeResolver, TimelineObservation,
};
pub use video::{
    MAX_VIDEO_ENCODED_INPUT_BYTES, MAX_VIDEO_ENCODED_OUTPUT_BYTES, MAX_VIDEO_HEIGHT,
    MAX_VIDEO_MEANINGFUL_FRAMES, MAX_VIDEO_PRESENTATION_DURATION, MAX_VIDEO_PRESENTATION_SEGMENTS,
    MAX_VIDEO_SOURCE_DURATION, MAX_VIDEO_SOURCE_FRAMES, MAX_VIDEO_WIDTH,
    MINIMUM_VISIBLE_FRAME_NANOS, MODEL_GAP_HOLD_NANOS, MODEL_MEANINGFUL_HOLD_NANOS,
    PresentationRange, PresentationTime, TEMPORAL_VIDEO_MANIFEST_SCHEMA_VERSION,
    TEMPORAL_VIDEO_PLAN_VERSION, TERMINAL_HOLD_NANOS, TemporalVideoGeneration,
    TemporalVideoGenerationClip, TemporalVideoGenerationRequest, TemporalVideoGenerationResult,
    TemporalVideoManifest, VIDEO_MEANINGFUL_SELECTOR_NAME, VIDEO_MEANINGFUL_SELECTOR_VERSION,
    VideoArtifactEvidenceHandle, VideoArtifactRead, VideoGapEvidence, VideoOutputGeometry,
    VideoPlanInput, VideoPresentationPlan, VideoPresentationPolicy, VideoPresentationSegment,
    VideoSegmentSource, VideoSelectionIdentity, VideoTimingBasis, canonical_video_cache_parameters,
};
