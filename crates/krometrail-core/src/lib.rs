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
pub mod range_handle;
pub mod recording;
pub mod time;
pub mod timeline;
pub mod video;

mod validation;

pub use artifacts::{
    AnalysisScale, ArtifactCacheDisposition, ArtifactEpochSelection, ArtifactFailurePolicy,
    ArtifactGeneration, ArtifactGenerationContext, ArtifactGenerationRequest,
    ArtifactGenerationResult, ArtifactGeneratorRequest, ArtifactHandle, ArtifactLabelsRequest,
    ArtifactManifest, ArtifactMarker, ArtifactMarkerId, ArtifactOutcome, ArtifactSampling,
    DEFAULT_ARTIFACT_BLACK_BACKGROUND, DEFAULT_ARTIFACT_NOISE_FLOOR, DEFAULT_ARTIFACT_TILE_LIMIT,
    DEFAULT_DIFFERENCE_MAP_MAX_BYTES, DEFAULT_DIFFERENCE_MAP_MAX_HEIGHT,
    DEFAULT_DIFFERENCE_MAP_MAX_WIDTH, DEFAULT_STORYBOARD_MAX_BYTES, DEFAULT_STORYBOARD_MAX_HEIGHT,
    DEFAULT_STORYBOARD_MAX_WIDTH, DifferenceMapRequest, FrameSelector,
    MAX_ANALYSIS_DOWNSCALE_FACTOR, MAX_FILMSTRIP_TILE_LIMIT, MAX_STORYBOARD_TILE_LIMIT,
    MIN_ANALYSIS_DOWNSCALE_FACTOR, MIN_FILMSTRIP_TILE_LIMIT, MIN_STORYBOARD_TILE_LIMIT,
    MotionHistoryRequest, NormalizationRequest, OutputLimitsRequest, RegionFilmstripRequest,
    StoryboardRequest, VisualEpoch,
};
pub use browser::{
    AcceptedLocator, AccessibleProperty, AccessibleValue, ActionCategory, ActionDefinition,
    ActionabilityRequirement, ActivatePageRequest, BROWSER_EVENT_REGISTRY,
    BROWSER_OPERATION_REGISTRY, BatchFailurePolicy, BatchOptions, BatchOutcome, BatchRequest,
    BatchResult, BatchSkipReason, BatchStepResult, BatchStepStatus, BrowserActionRequest,
    BrowserClosure, BrowserCompatibility, BrowserDialogType, BrowserEvent, BrowserEventBatch,
    BrowserEventClass, BrowserEventCollectionGap, BrowserEventCollectionState,
    BrowserEventCollectionStatus, BrowserEventDefinition, BrowserEventGapReason, BrowserEventKind,
    BrowserEventOrdinal, BrowserEventPayload, BrowserEventSeverity, BrowserInstallation,
    BrowserInstallationSource, BrowserOperationDefinition, BrowserOperationKind,
    BrowserOperationRequest, BrowserOperationResult, BrowserOperationScope,
    BrowserOperationScopeKind, BrowserOwnership, BrowserProduct, BrowserProductVersion,
    BrowserSessionEvent, BrowserSessionState, BrowserSourceClock, BrowserSourceTimestamp,
    BrowserStatus, BrowserStopOutcome, BrowserVersion, CancelDownloadRequest, CancelDownloadResult,
    CapabilitySupport, ClickRequest, ClipboardRead, ClipboardWriteResult, ClosePageRequest,
    CompletionKind, ConsoleArgumentType, ConsoleEvent, ConsoleEventSource, ConsoleLevel,
    ConsoleMethod, CoordinateSpace, CreatePageRequest, CssPoint, CssRect, CssSize,
    DEFAULT_MANAGED_PROFILE_NAME, DEFAULT_SEMANTIC_MATCH_LIMIT, DialogAction, DialogClosedEvent,
    DialogOpenedEvent, DocumentReadiness, DownloadDisplayName, DownloadInventory, DownloadSequence,
    DownloadState, DragRequest, EffectiveViewport, ElementLocator, ElementState, EncodedScreenshot,
    EvaluationResult, EvaluationValue, EventRedactor, ExceptionEvent, FillMode, FillRequest,
    FrameAccess, GoBackRequest, GoForwardRequest, HandleDialogRequest, HoverRequest, HttpMethod,
    HttpStatus, InspectPageRequest, InteractionAnchor, InteractionLocator, InteractionOutcome,
    InteractionRecord, InteractionResult, InteractionTiming, KeyChord, KeySegment,
    ListFramesRequest, ListPageAssetsRequest, ListPageContextsRequest, ListPagesRequest,
    LiveObservation, LiveObservationRequest, LocatorKind, LocatorSummary,
    MAX_BROWSER_EVENT_BATCH_BYTES, MAX_BROWSER_EVENT_BATCH_ROWS, MAX_BROWSER_EVENT_PAYLOAD_BYTES,
    MAX_CLIPBOARD_TEXT_BYTES, MAX_CONSOLE_ARGUMENT_TYPES, MAX_DOWNLOAD_WAIT_MILLIS,
    MAX_EVENT_STACK_FRAMES, MAX_MANAGED_DOWNLOAD_BYTES, MAX_MANAGED_DOWNLOADS,
    MAX_NETWORK_INITIATOR_STACK_FRAMES, MAX_OPERATION_TIMEOUT, MAX_PAGE_ASSETS, MAX_PAGE_FRAMES,
    MAX_REDACTED_FUNCTION_BYTES, MAX_REDACTED_NAME_BYTES, MAX_REDACTED_TEXT_BYTES,
    MAX_SEMANTIC_MATCH_LIMIT, MAX_SEMANTIC_QUERY_TEXT_BYTES, MAX_SEMANTIC_RELAXED_CANDIDATES,
    MAX_VIEWPORT_DEVICE_SCALE, MAX_VIEWPORT_DIMENSION, MAX_WAIT_POLL_INTERVAL,
    MIN_OPERATION_TIMEOUT_MILLIS, MIN_WAIT_POLL_INTERVAL, ManagedDownload, ManagedDownloadRead,
    ManagedProfilePersistence, ManagedProfileRef, ManagedProfileSummary, Modifier, Modifiers,
    MouseButton, NamedKey, NavigatePageRequest, NavigationEvent, NavigationFrameScope,
    NavigationState, NavigationTransition, NetworkFailureKind, NetworkInitiator,
    NetworkInitiatorKind, NetworkRequestFailed, NetworkRequestFinished, NetworkRequestStarted,
    NetworkResourceType, NetworkResponseReceived, NodeReference, ObservationContext,
    ObservationPart, OpenDialogState, OperationEvidence, OperationMutability, PageAssetInventory,
    PageAssetKind, PageAssetMetadata, PageChange, PageContextInventory, PageContextStatus,
    PageFrameInventory, PageFrameReference, PageFrameStatus, PageLifecycleEvent, PageLifecycleName,
    PageOperationOutcome, PageOperationResult, PageSelection, PageSequence, PageSnapshot,
    PageState, PageStatus, PageTarget, PressKeysRequest, ProfileIdentity, ProfileRef,
    QueryPageRequest, QueryPageResult, REQUIRED_RENDERER_CAPABILITIES, ReadClipboardRequest,
    ReadManagedDownloadRequest, ReadOnlyEvaluationRequest, RedactedText, RelaxedMatchCandidates,
    ReloadPageRequest, RendererCapability, SanitizedParameters, SanitizedStackFrame, SanitizedUrl,
    SanitizedUrlScheme, ScreenshotMetadata, ScreenshotRequest, ScreenshotTarget, ScrollDelta,
    ScrollRequest, SelectOptionRequest, SelectPageRequest, SelectValue, SemanticDocumentScope,
    SemanticMatch, SemanticQuery, SemanticQueryOutcome, SemanticTextMatch, SemanticTextMatchMode,
    SetViewportRequest, ShutdownFailurePhase, ShutdownQuality, SnapshotGeneration, SnapshotNode,
    SnapshotNodeId, SnapshotPageAnchor, SnapshotPageRequest, SupervisedTarget,
    TargetLifecycleEvent, TargetVisibility, TargetVisibilityEvent, UploadFilesRequest, UrlMatch,
    ValidatedFilePath, ViewportGuidance, ViewportGuidanceCode, ViewportIntent,
    ViewportMaterialization, ViewportMetrics, ViewportOperationResult, ViewportOverride,
    ViewportPreset, ViewportState, WaitCondition, WaitForDownloadRequest, WaitForPageRequest,
    WaitForPageResult, WaitOutcome, WaitPresence, WaitProbe, WaitRequest, WaitResult,
    WaitTextMatch, WriteClipboardRequest, viewport_guidance, wait_timeout_error,
};
pub use capabilities::{
    CAPABILITY_REGISTRY, CapabilityDefault, CapabilityDefinition, CapabilityId, CapabilitySnapshot,
    CapabilityState, RecordingSubsystem, capability, validate_capability_selection,
};
pub use debug_bundle::{
    BundleArtifactEvidence, BundleContextEvidence, BundleDegradation, BundleEpochScope,
    BundleEpochVisualSummary, BundleWarning, EffectiveBundlePolicy, EvidencePosture,
    MAX_BUNDLE_ARTIFACT_MARKERS, MAX_BUNDLE_CALLER_MARKERS, MAX_BUNDLE_HEADER_BYTES,
    MAX_BUNDLE_TIMELINE_ROWS, OrientationPolicy, TEMPORAL_DEBUG_BUNDLE_OPERATION,
    TemporalDebugBundle, TemporalDebugBundleContext, TemporalDebugBundleOperationDefinition,
    TemporalDebugBundleRequest, TemporalDebugBundles, TemporalDebugHeader,
};
pub use error::{
    EmptyTextError, ErrorCode, ErrorContext, KrometrailError, NonEmptyText, PersistenceFailure,
    PersistenceFailureCategory, PersistenceOperation, PersistenceRecoverability, Result,
    RetryAdvice,
};
pub use ids::{
    ArtifactId, BrowserEventId, DownloadId, FrameId, GapId, IdValue, InteractionId, MarkerId,
    NavigationId, NetworkRequestId, ResolvedRangeHandleId, SegmentId, SessionId, TargetId,
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
    SourceFramesRequest, SourceReadLimitsRequest, coalesce_protected_ranges,
};
pub use range_handle::{
    MAX_RESOLVED_RANGE_HANDLE_BUDGET_BYTES, MAX_RESOLVED_RANGE_HANDLES, ResolvedRangeHandles,
};
pub use recording::{
    ByteOffset, CaptureFailure, CaptureFailureStage, CaptureGap, CaptureGapReason, CaptureOrdinal,
    CaptureStatistics, CaptureStreamState, CaptureTimingSummary, CaptureWarning, CapturedFrame,
    DEFAULT_ARTIFACT_GRACE, DEFAULT_DISK_BUDGET_BYTES, DEFAULT_RETENTION_MAX_AGE,
    DEFAULT_TRIM_HIGH_WATER_PERCENT, DeviceScaleFactor, DiskBudgetBytes, EncodedFrame,
    FrameAddress, ImageFormat, PinChange, PixelDimensions, RecordingBudgetState, RecordingSession,
    RetainedPoint, RetentionLifecycle, RetentionRange, RetentionStatus, SessionDeletion,
    StorageUsage, TargetCaptureStatus,
};
pub use time::{ObservedTime, SessionOrigin, SessionRange, SessionTime, SourceTime};
pub use timeline::{
    AnchorScope, BrowserEventContext, BrowserEventDetailRequest, BrowserEventFilter,
    BrowserEventSelection, BrowserEventSelectionReason, CadenceSummary, CaptureGapPolicy,
    CaptureGapSummary, CaptureQuality, CaptureQualityWarning, CaptureStatusEvidence,
    CaptureStatusPoint, CaptureWarningSummary, DEFAULT_CHRONOLOGICAL_EVENT_LIMIT,
    DEFAULT_COMPACT_EVENT_LIMIT, EventCompactLimit, EventQueryWarning, FrameAvailability,
    FramePoint, InteractionWindow, IntervalAnchorScope, MAX_CAPTURE_QUALITY_FRAMES,
    MAX_COMPACT_EVENT_LIMIT, MAX_FOCUS_TIMES, MAX_NATURAL_ANCHOR_WINDOW, ObservationKind,
    ObservationPayloadRef, RangeResolutionOptions, ResolvedAnchor, ResolvedAnchorReference,
    ResolvedRange, RetentionPolicy, RetentionWarning, SelectedBrowserEvent,
    TEMPORAL_CONTEXT_OPERATION_REGISTRY, TEMPORAL_RANGE_RESOLUTION_OPERATION, TemporalContext,
    TemporalContextOperationDefinition, TemporalContextOperationKind, TemporalContextQuery,
    TemporalContextRequest, TemporalContextService, TemporalQuery, TemporalQueryRequest,
    TemporalQueryService, TemporalRangeAnchor, TemporalRangeAnchorKind, TemporalRangeResolution,
    TemporalRangeResolutionOperationDefinition, TemporalRangeResolver, TimelineObservation,
};
pub use video::{
    MAX_VIDEO_ENCODED_INPUT_BYTES, MAX_VIDEO_ENCODED_OUTPUT_BYTES, MAX_VIDEO_HEIGHT,
    MAX_VIDEO_MEANINGFUL_FRAMES, MAX_VIDEO_PRESENTATION_DURATION, MAX_VIDEO_PRESENTATION_SEGMENTS,
    MAX_VIDEO_SOURCE_DURATION, MAX_VIDEO_SOURCE_FRAMES, MAX_VIDEO_WIDTH,
    MINIMUM_VISIBLE_FRAME_NANOS, MODEL_GAP_HOLD_NANOS, MODEL_MEANINGFUL_HOLD_NANOS,
    PresentationRange, PresentationTime, TEMPORAL_VIDEO_MANIFEST_SCHEMA_VERSION,
    TEMPORAL_VIDEO_OPERATION, TEMPORAL_VIDEO_PLAN_VERSION, TERMINAL_HOLD_NANOS,
    TemporalVideoGeneration, TemporalVideoGenerationClip, TemporalVideoGenerationRequest,
    TemporalVideoGenerationResult, TemporalVideoManifest, TemporalVideoOperationDefinition,
    VIDEO_MEANINGFUL_SELECTOR_NAME, VIDEO_MEANINGFUL_SELECTOR_VERSION, VideoArtifactEvidenceHandle,
    VideoArtifactRead, VideoGapEvidence, VideoOutputGeometry, VideoPlanInput,
    VideoPresentationPlan, VideoPresentationPolicy, VideoPresentationSegment, VideoSegmentSource,
    VideoSelectionIdentity, VideoTimingBasis, canonical_video_cache_parameters,
};
