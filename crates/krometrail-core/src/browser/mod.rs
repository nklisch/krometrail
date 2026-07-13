//! Browser and page-target domain contracts.

mod observation;
mod operation;
mod session;
mod target;

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
    BrowserOperationRequest, BrowserOperationResult, OperationEvidence, OperationMutability,
};
pub use session::{
    BrowserCompatibility, BrowserOwnership, BrowserSessionEvent, BrowserSessionState,
    BrowserStopOutcome, CapabilitySupport, REQUIRED_RENDERER_CAPABILITIES, RendererCapability,
    SupervisedTarget, TargetVisibility, renderer_capability_is_required,
};
pub use target::{
    BrowserInstallation, BrowserInstallationSource, BrowserProduct, BrowserProductVersion,
    BrowserVersion, PageTarget, ProfileIdentity, ProfileRef,
};
