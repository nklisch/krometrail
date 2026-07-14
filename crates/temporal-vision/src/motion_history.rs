use std::num::NonZeroU8;

use serde::{Deserialize, Serialize};

use crate::{
    BinaryMask, ComparisonOutcome, ErrorCode, FrameSequence, MeasurementParameters,
    NormalizedSequence, PixelDimensions, Result, Rgb8, TimeRange, VisionError,
    measure::classify_pixel_change,
    measure_adjacent,
    render::{ArtifactLabels, RenderLimits},
};

/// One deterministic integer exponential-decay curve over pair recency rank.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MotionDecay {
    peak_intensity: u16,
    half_life_ranks: NonZeroU8,
}

impl MotionDecay {
    pub const DEFAULT_PEAK_INTENSITY: u16 = u16::MAX;
    pub const DEFAULT_HALF_LIFE_RANKS: u8 = 1;
    pub const DEFAULT: Self = Self {
        peak_intensity: Self::DEFAULT_PEAK_INTENSITY,
        half_life_ranks: NonZeroU8::MIN,
    };

    pub const fn new(peak_intensity: u16, half_life_ranks: NonZeroU8) -> Self {
        Self {
            peak_intensity,
            half_life_ranks,
        }
    }

    pub const fn peak_intensity(self) -> u16 {
        self.peak_intensity
    }

    pub const fn half_life_ranks(self) -> NonZeroU8 {
        self.half_life_ranks
    }

    /// Weight for a pair where rank zero is the newest pair in its continuity segment.
    pub const fn weight_at(self, rank_from_newest: u32) -> u16 {
        let shift = rank_from_newest / self.half_life_ranks.get() as u32;
        if shift >= u16::BITS {
            0
        } else {
            self.peak_intensity >> shift
        }
    }

    /// Number of recency ranks that can contribute a nonzero weight.
    pub const fn live_window(self) -> u32 {
        u16::BITS * self.half_life_ranks.get() as u32
    }
}

impl Default for MotionDecay {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Fixed source-derived choices for one motion-history artifact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MotionHistoryParameters {
    reference_frame_index: usize,
    measurement: MeasurementParameters,
    decay: MotionDecay,
    reference_strength: u8,
    accent_color: Rgb8,
    outline_color: Rgb8,
    labels: ArtifactLabels,
    limits: RenderLimits,
}

impl MotionHistoryParameters {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        reference_frame_index: usize,
        measurement: MeasurementParameters,
        decay: MotionDecay,
        reference_strength: u8,
        accent_color: Rgb8,
        outline_color: Rgb8,
        labels: ArtifactLabels,
        limits: RenderLimits,
    ) -> Self {
        Self {
            reference_frame_index,
            measurement,
            decay,
            reference_strength,
            accent_color,
            outline_color,
            labels,
            limits,
        }
    }

    pub const fn reference_frame_index(&self) -> usize {
        self.reference_frame_index
    }

    pub const fn measurement(&self) -> MeasurementParameters {
        self.measurement
    }

    pub const fn decay(&self) -> MotionDecay {
        self.decay
    }

    pub const fn reference_strength(&self) -> u8 {
        self.reference_strength
    }

    pub const fn accent_color(&self) -> Rgb8 {
        self.accent_color
    }

    pub const fn outline_color(&self) -> Rgb8 {
        self.outline_color
    }

    pub const fn labels(&self) -> &ArtifactLabels {
        &self.labels
    }

    pub const fn limits(&self) -> RenderLimits {
        self.limits
    }
}

/// Render-independent result of deterministic gap-aware motion accumulation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MotionHistoryPlan<FrameId> {
    accumulation: Box<[u16]>,
    ever_changed: BinaryMask,
    outline: BinaryMask,
    dimensions: PixelDimensions,
    reference_frame_index: usize,
    reference_frame_id: FrameId,
    continuity_segment_count: usize,
    live_window: u32,
    measured_pair_count: usize,
    gap_pair_count: usize,
    changed_pixel_count: u64,
    max_segment_rank: u32,
    range: TimeRange,
}

impl<F> MotionHistoryPlan<F> {
    pub fn accumulation(&self) -> &[u16] {
        &self.accumulation
    }

    pub const fn ever_changed(&self) -> &BinaryMask {
        &self.ever_changed
    }

    pub const fn outline(&self) -> &BinaryMask {
        &self.outline
    }

    pub const fn dimensions(&self) -> PixelDimensions {
        self.dimensions
    }

    pub const fn reference_frame_index(&self) -> usize {
        self.reference_frame_index
    }

    pub const fn reference_frame_id(&self) -> &F {
        &self.reference_frame_id
    }

    pub const fn continuity_segment_count(&self) -> usize {
        self.continuity_segment_count
    }

    pub const fn live_window(&self) -> u32 {
        self.live_window
    }

    pub const fn measured_pair_count(&self) -> usize {
        self.measured_pair_count
    }

    pub const fn gap_pair_count(&self) -> usize {
        self.gap_pair_count
    }

    pub const fn changed_pixel_count(&self) -> u64 {
        self.changed_pixel_count
    }

    pub const fn max_segment_rank(&self) -> u32 {
        self.max_segment_rank
    }

    pub const fn range(&self) -> TimeRange {
        self.range
    }
}

/// Build a bounded motion-history plan without rendering or encoding an image.
pub fn build_motion_history_plan<F, M, G, P>(
    source: &FrameSequence<F, M, G, P>,
    normalized: &NormalizedSequence<F>,
    parameters: &MotionHistoryParameters,
) -> Result<MotionHistoryPlan<F>>
where
    F: Clone + Eq,
    M: Eq,
    G: Eq,
    P: AsRef<[u8]>,
{
    validate_source_alignment(source, normalized, parameters.reference_frame_index)?;
    let dimensions = normalized.dimensions();
    let pixel_count = dimensions.pixel_count()?;
    ensure_plan_memory_fits(pixel_count, parameters.limits)?;

    let comparisons = measure_adjacent(normalized, parameters.measurement)?;
    let measured_pair_count = comparisons
        .iter()
        .filter(|comparison| matches!(comparison.outcome(), ComparisonOutcome::Measured(_)))
        .count();
    let gap_pair_count = comparisons.len() - measured_pair_count;
    let mut accumulation = vec![0_u16; pixel_count];
    let mut segment_accumulation = vec![0_u16; pixel_count];
    let mask_byte_len = pixel_count.checked_add(7).ok_or_else(motion_limit_error)? / 8;
    let mut ever_changed_bits = vec![0_u8; mask_byte_len];
    let mut continuity_segment_count = 0_usize;
    let mut max_segment_rank = 0_u32;
    let width = usize::try_from(dimensions.width()).map_err(|_| motion_limit_error())?;

    let mut segment_start = None;
    for (comparison_index, comparison) in comparisons.iter().enumerate() {
        match comparison.outcome() {
            ComparisonOutcome::Measured(_) => {
                segment_start.get_or_insert(comparison_index);
            }
            ComparisonOutcome::GapBoundary { .. } => {
                if let Some(start) = segment_start.take() {
                    accumulate_segment(
                        &comparisons[start..comparison_index],
                        normalized,
                        parameters,
                        width,
                        &mut segment_accumulation,
                        &mut accumulation,
                        &mut ever_changed_bits,
                    )?;
                    continuity_segment_count = continuity_segment_count
                        .checked_add(1)
                        .ok_or_else(motion_limit_error)?;
                    max_segment_rank =
                        max_segment_rank.max(segment_rank(comparison_index - start)?);
                }
            }
        }
    }
    if let Some(start) = segment_start {
        accumulate_segment(
            &comparisons[start..],
            normalized,
            parameters,
            width,
            &mut segment_accumulation,
            &mut accumulation,
            &mut ever_changed_bits,
        )?;
        continuity_segment_count = continuity_segment_count
            .checked_add(1)
            .ok_or_else(motion_limit_error)?;
        max_segment_rank = max_segment_rank.max(segment_rank(comparisons.len() - start)?);
    }

    let changed_pixel_count = ever_changed_bits
        .iter()
        .map(|byte| u64::from(byte.count_ones()))
        .sum();
    let outline_bits = build_outline(&ever_changed_bits, dimensions)?;
    let ever_changed = BinaryMask::new(dimensions, ever_changed_bits)?;
    let outline = BinaryMask::new(dimensions, outline_bits)?;

    Ok(MotionHistoryPlan {
        accumulation: accumulation.into_boxed_slice(),
        ever_changed,
        outline,
        dimensions,
        reference_frame_index: parameters.reference_frame_index,
        reference_frame_id: normalized.frames()[parameters.reference_frame_index]
            .id()
            .clone(),
        continuity_segment_count,
        live_window: parameters.decay.live_window(),
        measured_pair_count,
        gap_pair_count,
        changed_pixel_count,
        max_segment_rank,
        range: source.range(),
    })
}

fn validate_source_alignment<F: Eq, M: Eq, G: Eq, P: AsRef<[u8]>>(
    source: &FrameSequence<F, M, G, P>,
    normalized: &NormalizedSequence<F>,
    reference_frame_index: usize,
) -> Result<()> {
    let aligned_frames = source.frames().len() == normalized.frames().len()
        && source.dimensions() == normalized.source_dimensions()
        && source.frames().iter().zip(normalized.frames()).all(
            |(source_frame, normalized_frame)| {
                source_frame.id() == normalized_frame.id()
                    && source_frame.timestamp() == normalized_frame.timestamp()
                    && normalized_frame.dimensions() == normalized.dimensions()
            },
        );
    let aligned_gaps = source.gaps().len() == normalized.gap_ranges().len()
        && source
            .gaps()
            .iter()
            .map(|gap| gap.range())
            .eq(normalized.gap_ranges().iter().copied());
    if !aligned_frames || !aligned_gaps || reference_frame_index >= normalized.frames().len() {
        return Err(VisionError::new(
            ErrorCode::InvalidParameter,
            "normalized frames, gaps, or reference index do not match the source sequence",
        ));
    }
    Ok(())
}

fn ensure_plan_memory_fits(pixel_count: usize, limits: RenderLimits) -> Result<()> {
    let mask_bytes = pixel_count
        .checked_add(7)
        .and_then(|value| value.checked_div(8))
        .ok_or_else(motion_limit_error)?;
    let bytes = pixel_count
        .checked_mul(std::mem::size_of::<u16>())
        .and_then(|value| value.checked_mul(2))
        .and_then(|value| value.checked_add(mask_bytes.checked_mul(2)?))
        .ok_or_else(motion_limit_error)?;
    if bytes > limits.max_canvas_bytes() {
        return Err(motion_limit_error());
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn accumulate_segment<F>(
    comparisons: &[crate::FrameComparison],
    normalized: &NormalizedSequence<F>,
    parameters: &MotionHistoryParameters,
    width: usize,
    segment_accumulation: &mut [u16],
    accumulation: &mut [u16],
    ever_changed_bits: &mut [u8],
) -> Result<()> {
    segment_accumulation.fill(0);
    for (offset, comparison) in comparisons.iter().enumerate() {
        debug_assert!(matches!(
            comparison.outcome(),
            ComparisonOutcome::Measured(_)
        ));
        let rank =
            u32::try_from(comparisons.len() - 1 - offset).map_err(|_| motion_limit_error())?;
        let weight = parameters.decay.weight_at(rank);
        let earlier = normalized.frames()[comparison.earlier_frame_index()].linear_rgb16();
        let later = normalized.frames()[comparison.later_frame_index()].linear_rgb16();
        for (pixel, (before, after)) in earlier
            .chunks_exact(3)
            .zip(later.chunks_exact(3))
            .enumerate()
        {
            let x = u32::try_from(pixel % width).map_err(|_| motion_limit_error())?;
            let y = u32::try_from(pixel / width).map_err(|_| motion_limit_error())?;
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
            if !classify_pixel_change(before, after, parameters.measurement)?.changed {
                continue;
            }
            set_bit(ever_changed_bits, pixel);
            if weight != 0 {
                segment_accumulation[pixel] = segment_accumulation[pixel].saturating_add(weight);
            }
        }
    }
    for (composite, segment) in accumulation.iter_mut().zip(segment_accumulation.iter()) {
        *composite = (*composite).max(*segment);
    }
    Ok(())
}

fn segment_rank(pair_count: usize) -> Result<u32> {
    u32::try_from(pair_count.saturating_sub(1)).map_err(|_| motion_limit_error())
}

fn build_outline(ever_changed: &[u8], dimensions: PixelDimensions) -> Result<Vec<u8>> {
    let pixel_count = dimensions.pixel_count()?;
    let mut outline = vec![0_u8; ever_changed.len()];
    let width = usize::try_from(dimensions.width()).map_err(|_| motion_limit_error())?;
    let height = usize::try_from(dimensions.height()).map_err(|_| motion_limit_error())?;
    for pixel in 0..pixel_count {
        if !bit_is_set(ever_changed, pixel) {
            continue;
        }
        let x = pixel % width;
        let y = pixel / width;
        let boundary = x == 0
            || y == 0
            || x + 1 == width
            || y + 1 == height
            || !bit_is_set(ever_changed, pixel - 1)
            || !bit_is_set(ever_changed, pixel + 1)
            || !bit_is_set(ever_changed, pixel - width)
            || !bit_is_set(ever_changed, pixel + width);
        if boundary {
            set_bit(&mut outline, pixel);
        }
    }
    Ok(outline)
}

fn bit_is_set(bits: &[u8], index: usize) -> bool {
    bits[index / 8] & (0x80 >> (index % 8)) != 0
}

fn set_bit(bits: &mut [u8], index: usize) {
    bits[index / 8] |= 0x80 >> (index % 8);
}

fn motion_limit_error() -> VisionError {
    VisionError::new(
        ErrorCode::ResourceLimitExceeded,
        "motion-history processing exceeds configured integer or memory limits",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::{NonZeroU32, NonZeroUsize};

    use crate::{
        DeclaredGap, Frame, IntegerScale, Marker, NormalizationParameters, PixelFormat,
        ProcessingLimits, Timestamp, normalize_sequence,
    };

    fn parameters(limits: RenderLimits) -> MotionHistoryParameters {
        MotionHistoryParameters::new(
            0,
            MeasurementParameters::new(0),
            MotionDecay::default(),
            64,
            Rgb8::new(255, 176, 0),
            Rgb8::new(255, 255, 255),
            ArtifactLabels::new("motion", "fixture").unwrap(),
            limits,
        )
    }

    #[test]
    fn decay_curve_is_exact_at_boundaries() {
        let decay = MotionDecay::new(60_000, NonZeroU8::new(2).unwrap());
        assert_eq!(decay.weight_at(0), 60_000);
        assert_eq!(decay.weight_at(1), 60_000);
        assert_eq!(decay.weight_at(2), 30_000);
        assert_eq!(decay.weight_at(31), 1);
        assert_eq!(decay.weight_at(32), 0);
        assert_eq!(decay.live_window(), 32);
    }

    #[test]
    fn outline_uses_four_connectivity_and_image_edges() {
        let dimensions = PixelDimensions::new(3, 3).unwrap();
        let full = [0xff, 0x80];
        assert_eq!(build_outline(&full, dimensions).unwrap(), [0xf7, 0x80]);
        let isolated = [0x10, 0x00];
        assert_eq!(build_outline(&isolated, dimensions).unwrap(), isolated);
    }

    #[test]
    fn accumulation_saturates_resets_at_gaps_and_respects_the_mask() {
        let dimensions = PixelDimensions::new(2, 1).unwrap();
        let frame = |id, time, value| {
            Frame::new(
                id,
                Timestamp::from_nanos(time),
                dimensions,
                PixelFormat::Rgba8SrgbStraight,
                vec![value, value, value, 255, value, value, value, 255].into_boxed_slice(),
            )
            .unwrap()
        };
        let gap = DeclaredGap::new(
            1_u8,
            TimeRange::new(Timestamp::from_nanos(15), Timestamp::from_nanos(15)).unwrap(),
            "loss",
            None,
        )
        .unwrap();
        let source = FrameSequence::new(
            vec![
                frame(0_u8, 0, 0),
                frame(1, 10, 255),
                frame(2, 20, 0),
                frame(3, 30, 255),
                frame(4, 40, 0),
            ],
            Vec::<Marker<u8>>::new(),
            vec![gap],
            None,
            Some(BinaryMask::new(dimensions, [0x80]).unwrap()),
        )
        .unwrap();
        let normalized = normalize_sequence(
            &source,
            NormalizationParameters::new(
                Rgb8::new(0, 0, 0),
                None,
                IntegerScale::IDENTITY,
                ProcessingLimits::default(),
            ),
        )
        .unwrap();
        let plan =
            build_motion_history_plan(&source, &normalized, &parameters(RenderLimits::default()))
                .unwrap();

        assert_eq!(plan.accumulation(), &[u16::MAX, 0]);
        assert_eq!(plan.continuity_segment_count(), 2);
        assert_eq!(plan.measured_pair_count(), 3);
        assert_eq!(plan.gap_pair_count(), 1);
        assert_eq!(plan.changed_pixel_count(), 1);
        assert_eq!(plan.ever_changed().bits(), &[0x80]);
        assert_eq!(plan.outline().bits(), &[0x80]);
    }

    #[test]
    fn plan_memory_is_bounded_before_accumulator_allocation() {
        let limits = RenderLimits::new(
            NonZeroU32::new(10).unwrap(),
            NonZeroU32::new(10).unwrap(),
            NonZeroUsize::new(4).unwrap(),
            NonZeroUsize::new(10).unwrap(),
        );
        assert_eq!(
            ensure_plan_memory_fits(1, limits).unwrap_err().code,
            ErrorCode::ResourceLimitExceeded
        );
    }
}
