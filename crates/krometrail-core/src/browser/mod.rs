//! Browser and page-target domain contracts.

mod control;
mod interaction;
mod observation;
mod operation;
mod session;
mod target;

pub use control::{
    BrowserStatus, ClosePageRequest, CreatePageRequest, DEFAULT_MANAGED_PROFILE_NAME,
    GoBackRequest, GoForwardRequest, InteractionAnchor, InteractionTiming, ListPagesRequest,
    NavigatePageRequest, PageChange, PageOperationOutcome, PageOperationResult, PageSelection,
    PageStatus, ReloadPageRequest, SelectPageRequest,
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
