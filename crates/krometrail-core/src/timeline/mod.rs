//! Timeline observation and temporal-range contracts.

mod observation;
mod query;
pub mod range;

pub use observation::{ObservationKind, ObservationPayloadRef, TimelineObservation};
pub use query::{TemporalQuery, TemporalQueryRequest, TemporalQueryService};
pub use range::{
    AnchorScope, CaptureGapPolicy, FrameAvailability, InteractionWindow, MAX_NATURAL_ANCHOR_WINDOW,
    RangeResolutionOptions, ResolvedRange, RetentionPolicy, RetentionWarning, TemporalRangeAnchor,
    TemporalRangeAnchorKind, TemporalRangeResolver,
};
