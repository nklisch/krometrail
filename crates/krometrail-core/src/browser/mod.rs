//! Browser and page-target domain contracts.

mod batch;
mod control;
mod events;
mod interaction;
mod observation;
mod operation;
mod privacy;
mod session;
mod target;
mod wait;

pub use batch::{
    BatchFailurePolicy, BatchOptions, BatchOutcome, BatchRequest, BatchResult, BatchSkipReason,
    BatchStepResult, BatchStepStatus, wait_timeout_error,
};
pub use control::{
    BrowserStatus, ClosePageRequest, CreatePageRequest, DEFAULT_MANAGED_PROFILE_NAME,
    GoBackRequest, GoForwardRequest, InteractionAnchor, InteractionTiming, ListPagesRequest,
    NavigatePageRequest, PageChange, PageOperationOutcome, PageOperationResult, PageSelection,
    PageStatus, ReloadPageRequest, SelectPageRequest,
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
pub use observation::{
    AccessibleProperty, AccessibleValue, CoordinateSpace, CssPoint, CssRect, CssSize,
    DocumentReadiness, ElementLocator, EncodedScreenshot, EvaluationResult, EvaluationValue,
    InspectPageRequest, LiveObservation, LiveObservationRequest, NavigationState, NodeReference,
    ObservationContext, ObservationPart, PageSnapshot, PageState, ReadOnlyEvaluationRequest,
    ScreenshotMetadata, ScreenshotRequest, ScreenshotTarget, SnapshotGeneration, SnapshotNode,
    SnapshotNodeId, SnapshotPageRequest, ViewportState,
};
pub use operation::{
    BROWSER_OPERATION_REGISTRY, BrowserOperationDefinition, BrowserOperationKind,
    BrowserOperationRequest, BrowserOperationResult, BrowserOperationScope,
    BrowserOperationScopeKind, OperationEvidence, OperationMutability,
};
pub use privacy::{
    EventRedactor, MAX_REDACTED_FUNCTION_BYTES, MAX_REDACTED_NAME_BYTES, MAX_REDACTED_TEXT_BYTES,
    RedactedText, SanitizedUrl, SanitizedUrlScheme,
};
pub use session::{
    BrowserCompatibility, BrowserOwnership, BrowserSessionEvent, BrowserSessionState,
    BrowserStopOutcome, CapabilitySupport, REQUIRED_RENDERER_CAPABILITIES, RendererCapability,
    SupervisedTarget, TargetVisibility, renderer_capability_is_required,
};
pub use target::{
    BrowserInstallation, BrowserInstallationSource, BrowserProduct, BrowserProductVersion,
    BrowserVersion, ManagedProfilePersistence, ManagedProfileRef, PageTarget, ProfileIdentity,
    ProfileRef,
};
pub(crate) use wait::validate_operation_timeout;
pub use wait::{
    ElementState, MAX_OPERATION_TIMEOUT, MAX_WAIT_POLL_INTERVAL, MIN_WAIT_POLL_INTERVAL, UrlMatch,
    WaitCondition, WaitOutcome, WaitPresence, WaitProbe, WaitRequest, WaitResult, WaitTextMatch,
};
