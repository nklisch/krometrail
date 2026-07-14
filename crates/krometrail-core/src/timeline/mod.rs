//! Timeline observation and temporal-range contracts.

mod context;
mod observation;
mod query;
pub mod range;

pub use context::{
    BrowserEventContext, BrowserEventDetailRequest, BrowserEventFilter, BrowserEventSelection,
    BrowserEventSelectionReason, CadenceSummary, CaptureGapSummary, CaptureQuality,
    CaptureQualityWarning, CaptureStatusEvidence, CaptureStatusPoint, CaptureWarningSummary,
    DEFAULT_CHRONOLOGICAL_EVENT_LIMIT, DEFAULT_COMPACT_EVENT_LIMIT, EventCompactLimit,
    EventQueryWarning, FramePoint, MAX_CAPTURE_QUALITY_FRAMES, MAX_COMPACT_EVENT_LIMIT,
    MAX_FOCUS_TIMES, SelectedBrowserEvent, TEMPORAL_CONTEXT_OPERATION_REGISTRY, TemporalContext,
    TemporalContextOperationDefinition, TemporalContextOperationKind, TemporalContextQuery,
    TemporalContextRequest, TemporalContextService,
};
pub use observation::{ObservationKind, ObservationPayloadRef, TimelineObservation};
pub use query::{TemporalQuery, TemporalQueryRequest, TemporalQueryService};
pub use range::{
    AnchorScope, CaptureGapPolicy, FrameAvailability, InteractionWindow, MAX_NATURAL_ANCHOR_WINDOW,
    RangeResolutionOptions, ResolvedAnchor, ResolvedAnchorReference, ResolvedRange,
    RetentionPolicy, RetentionWarning, TemporalRangeAnchor, TemporalRangeAnchorKind,
    TemporalRangeResolver,
};
