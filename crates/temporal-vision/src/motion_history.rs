use std::{collections::BTreeMap, fmt::Display, num::NonZeroU8};

use serde::{Deserialize, Serialize};

use crate::{
    AlgorithmDescriptor, ArtifactKind, ArtifactManifest, BinaryMask, ComparisonOutcome,
    EncodedImage, ErrorCode, EvidenceClass, FrameSequence, GeneratedArtifact,
    MeasurementParameters, NormalizationKind, NormalizationStep, NormalizedSequence,
    ParameterValue, Parameters, PixelDimensions, Result, Rgb8, SharedAdjacentAnalysis, TimeRange,
    Timestamp, VisionError, generator_descriptor,
    measure::{PixelClassifier, linear_luminance, pair_pixels},
    measure_adjacent,
    provenance::analysis_sampling_parameters,
    render::{
        ArtifactLabels, RenderLimits,
        canvas::{BLACK, Canvas, MUTED, PANEL, WARNING, WHITE, canvas_limit_error},
        font::{CELL_WIDTH, draw_text, untruncated},
    },
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
    build_motion_history_plan_with_analysis(source, normalized, parameters, None)
}

pub fn build_motion_history_plan_with_analysis<F, M, G, P>(
    source: &FrameSequence<F, M, G, P>,
    normalized: &NormalizedSequence<F>,
    parameters: &MotionHistoryParameters,
    shared: Option<&SharedAdjacentAnalysis>,
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

    let comparisons_owned;
    let comparisons = if let Some(shared) = shared {
        shared.comparisons()
    } else {
        comparisons_owned = measure_adjacent(normalized, parameters.measurement)?;
        &comparisons_owned
    };
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
                        start,
                        normalized,
                        parameters,
                        shared,
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
            start,
            normalized,
            parameters,
            shared,
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
    if plan_working_bytes(pixel_count)? > limits.max_canvas_bytes() {
        return Err(motion_limit_error());
    }
    Ok(())
}

fn plan_working_bytes(pixel_count: usize) -> Result<usize> {
    let mask_bytes = pixel_count
        .checked_add(7)
        .and_then(|value| value.checked_div(8))
        .ok_or_else(motion_limit_error)?;
    pixel_count
        .checked_mul(std::mem::size_of::<u16>())
        .and_then(|value| value.checked_mul(2))
        .and_then(|value| value.checked_add(mask_bytes.checked_mul(2)?))
        .ok_or_else(motion_limit_error)
}

#[allow(clippy::too_many_arguments)]
fn accumulate_segment<F>(
    comparisons: &[crate::FrameComparison],
    comparison_start: usize,
    normalized: &NormalizedSequence<F>,
    parameters: &MotionHistoryParameters,
    shared: Option<&SharedAdjacentAnalysis>,
    segment_accumulation: &mut [u16],
    accumulation: &mut [u16],
    ever_changed_bits: &mut [u8],
) -> Result<()> {
    segment_accumulation.fill(0);
    let pairs = comparisons
        .iter()
        .map(|comparison| {
            debug_assert!(matches!(
                comparison.outcome(),
                ComparisonOutcome::Measured(_)
            ));
            pair_pixels(
                normalized,
                comparison.earlier_frame_index(),
                comparison.later_frame_index(),
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let dimensions = normalized.dimensions();
    let width = usize::try_from(dimensions.width()).map_err(|_| motion_limit_error())?;
    let pixel_count = dimensions.pixel_count()?;
    let mask = normalized.analysis_mask();
    let classifier =
        PixelClassifier::new(parameters.measurement).map_err(|_| motion_limit_error())?;
    let (local_accumulation, local_ever_changed) = crate::parallel::map_reduce(
        pairs.len(),
        || {
            (
                vec![0_u16; pixel_count],
                vec![0_u8; ever_changed_bits.len()],
            )
        },
        |(local_accumulation, local_ever_changed), offset| {
            let rank = match u32::try_from(pairs.len() - 1 - offset) {
                Ok(rank) => rank,
                Err(_) => return,
            };
            let weight = parameters.decay.weight_at(rank);
            let pair = &pairs[offset];
            for y in 0..dimensions.height() {
                let row = match usize::try_from(y)
                    .ok()
                    .and_then(|row| row.checked_mul(width))
                {
                    Some(row) => row,
                    None => return,
                };
                let byte_row = match row.checked_mul(3) {
                    Some(row) => row,
                    None => return,
                };
                let end = match byte_row.checked_add(width * 3) {
                    Some(end) => end,
                    None => return,
                };
                let mut mask_cursor = mask.map(|mask| {
                    let (bits, bit_offset) = mask
                        .row_bits(y)
                        .expect("mask dimensions match normalized frames");
                    (bits, 0_usize, 0x80_u8 >> bit_offset)
                });
                let change_start = usize::try_from(y)
                    .ok()
                    .and_then(|row| row.checked_mul(width))
                    .expect("normalized dimensions fit the change-mask index space");
                let mut change_cursor = shared
                    .and_then(|analysis| analysis.change_mask_for_pair(comparison_start + offset))
                    .map(|bits| (bits, change_start / 8, 0x80_u8 >> (change_start % 8)));
                for (x, (before, after)) in pair.earlier[byte_row..end]
                    .chunks_exact(3)
                    .zip(pair.later[byte_row..end].chunks_exact(3))
                    .enumerate()
                {
                    let changed_by_mask = if let Some((bits, byte, bit)) = change_cursor.as_mut() {
                        let changed = bits[*byte] & *bit != 0;
                        *bit >>= 1;
                        if *bit == 0 {
                            *bit = 0x80;
                            *byte += 1;
                        }
                        Some(changed)
                    } else {
                        None
                    };
                    let included = if let Some((bits, byte, bit)) = mask_cursor.as_mut() {
                        let included = bits[*byte] & *bit != 0;
                        *bit >>= 1;
                        if *bit == 0 {
                            *bit = 0x80;
                            *byte += 1;
                        }
                        included
                    } else {
                        true
                    };
                    if !included {
                        continue;
                    }
                    let before: &[u16; 3] = before
                        .try_into()
                        .expect("chunks_exact yields three-channel pixels");
                    let after: &[u16; 3] = after
                        .try_into()
                        .expect("chunks_exact yields three-channel pixels");
                    match changed_by_mask {
                        Some(false) => continue,
                        Some(true) => {}
                        None if !classifier.classify(before, after).changed => continue,
                        None => {}
                    }
                    let pixel = row + x;
                    set_bit(local_ever_changed, pixel);
                    if weight != 0 {
                        local_accumulation[pixel] =
                            local_accumulation[pixel].saturating_add(weight);
                    }
                }
            }
        },
        |(mut left_accumulation, mut left_changed), (right_accumulation, right_changed)| {
            for (left, right) in left_accumulation.iter_mut().zip(right_accumulation) {
                *left = left.saturating_add(right);
            }
            for (left, right) in left_changed.iter_mut().zip(right_changed) {
                *left |= right;
            }
            (left_accumulation, left_changed)
        },
    );
    segment_accumulation.copy_from_slice(&local_accumulation);
    for (target, local) in ever_changed_bits.iter_mut().zip(local_ever_changed) {
        *target |= local;
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

const HEADER_HEIGHT: u32 = 38;
const FOOTER_HEIGHT: u32 = 94;
const LEGEND_X: u32 = 4;
const LEGEND_Y_OFFSET: u32 = 30;
const LEGEND_HEIGHT: u32 = 8;
const PNG_PROFILE: &str = "png-0.17.16-rgb8-best-no_filter-no_chunks";

/// Encoded motion-history evidence and its machine-readable provenance.
pub type MotionHistoryArtifact<ArtifactId, FrameId, MarkerId, GapId> =
    GeneratedArtifact<ArtifactId, FrameId, MarkerId, GapId>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MotionHistoryLayout {
    dimensions: PixelDimensions,
    main_y: u32,
    footer_y: u32,
}

impl MotionHistoryLayout {
    fn new(source: PixelDimensions, limits: RenderLimits) -> Result<Self> {
        let height = HEADER_HEIGHT
            .checked_add(source.height())
            .and_then(|value| value.checked_add(FOOTER_HEIGHT))
            .ok_or_else(canvas_limit_error)?;
        if source.width() > limits.max_width() || height > limits.max_height() {
            return Err(canvas_limit_error());
        }
        let dimensions =
            PixelDimensions::new(source.width(), height).map_err(|_| canvas_limit_error())?;
        let canvas_bytes = dimensions
            .pixel_count()?
            .checked_mul(3)
            .ok_or_else(canvas_limit_error)?;
        let total_working_bytes = plan_working_bytes(source.pixel_count()?)?
            .checked_add(canvas_bytes)
            .ok_or_else(canvas_limit_error)?;
        if total_working_bytes > limits.max_canvas_bytes() {
            return Err(canvas_limit_error());
        }
        Ok(Self {
            dimensions,
            main_y: HEADER_HEIGHT,
            footer_y: HEADER_HEIGHT + source.height(),
        })
    }
}

/// Render and encode one deterministic, bounded, source-derived motion-history image.
pub fn generate_motion_history<A, F, M, G, P>(
    artifact_id: A,
    source: &FrameSequence<F, M, G, P>,
    normalized: &NormalizedSequence<F>,
    parameters: MotionHistoryParameters,
) -> Result<MotionHistoryArtifact<A, F, M, G>>
where
    F: Clone + Eq + Display,
    M: Clone + Eq,
    G: Clone + Eq,
    P: AsRef<[u8]>,
{
    generate_motion_history_with_analysis(artifact_id, source, normalized, parameters, None)
}

pub fn generate_motion_history_with_analysis<A, F, M, G, P>(
    artifact_id: A,
    source: &FrameSequence<F, M, G, P>,
    normalized: &NormalizedSequence<F>,
    parameters: MotionHistoryParameters,
    shared: Option<&SharedAdjacentAnalysis>,
) -> Result<MotionHistoryArtifact<A, F, M, G>>
where
    F: Clone + Eq + Display,
    M: Clone + Eq,
    G: Clone + Eq,
    P: AsRef<[u8]>,
{
    validate_source_alignment(source, normalized, parameters.reference_frame_index)?;
    let layout = MotionHistoryLayout::new(normalized.dimensions(), parameters.limits)?;
    let plan = build_motion_history_plan_with_analysis(source, normalized, &parameters, shared)?;
    let mut canvas = Canvas::new(
        layout.dimensions,
        BLACK,
        parameters.limits.max_canvas_bytes(),
    )?;
    draw_motion_history(&mut canvas, layout, source, normalized, &plan, &parameters)?;
    let (bytes, hash) = crate::encode::encode_png(
        layout.dimensions,
        canvas.pixels(),
        parameters.limits.max_encoded_bytes(),
    )?;

    let mut normalization = normalized.normalization_steps().to_vec();
    normalization.push(parameters.measurement.provenance_step()?);
    normalization.push(display_step()?);
    let manifest = ArtifactManifest::from_sequence(
        artifact_id,
        ArtifactKind::MotionHistory,
        EvidenceClass::SourceDerived,
        {
            let descriptor = generator_descriptor(ArtifactKind::MotionHistory);
            AlgorithmDescriptor::new(descriptor.name, descriptor.version)?
        },
        source,
        vec![plan.reference_frame_id().clone()],
        normalization,
        manifest_parameters(source, &plan, &parameters)?,
        layout.dimensions,
        hash,
    )?;
    Ok(GeneratedArtifact::new(
        EncodedImage::new(layout.dimensions, bytes),
        manifest,
    ))
}

fn draw_motion_history<F: Display + Eq, M: Eq, G: Eq, P: AsRef<[u8]>>(
    canvas: &mut Canvas,
    layout: MotionHistoryLayout,
    source: &FrameSequence<F, M, G, P>,
    normalized: &NormalizedSequence<F>,
    plan: &MotionHistoryPlan<F>,
    parameters: &MotionHistoryParameters,
) -> Result<()> {
    canvas.fill_rect(0, 0, layout.dimensions.width(), HEADER_HEIGHT, BLACK)?;
    draw_clipped_text(
        canvas,
        0,
        1,
        layout.dimensions.width(),
        parameters.labels.title(),
        WHITE,
    )?;
    draw_clipped_text(
        canvas,
        0,
        13,
        layout.dimensions.width(),
        parameters.labels.source(),
        MUTED,
    )?;
    draw_clipped_text(
        canvas,
        0,
        25,
        layout.dimensions.width(),
        &format!(
            "MOTION HISTORY | RANGE {} - {}",
            format_time(source.range().start()),
            format_time(source.range().end())
        ),
        MUTED,
    )?;

    let reference = &normalized.frames()[plan.reference_frame_index()];
    let width = usize::try_from(plan.dimensions().width()).map_err(|_| canvas_limit_error())?;
    for (pixel, rgb) in reference.linear_rgb16().chunks_exact(3).enumerate() {
        let luminance = linear_luminance(rgb)?;
        let gray =
            u8::try_from((luminance * u128::from(parameters.reference_strength) + 32_767) / 65_535)
                .map_err(|_| canvas_limit_error())?;
        let alpha = u32::from(plan.accumulation[pixel]);
        let accent = parameters.accent_color.channels();
        let color = accent.map(|channel| {
            let value = u32::from(gray) * (65_535 - alpha) + u32::from(channel) * alpha + 32_767;
            (value / 65_535) as u8
        });
        let x = u32::try_from(pixel % width).map_err(|_| canvas_limit_error())?;
        let y = u32::try_from(pixel / width).map_err(|_| canvas_limit_error())?;
        canvas.set_pixel(x, layout.main_y + y, color)?;
    }
    for pixel in 0..plan.dimensions().pixel_count()? {
        let x = u32::try_from(pixel % width).map_err(|_| canvas_limit_error())?;
        let y = u32::try_from(pixel / width).map_err(|_| canvas_limit_error())?;
        if plan.outline.includes(x, y) == Some(true) {
            canvas.set_pixel(x, layout.main_y + y, parameters.outline_color.channels())?;
        }
    }
    draw_footer(canvas, layout, source, plan, parameters)
}

fn draw_footer<F: Eq, M: Eq, G: Eq, P: AsRef<[u8]>>(
    canvas: &mut Canvas,
    layout: MotionHistoryLayout,
    source: &FrameSequence<F, M, G, P>,
    plan: &MotionHistoryPlan<F>,
    parameters: &MotionHistoryParameters,
) -> Result<()> {
    canvas.fill_rect(
        0,
        layout.footer_y,
        layout.dimensions.width(),
        FOOTER_HEIGHT,
        PANEL,
    )?;
    let lines = annotation_lines(source, plan);
    draw_clipped_text(
        canvas,
        4,
        layout.footer_y + 3,
        layout.dimensions.width().saturating_sub(8),
        &lines.range,
        WHITE,
    )?;
    draw_clipped_text(
        canvas,
        4,
        layout.footer_y + 15,
        layout.dimensions.width().saturating_sub(8),
        &lines.decay,
        MUTED,
    )?;
    draw_decay_ramp(canvas, layout, plan, parameters)?;
    draw_clipped_text(
        canvas,
        4,
        layout.footer_y + 42,
        layout.dimensions.width().saturating_sub(8),
        lines.disclaimer,
        MUTED,
    )?;
    draw_clipped_text(
        canvas,
        4,
        layout.footer_y + 54,
        layout.dimensions.width().saturating_sub(8),
        lines.direction,
        WHITE,
    )?;
    draw_clipped_text(
        canvas,
        4,
        layout.footer_y + 66,
        layout.dimensions.width().saturating_sub(8),
        lines.disambiguation,
        MUTED,
    )?;
    if let Some(gap) = lines.gap {
        canvas.fill_rect(
            0,
            layout.footer_y + 78,
            layout.dimensions.width(),
            16,
            WARNING,
        )?;
        draw_clipped_text(
            canvas,
            4,
            layout.footer_y + 80,
            layout.dimensions.width().saturating_sub(8),
            &gap,
            BLACK,
        )?;
    }
    Ok(())
}

fn draw_decay_ramp<F>(
    canvas: &mut Canvas,
    layout: MotionHistoryLayout,
    plan: &MotionHistoryPlan<F>,
    parameters: &MotionHistoryParameters,
) -> Result<()> {
    let width = layout.dimensions.width().saturating_sub(LEGEND_X * 2);
    if width == 0 {
        return Ok(());
    }
    let oldest_rank = plan
        .live_window()
        .saturating_sub(1)
        .min(plan.max_segment_rank());
    for x in 0..width {
        let rank = if width == 1 {
            oldest_rank
        } else {
            u32::try_from(u64::from(oldest_rank) * u64::from(width - 1 - x) / u64::from(width - 1))
                .map_err(|_| canvas_limit_error())?
        };
        let alpha = u32::from(parameters.decay.weight_at(rank));
        let color = parameters
            .accent_color
            .channels()
            .map(|channel| (u32::from(channel) * alpha / 65_535) as u8);
        for y in 0..LEGEND_HEIGHT {
            canvas.set_pixel(LEGEND_X + x, layout.footer_y + LEGEND_Y_OFFSET + y, color)?;
        }
    }
    Ok(())
}

#[derive(Debug, Eq, PartialEq)]
struct AnnotationLines {
    range: String,
    decay: String,
    disclaimer: &'static str,
    direction: &'static str,
    disambiguation: &'static str,
    gap: Option<String>,
}

fn annotation_lines<F, M, G, P>(
    source: &FrameSequence<F, M, G, P>,
    plan: &MotionHistoryPlan<F>,
) -> AnnotationLines
where
    F: Eq,
    M: Eq,
    G: Eq,
    P: AsRef<[u8]>,
{
    let span = source
        .range()
        .end()
        .as_nanos()
        .saturating_sub(source.range().start().as_nanos());
    let oldest_rank = plan
        .live_window()
        .saturating_sub(1)
        .min(plan.max_segment_rank());
    AnnotationLines {
        range: format!(
            "START {} | END {} | SPAN {} NS",
            format_time(source.range().start()),
            format_time(source.range().end()),
            span
        ),
        decay: format!(
            "DECAY: OLDEST RETAINED RANK {} -> NEWEST RANK 0",
            oldest_rank
        ),
        disclaimer: "MOTION HISTORY - SOURCE-DERIVED; NO DIRECTION INFERRED",
        direction: "TIME -> START TO END; INTENSITY IS PER-SEGMENT RECENCY",
        disambiguation: "OVERLAP MAY SMEAR DETAIL; INSPECT STORYBOARD OR REGION FILMSTRIP",
        gap: (!source.gaps().is_empty()).then(|| {
            format!(
                "GAP - {} DECLARED; UNSEEN BEHAVIOR MAY HAVE OCCURRED",
                source.gaps().len()
            )
        }),
    }
}

fn draw_clipped_text(
    canvas: &mut Canvas,
    x: u32,
    y: u32,
    width: u32,
    text: &str,
    color: [u8; 3],
) -> Result<()> {
    let cells = usize::try_from(width / CELL_WIDTH).unwrap_or(0);
    if cells == 0 {
        return Ok(());
    }
    if let Some(text) = untruncated(text, cells) {
        draw_text(canvas, x, y, &text, color)?;
    }
    Ok(())
}

fn format_time(timestamp: Timestamp) -> String {
    let milliseconds = timestamp.as_nanos() / 1_000_000;
    let micros = timestamp.as_nanos() % 1_000_000 / 1_000;
    format!("{milliseconds}.{micros:03} MS")
}

fn display_step() -> Result<NormalizationStep> {
    NormalizationStep::new(
        NormalizationKind::ColorSpaceConversion,
        "motion-history-display-rgb8-v1",
        parameter_map([
            (
                "reference",
                ParameterValue::Text("linear16_luminance_subdued_rgb8".into()),
            ),
            (
                "motion",
                ParameterValue::Text("u16_straight_alpha_accent_rgb8".into()),
            ),
            (
                "outline",
                ParameterValue::Text("four_connectivity_ever_changed".into()),
            ),
        ])?,
    )
}

fn manifest_parameters<F, M, G, P>(
    source: &FrameSequence<F, M, G, P>,
    plan: &MotionHistoryPlan<F>,
    request: &MotionHistoryParameters,
) -> Result<Parameters>
where
    F: Display + Eq,
    M: Eq,
    G: Eq,
    P: AsRef<[u8]>,
{
    let mut values = parameter_map([
        ("title", ParameterValue::Text(request.labels.title().into())),
        (
            "source",
            ParameterValue::Text(request.labels.source().into()),
        ),
        (
            "reference_frame_index",
            unsigned_usize(request.reference_frame_index)?,
        ),
        (
            "reference_frame_id",
            ParameterValue::Text(plan.reference_frame_id().to_string().into()),
        ),
        (
            "peak_intensity",
            ParameterValue::Unsigned(u64::from(request.decay.peak_intensity())),
        ),
        (
            "half_life_ranks",
            ParameterValue::Unsigned(u64::from(request.decay.half_life_ranks().get())),
        ),
        (
            "live_window",
            ParameterValue::Unsigned(u64::from(plan.live_window())),
        ),
        (
            "reference_strength",
            ParameterValue::Unsigned(u64::from(request.reference_strength)),
        ),
        ("accent_rgb8", rgb_parameter(request.accent_color)),
        ("outline_rgb8", rgb_parameter(request.outline_color)),
        (
            "continuity_segment_count",
            unsigned_usize(plan.continuity_segment_count())?,
        ),
        (
            "measured_pair_count",
            unsigned_usize(plan.measured_pair_count())?,
        ),
        ("gap_pair_count", unsigned_usize(plan.gap_pair_count())?),
        ("declared_gap_count", unsigned_usize(source.gaps().len())?),
        (
            "changed_pixel_count",
            ParameterValue::Unsigned(plan.changed_pixel_count()),
        ),
        (
            "max_segment_rank",
            ParameterValue::Unsigned(u64::from(plan.max_segment_rank())),
        ),
        (
            "accumulation",
            ParameterValue::Text("saturating_u16_per_segment_then_pixelwise_max".into()),
        ),
        (
            "cross_gap_policy",
            ParameterValue::Text("never_accumulate_across_declared_gap".into()),
        ),
        (
            "outline",
            ParameterValue::Text("ever_changed_four_connectivity_boundary".into()),
        ),
        ("direction_inference", ParameterValue::Text("none".into())),
        (
            "disambiguation",
            ParameterValue::Text("storyboard_or_region_filmstrip".into()),
        ),
        (
            "layout",
            ParameterValue::Text("fixed_header_combined_image_footer_v1".into()),
        ),
        (
            "header_height",
            ParameterValue::Unsigned(u64::from(HEADER_HEIGHT)),
        ),
        (
            "footer_height",
            ParameterValue::Unsigned(u64::from(FOOTER_HEIGHT)),
        ),
        ("encoding", ParameterValue::Text(PNG_PROFILE.into())),
        (
            "max_canvas_bytes",
            unsigned_usize(request.limits.max_canvas_bytes())?,
        ),
        (
            "max_encoded_bytes",
            unsigned_usize(request.limits.max_encoded_bytes())?,
        ),
    ])?;
    if let Some(sampling) = analysis_sampling_parameters(source)? {
        values.insert("analysis_sampling", sampling)?;
    }
    Ok(values)
}

fn rgb_parameter(color: Rgb8) -> ParameterValue {
    ParameterValue::List(
        color
            .channels()
            .into_iter()
            .map(|channel| ParameterValue::Unsigned(u64::from(channel)))
            .collect(),
    )
}

fn parameter_map<const N: usize>(
    entries: [(&'static str, ParameterValue); N],
) -> Result<Parameters> {
    Parameters::new(
        entries
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect::<BTreeMap<_, _>>(),
    )
}

fn unsigned_usize(value: usize) -> Result<ParameterValue> {
    Ok(ParameterValue::Unsigned(
        u64::try_from(value).map_err(|_| motion_limit_error())?,
    ))
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
    use std::io::Cursor;
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
    fn narrow_motion_history_omits_unreadable_label_bands() {
        let dimensions = PixelDimensions::new(239, 1).unwrap();
        let source = FrameSequence::new(
            vec![
                Frame::new(
                    0_u8,
                    Timestamp::ZERO,
                    dimensions,
                    PixelFormat::Rgba8SrgbStraight,
                    (0..dimensions.width())
                        .flat_map(|_| [0, 0, 0, 255])
                        .collect::<Vec<_>>()
                        .into_boxed_slice(),
                )
                .unwrap(),
            ],
            Vec::<Marker<u8>>::new(),
            Vec::<DeclaredGap<u8>>::new(),
            None,
            None,
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
        let labels = ArtifactLabels::new(
            "a deliberately long title that cannot fit",
            "a deliberately long source context that cannot fit",
        )
        .unwrap();
        let rendered = generate_motion_history(
            1_u8,
            &source,
            &normalized,
            MotionHistoryParameters::new(
                0,
                MeasurementParameters::new(0),
                MotionDecay::default(),
                64,
                Rgb8::new(255, 176, 0),
                Rgb8::new(255, 255, 255),
                labels,
                RenderLimits::default(),
            ),
        )
        .unwrap();
        let decoder = png::Decoder::new(Cursor::new(rendered.image().bytes()));
        let mut reader = decoder.read_info().unwrap();
        let mut pixels = vec![0_u8; reader.output_buffer_size()];
        let info = reader.next_frame(&mut pixels).unwrap();
        assert_eq!(info.color_type, png::ColorType::Rgb);
        assert_eq!(info.width, dimensions.width());
        assert_eq!(
            info.height,
            HEADER_HEIGHT + dimensions.height() + FOOTER_HEIGHT
        );
        let image = &pixels[..info.buffer_size()];
        for y in 0..HEADER_HEIGHT {
            for x in 0..dimensions.width() {
                let offset = (y * dimensions.width() + x) as usize * 3;
                assert_eq!(&image[offset..offset + 3], BLACK);
            }
        }
        let footer_y = HEADER_HEIGHT + dimensions.height();
        for y in footer_y..footer_y + FOOTER_HEIGHT {
            if (footer_y + LEGEND_Y_OFFSET..footer_y + LEGEND_Y_OFFSET + LEGEND_HEIGHT).contains(&y)
            {
                continue;
            }
            for x in 0..dimensions.width() {
                let offset = (y * dimensions.width() + x) as usize * 3;
                assert_eq!(&image[offset..offset + 3], PANEL);
            }
        }
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

        let lines = annotation_lines(&source, &plan);
        assert_eq!(lines.range, "START 0.000 MS | END 0.000 MS | SPAN 40 NS");
        assert_eq!(
            lines.decay,
            "DECAY: OLDEST RETAINED RANK 1 -> NEWEST RANK 0"
        );
        assert_eq!(
            lines.disclaimer,
            "MOTION HISTORY - SOURCE-DERIVED; NO DIRECTION INFERRED"
        );
        assert_eq!(
            lines.direction,
            "TIME -> START TO END; INTENSITY IS PER-SEGMENT RECENCY"
        );
        assert_eq!(
            lines.gap.as_deref(),
            Some("GAP - 1 DECLARED; UNSEEN BEHAVIOR MAY HAVE OCCURRED")
        );
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
