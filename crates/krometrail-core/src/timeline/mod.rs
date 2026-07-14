//! Timeline observation and temporal-range contracts.

mod observation;
pub mod range;

pub use observation::{ObservationKind, ObservationPayloadRef, TimelineObservation};
pub use range::{
    AnchorScope, CaptureGapPolicy, InteractionWindow, RangeResolutionOptions, ResolvedRange,
    RetentionPolicy, RetentionWarning, TemporalRangeAnchor, TemporalRangeAnchorKind,
};
