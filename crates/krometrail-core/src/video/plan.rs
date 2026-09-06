use std::{collections::HashSet, time::Duration};

use serde::{Deserialize, Deserializer, Serialize};

use crate::{
    CapturedFrame, ErrorCode, FrameId, GapId, KrometrailError, NonEmptyText, PixelDimensions,
    ResolvedRange, Result, SessionRange, SessionTime, VisualEpoch,
    error::invalid,
    validation::{delegate_json_schema, deserialize_validated},
};

pub const TEMPORAL_VIDEO_PLAN_VERSION: &str = "temporal-video-plan-v1";
pub const MAX_VIDEO_SOURCE_DURATION: Duration = Duration::from_secs(30);
pub const MAX_VIDEO_PRESENTATION_DURATION: Duration = Duration::from_secs(60);
pub const MAX_VIDEO_SOURCE_FRAMES: usize = 480;
pub const MAX_VIDEO_MEANINGFUL_FRAMES: usize = 12;
pub const MAX_VIDEO_PRESENTATION_SEGMENTS: usize = 512;
pub const MAX_VIDEO_WIDTH: u32 = 1_920;
pub const MAX_VIDEO_HEIGHT: u32 = 1_080;
pub const MAX_VIDEO_ENCODED_INPUT_BYTES: u64 = 512 * 1024 * 1024;
pub const MAX_VIDEO_ENCODED_OUTPUT_BYTES: u64 = 64 * 1024 * 1024;
pub const MINIMUM_VISIBLE_FRAME_NANOS: u64 = 1_000_000;
pub const TERMINAL_HOLD_NANOS: u64 = 250_000_000;
pub const MODEL_MEANINGFUL_HOLD_NANOS: u64 = 1_000_000_000;
pub const MODEL_GAP_HOLD_NANOS: u64 = 500_000_000;

define_stable_enum! {
    pub enum VideoPresentationPolicy {
        RealTime => "real_time",
        ModelOptimized => "model_optimized",
    }
}

define_stable_enum! {
    pub enum VideoTimingBasis {
        RecordedDelta => "recorded_delta",
        MinimumVisibleFrame => "minimum_visible_frame",
        TerminalHold => "terminal_hold",
        RecordedGap => "recorded_gap",
        ModelMeaningfulHold => "model_meaningful_hold",
        ModelGapHold => "model_gap_hold",
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct PresentationTime(u64);

#[derive(schemars::JsonSchema)]
#[schemars(transparent)]
#[allow(dead_code)] // Schema-only proxy keeps the serialized u64 ceiling source-aligned.
struct PresentationTimeSchema(#[schemars(range(max = 60_000_000_000_u64))] u64);

impl PresentationTime {
    pub const ZERO: Self = Self(0);

    pub fn from_nanos(value: u64) -> Result<Self> {
        if value > max_presentation_nanos() {
            return Err(limit_error(
                "video presentation time exceeds the 60 second server limit",
            ));
        }
        Ok(Self(value))
    }

    pub const fn as_nanos(self) -> u64 {
        self.0
    }
}

impl<'de> Deserialize<'de> for PresentationTime {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        deserialize_validated(deserializer, |value: u64| Self::from_nanos(value))
    }
}

delegate_json_schema!(PresentationTime => PresentationTimeSchema);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct PresentationRange {
    start: PresentationTime,
    end: PresentationTime,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct PresentationRangeWire {
    start: PresentationTime,
    end: PresentationTime,
}

impl PresentationRange {
    pub fn new(start: PresentationTime, end: PresentationTime) -> Result<Self> {
        if start >= end {
            return Err(invalid(
                "video presentation ranges must be non-empty and half-open",
            ));
        }
        Ok(Self { start, end })
    }

    pub const fn start(self) -> PresentationTime {
        self.start
    }

    pub const fn end(self) -> PresentationTime {
        self.end
    }

    pub const fn duration_nanos(self) -> u64 {
        self.end.0 - self.start.0
    }
}

impl<'de> Deserialize<'de> for PresentationRange {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        deserialize_validated(deserializer, |wire: PresentationRangeWire| {
            Self::new(wire.start, wire.end)
        })
    }
}

delegate_json_schema!(PresentationRange => PresentationRangeWire);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct VideoOutputGeometry {
    source: PixelDimensions,
    scaled: PixelDimensions,
    canvas: PixelDimensions,
    pad_right: u8,
    pad_bottom: u8,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct VideoOutputGeometryWire {
    source: PixelDimensions,
    #[schemars(with = "VideoCanvasDimensionsSchema")]
    scaled: PixelDimensions,
    #[schemars(with = "VideoCanvasDimensionsSchema")]
    canvas: PixelDimensions,
    #[schemars(range(max = 1_u8))]
    pad_right: u8,
    #[schemars(range(max = 1_u8))]
    pad_bottom: u8,
}

#[derive(schemars::JsonSchema)]
#[allow(dead_code)] // Schema-only proxy publishes the independently enforceable output ceilings.
struct VideoCanvasDimensionsSchema {
    #[schemars(range(min = 1_u32, max = 1_920_u32))]
    width: u32,
    #[schemars(range(min = 1_u32, max = 1_080_u32))]
    height: u32,
}

impl VideoOutputGeometry {
    pub fn new(
        source: PixelDimensions,
        scaled: PixelDimensions,
        canvas: PixelDimensions,
    ) -> Result<Self> {
        let pad_right = canvas
            .width()
            .checked_sub(scaled.width())
            .ok_or_else(|| invalid("video canvas width must not crop the scaled image"))?;
        let pad_bottom = canvas
            .height()
            .checked_sub(scaled.height())
            .ok_or_else(|| invalid("video canvas height must not crop the scaled image"))?;
        let pad_right = u8::try_from(pad_right)
            .map_err(|_| invalid("video output permits at most one right padding pixel"))?;
        let pad_bottom = u8::try_from(pad_bottom)
            .map_err(|_| invalid("video output permits at most one bottom padding pixel"))?;
        let value = Self {
            source,
            scaled,
            canvas,
            pad_right,
            pad_bottom,
        };
        value.validate()?;
        Ok(value)
    }

    pub const fn source(self) -> PixelDimensions {
        self.source
    }

    pub const fn scaled(self) -> PixelDimensions {
        self.scaled
    }

    pub const fn canvas(self) -> PixelDimensions {
        self.canvas
    }

    pub const fn pad_right(self) -> u8 {
        self.pad_right
    }

    pub const fn pad_bottom(self) -> u8 {
        self.pad_bottom
    }

    fn validate(self) -> Result<()> {
        if self.scaled.width() > self.source.width() || self.scaled.height() > self.source.height()
        {
            return Err(invalid(
                "video output geometry must not upscale source frames",
            ));
        }
        if u64::from(self.source.width()) * u64::from(self.scaled.height())
            != u64::from(self.source.height()) * u64::from(self.scaled.width())
        {
            return Err(invalid(
                "video output geometry must preserve the source aspect ratio",
            ));
        }
        if self.pad_right > 1
            || self.pad_bottom > 1
            || self.canvas.width() != self.scaled.width() + u32::from(self.pad_right)
            || self.canvas.height() != self.scaled.height() + u32::from(self.pad_bottom)
        {
            return Err(invalid(
                "video canvas padding must be an explicit zero or one pixel on each trailing edge",
            ));
        }
        if !self.canvas.width().is_multiple_of(2) || !self.canvas.height().is_multiple_of(2) {
            return Err(invalid(
                "video canvas dimensions must be even for the yuv420p profile",
            ));
        }
        if self.canvas.width() > MAX_VIDEO_WIDTH || self.canvas.height() > MAX_VIDEO_HEIGHT {
            return Err(limit_error(
                "video output geometry exceeds the 1920 by 1080 server limit",
            ));
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for VideoOutputGeometry {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let wire = VideoOutputGeometryWire::deserialize(deserializer)?;
        let value =
            Self::new(wire.source, wire.scaled, wire.canvas).map_err(serde::de::Error::custom)?;
        if value.pad_right != wire.pad_right || value.pad_bottom != wire.pad_bottom {
            return Err(serde::de::Error::custom(
                "video output padding must match scaled and canvas dimensions",
            ));
        }
        Ok(value)
    }
}

delegate_json_schema!(VideoOutputGeometry => VideoOutputGeometryWire);

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VideoSegmentSource {
    SourceFrame {
        frame_id: FrameId,
        session_time: SessionTime,
    },
    GapSlate {
        gap_ids: Vec<GapId>,
        source_range: SessionRange,
    },
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum VideoSegmentSourceWire {
    SourceFrame {
        frame_id: FrameId,
        session_time: SessionTime,
    },
    GapSlate {
        #[schemars(length(min = 1))]
        gap_ids: Vec<GapId>,
        source_range: SessionRange,
    },
}

impl VideoSegmentSource {
    pub fn source_frame(frame_id: FrameId, session_time: SessionTime) -> Result<Self> {
        let value = Self::SourceFrame {
            frame_id,
            session_time,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn gap_slate(gap_ids: Vec<GapId>, source_range: SessionRange) -> Result<Self> {
        let value = Self::GapSlate {
            gap_ids,
            source_range,
        };
        value.validate()?;
        Ok(value)
    }

    fn validate(&self) -> Result<()> {
        match self {
            Self::SourceFrame { frame_id, .. } => validate_frame_id(*frame_id),
            Self::GapSlate {
                gap_ids,
                source_range,
            } => {
                if gap_ids.is_empty()
                    || gap_ids.iter().any(|id| id.as_uuid().is_nil())
                    || has_duplicates(gap_ids)
                {
                    return Err(invalid(
                        "video gap slates require unique non-nil contributing gap ids",
                    ));
                }
                if source_range.start() >= source_range.end() {
                    return Err(invalid("video gap slate source ranges must be non-empty"));
                }
                Ok(())
            }
        }
    }
}

impl<'de> Deserialize<'de> for VideoSegmentSource {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        match VideoSegmentSourceWire::deserialize(deserializer)? {
            VideoSegmentSourceWire::SourceFrame {
                frame_id,
                session_time,
            } => Self::source_frame(frame_id, session_time),
            VideoSegmentSourceWire::GapSlate {
                gap_ids,
                source_range,
            } => Self::gap_slate(gap_ids, source_range),
        }
        .map_err(serde::de::Error::custom)
    }
}

delegate_json_schema!(VideoSegmentSource => VideoSegmentSourceWire);

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct VideoPresentationSegment {
    index: u32,
    source: VideoSegmentSource,
    presentation: PresentationRange,
    timing_basis: VideoTimingBasis,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct VideoPresentationSegmentWire {
    index: u32,
    source: VideoSegmentSource,
    presentation: PresentationRange,
    timing_basis: VideoTimingBasis,
}

impl VideoPresentationSegment {
    pub fn new(
        index: u32,
        source: VideoSegmentSource,
        presentation: PresentationRange,
        timing_basis: VideoTimingBasis,
    ) -> Result<Self> {
        source.validate()?;
        Ok(Self {
            index,
            source,
            presentation,
            timing_basis,
        })
    }

    pub const fn index(&self) -> u32 {
        self.index
    }

    pub const fn source(&self) -> &VideoSegmentSource {
        &self.source
    }

    pub const fn presentation(&self) -> PresentationRange {
        self.presentation
    }

    pub const fn timing_basis(&self) -> VideoTimingBasis {
        self.timing_basis
    }
}

impl<'de> Deserialize<'de> for VideoPresentationSegment {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        deserialize_validated(deserializer, |wire: VideoPresentationSegmentWire| {
            Self::new(
                wire.index,
                wire.source,
                wire.presentation,
                wire.timing_basis,
            )
        })
    }
}

delegate_json_schema!(VideoPresentationSegment => VideoPresentationSegmentWire);

#[derive(Clone, Debug, PartialEq)]
pub struct VideoPlanInput {
    range: ResolvedRange,
    epoch: VisualEpoch,
    frames: Vec<CapturedFrame>,
    meaningful_frame_ids: Vec<FrameId>,
    output: VideoOutputGeometry,
    policy: VideoPresentationPolicy,
}

impl VideoPlanInput {
    pub fn new(
        range: ResolvedRange,
        epoch: VisualEpoch,
        frames: Vec<CapturedFrame>,
        meaningful_frame_ids: Vec<FrameId>,
        output: VideoOutputGeometry,
        policy: VideoPresentationPolicy,
    ) -> Result<Self> {
        range.validate()?;
        validate_epoch(&range, &epoch, &frames)?;
        validate_meaningful_frames(&epoch.frame_ids, &meaningful_frame_ids)?;
        if output.source() != epoch.image {
            return Err(invalid(
                "video output source geometry must match the visual epoch image geometry",
            ));
        }
        let value = Self {
            range,
            epoch,
            frames,
            meaningful_frame_ids,
            output,
            policy,
        };
        value.validate_limits()?;
        Ok(value)
    }

    pub const fn range(&self) -> &ResolvedRange {
        &self.range
    }

    pub const fn epoch(&self) -> &VisualEpoch {
        &self.epoch
    }

    pub fn frames(&self) -> &[CapturedFrame] {
        &self.frames
    }

    pub fn meaningful_frame_ids(&self) -> &[FrameId] {
        &self.meaningful_frame_ids
    }

    pub const fn output(&self) -> VideoOutputGeometry {
        self.output
    }

    pub const fn policy(&self) -> VideoPresentationPolicy {
        self.policy
    }

    fn validate_limits(&self) -> Result<()> {
        if self.frames.len() > MAX_VIDEO_SOURCE_FRAMES {
            return Err(limit_error(
                "video plan exceeds the 480 source frame server limit",
            ));
        }
        if self.meaningful_frame_ids.len() > MAX_VIDEO_MEANINGFUL_FRAMES {
            return Err(limit_error(
                "video plan exceeds the 12 meaningful frame server limit",
            ));
        }
        let first = self
            .frames
            .first()
            .expect("epoch validation requires frames");
        let last = self
            .frames
            .last()
            .expect("epoch validation requires frames");
        let span = last
            .session_time()
            .as_nanos()
            .checked_sub(first.session_time().as_nanos())
            .ok_or_else(|| invalid("video source frame times must be ordered"))?;
        if span > max_source_nanos() {
            return Err(limit_error(
                "video plan exceeds the 30 second source range server limit",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct VideoPresentationPlan {
    version: NonEmptyText,
    policy: VideoPresentationPolicy,
    requested_range: SessionRange,
    resolved_range: SessionRange,
    presented_source_range: SessionRange,
    epoch: VisualEpoch,
    input_frame_ids: Vec<FrameId>,
    input_frame_times: Vec<SessionTime>,
    meaningful_frame_ids: Vec<FrameId>,
    segments: Vec<VideoPresentationSegment>,
    output: VideoOutputGeometry,
    duration: PresentationTime,
}

#[derive(Deserialize, schemars::JsonSchema)]
enum TemporalVideoPlanVersionWire {
    #[serde(rename = "temporal-video-plan-v1")]
    V1,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct VideoPresentationPlanWire {
    version: TemporalVideoPlanVersionWire,
    policy: VideoPresentationPolicy,
    requested_range: SessionRange,
    resolved_range: SessionRange,
    presented_source_range: SessionRange,
    epoch: VisualEpoch,
    #[schemars(length(min = 1, max = 120))]
    input_frame_ids: Vec<FrameId>,
    #[schemars(length(min = 1, max = 120))]
    input_frame_times: Vec<SessionTime>,
    #[schemars(length(max = 12))]
    meaningful_frame_ids: Vec<FrameId>,
    #[schemars(length(min = 1, max = 512))]
    segments: Vec<VideoPresentationSegment>,
    output: VideoOutputGeometry,
    duration: PresentationTime,
}

impl VideoPresentationPlan {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        policy: VideoPresentationPolicy,
        requested_range: SessionRange,
        resolved_range: SessionRange,
        presented_source_range: SessionRange,
        epoch: VisualEpoch,
        input_frame_ids: Vec<FrameId>,
        input_frame_times: Vec<SessionTime>,
        meaningful_frame_ids: Vec<FrameId>,
        segments: Vec<VideoPresentationSegment>,
        output: VideoOutputGeometry,
    ) -> Result<Self> {
        let duration = validate_plan_parts(
            requested_range,
            resolved_range,
            presented_source_range,
            &epoch,
            &input_frame_ids,
            &input_frame_times,
            &meaningful_frame_ids,
            &segments,
            output,
            policy,
        )?;
        Ok(Self {
            version: NonEmptyText::new(TEMPORAL_VIDEO_PLAN_VERSION)
                .expect("plan version is non-empty"),
            policy,
            requested_range,
            resolved_range,
            presented_source_range,
            epoch,
            input_frame_ids,
            input_frame_times,
            meaningful_frame_ids,
            segments,
            output,
            duration,
        })
    }

    pub fn version(&self) -> &str {
        self.version.as_str()
    }

    pub const fn policy(&self) -> VideoPresentationPolicy {
        self.policy
    }

    pub const fn requested_range(&self) -> SessionRange {
        self.requested_range
    }

    pub const fn resolved_range(&self) -> SessionRange {
        self.resolved_range
    }

    pub const fn presented_source_range(&self) -> SessionRange {
        self.presented_source_range
    }

    pub const fn epoch(&self) -> &VisualEpoch {
        &self.epoch
    }

    pub fn input_frame_ids(&self) -> &[FrameId] {
        &self.input_frame_ids
    }

    pub fn input_frame_times(&self) -> &[SessionTime] {
        &self.input_frame_times
    }

    pub fn meaningful_frame_ids(&self) -> &[FrameId] {
        &self.meaningful_frame_ids
    }

    pub fn segments(&self) -> &[VideoPresentationSegment] {
        &self.segments
    }

    pub const fn output(&self) -> VideoOutputGeometry {
        self.output
    }

    pub const fn duration(&self) -> PresentationTime {
        self.duration
    }
}

impl<'de> Deserialize<'de> for VideoPresentationPlan {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let wire = VideoPresentationPlanWire::deserialize(deserializer)?;
        let _version = wire.version;
        let value = Self::new(
            wire.policy,
            wire.requested_range,
            wire.resolved_range,
            wire.presented_source_range,
            wire.epoch,
            wire.input_frame_ids,
            wire.input_frame_times,
            wire.meaningful_frame_ids,
            wire.segments,
            wire.output,
        )
        .map_err(serde::de::Error::custom)?;
        if value.duration != wire.duration {
            return Err(serde::de::Error::custom(
                "video presentation duration must equal the exact segment endpoint",
            ));
        }
        Ok(value)
    }
}

delegate_json_schema!(VideoPresentationPlan => VideoPresentationPlanWire);

#[allow(clippy::too_many_arguments)]
fn validate_plan_parts(
    requested_range: SessionRange,
    resolved_range: SessionRange,
    presented_source_range: SessionRange,
    epoch: &VisualEpoch,
    input_frame_ids: &[FrameId],
    input_frame_times: &[SessionTime],
    meaningful_frame_ids: &[FrameId],
    segments: &[VideoPresentationSegment],
    output: VideoOutputGeometry,
    policy: VideoPresentationPolicy,
) -> Result<PresentationTime> {
    if resolved_range.start() < requested_range.start()
        || resolved_range.end() > requested_range.end()
        || presented_source_range.start() < resolved_range.start()
        || presented_source_range.end() > resolved_range.end()
    {
        return Err(invalid(
            "video requested, resolved, and presented ranges must narrow monotonically",
        ));
    }
    if input_frame_ids.is_empty()
        || input_frame_times.len() != input_frame_ids.len()
        || input_frame_ids.iter().any(|id| id.as_uuid().is_nil())
        || has_duplicates(input_frame_ids)
        || epoch.frame_ids != input_frame_ids
    {
        return Err(invalid(
            "video plan input frame ids must be unique, non-nil, and match the visual epoch",
        ));
    }
    if input_frame_times.windows(2).any(|pair| pair[0] > pair[1]) {
        return Err(invalid(
            "video plan input frame times must preserve nondecreasing capture order",
        ));
    }
    let first_time = input_frame_times[0];
    let last_time = *input_frame_times
        .last()
        .expect("validated video input times are non-empty");
    if presented_source_range.start() != first_time || presented_source_range.end() != last_time {
        return Err(invalid(
            "video presented source range must exactly span the first and last input frames",
        ));
    }
    let source_duration = last_time
        .as_nanos()
        .checked_sub(first_time.as_nanos())
        .ok_or_else(|| invalid("video input frame times must be ordered"))?;
    if source_duration > max_source_nanos() {
        return Err(limit_error(
            "video plan exceeds the 30 second source range server limit",
        ));
    }
    validate_meaningful_frames(input_frame_ids, meaningful_frame_ids)?;
    if output.source() != epoch.image {
        return Err(invalid(
            "video plan output source geometry must match the visual epoch",
        ));
    }
    if input_frame_ids.len() > MAX_VIDEO_SOURCE_FRAMES {
        return Err(limit_error(
            "video plan exceeds the 480 source frame server limit",
        ));
    }
    if meaningful_frame_ids.len() > MAX_VIDEO_MEANINGFUL_FRAMES {
        return Err(limit_error(
            "video plan exceeds the 12 meaningful frame server limit",
        ));
    }
    if segments.is_empty() {
        return Err(invalid("video presentation plan must contain segments"));
    }
    if segments.len() > MAX_VIDEO_PRESENTATION_SEGMENTS {
        return Err(limit_error(
            "video plan exceeds the 512 presentation segment server limit",
        ));
    }

    let mut expected_start = PresentationTime::ZERO;
    let mut last_frame_position = None;
    let mut visible_frames = HashSet::new();
    for (position, segment) in segments.iter().enumerate() {
        if segment.index != position as u32 || segment.presentation.start() != expected_start {
            return Err(invalid(
                "video presentation segments must have contiguous zero-based indices and time",
            ));
        }
        segment.source.validate()?;
        match &segment.source {
            VideoSegmentSource::SourceFrame {
                frame_id,
                session_time,
            } => {
                let frame_position = input_frame_ids
                    .iter()
                    .position(|candidate| candidate == frame_id)
                    .ok_or_else(|| invalid("video segment references a frame outside its epoch"))?;
                if input_frame_times[frame_position] != *session_time {
                    return Err(invalid(
                        "video source-frame segment time must exactly match its input frame identity",
                    ));
                }
                if last_frame_position.is_some_and(|last| frame_position < last) {
                    return Err(invalid(
                        "video source-frame segments must preserve capture order",
                    ));
                }
                if !presented_source_range.contains(*session_time) {
                    return Err(invalid(
                        "video source-frame segment time lies outside presented evidence",
                    ));
                }
                last_frame_position = Some(frame_position);
                visible_frames.insert(*frame_id);
            }
            VideoSegmentSource::GapSlate { source_range, .. } => {
                if source_range.start() < presented_source_range.start()
                    || source_range.end() > presented_source_range.end()
                {
                    return Err(invalid(
                        "video gap slate lies outside the presented source range",
                    ));
                }
            }
        }
        expected_start = segment.presentation.end();
    }
    if meaningful_frame_ids
        .iter()
        .any(|frame_id| !visible_frames.contains(frame_id))
    {
        return Err(invalid(
            "every meaningful frame must have a visible presentation segment",
        ));
    }
    validate_canonical_timing(
        policy,
        input_frame_ids,
        input_frame_times,
        meaningful_frame_ids,
        segments,
    )?;
    Ok(expected_start)
}

fn validate_canonical_timing(
    policy: VideoPresentationPolicy,
    input_frame_ids: &[FrameId],
    input_frame_times: &[SessionTime],
    meaningful_frame_ids: &[FrameId],
    segments: &[VideoPresentationSegment],
) -> Result<()> {
    let mut gap_ranges = Vec::new();
    for segment in segments {
        let duration = segment.presentation.duration_nanos();
        match segment.source() {
            VideoSegmentSource::SourceFrame { .. } => {
                if matches!(
                    segment.timing_basis,
                    VideoTimingBasis::RecordedGap | VideoTimingBasis::ModelGapHold
                ) {
                    return Err(invalid("video source frames cannot use a gap timing basis"));
                }
                if policy == VideoPresentationPolicy::RealTime
                    && segment.timing_basis == VideoTimingBasis::ModelMeaningfulHold
                {
                    return Err(invalid(
                        "real-time video plans cannot use model-optimized frame holds",
                    ));
                }
            }
            VideoSegmentSource::GapSlate { source_range, .. } => {
                if !matches!(
                    segment.timing_basis,
                    VideoTimingBasis::RecordedGap | VideoTimingBasis::ModelGapHold
                ) {
                    return Err(invalid("video gap slates require a gap timing basis"));
                }
                let source_duration = range_duration(*source_range)?;
                let (expected_duration, expected_basis) = match policy {
                    VideoPresentationPolicy::RealTime => {
                        (source_duration, VideoTimingBasis::RecordedGap)
                    }
                    VideoPresentationPolicy::ModelOptimized
                        if source_duration < MODEL_GAP_HOLD_NANOS =>
                    {
                        (MODEL_GAP_HOLD_NANOS, VideoTimingBasis::ModelGapHold)
                    }
                    VideoPresentationPolicy::ModelOptimized => {
                        (source_duration, VideoTimingBasis::RecordedGap)
                    }
                };
                if duration != expected_duration || segment.timing_basis != expected_basis {
                    return Err(invalid(
                        "video gap timing must match the canonical v1 presentation policy",
                    ));
                }
                if gap_ranges
                    .last()
                    .is_some_and(|prior: &SessionRange| prior.end() >= source_range.start())
                {
                    return Err(invalid(
                        "video gap slate source ranges must remain ordered, disjoint, and coalesced",
                    ));
                }
                gap_ranges.push(*source_range);
            }
        }
    }

    for (position, frame_id) in input_frame_ids.iter().enumerate() {
        let source_segments: Vec<_> = segments
            .iter()
            .filter(|segment| {
                matches!(
                    segment.source(),
                    VideoSegmentSource::SourceFrame { frame_id: candidate, .. } if candidate == frame_id
                )
            })
            .collect();
        let (base_visible_duration, ordinary_basis) = if position + 1 == input_frame_ids.len() {
            (TERMINAL_HOLD_NANOS, VideoTimingBasis::TerminalHold)
        } else {
            let interval =
                SessionRange::new(input_frame_times[position], input_frame_times[position + 1])?;
            let source_duration = range_duration(interval)?;
            if source_duration == 0 {
                (
                    MINIMUM_VISIBLE_FRAME_NANOS,
                    VideoTimingBasis::MinimumVisibleFrame,
                )
            } else {
                let obscured_duration = gap_ranges.iter().try_fold(0_u64, |total, gap| {
                    let start = gap.start().max(interval.start());
                    let end = gap.end().min(interval.end());
                    let overlap = end.as_nanos().saturating_sub(start.as_nanos());
                    total.checked_add(overlap).ok_or_else(|| {
                        limit_error("video source gap duration accounting overflowed")
                    })
                })?;
                let visible = source_duration
                    .checked_sub(obscured_duration)
                    .ok_or_else(|| {
                        invalid("video gap ranges exceed their source-frame interval")
                    })?;
                (visible, VideoTimingBasis::RecordedDelta)
            }
        };

        let meaningful = meaningful_frame_ids.contains(frame_id);
        if source_segments.is_empty() {
            if base_visible_duration != 0 || meaningful {
                return Err(invalid(
                    "video source timing omits a frame with canonical visible duration",
                ));
            }
            continue;
        }
        if base_visible_duration == 0 {
            return Err(invalid(
                "video source timing presents a frame fully replaced by capture gaps",
            ));
        }

        let actual_duration = source_segments.iter().try_fold(0_u64, |total, segment| {
            total
                .checked_add(segment.presentation.duration_nanos())
                .ok_or_else(|| limit_error("video source presentation duration overflowed"))
        })?;
        let model_adjusted = policy == VideoPresentationPolicy::ModelOptimized
            && meaningful
            && base_visible_duration < MODEL_MEANINGFUL_HOLD_NANOS;
        let expected_duration = if model_adjusted {
            MODEL_MEANINGFUL_HOLD_NANOS
        } else {
            base_visible_duration
        };
        if actual_duration != expected_duration {
            return Err(invalid(
                "video source timing must match the canonical v1 presentation duration",
            ));
        }
        for (source_position, segment) in source_segments.iter().enumerate() {
            let expected_basis = if model_adjusted && source_position == 0 {
                VideoTimingBasis::ModelMeaningfulHold
            } else {
                ordinary_basis
            };
            if segment.timing_basis != expected_basis {
                return Err(invalid(
                    "video source timing basis must match the canonical v1 presentation policy",
                ));
            }
        }
    }

    let last_id = input_frame_ids
        .last()
        .expect("validated input frame ids are non-empty");
    if !matches!(
        segments.last().map(VideoPresentationSegment::source),
        Some(VideoSegmentSource::SourceFrame { frame_id, .. }) if frame_id == last_id
    ) {
        return Err(invalid(
            "video presentation must end with the terminal source frame",
        ));
    }
    Ok(())
}

fn range_duration(range: SessionRange) -> Result<u64> {
    range
        .end()
        .as_nanos()
        .checked_sub(range.start().as_nanos())
        .ok_or_else(|| invalid("video source ranges must be ordered"))
}

fn validate_epoch(
    range: &ResolvedRange,
    epoch: &VisualEpoch,
    frames: &[CapturedFrame],
) -> Result<()> {
    if epoch.frame_ids.is_empty()
        || epoch.frame_ids.iter().any(|id| id.as_uuid().is_nil())
        || has_duplicates(&epoch.frame_ids)
        || frames.is_empty()
        || frames.len() != epoch.frame_ids.len()
    {
        return Err(invalid(
            "video visual epochs require matching unique non-empty frame metadata",
        ));
    }
    if frames
        .iter()
        .zip(&epoch.frame_ids)
        .any(|(frame, id)| frame.id() != *id)
    {
        return Err(invalid(
            "video frame metadata must exactly match visual epoch frame order",
        ));
    }
    let Some(start) = range
        .frame_ids
        .iter()
        .position(|id| *id == epoch.frame_ids[0])
    else {
        return Err(invalid(
            "video epoch frames must belong to the resolved range",
        ));
    };
    if range.frame_ids.get(start..start + epoch.frame_ids.len()) != Some(&epoch.frame_ids) {
        return Err(invalid(
            "video epoch frames must form one ordered contiguous resolved-range slice",
        ));
    }

    let mut prior_ordinal = None;
    let mut prior_time = None;
    for frame in frames {
        frame.validate()?;
        if frame.session_id() != range.session_id
            || frame.target_id() != range.target_id
            || !range.resolved_range.contains(frame.session_time())
        {
            return Err(invalid(
                "video source frames must match and remain within the resolved scope",
            ));
        }
        if frame.image() != epoch.image
            || frame.viewport() != epoch.viewport
            || frame.device_scale_factor() != epoch.device_scale_factor
        {
            return Err(invalid(
                "video source frames must belong to one exact visual geometry epoch",
            ));
        }
        if prior_ordinal.is_some_and(|value| frame.capture_ordinal().get() <= value)
            || prior_time.is_some_and(|value| frame.session_time() < value)
        {
            return Err(invalid(
                "video source frames must preserve strict capture order and nondecreasing session time",
            ));
        }
        prior_ordinal = Some(frame.capture_ordinal().get());
        prior_time = Some(frame.session_time());
    }
    Ok(())
}

fn validate_meaningful_frames(input: &[FrameId], meaningful: &[FrameId]) -> Result<()> {
    if meaningful.iter().any(|id| id.as_uuid().is_nil()) || has_duplicates(meaningful) {
        return Err(invalid(
            "meaningful video frame ids must be unique and non-nil",
        ));
    }
    let mut prior_position = None;
    for id in meaningful {
        let position = input
            .iter()
            .position(|candidate| candidate == id)
            .ok_or_else(|| invalid("meaningful video frame must belong to the input epoch"))?;
        if prior_position.is_some_and(|prior| position <= prior) {
            return Err(invalid(
                "meaningful video frame ids must preserve source-frame order",
            ));
        }
        prior_position = Some(position);
    }
    Ok(())
}

fn validate_frame_id(id: FrameId) -> Result<()> {
    if id.as_uuid().is_nil() {
        Err(invalid("video source frame id must be non-nil"))
    } else {
        Ok(())
    }
}

fn has_duplicates<T: Eq + std::hash::Hash>(values: &[T]) -> bool {
    let mut seen = HashSet::with_capacity(values.len());
    values.iter().any(|value| !seen.insert(value))
}

const fn max_source_nanos() -> u64 {
    MAX_VIDEO_SOURCE_DURATION.as_nanos() as u64
}

const fn max_presentation_nanos() -> u64 {
    MAX_VIDEO_PRESENTATION_DURATION.as_nanos() as u64
}

fn limit_error(message: &'static str) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::ResourceLimitExceeded,
        NonEmptyText::new(message).expect("video limit messages are non-empty"),
    )
}
