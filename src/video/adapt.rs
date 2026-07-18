use std::{num::NonZeroUsize, sync::Arc};

use image::{ImageBuffer, Rgba, imageops::FilterType};
use krometrail_core::{
    ArtifactSourceFingerprint, ErrorCode, FrameId, ImageFormat, KrometrailError, NonEmptyText,
    OutputLimitsRequest, PixelDimensions, Result, SessionTime, VIDEO_MEANINGFUL_SELECTOR_NAME,
    VIDEO_MEANINGFUL_SELECTOR_VERSION, VideoEncodeFrame, VideoEncodingProfile, VideoOutputGeometry,
    VideoPresentationPlan, VideoSegmentSource, VideoSelectionIdentity,
};
use sha2::{Digest, Sha256};
use temporal_vision::{
    Frame, FrameSequence, IntegerScale, MeasurementParameters, NormalizationParameters,
    PixelDimensions as VisionDimensions, PixelFormat, ProcessingLimits, Rgb8, StoryboardTileLimit,
    Timestamp, normalize_sequence, select_storyboard_frames,
};

use crate::artifacts::{
    cache::SourceFingerprint,
    decode::{DecodeLimits, decode_frame},
    epoch::{EpochPlan, WorkCancellation},
};

use super::slate::render_gap_slate;

const ANALYSIS_MAX_EDGE: u32 = 256;
const MEANINGFUL_TILE_LIMIT: u8 = 12;
const ANALYSIS_FILTER: &str = "image-filter-triangle";
const NORMALIZATION_PROFILE: &str = "rgba8-srgb-straight;black-background;identity-scale";
const TEMPORAL_VISION_SELECTOR_VERSION: &str = "storyboard-selector-v1";

#[derive(Clone, Debug)]
pub(crate) struct PreparedVideoEpoch {
    pub epoch: EpochPlan,
    pub plan: VideoPresentationPlan,
    pub selection: Option<VideoSelectionIdentity>,
    pub profile: VideoEncodingProfile,
    pub sources: Vec<ArtifactSourceFingerprint>,
    pub cache_sources: Vec<SourceFingerprint>,
}

pub(crate) fn output_geometry(
    source: PixelDimensions,
    output: OutputLimitsRequest,
) -> Result<VideoOutputGeometry> {
    let divisor = gcd(source.width(), source.height());
    let unit_width = source.width() / divisor;
    let unit_height = source.height() / divisor;
    for multiple in (1..=divisor).rev() {
        let width = unit_width * multiple;
        let height = unit_height * multiple;
        let canvas_width = width + (width % 2);
        let canvas_height = height + (height % 2);
        if canvas_width <= output.max_width() && canvas_height <= output.max_height() {
            return VideoOutputGeometry::new(
                source,
                PixelDimensions::new(width, height)?,
                PixelDimensions::new(canvas_width, canvas_height)?,
            );
        }
    }
    Err(limit_error(
        "video output limits cannot fit an aspect-preserving even canvas",
    ))
}

pub(crate) fn meaningful_selection(
    epoch: &EpochPlan,
    anchor: SessionTime,
    cancellation: &WorkCancellation,
) -> Result<(Vec<FrameId>, VideoSelectionIdentity)> {
    let source = epoch.descriptor.image;
    let scale = u32::max(
        source.width().div_ceil(ANALYSIS_MAX_EDGE),
        source.height().div_ceil(ANALYSIS_MAX_EDGE),
    )
    .max(1);
    let thumbnail = VisionDimensions::new(
        (source.width() / scale).max(1),
        (source.height() / scale).max(1),
    )
    .map_err(vision_error)?;
    let source_pixels = usize::try_from(u64::from(source.width()) * u64::from(source.height()))
        .map_err(|_| limit_error("video analysis source dimensions exceed this platform"))?;
    let source_bytes = source_pixels
        .checked_mul(4)
        .ok_or_else(|| limit_error("video analysis source bytes overflow"))?;
    let decode_limits = DecodeLimits::new(
        source.width().max(source.height()),
        source_pixels,
        source_bytes,
        source_bytes as u64,
    );
    let mut thumbnails = Vec::with_capacity(epoch.frames.len());
    for encoded in &epoch.frames {
        cancellation.check()?;
        let decoded = decode_frame(encoded, decode_limits)?;
        let (id, timestamp, dimensions, _, pixels) = decoded.into_parts();
        let image = ImageBuffer::<Rgba<u8>, _>::from_raw(
            dimensions.width(),
            dimensions.height(),
            pixels.into_vec(),
        )
        .ok_or_else(|| generation_error("decoded video analysis frame has invalid RGBA layout"))?;
        let resized = image::imageops::resize(
            &image,
            thumbnail.width(),
            thumbnail.height(),
            FilterType::Triangle,
        );
        thumbnails.push(
            Frame::new(
                id,
                timestamp,
                thumbnail,
                PixelFormat::Rgba8SrgbStraight,
                resized.into_raw().into_boxed_slice(),
            )
            .map_err(vision_error)?,
        );
    }
    cancellation.check()?;
    let sequence = FrameSequence::new(
        thumbnails,
        epoch.markers.clone(),
        epoch.gaps.clone(),
        None,
        None,
    )
    .map_err(vision_error)?;
    let frame_count = NonZeroUsize::new(sequence.frames().len())
        .expect("validated epoch always contains source frames");
    let pixels = thumbnail.pixel_count().map_err(vision_error)?;
    let retained = pixels
        .checked_mul(6)
        .and_then(|bytes| bytes.checked_mul(frame_count.get()))
        .ok_or_else(|| limit_error("video analysis memory accounting overflowed"))?;
    let normalized = normalize_sequence(
        &sequence,
        NormalizationParameters::new(
            Rgb8::new(0, 0, 0),
            None,
            IntegerScale::IDENTITY,
            ProcessingLimits::new(
                frame_count,
                NonZeroUsize::new(pixels).expect("thumbnail dimensions are nonzero"),
                NonZeroUsize::new(retained).expect("normalized sequence is nonempty"),
            ),
        ),
    )
    .map_err(vision_error)?;
    let clamped_anchor = anchor.max(epoch.frames[0].metadata().session_time()).min(
        epoch.frames[epoch.frames.len() - 1]
            .metadata()
            .session_time(),
    );
    let measurement = MeasurementParameters::default();
    let selection = select_storyboard_frames(
        &sequence,
        &normalized,
        Timestamp::from_nanos(clamped_anchor.as_nanos()),
        StoryboardTileLimit::new(MEANINGFUL_TILE_LIMIT).expect("twelve is a valid tile limit"),
        measurement,
    )
    .map_err(vision_error)?;
    let selected = selection
        .selected_frames()
        .iter()
        .map(|frame| *frame.frame_id())
        .collect();
    let identity = VideoSelectionIdentity::meaningful_v1(selection_parameter_hash(
        epoch,
        clamped_anchor,
        thumbnail,
        measurement,
    ));
    Ok((selected, identity))
}

pub(crate) fn encode_inputs(
    prepared: &PreparedVideoEpoch,
    cancellation: &WorkCancellation,
) -> Result<Vec<VideoEncodeFrame>> {
    let mut inputs = Vec::with_capacity(prepared.plan.segments().len());
    for segment in prepared.plan.segments() {
        cancellation.check()?;
        match segment.source() {
            VideoSegmentSource::SourceFrame { frame_id, .. } => {
                let frame = prepared
                    .epoch
                    .frames
                    .iter()
                    .find(|frame| frame.metadata().id() == *frame_id)
                    .ok_or_else(|| {
                        generation_error("video plan references an unavailable source frame")
                    })?;
                inputs.push(VideoEncodeFrame::new(
                    segment.index(),
                    segment.source().clone(),
                    frame.metadata().format(),
                    frame.metadata().image(),
                    Arc::<[u8]>::from(frame.bytes()),
                )?);
            }
            VideoSegmentSource::GapSlate { source_range, .. } => {
                inputs.push(VideoEncodeFrame::new(
                    segment.index(),
                    segment.source().clone(),
                    ImageFormat::Png,
                    prepared.plan.output().canvas(),
                    render_gap_slate(prepared.plan.output().canvas(), *source_range)?,
                )?);
            }
        }
    }
    Ok(inputs)
}

fn selection_parameter_hash(
    epoch: &EpochPlan,
    anchor: SessionTime,
    thumbnail: VisionDimensions,
    measurement: MeasurementParameters,
) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"krometrail-video-meaningful-selection-v1");
    hash.update(VIDEO_MEANINGFUL_SELECTOR_NAME.as_bytes());
    hash.update(VIDEO_MEANINGFUL_SELECTOR_VERSION.as_bytes());
    hash.update(TEMPORAL_VISION_SELECTOR_VERSION.as_bytes());
    hash.update(ANALYSIS_FILTER.as_bytes());
    hash.update(ANALYSIS_MAX_EDGE.to_be_bytes());
    hash.update(NORMALIZATION_PROFILE.as_bytes());
    hash.update(anchor.as_nanos().to_be_bytes());
    hash.update(thumbnail.width().to_be_bytes());
    hash.update(thumbnail.height().to_be_bytes());
    hash.update([MEANINGFUL_TILE_LIMIT]);
    hash.update(measurement.noise_floor().to_be_bytes());
    for frame in &epoch.frames {
        hash.update(frame.metadata().id().as_uuid().as_bytes());
        hash.update(frame.metadata().session_time().as_nanos().to_be_bytes());
    }
    for gap in &epoch.gaps {
        hash.update(gap.id().as_uuid().as_bytes());
        hash.update(gap.range().start().as_nanos().to_be_bytes());
        hash.update(gap.range().end().as_nanos().to_be_bytes());
    }
    hash.finalize().into()
}

const fn gcd(mut left: u32, mut right: u32) -> u32 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

fn vision_error(error: temporal_vision::VisionError) -> KrometrailError {
    generation_error(format!("video adaptation failed: {}", error.message))
}

fn generation_error(message: impl Into<String>) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::ArtifactGenerationFailed,
        NonEmptyText::new(message.into()).expect("video adaptation messages are non-empty"),
    )
}

fn limit_error(message: &'static str) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::ResourceLimitExceeded,
        NonEmptyText::new(message).expect("video limit messages are non-empty"),
    )
}
