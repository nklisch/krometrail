use std::marker::PhantomData;

use crate::{
    ComparisonOutcome, ErrorCode, FrameComparison, MeasurementParameters, NormalizedSequence,
    Result, VisionError,
    difference_map::{DifferenceAccumulators, DifferenceMapLimits},
    measure::{MeasurementAccumulator, classify_pixel_change, intersecting_gap_count},
    motion_history::{MotionAccumulatorCore, MotionDecay, MotionHistoryParameters},
};

const CHECKPOINT_PIXEL_STRIDE: usize = 4_096;
#[allow(dead_code)]
pub(crate) const TRACE_METADATA_BUDGET: usize = 64;

/// Request-local adjacent-pair results. The comparison trace is the only
/// replayable per-pair data; consumer cores are output-local working memory.
#[derive(Debug)]
pub(crate) struct PairAnalysisContext<'a> {
    normalized_identity: usize,
    _normalized: PhantomData<&'a ()>,
    measurement: MeasurementParameters,
    comparisons: Box<[FrameComparison]>,
    difference: Option<DifferenceAccumulators>,
    motion: Option<MotionAccumulatorCore>,
}

impl<'a> PairAnalysisContext<'a> {
    pub(crate) fn comparisons(&self) -> &[FrameComparison] {
        &self.comparisons
    }

    #[allow(dead_code)]
    pub(crate) fn trace_bytes(&self) -> usize {
        self.comparisons.len() * std::mem::size_of::<FrameComparison>()
    }

    #[allow(dead_code)]
    pub(crate) fn trace_allocation_budget(&self) -> usize {
        self.trace_bytes() + TRACE_METADATA_BUDGET
    }

    pub(crate) fn ensure_normalized<F>(
        &self,
        normalized: &NormalizedSequence<F>,
        measurement: MeasurementParameters,
    ) -> Result<()> {
        if self.normalized_identity != normalized as *const NormalizedSequence<F> as usize
            || self.measurement != measurement
        {
            return Err(VisionError::new(
                ErrorCode::InvalidParameter,
                "pair-analysis context does not match the normalized request",
            ));
        }
        Ok(())
    }

    pub(crate) fn into_difference(self) -> Option<DifferenceAccumulators> {
        self.difference
    }

    #[allow(dead_code)]
    pub(crate) fn take_difference(&mut self) -> Option<DifferenceAccumulators> {
        self.difference.take()
    }

    pub(crate) fn take_motion(&mut self, decay: MotionDecay) -> Option<MotionAccumulatorCore> {
        if self
            .motion
            .as_ref()
            .is_some_and(|core| core.decay() != decay)
        {
            return None;
        }
        self.motion.take()
    }
}

/// Build one deterministic row-major adjacent-pair traversal for the requested
/// output cores. A context is intentionally tied to the exact normalized
/// sequence object and never escapes the request that created it.
pub(crate) fn build_pair_analysis_context<'a, F>(
    normalized: &'a NormalizedSequence<F>,
    measurement: MeasurementParameters,
    difference_limits: Option<DifferenceMapLimits>,
    motion_parameters: Option<&MotionHistoryParameters>,
    mut checkpoint: impl FnMut() -> Result<()>,
) -> Result<PairAnalysisContext<'a>> {
    let difference = difference_limits
        .map(|limits| DifferenceAccumulators::empty(normalized, limits))
        .transpose()?;
    let motion = motion_parameters
        .map(|parameters| MotionAccumulatorCore::empty(normalized, parameters))
        .transpose()?;
    let pair_count = normalized.frames().len().saturating_sub(1);
    let mut comparisons = Vec::with_capacity(pair_count);
    let mut difference = difference;
    let mut motion = motion;
    let mut pair_index = 0;

    while pair_index < pair_count {
        let gap_count = pair_gap_count(normalized, pair_index);
        if gap_count > 0 {
            checkpoint()?;
            let comparison = metadata_comparison(normalized, pair_index, gap_count)?;
            if let Some(core) = motion.as_mut() {
                core.record_gap_pair()?;
            }
            comparisons.push(comparison);
            pair_index += 1;
            continue;
        }

        let mut segment_end = pair_index;
        while segment_end < pair_count && pair_gap_count(normalized, segment_end) == 0 {
            segment_end += 1;
        }
        let segment_pair_count = segment_end - pair_index;
        if let Some(core) = motion.as_mut() {
            core.begin_segment(segment_pair_count)?;
        }
        for current_pair in pair_index..segment_end {
            checkpoint()?;
            let rank =
                u32::try_from(segment_end - 1 - current_pair).map_err(|_| context_limit_error())?;
            let comparison = process_measured_pair(
                normalized,
                current_pair,
                measurement,
                motion.as_ref().map(|core| core.decay().weight_at(rank)),
                difference.as_mut(),
                motion.as_mut(),
                &mut checkpoint,
            )?;
            comparisons.push(comparison);
        }
        if let Some(core) = motion.as_mut() {
            core.finish_segment();
        }
        pair_index = segment_end;
    }

    Ok(PairAnalysisContext {
        normalized_identity: normalized as *const NormalizedSequence<F> as usize,
        _normalized: PhantomData,
        measurement,
        comparisons: comparisons.into_boxed_slice(),
        difference,
        motion,
    })
}

fn pair_gap_count<F>(normalized: &NormalizedSequence<F>, pair_index: usize) -> usize {
    let earlier = &normalized.frames()[pair_index];
    let later = &normalized.frames()[pair_index + 1];
    intersecting_gap_count(
        normalized.gap_ranges(),
        earlier.timestamp(),
        later.timestamp(),
    )
}

fn metadata_comparison<F>(
    normalized: &NormalizedSequence<F>,
    pair_index: usize,
    gap_count: usize,
) -> Result<FrameComparison> {
    let elapsed_nanos = elapsed_nanos(normalized, pair_index)?;
    let declared_gap_count =
        std::num::NonZeroUsize::new(gap_count).ok_or_else(context_limit_error)?;
    Ok(FrameComparison::from_parts(
        pair_index,
        pair_index + 1,
        elapsed_nanos,
        ComparisonOutcome::GapBoundary { declared_gap_count },
    ))
}

fn process_measured_pair<F>(
    normalized: &NormalizedSequence<F>,
    pair_index: usize,
    measurement: MeasurementParameters,
    motion_weight: Option<u16>,
    mut difference: Option<&mut DifferenceAccumulators>,
    mut motion: Option<&mut MotionAccumulatorCore>,
    checkpoint: &mut impl FnMut() -> Result<()>,
) -> Result<FrameComparison> {
    let earlier = &normalized.frames()[pair_index];
    let later = &normalized.frames()[pair_index + 1];
    let elapsed_nanos = elapsed_nanos(normalized, pair_index)?;
    let later_offset = difference
        .as_ref()
        .map(|_| {
            later
                .timestamp()
                .as_nanos()
                .checked_sub(normalized.frames()[0].timestamp().as_nanos())
                .ok_or_else(context_limit_error)
        })
        .transpose()?;
    let width =
        usize::try_from(normalized.dimensions().width()).map_err(|_| context_limit_error())?;
    let mut aggregate = MeasurementAccumulator::new(u128::from(normalized.analysis_pixel_count()));

    for (pixel, (before, after)) in earlier
        .linear_rgb16()
        .chunks_exact(3)
        .zip(later.linear_rgb16().chunks_exact(3))
        .enumerate()
    {
        if pixel % CHECKPOINT_PIXEL_STRIDE == 0 {
            checkpoint()?;
        }
        let x = u32::try_from(pixel % width).map_err(|_| context_limit_error())?;
        let y = u32::try_from(pixel / width).map_err(|_| context_limit_error())?;
        if normalized
            .analysis_mask()
            .is_some_and(|mask| mask.includes(x, y) != Some(true))
        {
            continue;
        }
        let before: &[u16; 3] = before
            .try_into()
            .expect("chunks_exact yields three-channel pixels");
        let after: &[u16; 3] = after
            .try_into()
            .expect("chunks_exact yields three-channel pixels");
        if let Some(core) = difference.as_mut() {
            core.record_comparable(pixel)?;
        }
        let change = classify_pixel_change(before, after, measurement)?;
        aggregate.record(x, y, before, after, change)?;
        if let (Some(core), Some(offset)) = (difference.as_mut(), later_offset) {
            if change.changed {
                core.record_change(pixel, offset, change.weighted_square)?;
            }
        }
        if let Some(core) = motion.as_mut() {
            core.record_pixel(pixel, change.changed, motion_weight.unwrap_or(0))?;
        }
    }

    if let Some(core) = motion.as_mut() {
        core.record_measured_pair()?;
    }
    Ok(FrameComparison::from_parts(
        pair_index,
        pair_index + 1,
        elapsed_nanos,
        ComparisonOutcome::Measured(aggregate.finish()?),
    ))
}

fn elapsed_nanos<F>(normalized: &NormalizedSequence<F>, pair_index: usize) -> Result<u64> {
    normalized.frames()[pair_index + 1]
        .timestamp()
        .as_nanos()
        .checked_sub(normalized.frames()[pair_index].timestamp().as_nanos())
        .ok_or_else(|| {
            VisionError::new(
                ErrorCode::OutOfOrder,
                "comparison timestamps are not in nondecreasing order",
            )
        })
}

fn context_limit_error() -> VisionError {
    VisionError::new(
        ErrorCode::ResourceLimitExceeded,
        "temporal pair-analysis context exceeds the supported integer representation",
    )
}

#[cfg(test)]
mod tests {
    use std::{mem::size_of, num::NonZeroU8};

    use sha2::{Digest, Sha256};

    use super::*;
    use crate::{
        ArtifactLabels, BinaryMask, DeclaredGap, DifferenceMapLimits, DifferenceMapParameters,
        ErrorCode, Frame, FrameComparison, FrameSequence, FrequencyMode, IntegerScale,
        MeasurementParameters, MotionDecay, MotionHistoryParameters, NormalizationParameters,
        NormalizedSequence, PixelDimensions, PixelFormat, ProcessingLimits, RenderLimits, Rgb8,
        StoryboardParameters, StoryboardTileLimit, TimePalette, TimeRange, Timestamp, VisionError,
        generate_motion_history, generate_storyboard, normalize_sequence, render_difference_map,
    };

    type Source = FrameSequence<u8, u8, u8, Box<[u8]>>;

    fn fixture(
        scale: IntegerScale,
        masked: bool,
        gapped: bool,
        equal_time: bool,
    ) -> (Source, NormalizedSequence<u8>) {
        let dimensions = PixelDimensions::new(4, 2).unwrap();
        let values = [
            [0_u8, 0, 0, 0, 0, 0, 0, 0],
            [255, 0, 0, 0, 0, 0, 0, 0],
            [0, 255, 0, 0, 0, 0, 0, 0],
            [0, 0, 255, 0, 0, 0, 0, 0],
            [0, 0, 0, 255, 0, 0, 0, 0],
        ];
        let times = if equal_time {
            [0, 0, 10, 10, 20]
        } else {
            [0, 10, 20, 30, 40]
        };
        let frames = values
            .into_iter()
            .enumerate()
            .map(|(index, values)| {
                let mut pixels = vec![0_u8; dimensions.rgba8_byte_len().unwrap()];
                for (pixel, value) in values.into_iter().enumerate() {
                    pixels[pixel * 4..pixel * 4 + 4].copy_from_slice(&[
                        value,
                        value.wrapping_add(17),
                        value.wrapping_add(31),
                        u8::MAX,
                    ]);
                }
                Frame::new(
                    index as u8,
                    Timestamp::from_nanos(times[index]),
                    dimensions,
                    PixelFormat::Rgba8SrgbStraight,
                    pixels.into_boxed_slice(),
                )
                .unwrap()
            })
            .collect();
        let mask = masked.then(|| BinaryMask::new(dimensions, [0xcc]).unwrap());
        let gaps = if gapped {
            vec![
                DeclaredGap::new(
                    1_u8,
                    TimeRange::new(Timestamp::from_nanos(5), Timestamp::from_nanos(5)).unwrap(),
                    "test gap",
                    None,
                )
                .unwrap(),
            ]
        } else {
            Vec::new()
        };
        let source = FrameSequence::new(frames, Vec::new(), gaps, None, mask).unwrap();
        let normalized = normalize_sequence(
            &source,
            NormalizationParameters::new(
                Rgb8::new(0, 0, 0),
                None,
                scale,
                ProcessingLimits::default(),
            ),
        )
        .unwrap_or_else(|error| panic!("fixture normalization failed scale={scale:?} masked={masked} gapped={gapped}: {error}"));
        (source, normalized)
    }

    fn difference_parameters() -> DifferenceMapParameters {
        DifferenceMapParameters::new(
            0,
            FrequencyMode::NormalizedFrequency,
            TimePalette::Spectral,
            None,
            MeasurementParameters::new(0),
            Rgb8::new(0, 0, 0),
            DifferenceMapLimits::default(),
        )
    }

    fn motion_parameters() -> MotionHistoryParameters {
        MotionHistoryParameters::new(
            0,
            MeasurementParameters::new(0),
            MotionDecay::new(u16::MAX, NonZeroU8::new(2).unwrap()),
            64,
            Rgb8::new(255, 176, 0),
            Rgb8::new(255, 255, 255),
            ArtifactLabels::new("motion", "fixture").unwrap(),
            RenderLimits::default(),
        )
    }

    fn storyboard_parameters(anchor: Timestamp) -> StoryboardParameters {
        StoryboardParameters::new(
            anchor,
            StoryboardTileLimit::new(4).unwrap(),
            MeasurementParameters::new(0),
            ArtifactLabels::new("storyboard", "fixture").unwrap(),
            RenderLimits::default(),
        )
    }

    fn output_digest<A, F, M, G>(artifact: &crate::GeneratedArtifact<A, F, M, G>) -> String
    where
        A: serde::Serialize,
        F: serde::Serialize,
        M: serde::Serialize,
        G: serde::Serialize,
    {
        let mut digest = Sha256::new();
        digest.update(serde_json::to_vec(artifact.manifest()).unwrap());
        digest.update(artifact.image().bytes());
        format!("{:x}", digest.finalize())
    }

    #[test]
    fn context_matches_direct_kernels_and_artifacts_across_required_evidence() {
        for scale in [
            IntegerScale::IDENTITY,
            IntegerScale::down(NonZeroU8::new(2).unwrap()).unwrap(),
        ] {
            for masked in [false, true] {
                for gapped in [false, true] {
                    let (source, normalized) = fixture(scale, masked, gapped, false);
                    let measurement = MeasurementParameters::new(0);
                    let motion = motion_parameters();
                    let context = build_pair_analysis_context(
                        &normalized,
                        measurement,
                        Some(DifferenceMapLimits::default()),
                        Some(&motion),
                        || Ok(()),
                    )
                    .unwrap();
                    let direct_comparisons =
                        crate::measure_adjacent(&normalized, measurement).unwrap();
                    assert_eq!(context.comparisons(), direct_comparisons.as_ref());
                    assert_eq!(size_of::<FrameComparison>(), 80);
                    assert!(
                        context.trace_allocation_budget() <= 80 * (source.frames().len() - 1) + 64
                    );
                    assert!(context.trace_allocation_budget() < 100 * 1024 * 1024);

                    let direct_selection = crate::select::select_storyboard_frames_direct(
                        &source,
                        &normalized,
                        source.frames()[2].timestamp(),
                        StoryboardTileLimit::new(4).unwrap(),
                        measurement,
                    )
                    .unwrap();
                    let context_selection =
                        crate::select::select_storyboard_frames_with_comparisons_for_test(
                            &source,
                            &normalized,
                            source.frames()[2].timestamp(),
                            StoryboardTileLimit::new(4).unwrap(),
                            measurement,
                            context.comparisons(),
                        )
                        .unwrap();
                    assert_eq!(context_selection, direct_selection);

                    let direct_difference =
                        crate::difference_map::DifferenceAccumulators::accumulate_direct(
                            &normalized,
                            measurement,
                            DifferenceMapLimits::default(),
                        )
                        .unwrap();
                    let context_difference = context
                        .into_difference()
                        .expect("difference core requested");
                    assert_eq!(context_difference, direct_difference);

                    let direct_motion = crate::motion_history::build_motion_history_plan_direct(
                        &source,
                        &normalized,
                        &motion,
                    )
                    .unwrap();
                    let mut motion_context = build_pair_analysis_context(
                        &normalized,
                        measurement,
                        None,
                        Some(&motion),
                        || Ok(()),
                    )
                    .unwrap();
                    let context_motion =
                        crate::motion_history::build_motion_history_plan_with_context(
                            &source,
                            &normalized,
                            &motion,
                            &mut motion_context,
                        )
                        .unwrap();
                    assert_eq!(context_motion, direct_motion);

                    let anchor = source.frames()[2].timestamp();
                    let storyboard_request = storyboard_parameters(anchor);
                    let direct_storyboard = crate::render::generate_storyboard_direct(
                        1_u8,
                        Some(2_u8),
                        &source,
                        &normalized,
                        storyboard_request.clone(),
                    )
                    .unwrap();
                    let context_storyboard = generate_storyboard(
                        1_u8,
                        Some(2_u8),
                        &source,
                        &normalized,
                        storyboard_request,
                    )
                    .unwrap();
                    assert_eq!(
                        output_digest(context_storyboard.storyboard()),
                        output_digest(direct_storyboard.storyboard())
                    );
                    assert_eq!(
                        output_digest(context_storyboard.orientation().unwrap()),
                        output_digest(direct_storyboard.orientation().unwrap())
                    );
                    assert_eq!(context_storyboard, direct_storyboard);
                    let direct_difference_artifact =
                        crate::difference_map::render_difference_map_direct(
                            3_u8,
                            &source,
                            &normalized,
                            difference_parameters(),
                        )
                        .unwrap();
                    let context_difference_artifact =
                        render_difference_map(3_u8, &source, &normalized, difference_parameters())
                            .unwrap();
                    assert_eq!(
                        output_digest(&context_difference_artifact),
                        output_digest(&direct_difference_artifact)
                    );
                    assert_eq!(context_difference_artifact, direct_difference_artifact);
                    let direct_motion_artifact =
                        crate::motion_history::generate_motion_history_direct(
                            4_u8,
                            &source,
                            &normalized,
                            motion.clone(),
                        )
                        .unwrap();
                    let context_motion_artifact =
                        generate_motion_history(4_u8, &source, &normalized, motion).unwrap();
                    assert_eq!(
                        output_digest(&context_motion_artifact),
                        output_digest(&direct_motion_artifact)
                    );
                    assert_eq!(context_motion_artifact, direct_motion_artifact);
                }
            }
        }
    }

    #[test]
    fn context_preserves_equal_timestamps_threshold_identity_and_cancellation() {
        let (source, normalized) = fixture(IntegerScale::IDENTITY, false, false, true);
        let measurement = MeasurementParameters::new(0);
        let mut context = build_pair_analysis_context(
            &normalized,
            measurement,
            None,
            Some(&motion_parameters()),
            || Ok(()),
        )
        .unwrap();
        assert_eq!(
            context.comparisons(),
            crate::measure_adjacent(&normalized, measurement)
                .unwrap()
                .as_ref()
        );
        assert!(
            context
                .comparisons()
                .iter()
                .any(|comparison| comparison.elapsed_nanos() == 0)
        );
        let mut checks = 0;
        let error = build_pair_analysis_context(&normalized, measurement, None, None, || {
            checks += 1;
            if checks == 2 {
                Err(VisionError::new(
                    ErrorCode::ResourceLimitExceeded,
                    "test cancellation",
                ))
            } else {
                Ok(())
            }
        })
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::ResourceLimitExceeded);
        assert!(checks >= 2);
        let threshold = MeasurementParameters::new(normalized.frames()[1].linear_rgb16()[0]);
        let direct = crate::measure_pair(&normalized, 0, 1, threshold).unwrap();
        let threshold_context =
            build_pair_analysis_context(&normalized, threshold, None, None, || Ok(())).unwrap();
        assert_eq!(threshold_context.comparisons()[0], direct);
        let _ = context.take_motion(motion_parameters().decay());
        let _ = source;
    }
}
