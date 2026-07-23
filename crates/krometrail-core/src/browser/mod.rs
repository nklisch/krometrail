//! Browser and page-target domain contracts.

mod assets;
mod batch;
mod contexts;
mod control;
mod events;
mod interaction;
mod local_io;
mod observation;
mod operation;
mod postcondition;
mod privacy;
mod session;
mod target;
mod viewport;
mod wait;

pub use assets::{
    ListPageAssetsRequest, MAX_PAGE_ASSETS, PageAssetInventory, PageAssetKind, PageAssetMetadata,
};
pub use batch::{
    BatchFailurePolicy, BatchOptions, BatchOutcome, BatchRequest, BatchResult, BatchSkipReason,
    BatchStepResult, BatchStepStatus, wait_timeout_error,
};
pub use contexts::{
    FrameAccess, ListFramesRequest, ListPageContextsRequest, MAX_KNOWN_PAGE_TARGETS,
    MAX_PAGE_FRAMES, PageContextInventory, PageContextStatus, PageFrameInventory,
    PageFrameReference, PageFrameStatus, PageSequence, SemanticDocumentScope, WaitForPageRequest,
    WaitForPageResult,
};
pub use control::{
    ActivatePageRequest, BrowserStatus, ClosePageRequest, CreatePageRequest,
    DEFAULT_MANAGED_PROFILE_NAME, GoBackRequest, GoForwardRequest, InteractionAnchor,
    InteractionTiming, ListPagesRequest, NavigatePageRequest, OpenDialogState, PageChange,
    PageOperationOutcome, PageOperationResult, PageSelection, PageStatus, ReloadPageRequest,
    SelectPageRequest,
};
pub use events::{
    BROWSER_EVENT_REGISTRY, BrowserDialogType, BrowserEvent, BrowserEventBatch, BrowserEventClass,
    BrowserEventCollectionGap, BrowserEventCollectionState, BrowserEventCollectionStatus,
    BrowserEventDefinition, BrowserEventGapReason, BrowserEventKind, BrowserEventOrdinal,
    BrowserEventPayload, BrowserEventSeverity, BrowserSourceClock, BrowserSourceTimestamp,
    ConsoleArgumentType, ConsoleEvent, ConsoleEventSource, ConsoleLevel, ConsoleMethod,
    DialogClosedEvent, DialogOpenedEvent, ExceptionEvent, HttpMethod, HttpStatus,
    MAX_BROWSER_EVENT_BATCH_BYTES, MAX_BROWSER_EVENT_BATCH_ROWS, MAX_BROWSER_EVENT_PAYLOAD_BYTES,
    MAX_CONSOLE_ARGUMENT_TYPES, MAX_EVENT_STACK_FRAMES, MAX_NETWORK_INITIATOR_STACK_FRAMES,
    NavigationEvent, NavigationFrameScope, NavigationTransition, NetworkFailureKind,
    NetworkInitiator, NetworkInitiatorKind, NetworkRequestFailed, NetworkRequestFinished,
    NetworkRequestStarted, NetworkResourceType, NetworkResponseReceived, PageLifecycleEvent,
    PageLifecycleName, SanitizedStackFrame, TargetLifecycleEvent, TargetVisibilityEvent,
};
pub use interaction::{
    AcceptedLocator, ActionCategory, ActionDefinition, ActionabilityRequirement,
    BrowserActionRequest, ClickRequest, CompletionKind, DialogAction, DragRequest, FillMode,
    FillRequest, HandleDialogRequest, HoverRequest, InteractionLocator, InteractionOutcome,
    InteractionRecord, InteractionResult, KeyChord, KeySegment, LocatorKind, LocatorSummary,
    Modifier, Modifiers, MouseButton, NamedKey, PressKeysRequest, SanitizedParameters, ScrollDelta,
    ScrollRequest, SelectOptionRequest, SelectValue, UploadFilesRequest, ValidatedFilePath,
};
pub use local_io::{
    CancelDownloadRequest, CancelDownloadResult, ClipboardRead, ClipboardWriteResult,
    DownloadDisplayName, DownloadInventory, DownloadSequence, DownloadState, ListDownloadsRequest,
    MAX_CLIPBOARD_TEXT_BYTES, MAX_DOWNLOAD_WAIT_MILLIS, MAX_MANAGED_DOWNLOAD_BYTES,
    MAX_MANAGED_DOWNLOADS, ManagedDownload, ManagedDownloadRead, ReadClipboardRequest,
    ReadManagedDownloadRequest, WaitForDownloadRequest, WriteClipboardRequest,
};
pub use observation::{
    AccessibleProperty, AccessibleValue, CoordinateSpace, CssPoint, CssRect, CssSize,
    DEFAULT_SEMANTIC_MATCH_LIMIT, DocumentReadiness, ElementLocator, EncodedScreenshot,
    EvaluationResult, EvaluationValue, InspectPageRequest, LiveObservation, LiveObservationRequest,
    MAX_GENERIC_CONTAINER_TEXT_BYTES, MAX_SEMANTIC_MATCH_LIMIT, MAX_SEMANTIC_QUERY_TEXT_BYTES,
    MAX_SEMANTIC_RELAXED_CANDIDATES, NavigationState, NodeReference, NormalizedTextNeedle,
    ObservationContext, ObservationPart, PageSnapshot, PageState, QueryPageRequest,
    QueryPageResult, ReadOnlyEvaluationRequest, RelaxedMatchCandidates, ScreenshotMetadata,
    ScreenshotRequest, ScreenshotTarget, SemanticMatch, SemanticQuery, SemanticQueryOutcome,
    SemanticTextMatch, SemanticTextMatchMode, SnapshotGeneration, SnapshotNode, SnapshotNodeId,
    SnapshotPageAnchor, SnapshotPageRequest, ViewportState, collapsed_semantic_text_bytes,
    normalize_semantic_text,
};
pub use operation::{
    BROWSER_OPERATION_REGISTRY, BrowserOperationDefinition, BrowserOperationKind,
    BrowserOperationRequest, BrowserOperationResult, BrowserOperationScope,
    BrowserOperationScopeKind, OperationEvidence, OperationMutability,
};
pub use postcondition::{
    DownloadFact, DownloadPostcondition, ExpectationChannel, ExpectationNote, ExpectationTarget,
    ExpectationTargetRole, FlagObservation, InteractionExpectation, InteractionPostcondition,
    MAX_SIDE_CHANNEL_FACTS, NewPageFact, NewPagePostcondition, NodeStateFacts, PagePostcondition,
    SideChannelSignals, TargetNodeOutcome, TargetPostcondition,
};
pub use privacy::{
    EventRedactor, MAX_REDACTED_FUNCTION_BYTES, MAX_REDACTED_NAME_BYTES, MAX_REDACTED_TEXT_BYTES,
    RedactedText, SanitizedUrl, SanitizedUrlScheme,
};
pub use session::{
    BrowserClosure, BrowserCompatibility, BrowserOwnership, BrowserSessionEvent,
    BrowserSessionState, BrowserStopOutcome, CapabilitySupport, REQUIRED_RENDERER_CAPABILITIES,
    RendererCapability, ShutdownFailurePhase, ShutdownQuality, SupervisedTarget, TargetVisibility,
    renderer_capability_is_required,
};
pub use target::{
    BrowserInstallation, BrowserInstallationSource, BrowserProduct, BrowserProductVersion,
    BrowserVersion, ManagedProfilePersistence, ManagedProfileRef, ManagedProfileSummary,
    PageTarget, ProfileIdentity, ProfileRef,
};
pub use viewport::{
    EffectiveViewport, MAX_VIEWPORT_DEVICE_SCALE, MAX_VIEWPORT_DIMENSION, SetViewportRequest,
    ViewportGuidance, ViewportGuidanceCode, ViewportIntent, ViewportMaterialization,
    ViewportMetrics, ViewportOperationResult, ViewportOverride, ViewportPreset, viewport_guidance,
};
pub(crate) use wait::validate_operation_timeout;
pub use wait::{
    ElementState, MAX_OPERATION_TIMEOUT, MAX_WAIT_POLL_INTERVAL, MIN_OPERATION_TIMEOUT_MILLIS,
    MIN_SEMANTIC_WAIT_POLL_INTERVAL, MIN_SEMANTIC_WAIT_POLL_INTERVAL_MILLIS,
    MIN_WAIT_POLL_INTERVAL, UrlMatch, WaitCondition, WaitOutcome, WaitPresence, WaitProbe,
    WaitRequest, WaitResult, WaitTextMatch,
};
