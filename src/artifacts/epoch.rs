use std::{
    ops::Range,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use krometrail_core::{
    ArtifactMarker, ArtifactMarkerId, ArtifactSourceFingerprint, EncodedFrame, ErrorCode,
    ErrorContext, KrometrailError, NonEmptyText, ResolvedRange, Result, SessionRange, SessionTime,
    VisualEpoch,
};
use temporal_vision::select_indices;
use temporal_vision::{DeclaredGap, Marker, OwnedFrameSequence, TimeRange, Timestamp};

use super::{
    cache::SourceFingerprint,
    decode::{DecodeLimits, decode_frame},
};

pub(crate) const ADAPTER_VERSION: &str = "krometrail-artifact-adapter-v3";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct AdaptationLimits {
    pub max_source_frames: usize,
    pub max_encoded_source_bytes: usize,
    pub max_dimension: u32,
    pub max_pixels_per_frame: usize,
    pub max_decoded_bytes: usize,
    pub max_markers: usize,
}

impl AdaptationLimits {
    fn decode_limits(self) -> DecodeLimits {
        DecodeLimits::new(
            self.max_dimension,
            self.max_pixels_per_frame,
            self.max_decoded_bytes,
            self.max_decoded_bytes as u64,
        )
    }
}

#[derive(Default)]
struct CancellationState {
    cancelled: AtomicBool,
    notify: tokio::sync::Notify,
}

#[derive(Clone, Default)]
pub(crate) struct WorkCancellation(Arc<CancellationState>);

impl WorkCancellation {
    pub(crate) fn cancel(&self) {
        if !self.0.cancelled.swap(true, Ordering::AcqRel) {
            self.0.notify.notify_waiters();
        }
    }
    pub(crate) fn is_cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::Acquire)
    }
    pub(crate) fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(KrometrailError::new(
                ErrorCode::Cancelled,
                NonEmptyText::new("artifact generation was cancelled")
                    .expect("static cancellation error is non-empty"),
            ))
        } else {
            Ok(())
        }
    }
}

impl krometrail_core::CancellationSignal for WorkCancellation {
    fn is_cancelled(&self) -> bool {
        WorkCancellation::is_cancelled(self)
    }

    fn cancelled(&self) -> krometrail_core::PortFuture<'_, ()> {
        Box::pin(async move {
            loop {
                let notified = self.0.notify.notified();
                tokio::pin!(notified);
                notified.as_mut().enable();
                if self.is_cancelled() {
                    return;
                }
                notified.await;
            }
        })
    }
}

#[derive(Clone, Debug)]
pub(crate) struct EpochPlan {
    pub descriptor: VisualEpoch,
    pub source_fingerprints: Vec<ArtifactSourceFingerprint>,
    pub cache_sources: Vec<SourceFingerprint>,
    pub frames: Vec<EncodedFrame>,
    pub markers: Vec<Marker<ArtifactMarkerId>>,
    pub gaps: Vec<DeclaredGap<krometrail_core::GapId>>,
    pub decoded_bytes: usize,
    pub source_frame_ids: Vec<krometrail_core::FrameId>,
    pub source_indices: Vec<usize>,
    pub source_range: temporal_vision::TimeRange,
}

impl EpochPlan {
    /// Investigation scope for any failure raised while adapting or generating from
    /// this epoch. Only identities and session time are exposed — never encoded
    /// bytes, cache identities, or filesystem locations.
    pub(crate) fn error_context(&self) -> ErrorContext {
        let Some(metadata) = self.frames.first().map(EncodedFrame::metadata) else {
            return ErrorContext::default();
        };
        ErrorContext {
            session_id: Some(metadata.session_id()),
            target_id: Some(metadata.target_id()),
            interaction_id: None,
            range: SessionRange::new(
                SessionTime::from_nanos(self.source_range.start().as_nanos()),
                SessionTime::from_nanos(self.source_range.end().as_nanos()),
            )
            .ok(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct EpochInput {
    pub sequence:
        OwnedFrameSequence<krometrail_core::FrameId, ArtifactMarkerId, krometrail_core::GapId>,
    /// Carried from the plan so generation failures can be related back to the
    /// session, target, and time range under investigation.
    pub context: ErrorContext,
}

pub(crate) fn bounded_plan(
    plan: &EpochPlan,
    max_frames: usize,
    include_frame_id: Option<krometrail_core::FrameId>,
) -> Result<EpochPlan> {
    if plan.frames.len() <= max_frames {
        return Ok(plan.clone());
    }
    let mut indices = select_indices(plan.frames.len(), max_frames);
    if let Some(frame_id) = include_frame_id
        && let Some(index) = plan
            .frames
            .iter()
            .position(|frame| frame.metadata().id() == frame_id)
        && !indices.contains(&index)
    {
        let candidates = indices.iter().enumerate();
        let candidates = if max_frames >= 3 {
            candidates
                .filter(|(_, candidate)| **candidate != 0 && **candidate != plan.frames.len() - 1)
                .collect::<Vec<_>>()
        } else {
            candidates.collect::<Vec<_>>()
        };
        let replace = candidates
            .into_iter()
            .max_by_key(|(_, candidate)| **candidate)
            .map(|(position, _)| position)
            .ok_or_else(|| {
                source_error(format!(
                    "bounded artifact selection could not retain frame {frame_id}"
                ))
                .with_context(plan.error_context())
                .with_recovery(
                    NonEmptyText::new(
                        "retry with a larger tile limit or a locator within the selected frames",
                    )
                    .expect("bounded selection recovery is non-empty"),
                )
            })?;
        indices[replace] = index;
        indices.sort_unstable();
    }
    let frames: Vec<EncodedFrame> = indices
        .iter()
        .map(|index| plan.frames[*index].clone())
        .collect();
    let source_fingerprints = indices
        .iter()
        .map(|index| plan.source_fingerprints[*index].clone())
        .collect();
    let cache_sources = indices
        .iter()
        .map(|index| plan.cache_sources[*index].clone())
        .collect();
    let decoded_bytes = frames.iter().try_fold(0_usize, |total, frame| {
        total
            .checked_add(decoded_len(frame)?)
            .ok_or_else(|| limit_error("bounded decoded bytes overflow"))
    })?;
    let start = frames
        .first()
        .expect("bounded artifact selection is non-empty")
        .metadata()
        .session_time()
        .as_nanos();
    let end = frames
        .last()
        .expect("bounded artifact selection is non-empty")
        .metadata()
        .session_time()
        .as_nanos();
    Ok(EpochPlan {
        frames,
        source_fingerprints,
        cache_sources,
        decoded_bytes,
        markers: clamp_markers(&plan.markers, start, end)?,
        gaps: clamp_declared_gaps(&plan.gaps, start, end)?,
        source_indices: indices
            .iter()
            .map(|index| plan.source_indices[*index])
            .collect(),
        ..plan.clone()
    })
}

/// Validate exact retained identities and build geometry/annotation plans without decoding.
/// This lets cache hits avoid image work and lets single-flight leaders own decode exactly once.
pub(crate) fn validate_and_plan(
    range: &ResolvedRange,
    frames: Vec<EncodedFrame>,
    markers: &[ArtifactMarker],
    limits: AdaptationLimits,
    cancellation: &WorkCancellation,
) -> Result<Vec<EpochPlan>> {
    cancellation.check()?;
    let context = range_context(range);
    if frames.len() != range.frame_ids.len() || frames.is_empty() {
        return Err(
            source_error("frame source did not return the exact resolved frame set")
                .with_context(context),
        );
    }
    if markers.len() > limits.max_markers {
        return Err(
            limit_error("artifact marker count exceeds the configured limit").with_context(context),
        );
    }
    let encoded_bytes = frames.iter().try_fold(0_usize, |total, frame| {
        total.checked_add(frame.bytes().len()).ok_or_else(|| {
            limit_error("encoded source bytes overflow").with_context(context.clone())
        })
    })?;
    if encoded_bytes > limits.max_encoded_source_bytes {
        return Err(
            limit_error("encoded source bytes exceed the configured limit").with_context(context),
        );
    }

    for (position, (expected_id, frame)) in range.frame_ids.iter().zip(&frames).enumerate() {
        let metadata = frame.metadata();
        if metadata.id() != *expected_id
            || metadata.session_id() != range.session_id
            || metadata.target_id() != range.target_id
            || !range.resolved_range.contains(metadata.session_time())
            || position > 0
                && (frames[position - 1].metadata().capture_ordinal() >= metadata.capture_ordinal()
                    || frames[position - 1].metadata().session_time() > metadata.session_time())
        {
            return Err(source_error(format!(
                "frame source order, scope, or metadata contradicts the resolved range at frame {}",
                metadata.id()
            ))
            .with_context(context));
        }
        let _ = decoded_len(frame).map_err(|error| error.with_context(context.clone()))?;
    }

    let mut spans = Vec::new();
    let mut start = 0_usize;
    for index in 1..frames.len() {
        if !same_epoch(frames[index - 1].metadata(), frames[index].metadata()) {
            spans.push(start..index);
            start = index;
        }
    }
    spans.push(start..frames.len());
    let gap_owners = assign_gap_owners(range, &frames, &spans);
    spans
        .into_iter()
        .enumerate()
        .map(|(epoch_index, span)| {
            plan_epoch(
                range,
                &frames,
                markers,
                epoch_index,
                span,
                &gap_owners[epoch_index],
            )
        })
        .collect()
}

pub(crate) fn decode_plan(
    plan: EpochPlan,
    limits: AdaptationLimits,
    cancellation: &WorkCancellation,
) -> Result<EpochInput> {
    let context = plan.error_context();
    let mut decoded = Vec::with_capacity(plan.frames.len());
    for frame in &plan.frames {
        cancellation.check()?;
        decoded.push(
            decode_frame(frame, limits.decode_limits())
                .map_err(|error| error.with_context(context.clone()))?,
        );
    }
    let sequence =
        temporal_vision::FrameSequence::new(decoded, plan.markers, plan.gaps, None, None)
            .map_err(|error| vision_error(error).with_context(context.clone()))?
            .with_source_provenance(
                plan.source_frame_ids,
                plan.source_indices,
                plan.source_range,
            )
            .map_err(|error| vision_error(error).with_context(context.clone()))?;
    Ok(EpochInput { sequence, context })
}

#[cfg(test)]
pub(crate) fn validate_and_partition(
    range: &ResolvedRange,
    frames: Vec<EncodedFrame>,
    markers: &[ArtifactMarker],
    limits: AdaptationLimits,
    cancellation: &WorkCancellation,
) -> Result<Vec<EpochInput>> {
    validate_and_plan(range, frames, markers, limits, cancellation)?
        .into_iter()
        .map(|plan| decode_plan(plan, limits, cancellation))
        .collect()
}

fn plan_epoch(
    range: &ResolvedRange,
    frames: &[EncodedFrame],
    markers: &[ArtifactMarker],
    epoch_index: usize,
    span: Range<usize>,
    gap_indices: &[usize],
) -> Result<EpochPlan> {
    let epoch_frames = frames[span].to_vec();
    let first = epoch_frames
        .first()
        .expect("epoch span is non-empty")
        .metadata();
    let last = epoch_frames
        .last()
        .expect("epoch span is non-empty")
        .metadata();
    let start_time = first.session_time();
    let end_time = last.session_time();
    let cache_sources: Vec<_> = epoch_frames
        .iter()
        .map(SourceFingerprint::from_frame)
        .collect();
    let decoded_bytes = epoch_frames.iter().try_fold(0_usize, |total, frame| {
        total
            .checked_add(decoded_len(frame)?)
            .ok_or_else(|| limit_error("epoch decoded bytes overflow"))
    })?;
    let source_frame_ids = epoch_frames
        .iter()
        .map(|frame| frame.metadata().id())
        .collect();
    let source_frame_count = epoch_frames.len();
    Ok(EpochPlan {
        descriptor: VisualEpoch {
            index: u32::try_from(epoch_index)
                .map_err(|_| limit_error("visual epoch count exceeds the result format"))?,
            frame_ids: epoch_frames
                .iter()
                .map(|frame| frame.metadata().id())
                .collect(),
            image: first.image(),
            viewport: first.viewport(),
            device_scale_factor: first.device_scale_factor(),
        },
        source_fingerprints: cache_sources
            .iter()
            .map(SourceFingerprint::store_fingerprint)
            .collect(),
        cache_sources,
        frames: epoch_frames,
        markers: clipped_markers(markers, start_time.as_nanos(), end_time.as_nanos())?,
        gaps: clipped_gaps(
            range,
            gap_indices,
            start_time.as_nanos(),
            end_time.as_nanos(),
        )?,
        decoded_bytes,
        source_frame_ids,
        source_indices: (0..source_frame_count).collect(),
        source_range: temporal_vision::TimeRange::new(
            temporal_vision::Timestamp::from_nanos(start_time.as_nanos()),
            temporal_vision::Timestamp::from_nanos(end_time.as_nanos()),
        )
        .map_err(vision_error)?,
    })
}

pub(crate) fn decoded_len(frame: &EncodedFrame) -> Result<usize> {
    let metadata = frame.metadata();
    let pixels =
        usize::try_from(u64::from(metadata.image().width()) * u64::from(metadata.image().height()))
            .map_err(|_| limit_error("decoded frame pixel count exceeds this platform"))?;
    pixels
        .checked_mul(4)
        .ok_or_else(|| limit_error("decoded frame byte count overflows"))
}

fn same_epoch(
    left: &krometrail_core::CapturedFrame,
    right: &krometrail_core::CapturedFrame,
) -> bool {
    // Delegates to the single visual-epoch authority so artifact partitioning
    // and capture-quality epoch summaries cannot drift apart.
    left.same_visual_epoch(right)
}

fn clipped_markers(
    markers: &[ArtifactMarker],
    start: u64,
    end: u64,
) -> Result<Vec<Marker<ArtifactMarkerId>>> {
    let mut selected: Vec<_> = markers
        .iter()
        .enumerate()
        .filter(|(_, marker)| (start..=end).contains(&marker.session_time().as_nanos()))
        .collect();
    selected.sort_by_key(|(position, marker)| (marker.session_time().as_nanos(), *position));
    selected
        .into_iter()
        .map(|(_, marker)| {
            Marker::new(
                marker.id().clone(),
                Timestamp::from_nanos(marker.session_time().as_nanos()),
                marker.kind().as_str(),
                marker.label().as_str(),
            )
            .map_err(vision_error)
        })
        .collect()
}

fn clipped_gaps(
    range: &ResolvedRange,
    gap_indices: &[usize],
    start: u64,
    end: u64,
) -> Result<Vec<DeclaredGap<krometrail_core::GapId>>> {
    let mut selected: Vec<_> = gap_indices
        .iter()
        .map(|position| {
            let gap = &range.gaps[*position];
            let (clipped_start, clipped_end) = clamp_range(
                gap.range().start().as_nanos(),
                gap.range().end().as_nanos(),
                start,
                end,
            );
            (*position, gap, clipped_start, clipped_end)
        })
        .collect();
    selected.sort_by_key(|(position, _, clipped_start, _)| (*clipped_start, *position));
    selected
        .into_iter()
        .map(|(_, gap, clipped_start, clipped_end)| {
            DeclaredGap::new(
                gap.id(),
                TimeRange::new(
                    Timestamp::from_nanos(clipped_start),
                    Timestamp::from_nanos(clipped_end),
                )
                .map_err(vision_error)?,
                gap.reason().as_str(),
                gap.estimated_missing_frames(),
            )
            .map_err(vision_error)
        })
        .collect()
}

fn clamp_markers(
    markers: &[Marker<ArtifactMarkerId>],
    start: u64,
    end: u64,
) -> Result<Vec<Marker<ArtifactMarkerId>>> {
    markers
        .iter()
        .filter(|marker| (start..=end).contains(&marker.timestamp().as_nanos()))
        .map(|marker| {
            Marker::new(
                marker.id().clone(),
                marker.timestamp(),
                marker.kind(),
                marker.label(),
            )
            .map_err(vision_error)
        })
        .collect()
}

fn assign_gap_owners(
    range: &ResolvedRange,
    frames: &[EncodedFrame],
    spans: &[Range<usize>],
) -> Vec<Vec<usize>> {
    let bounds: Vec<_> = spans
        .iter()
        .map(|span| {
            (
                frames[span.start].metadata().session_time().as_nanos(),
                frames[span.end - 1].metadata().session_time().as_nanos(),
            )
        })
        .collect();
    let mut owners = vec![Vec::new(); spans.len()];
    for (gap_index, gap) in range.gaps.iter().enumerate() {
        let gap_start = gap.range().start().as_nanos();
        let gap_end = gap.range().end().as_nanos();
        let owner = bounds
            .iter()
            .enumerate()
            .find(|(_, (start, end))| gap_start <= *end && gap_end >= *start)
            .map(|(index, _)| index)
            .unwrap_or_else(|| {
                bounds
                    .iter()
                    .enumerate()
                    .min_by_key(|(index, (start, end))| {
                        (distance_to_span(gap_start, gap_end, *start, *end), *index)
                    })
                    .map(|(index, _)| index)
                    .expect("validated frame partition is non-empty")
            });
        owners[owner].push(gap_index);
    }
    owners
}

fn distance_to_span(gap_start: u64, gap_end: u64, start: u64, end: u64) -> u64 {
    if gap_end < start {
        start - gap_end
    } else if gap_start > end {
        gap_start.saturating_sub(end)
    } else {
        0
    }
}

fn clamp_declared_gaps(
    gaps: &[DeclaredGap<krometrail_core::GapId>],
    start: u64,
    end: u64,
) -> Result<Vec<DeclaredGap<krometrail_core::GapId>>> {
    let mut clamped: Vec<_> = gaps
        .iter()
        .enumerate()
        .map(|(position, gap)| {
            let (clipped_start, clipped_end) = clamp_range(
                gap.range().start().as_nanos(),
                gap.range().end().as_nanos(),
                start,
                end,
            );
            Ok((
                position,
                DeclaredGap::new(
                    *gap.id(),
                    TimeRange::new(
                        Timestamp::from_nanos(clipped_start),
                        Timestamp::from_nanos(clipped_end),
                    )
                    .map_err(vision_error)?,
                    gap.reason(),
                    gap.estimated_missing_frames(),
                )
                .map_err(vision_error)?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    clamped.sort_by_key(|(position, gap)| (gap.range().start().as_nanos(), *position));
    Ok(clamped.into_iter().map(|(_, gap)| gap).collect())
}

fn clamp_range(start_value: u64, end_value: u64, start: u64, end: u64) -> (u64, u64) {
    if end_value < start {
        (start, start)
    } else if start_value > end {
        (end, end)
    } else {
        (start_value.max(start), end_value.min(end))
    }
}

/// Investigation scope for a resolved range: identities and session time only.
fn range_context(range: &ResolvedRange) -> ErrorContext {
    ErrorContext {
        session_id: Some(range.session_id),
        target_id: Some(range.target_id),
        interaction_id: None,
        range: Some(range.resolved_range),
    }
}

fn vision_error(error: temporal_vision::VisionError) -> KrometrailError {
    source_error(format!(
        "frame sequence adaptation failed: {}",
        error.message
    ))
}
fn source_error(message: impl Into<String>) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::ArtifactGenerationFailed,
        NonEmptyText::new(message).expect("adaptation errors are non-empty"),
    )
}
fn limit_error(message: impl Into<String>) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::ResourceLimitExceeded,
        NonEmptyText::new(message).expect("adaptation limit errors are non-empty"),
    )
    .with_recovery(
        NonEmptyText::new("narrow the source range or reduce the artifact scope")
            .expect("adaptation limit recovery is non-empty"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::ImageEncoder as _;
    use krometrail_core::{
        ArtifactId, CaptureGap, CaptureGapReason, CaptureOrdinal, CapturedFrame, DeviceScaleFactor,
        EncodedFrame, FrameId, ImageFormat, RangeResolutionOptions, SessionId, SessionRange,
        SessionTime, TargetId, TemporalRangeAnchorKind,
    };
    use temporal_vision::{
        FilmstripTileLimit, IntegerScale, RegionDefinition, RegionFilmstripLabels,
        RegionFilmstripParameters, RegionFilmstripRenderLimits, Rgb8, SignedPixelRect,
    };
    use uuid::Uuid;

    const RECORDED_FIRST_FRAME: u64 = 116_013_596_177;
    const RECORDED_LAST_FRAME: u64 = 120_262_122_018;
    const RECORDED_RESOLVED_END: u64 = 120_300_000_000;
    const RECORDED_EFFECTIVE_ANCHOR: u64 = 118_150_000_000;
    const RECORDED_IMAGE_WIDTH: u32 = 1_673;
    const RECORDED_IMAGE_HEIGHT: u32 = 1_288;

    fn recorded_png() -> Vec<u8> {
        let rgba = vec![0_u8; RECORDED_IMAGE_WIDTH as usize * RECORDED_IMAGE_HEIGHT as usize * 4];
        let mut encoded = Vec::new();
        image::codecs::png::PngEncoder::new(&mut encoded)
            .write_image(
                &rgba,
                RECORDED_IMAGE_WIDTH,
                RECORDED_IMAGE_HEIGHT,
                image::ExtendedColorType::Rgba8,
            )
            .unwrap();
        encoded
    }

    fn recorded_frames() -> Vec<EncodedFrame> {
        let session = SessionId::from_uuid(Uuid::from_u128(1));
        let target = TargetId::from_uuid(Uuid::from_u128(2));
        let span = u128::from(RECORDED_LAST_FRAME - RECORDED_FIRST_FRAME);
        let encoded = recorded_png();
        (0..184)
            .map(|index| {
                let time = RECORDED_FIRST_FRAME
                    + u64::try_from(span * u128::try_from(index).unwrap() / u128::from(183_u64))
                        .unwrap();
                let metadata = CapturedFrame::new(
                    FrameId::from_uuid(Uuid::from_u128(100 + u128::try_from(index).unwrap())),
                    session,
                    target,
                    CaptureOrdinal::new(u64::try_from(index + 1).unwrap()).unwrap(),
                    None,
                    krometrail_core::ObservedTime::from_nanos(time + 1),
                    SessionTime::from_nanos(time),
                    ImageFormat::Png,
                    krometrail_core::PixelDimensions::new(
                        RECORDED_IMAGE_WIDTH,
                        RECORDED_IMAGE_HEIGHT,
                    )
                    .unwrap(),
                    krometrail_core::PixelDimensions::new(
                        RECORDED_IMAGE_WIDTH,
                        RECORDED_IMAGE_HEIGHT,
                    )
                    .unwrap(),
                    DeviceScaleFactor::new(1.0).unwrap(),
                    vec![],
                )
                .unwrap();
                EncodedFrame::new(metadata, encoded.clone()).unwrap()
            })
            .collect()
    }

    fn recorded_gaps(session: SessionId, target: TargetId) -> Vec<CaptureGap> {
        [
            (119_325_329_378, 119_325_329_378),
            (120_159_653_105, 120_159_653_105),
            (120_284_683_060, 120_350_681_771),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (start, end))| {
            CaptureGap::new(
                krometrail_core::GapId::from_uuid(Uuid::from_u128(
                    1_000 + u128::try_from(index).unwrap(),
                )),
                session,
                target,
                SessionRange::new(SessionTime::from_nanos(start), SessionTime::from_nanos(end))
                    .unwrap(),
                krometrail_core::ObservedTime::from_nanos(end),
                CaptureGapReason::IngestionQueueSaturated,
                (index == 2).then(|| std::num::NonZeroU64::new(7).unwrap()),
                None,
            )
            .unwrap()
        })
        .collect()
    }

    fn recorded_range(frames: &[EncodedFrame], gaps: Vec<CaptureGap>) -> ResolvedRange {
        let session = frames[0].metadata().session_id();
        let target = frames[0].metadata().target_id();
        let resolved = SessionRange::new(
            SessionTime::from_nanos(116_000_000_000),
            SessionTime::from_nanos(RECORDED_RESOLVED_END),
        )
        .unwrap();
        ResolvedRange::new(
            session,
            target,
            TemporalRangeAnchorKind::SessionTime,
            resolved,
            resolved,
            frames.iter().map(|frame| frame.metadata().id()).collect(),
            vec![],
            vec![],
            vec![],
            gaps,
            vec![],
            RangeResolutionOptions::DEFAULT,
        )
        .unwrap()
    }

    fn recorded_limits() -> AdaptationLimits {
        AdaptationLimits {
            max_source_frames: 184,
            max_encoded_source_bytes: 10_000_000,
            max_dimension: 8_192,
            max_pixels_per_frame: 16_777_216,
            max_decoded_bytes: 512 * 1024 * 1024,
            max_markers: 256,
        }
    }

    fn recorded_plan(frames: Vec<EncodedFrame>, gaps: Vec<CaptureGap>) -> EpochPlan {
        validate_and_plan(
            &recorded_range(&frames, gaps),
            frames,
            &[],
            recorded_limits(),
            &WorkCancellation::default(),
        )
        .unwrap()
        .pop()
        .unwrap()
    }

    #[test]
    fn recorded_region_locator_bounding_preserves_gap_span_for_tile_limits() {
        let frames = recorded_frames();
        let session = frames[0].metadata().session_id();
        let target = frames[0].metadata().target_id();
        let plan = recorded_plan(frames, recorded_gaps(session, target));
        // Unit 4's resolver input is intentionally retained here. The trailing recorded gap
        // is clamped to the epoch boundary, retaining its loss estimate instead of disappearing.
        assert_eq!(plan.gaps.len(), 3);
        assert_eq!(
            plan.gaps[2].range(),
            TimeRange::new(
                Timestamp::from_nanos(RECORDED_LAST_FRAME),
                Timestamp::from_nanos(RECORDED_LAST_FRAME)
            )
            .unwrap()
        );
        assert_eq!(
            plan.gaps[2].estimated_missing_frames(),
            std::num::NonZeroU64::new(7)
        );
        let limits = recorded_limits();
        let locator_index = plan
            .frames
            .iter()
            .enumerate()
            .min_by_key(|(_, frame)| {
                frame
                    .metadata()
                    .session_time()
                    .as_nanos()
                    .abs_diff(RECORDED_EFFECTIVE_ANCHOR)
            })
            .map(|(index, _)| index)
            .unwrap();
        assert_eq!(
            plan.frames[locator_index]
                .metadata()
                .session_time()
                .as_nanos(),
            118_149_467_091
        );
        let locator = plan.frames[locator_index].metadata().id();

        for tile_limit in [3, 6] {
            // This is the pre-fix selection: replacing the last selected endpoint with the
            // locator reproduces the observed "gap range lies outside" failure deterministically.
            let mut legacy_indices = select_indices(plan.frames.len(), tile_limit);
            let replacement = legacy_indices.len() - 1;
            legacy_indices[replacement] = locator_index;
            legacy_indices.sort_unstable();
            let legacy = EpochPlan {
                frames: legacy_indices
                    .iter()
                    .map(|index| plan.frames[*index].clone())
                    .collect(),
                source_fingerprints: legacy_indices
                    .iter()
                    .map(|index| plan.source_fingerprints[*index].clone())
                    .collect(),
                cache_sources: legacy_indices
                    .iter()
                    .map(|index| plan.cache_sources[*index].clone())
                    .collect(),
                source_indices: legacy_indices
                    .iter()
                    .map(|index| plan.source_indices[*index])
                    .collect(),
                decoded_bytes: 0,
                ..plan.clone()
            };
            let error = decode_plan(legacy, limits, &WorkCancellation::default()).unwrap_err();
            assert!(error.message.to_string().contains("gap range lies outside"));

            let bounded = bounded_plan(&plan, tile_limit, Some(locator)).unwrap();
            assert_eq!(
                bounded.frames.first().unwrap().metadata().id(),
                plan.frames.first().unwrap().metadata().id()
            );
            assert_eq!(
                bounded.frames.last().unwrap().metadata().id(),
                plan.frames.last().unwrap().metadata().id()
            );
            assert!(
                bounded
                    .frames
                    .iter()
                    .any(|frame| frame.metadata().id() == locator)
            );
            let input = decode_plan(bounded, limits, &WorkCancellation::default()).unwrap();
            assert_eq!(input.sequence.gaps().len(), 3);
            assert_eq!(
                input.sequence.gaps()[2].estimated_missing_frames(),
                std::num::NonZeroU64::new(7)
            );
        }
    }

    #[test]
    fn recorded_small_tile_limits_retain_locators_and_gaps_for_gapped_and_gap_free_ranges() {
        let frames = recorded_frames();
        let session = frames[0].metadata().session_id();
        let target = frames[0].metadata().target_id();
        let gapped = recorded_plan(frames.clone(), recorded_gaps(session, target));
        let gap_free = recorded_plan(frames, vec![]);
        let locator = gapped.frames[92].metadata().id();

        for (plan, expected_gaps) in [(&gapped, 3_usize), (&gap_free, 0_usize)] {
            for tile_limit in [1, 2] {
                let bounded = bounded_plan(plan, tile_limit, Some(locator)).unwrap();
                assert_eq!(bounded.frames.len(), tile_limit);
                assert!(
                    bounded
                        .frames
                        .iter()
                        .any(|frame| frame.metadata().id() == locator)
                );
                let input =
                    decode_plan(bounded, recorded_limits(), &WorkCancellation::default()).unwrap();
                assert_eq!(input.sequence.gaps().len(), expected_gaps);
                if expected_gaps != 0 {
                    assert_eq!(
                        input.sequence.gaps()[2].estimated_missing_frames(),
                        std::num::NonZeroU64::new(7)
                    );
                }
            }
        }
    }

    #[test]
    fn bounded_selection_drops_markers_outside_the_selected_span() {
        let marker = Marker::new(
            ArtifactMarkerId::Caller(NonEmptyText::new("outside").unwrap()),
            Timestamp::from_nanos(1),
            "event",
            "outside",
        )
        .unwrap();
        let selected = clamp_markers(&[marker], 10, 20).unwrap();
        assert!(selected.is_empty());
    }

    #[test]
    fn recorded_out_of_span_gap_is_retained_in_region_manifest_with_estimate() {
        let frames = recorded_frames();
        let session = frames[0].metadata().session_id();
        let target = frames[0].metadata().target_id();
        let plan = recorded_plan(frames, recorded_gaps(session, target));
        let locator = plan.frames[92].metadata().id();
        let bounded = bounded_plan(&plan, 6, Some(locator)).unwrap();
        let input = decode_plan(bounded, recorded_limits(), &WorkCancellation::default()).unwrap();
        let locator_frame_index = input
            .sequence
            .frames()
            .iter()
            .position(|frame| frame.id() == &locator)
            .unwrap();
        let region = RegionDefinition::FixedSourceImage {
            rect: SignedPixelRect::new(
                0,
                0,
                std::num::NonZeroU32::new(1).unwrap(),
                std::num::NonZeroU32::new(1).unwrap(),
            )
            .unwrap(),
        };
        let parameters = RegionFilmstripParameters::new(
            region,
            Timestamp::from_nanos(RECORDED_EFFECTIVE_ANCHOR),
            FilmstripTileLimit::new(6).unwrap(),
            Rgb8::new(0, 0, 0),
            Rgb8::new(255, 0, 255),
            IntegerScale::IDENTITY,
            RegionFilmstripLabels::new("Recorded region", "deterministic test").unwrap(),
            RegionFilmstripRenderLimits::new(
                std::num::NonZeroU32::new(1_920).unwrap(),
                std::num::NonZeroU32::new(4_096).unwrap(),
                std::num::NonZeroUsize::new(64 * 1024 * 1024).unwrap(),
                std::num::NonZeroUsize::new(64 * 1024 * 1024).unwrap(),
            ),
        )
        .with_locator_frame_index(locator_frame_index);
        let generated = temporal_vision::generate_region_filmstrip(
            ArtifactId::from_uuid(Uuid::from_u128(3)),
            &input.sequence,
            parameters,
        )
        .unwrap();
        assert_eq!(generated.manifest().gaps().len(), 3);
        assert_eq!(
            generated.manifest().gaps()[2].estimated_missing_frames(),
            std::num::NonZeroU64::new(7)
        );
        assert_eq!(
            generated.manifest().gaps()[2].range(),
            TimeRange::new(
                Timestamp::from_nanos(RECORDED_LAST_FRAME),
                Timestamp::from_nanos(RECORDED_LAST_FRAME)
            )
            .unwrap()
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancellation_after_notification_registration_is_not_lost() {
        let cancellation = WorkCancellation::default();
        let notified = cancellation.0.notify.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();

        cancellation.cancel();

        tokio::time::timeout(std::time::Duration::from_millis(100), notified)
            .await
            .expect("registered cancellation notification");
        assert!(cancellation.is_cancelled());
    }
}
