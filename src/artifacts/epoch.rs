use std::{
    ops::Range,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use krometrail_core::{
    ArtifactMarker, ArtifactMarkerId, ArtifactSourceFingerprint, EncodedFrame, ErrorCode,
    KrometrailError, NonEmptyText, ResolvedRange, Result, VisualEpoch,
};
use temporal_vision::{DeclaredGap, Marker, OwnedFrameSequence, TimeRange, Timestamp};

use super::{
    cache::SourceFingerprint,
    decode::{DecodeLimits, decode_frame},
};

pub(crate) const ADAPTER_VERSION: &str = "krometrail-artifact-adapter-v2";

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

#[derive(Debug)]
pub(crate) struct EpochInput {
    pub sequence:
        OwnedFrameSequence<krometrail_core::FrameId, ArtifactMarkerId, krometrail_core::GapId>,
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
        let replace = indices
            .iter()
            .enumerate()
            .max_by_key(|(_, candidate)| **candidate)
            .map(|(position, _)| position)
            .ok_or_else(|| limit_error("bounded artifact selection is empty"))?;
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
    Ok(EpochPlan {
        frames,
        source_fingerprints,
        cache_sources,
        decoded_bytes,
        source_indices: indices
            .iter()
            .map(|index| plan.source_indices[*index])
            .collect(),
        ..plan.clone()
    })
}

fn select_indices(frame_count: usize, limit: usize) -> Vec<usize> {
    if frame_count <= limit {
        return (0..frame_count).collect();
    }
    if limit == 1 {
        return vec![0];
    }
    let span = frame_count - 1;
    let denominator = limit - 1;
    (0..limit)
        .map(|slot| {
            let numerator = (slot as u128) * (span as u128);
            let quotient = numerator / denominator as u128;
            let remainder = numerator % denominator as u128;
            (quotient + u128::from(remainder > denominator as u128 - remainder)) as usize
        })
        .collect()
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
    if frames.len() != range.frame_ids.len() || frames.is_empty() {
        return Err(source_error(
            "frame source did not return the exact resolved frame set",
        ));
    }
    if markers.len() > limits.max_markers {
        return Err(limit_error(
            "artifact marker count exceeds the configured limit",
        ));
    }
    let encoded_bytes = frames.iter().try_fold(0_usize, |total, frame| {
        total
            .checked_add(frame.bytes().len())
            .ok_or_else(|| limit_error("encoded source bytes overflow"))
    })?;
    if encoded_bytes > limits.max_encoded_source_bytes {
        return Err(limit_error(
            "encoded source bytes exceed the configured limit",
        ));
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
            return Err(source_error(
                "frame source order, scope, or metadata contradicts the resolved range",
            ));
        }
        let _ = decoded_len(frame)?;
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
    spans
        .into_iter()
        .enumerate()
        .map(|(epoch_index, span)| plan_epoch(range, &frames, markers, epoch_index, span))
        .collect()
}

pub(crate) fn decode_plan(
    plan: EpochPlan,
    limits: AdaptationLimits,
    cancellation: &WorkCancellation,
) -> Result<EpochInput> {
    let mut decoded = Vec::with_capacity(plan.frames.len());
    for frame in &plan.frames {
        cancellation.check()?;
        decoded.push(decode_frame(frame, limits.decode_limits())?);
    }
    let sequence =
        temporal_vision::FrameSequence::new(decoded, plan.markers, plan.gaps, None, None)
            .map_err(vision_error)?
            .with_source_provenance(
                plan.source_frame_ids,
                plan.source_indices,
                plan.source_range,
            )
            .map_err(vision_error)?;
    Ok(EpochInput { sequence })
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
        gaps: clipped_gaps(range, start_time.as_nanos(), end_time.as_nanos())?,
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

fn decoded_len(frame: &EncodedFrame) -> Result<usize> {
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
    left.image() == right.image()
        && left.viewport() == right.viewport()
        && left.device_scale_factor().get().to_bits() == right.device_scale_factor().get().to_bits()
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
    start: u64,
    end: u64,
) -> Result<Vec<DeclaredGap<krometrail_core::GapId>>> {
    let mut selected: Vec<_> = range
        .gaps
        .iter()
        .enumerate()
        .filter_map(|(position, gap)| {
            let clipped_start = gap.range().start().as_nanos().max(start);
            let clipped_end = gap.range().end().as_nanos().min(end);
            (clipped_start <= clipped_end).then_some((position, gap, clipped_start, clipped_end))
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
