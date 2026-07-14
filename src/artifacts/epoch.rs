use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
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

pub(crate) const ADAPTER_VERSION: &str = "krometrail-artifact-adapter-v1";

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

#[derive(Clone, Default)]
pub(crate) struct WorkCancellation(Arc<AtomicBool>);

impl WorkCancellation {
    pub(crate) fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub(crate) fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
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

#[derive(Debug)]
pub(crate) struct EpochInput {
    pub descriptor: VisualEpoch,
    pub source_fingerprints: Vec<ArtifactSourceFingerprint>,
    pub cache_sources: Vec<SourceFingerprint>,
    pub sequence:
        OwnedFrameSequence<krometrail_core::FrameId, ArtifactMarkerId, krometrail_core::GapId>,
}

pub(crate) fn validate_and_partition(
    range: &ResolvedRange,
    frames: Vec<EncodedFrame>,
    markers: &[ArtifactMarker],
    limits: AdaptationLimits,
    cancellation: &WorkCancellation,
) -> Result<Vec<EpochInput>> {
    cancellation.check()?;
    if frames.len() != range.frame_ids.len() || frames.is_empty() {
        return Err(source_error(
            "frame source did not return the exact resolved frame set",
        ));
    }
    if frames.len() > limits.max_source_frames {
        return Err(limit_error("resolved range exceeds the source-frame limit"));
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

    let mut decoded_total = 0_usize;
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
        let pixels = usize::try_from(
            u64::from(metadata.image().width()) * u64::from(metadata.image().height()),
        )
        .map_err(|_| limit_error("decoded frame pixel count exceeds this platform"))?;
        let bytes = pixels
            .checked_mul(4)
            .ok_or_else(|| limit_error("decoded frame byte count overflows"))?;
        decoded_total = decoded_total
            .checked_add(bytes)
            .ok_or_else(|| limit_error("decoded sequence byte count overflows"))?;
    }
    if decoded_total > limits.max_decoded_bytes {
        return Err(limit_error(
            "decoded sequence bytes exceed the configured limit",
        ));
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

    let mut epochs = Vec::with_capacity(spans.len());
    for (epoch_index, span) in spans.into_iter().enumerate() {
        cancellation.check()?;
        let epoch_frames = &frames[span.clone()];
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
        let mut decoded = Vec::with_capacity(epoch_frames.len());
        let mut cache_sources = Vec::with_capacity(epoch_frames.len());
        for frame in epoch_frames {
            cancellation.check()?;
            decoded.push(decode_frame(frame, limits.decode_limits())?);
            cache_sources.push(SourceFingerprint::from_frame(frame));
        }
        let epoch_markers = clipped_markers(markers, start_time.as_nanos(), end_time.as_nanos())?;
        let epoch_gaps = clipped_gaps(range, start_time.as_nanos(), end_time.as_nanos())?;
        let descriptor = VisualEpoch {
            index: u32::try_from(epoch_index)
                .map_err(|_| limit_error("visual epoch count exceeds the result format"))?,
            frame_ids: epoch_frames
                .iter()
                .map(|frame| frame.metadata().id())
                .collect(),
            image: first.image(),
            viewport: first.viewport(),
            device_scale_factor: first.device_scale_factor(),
        };
        let source_fingerprints = cache_sources
            .iter()
            .map(SourceFingerprint::store_fingerprint)
            .collect();
        let sequence =
            temporal_vision::FrameSequence::new(decoded, epoch_markers, epoch_gaps, None, None)
                .map_err(vision_error)?;
        epochs.push(EpochInput {
            descriptor,
            source_fingerprints,
            cache_sources,
            sequence,
        });
    }
    Ok(epochs)
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
