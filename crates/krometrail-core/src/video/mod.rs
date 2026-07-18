//! Deterministic, infrastructure-free temporal-video contracts.

mod manifest;
mod plan;

pub use manifest::{
    TEMPORAL_VIDEO_MANIFEST_SCHEMA_VERSION, TemporalVideoManifest, VideoGapEvidence,
    canonical_video_cache_parameters,
};
pub use plan::{
    MAX_VIDEO_ENCODED_INPUT_BYTES, MAX_VIDEO_ENCODED_OUTPUT_BYTES, MAX_VIDEO_HEIGHT,
    MAX_VIDEO_MEANINGFUL_FRAMES, MAX_VIDEO_PRESENTATION_DURATION, MAX_VIDEO_PRESENTATION_SEGMENTS,
    MAX_VIDEO_SOURCE_DURATION, MAX_VIDEO_SOURCE_FRAMES, MAX_VIDEO_WIDTH,
    MINIMUM_VISIBLE_FRAME_NANOS, MODEL_GAP_HOLD_NANOS, MODEL_MEANINGFUL_HOLD_NANOS,
    PresentationRange, PresentationTime, TEMPORAL_VIDEO_PLAN_VERSION, TERMINAL_HOLD_NANOS,
    VideoOutputGeometry, VideoPlanInput, VideoPresentationPlan, VideoPresentationPolicy,
    VideoPresentationSegment, VideoSegmentSource, VideoTimingBasis,
};

#[cfg(test)]
mod tests;
