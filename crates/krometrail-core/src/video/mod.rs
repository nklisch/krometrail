//! Deterministic, infrastructure-free temporal-video contracts.

mod plan;

pub use plan::{
    MAX_VIDEO_ENCODED_INPUT_BYTES, MAX_VIDEO_ENCODED_OUTPUT_BYTES, MAX_VIDEO_HEIGHT,
    MAX_VIDEO_MEANINGFUL_FRAMES, MAX_VIDEO_PRESENTATION_DURATION, MAX_VIDEO_PRESENTATION_SEGMENTS,
    MAX_VIDEO_SOURCE_DURATION, MAX_VIDEO_SOURCE_FRAMES, MAX_VIDEO_WIDTH, PresentationRange,
    PresentationTime, TEMPORAL_VIDEO_PLAN_VERSION, VideoOutputGeometry, VideoPlanInput,
    VideoPresentationPlan, VideoPresentationPolicy, VideoPresentationSegment, VideoSegmentSource,
    VideoTimingBasis,
};

#[cfg(test)]
mod tests;
