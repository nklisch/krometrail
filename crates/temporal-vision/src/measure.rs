use std::num::NonZeroUsize;

use serde::{Deserialize, Deserializer, Serialize};

use crate::{
    BinaryMask, ErrorCode, NormalizationKind, NormalizationStep, NormalizedSequence,
    ParameterValue, PixelRect, Result, VisionError, normalize::make_parameters,
};

const RED_WEIGHT: u64 = 13_933;
const GREEN_WEIGHT: u64 = 46_871;
const BLUE_WEIGHT: u64 = 4_732;
const WEIGHT_SUM: u64 = 65_536;

/// Deterministic threshold settings for direct frame comparisons.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MeasurementParameters {
    noise_floor: u16,
}

impl MeasurementParameters {
    pub const DEFAULT_NOISE_FLOOR: u16 = 512;

    pub const fn new(noise_floor: u16) -> Self {
        Self { noise_floor }
    }

    pub const fn noise_floor(self) -> u16 {
        self.noise_floor
    }

    pub fn provenance_step(self) -> Result<NormalizationStep> {
        NormalizationStep::new(
            NormalizationKind::Thresholding,
            "weighted-linear-rgb-v1",
            make_parameters([
                (
                    "noise_floor",
                    ParameterValue::Unsigned(u64::from(self.noise_floor)),
                ),
                (
                    "comparison",
                    ParameterValue::Text("weighted_square > noise_floor^2 * weight_sum".into()),
                ),
                (
                    "weights",
                    ParameterValue::List(vec![
                        ParameterValue::Unsigned(RED_WEIGHT),
                        ParameterValue::Unsigned(GREEN_WEIGHT),
                        ParameterValue::Unsigned(BLUE_WEIGHT),
                    ]),
                ),
                ("weight_sum", ParameterValue::Unsigned(WEIGHT_SUM)),
                (
                    "below_floor_policy",
                    ParameterValue::Text("zero_all_aggregate_contributions".into()),
                ),
            ])?,
        )
    }
}

impl Default for MeasurementParameters {
    fn default() -> Self {
        Self::new(Self::DEFAULT_NOISE_FLOOR)
    }
}

/// Exact changed and compared pixel counts.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct ChangedPixelProportion {
    changed: u64,
    compared: u64,
}

impl ChangedPixelProportion {
    fn new(changed: u64, compared: u64) -> Result<Self> {
        if compared == 0 || changed > compared {
            return Err(VisionError::new(
                ErrorCode::InvalidParameter,
                "changed-pixel proportion requires a nonzero compared count and changed <= compared",
            ));
        }
        Ok(Self { changed, compared })
    }

    pub const fn changed(self) -> u64 {
        self.changed
    }

    pub const fn compared(self) -> u64 {
        self.compared
    }
}

impl<'de> Deserialize<'de> for ChangedPixelProportion {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            changed: u64,
            compared: u64,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.changed, wire.compared).map_err(serde::de::Error::custom)
    }
}

/// Thresholded direct visual-change aggregates for one frame pair.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MeasurementVector {
    absolute_pixel_difference: u64,
    changed_pixel_proportion: ChangedPixelProportion,
    changed_region_bounds: Option<PixelRect>,
    mean_luminance_difference: u16,
    mean_color_difference: u16,
    perceptual_frame_distance: u16,
}

impl MeasurementVector {
    pub const fn absolute_pixel_difference(&self) -> u64 {
        self.absolute_pixel_difference
    }

    pub const fn changed_pixel_proportion(&self) -> ChangedPixelProportion {
        self.changed_pixel_proportion
    }

    pub const fn changed_region_bounds(&self) -> Option<PixelRect> {
        self.changed_region_bounds
    }

    pub const fn mean_luminance_difference(&self) -> u16 {
        self.mean_luminance_difference
    }

    pub const fn mean_color_difference(&self) -> u16 {
        self.mean_color_difference
    }

    pub const fn perceptual_frame_distance(&self) -> u16 {
        self.perceptual_frame_distance
    }
}

/// Whether a comparison is measurable or crosses declared missing evidence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonOutcome {
    Measured(MeasurementVector),
    GapBoundary { declared_gap_count: NonZeroUsize },
}

/// Direct comparison metadata for two normalized frames.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FrameComparison {
    earlier_frame_index: usize,
    later_frame_index: usize,
    elapsed_nanos: u64,
    outcome: ComparisonOutcome,
}

impl FrameComparison {
    pub const fn earlier_frame_index(&self) -> usize {
        self.earlier_frame_index
    }

    pub const fn later_frame_index(&self) -> usize {
        self.later_frame_index
    }

    pub const fn elapsed_nanos(&self) -> u64 {
        self.elapsed_nanos
    }

    pub const fn outcome(&self) -> &ComparisonOutcome {
        &self.outcome
    }

    pub(crate) fn remap_indices(mut self, earlier: usize, later: usize) -> Self {
        self.earlier_frame_index = earlier;
        self.later_frame_index = later;
        self
    }
}

pub(crate) struct PairPixels<'a> {
    pub(crate) earlier_frame_index: usize,
    pub(crate) later_frame_index: usize,
    pub(crate) earlier_timestamp: crate::Timestamp,
    pub(crate) later_timestamp: crate::Timestamp,
    pub(crate) earlier: &'a [u16],
    pub(crate) later: &'a [u16],
    pub(crate) gap_count: usize,
}

pub(crate) fn pair_pixels<'a, F>(
    sequence: &'a NormalizedSequence<F>,
    earlier_frame_index: usize,
    later_frame_index: usize,
) -> Result<PairPixels<'a>> {
    if earlier_frame_index >= later_frame_index || later_frame_index >= sequence.frames().len() {
        return Err(VisionError::new(
            ErrorCode::InvalidParameter,
            "comparison indices must name two existing frames in increasing order",
        ));
    }
    let earlier = &sequence.frames()[earlier_frame_index];
    let later = &sequence.frames()[later_frame_index];
    let gap_count = intersecting_gap_count(
        sequence.gap_ranges(),
        earlier.timestamp(),
        later.timestamp(),
    );
    Ok(PairPixels {
        earlier_frame_index,
        later_frame_index,
        earlier_timestamp: earlier.timestamp(),
        later_timestamp: later.timestamp(),
        earlier: earlier.linear_rgb16(),
        later: later.linear_rgb16(),
        gap_count,
    })
}

/// Bit-packed changed-pixel masks aligned with adjacent-pair comparison indexes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PairChangeMasks {
    masks: Box<[Option<Box<[u8]>>]>,
}

type ComparisonAndMask = Result<(FrameComparison, Option<Box<[u8]>>)>;

impl PairChangeMasks {
    pub(crate) fn for_pair(&self, pair_index: usize) -> Option<&[u8]> {
        self.masks.get(pair_index).and_then(Option::as_deref)
    }

    pub fn bytes(&self) -> usize {
        self.masks
            .iter()
            .filter_map(Option::as_deref)
            .map(<[u8]>::len)
            .sum()
    }
}

/// Canonical adjacent-pair classification shared by artifact consumers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SharedAdjacentAnalysis<FrameId> {
    frame_ids: Box<[FrameId]>,
    frame_timestamps: Box<[crate::Timestamp]>,
    noise_floor: u16,
    comparisons: Box<[FrameComparison]>,
    change_masks: Option<PairChangeMasks>,
}

impl<FrameId> SharedAdjacentAnalysis<FrameId> {
    pub fn frame_ids(&self) -> &[FrameId] {
        &self.frame_ids
    }

    pub const fn noise_floor(&self) -> u16 {
        self.noise_floor
    }

    pub fn comparisons(&self) -> &[FrameComparison] {
        &self.comparisons
    }

    pub fn change_masks(&self) -> Option<&PairChangeMasks> {
        self.change_masks.as_ref()
    }

    pub(crate) fn change_mask_for_pair(&self, pair_index: usize) -> Option<&[u8]> {
        self.change_masks
            .as_ref()
            .and_then(|masks| masks.for_pair(pair_index))
    }

    pub fn change_mask_bytes(&self) -> usize {
        self.change_masks.as_ref().map_or(0, PairChangeMasks::bytes)
    }
}

impl<FrameId: Eq> SharedAdjacentAnalysis<FrameId> {
    /// A shared result is usable only for the exact normalized plan and threshold that built it.
    /// Callers deliberately fall back to their local measurement path when this check fails.
    pub(crate) fn is_compatible_with(
        &self,
        normalized: &NormalizedSequence<FrameId>,
        measurement: MeasurementParameters,
    ) -> bool {
        self.noise_floor == measurement.noise_floor()
            && self.frame_ids.len() == normalized.frames().len()
            && self
                .frame_ids
                .iter()
                .zip(self.frame_timestamps.iter())
                .zip(normalized.frames())
                .all(|((id, timestamp), frame)| id == frame.id() && *timestamp == frame.timestamp())
            && self.comparisons.len() == normalized.frames().len().saturating_sub(1)
            && self
                .comparisons
                .iter()
                .enumerate()
                .all(|(index, comparison)| {
                    comparison.earlier_frame_index() == index
                        && comparison.later_frame_index() == index + 1
                })
            && self
                .change_masks
                .as_ref()
                .is_none_or(|masks| masks.masks.len() == self.comparisons.len())
    }
}

/// Classify every adjacent pair once, optionally retaining changed-pixel masks for reducers.
pub fn analyze_adjacent_pairs<F: Clone>(
    normalized: &NormalizedSequence<F>,
    measurement: MeasurementParameters,
    want_change_masks: bool,
) -> Result<SharedAdjacentAnalysis<F>> {
    let pairs = (1..normalized.frames().len())
        .map(|later| pair_pixels(normalized, later - 1, later))
        .collect::<Result<Vec<_>>>()?;
    let dimensions = normalized.dimensions();
    let analysis_pixel_count = normalized.analysis_pixel_count();
    let mask = normalized.analysis_mask();
    let mask_bytes = if want_change_masks {
        Some(
            dimensions
                .pixel_count()?
                .checked_add(7)
                .ok_or_else(measurement_overflow)?
                / 8,
        )
    } else {
        None
    };
    let result = crate::parallel::map_reduce(
        pairs.len(),
        Vec::new,
        |comparisons, index| {
            let pair = &pairs[index];
            let elapsed_nanos = pair
                .later_timestamp
                .as_nanos()
                .checked_sub(pair.earlier_timestamp.as_nanos())
                .ok_or_else(|| {
                    VisionError::new(
                        ErrorCode::OutOfOrder,
                        "comparison timestamps are not in nondecreasing order",
                    )
                });
            let item = elapsed_nanos.and_then(|elapsed_nanos| {
                if let Some(declared_gap_count) = NonZeroUsize::new(pair.gap_count) {
                    Ok((
                        FrameComparison {
                            earlier_frame_index: pair.earlier_frame_index,
                            later_frame_index: pair.later_frame_index,
                            elapsed_nanos,
                            outcome: ComparisonOutcome::GapBoundary { declared_gap_count },
                        },
                        None,
                    ))
                } else {
                    let classifier = PixelClassifier::new(measurement)?;
                    let mut change_bits =
                        mask_bytes.map(|bytes| vec![0_u8; bytes].into_boxed_slice());
                    let vector = measure_pixels_with_classifier(
                        pair,
                        dimensions,
                        analysis_pixel_count,
                        mask,
                        &classifier,
                        change_bits.as_deref_mut(),
                    )?;
                    Ok((
                        FrameComparison {
                            earlier_frame_index: pair.earlier_frame_index,
                            later_frame_index: pair.later_frame_index,
                            elapsed_nanos,
                            outcome: ComparisonOutcome::Measured(vector),
                        },
                        change_bits,
                    ))
                }
            });
            comparisons.push(item);
        },
        |mut left: Vec<ComparisonAndMask>, right| {
            left.extend(right);
            left
        },
    );
    let mut comparisons = Vec::with_capacity(result.len());
    let mut masks = Vec::with_capacity(result.len());
    for item in result {
        let (comparison, mask) = item?;
        comparisons.push(comparison);
        masks.push(mask);
    }
    Ok(SharedAdjacentAnalysis {
        frame_ids: normalized
            .frames()
            .iter()
            .map(|frame| frame.id().clone())
            .collect(),
        frame_timestamps: normalized
            .frames()
            .iter()
            .map(|frame| frame.timestamp())
            .collect(),
        noise_floor: measurement.noise_floor(),
        comparisons: comparisons.into_boxed_slice(),
        change_masks: mask_bytes.map(|_| PairChangeMasks {
            masks: masks.into_boxed_slice(),
        }),
    })
}

fn comparison_from_pair(
    pair: &PairPixels<'_>,
    dimensions: crate::PixelDimensions,
    analysis_pixel_count: u64,
    mask: Option<&BinaryMask>,
    parameters: MeasurementParameters,
) -> Result<FrameComparison> {
    let elapsed_nanos = pair
        .later_timestamp
        .as_nanos()
        .checked_sub(pair.earlier_timestamp.as_nanos())
        .ok_or_else(|| {
            VisionError::new(
                ErrorCode::OutOfOrder,
                "comparison timestamps are not in nondecreasing order",
            )
        })?;
    let outcome = if let Some(declared_gap_count) = NonZeroUsize::new(pair.gap_count) {
        ComparisonOutcome::GapBoundary { declared_gap_count }
    } else {
        ComparisonOutcome::Measured(measure_pixels(
            pair,
            dimensions,
            analysis_pixel_count,
            mask,
            parameters,
        )?)
    };
    Ok(FrameComparison {
        earlier_frame_index: pair.earlier_frame_index,
        later_frame_index: pair.later_frame_index,
        elapsed_nanos,
        outcome,
    })
}

/// Measure one ordered pair of normalized frames.
pub fn measure_pair<F>(
    sequence: &NormalizedSequence<F>,
    earlier_frame_index: usize,
    later_frame_index: usize,
    parameters: MeasurementParameters,
) -> Result<FrameComparison> {
    let pair = pair_pixels(sequence, earlier_frame_index, later_frame_index)?;
    comparison_from_pair(
        &pair,
        sequence.dimensions(),
        sequence.analysis_pixel_count(),
        sequence.analysis_mask(),
        parameters,
    )
}

/// Measure every adjacent captured-frame pair in declaration order.
pub fn measure_adjacent<F>(
    sequence: &NormalizedSequence<F>,
    parameters: MeasurementParameters,
) -> Result<Box<[FrameComparison]>> {
    let pairs = (1..sequence.frames().len())
        .map(|later| pair_pixels(sequence, later - 1, later))
        .collect::<Result<Vec<_>>>()?;
    let mask = sequence.analysis_mask();
    let dimensions = sequence.dimensions();
    let analysis_pixel_count = sequence.analysis_pixel_count();
    let result = crate::parallel::map_reduce(
        pairs.len(),
        Vec::new,
        |comparisons, index| {
            comparisons.push(comparison_from_pair(
                &pairs[index],
                dimensions,
                analysis_pixel_count,
                mask,
                parameters,
            ));
        },
        |mut left: Vec<Result<FrameComparison>>, right| {
            left.extend(right);
            left
        },
    );
    result
        .into_iter()
        .collect::<Result<Vec<_>>>()
        .map(Vec::into_boxed_slice)
}

fn measure_pixels(
    pair: &PairPixels<'_>,
    dimensions: crate::PixelDimensions,
    analysis_pixel_count: u64,
    mask: Option<&BinaryMask>,
    parameters: MeasurementParameters,
) -> Result<MeasurementVector> {
    let classifier = PixelClassifier::new(parameters)?;
    measure_pixels_with_classifier(
        pair,
        dimensions,
        analysis_pixel_count,
        mask,
        &classifier,
        None,
    )
}

fn measure_pixels_with_classifier(
    pair: &PairPixels<'_>,
    dimensions: crate::PixelDimensions,
    analysis_pixel_count: u64,
    mask: Option<&BinaryMask>,
    classifier: &PixelClassifier,
    mut change_bits: Option<&mut [u8]>,
) -> Result<MeasurementVector> {
    let mut changed = 0_u128;
    let mut absolute_sum = 0_u128;
    let mut luminance_sum = 0_u128;
    let mut weighted_square_sum = 0_u128;
    let mut min_x = u32::MAX;
    let mut min_y = u32::MAX;
    let mut max_x = 0_u32;
    let mut max_y = 0_u32;

    let width = usize::try_from(dimensions.width()).map_err(|_| measurement_overflow())?;
    let row_stride = width.checked_mul(3).ok_or_else(measurement_overflow)?;
    for y in 0..dimensions.height() {
        let row = usize::try_from(y)
            .ok()
            .and_then(|row| row.checked_mul(row_stride))
            .ok_or_else(measurement_overflow)?;
        let pixel_row = row / 3;
        let end = row
            .checked_add(row_stride)
            .ok_or_else(measurement_overflow)?;
        let earlier_row = &pair.earlier[row..end];
        let later_row = &pair.later[row..end];
        let mut mask_cursor = mask.map(|mask| {
            let (bits, offset) = mask
                .row_bits(y)
                .expect("mask dimensions match normalized frames");
            (bits, 0_usize, 0x80_u8 >> offset)
        });
        for (x, (before, after)) in earlier_row
            .chunks_exact(3)
            .zip(later_row.chunks_exact(3))
            .enumerate()
        {
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
            let before_pixel: &[u16; 3] = before
                .try_into()
                .expect("chunks_exact yields three-channel pixels");
            let after_pixel: &[u16; 3] = after
                .try_into()
                .expect("chunks_exact yields three-channel pixels");
            let change = classifier.classify(before_pixel, after_pixel);
            if !change.changed {
                continue;
            }
            let x = u32::try_from(x).map_err(|_| measurement_overflow())?;
            let pixel_index = pixel_row
                .checked_add(usize::try_from(x).map_err(|_| measurement_overflow())?)
                .ok_or_else(measurement_overflow)?;
            if let Some(bits) = change_bits.as_deref_mut() {
                set_bit(bits, pixel_index);
            }
            let dr = u128::from(before[0].abs_diff(after[0]));
            let dg = u128::from(before[1].abs_diff(after[1]));
            let db = u128::from(before[2].abs_diff(after[2]));

            changed = changed.checked_add(1).ok_or_else(measurement_overflow)?;
            let channel_sum = dr
                .checked_add(dg)
                .and_then(|value| value.checked_add(db))
                .ok_or_else(measurement_overflow)?;
            absolute_sum = absolute_sum
                .checked_add(channel_sum)
                .ok_or_else(measurement_overflow)?;
            weighted_square_sum = weighted_square_sum
                .checked_add(u128::from(change.weighted_square))
                .ok_or_else(measurement_overflow)?;
            let before_luma = linear_luminance(before)?;
            let after_luma = linear_luminance(after)?;
            luminance_sum = luminance_sum
                .checked_add(before_luma.abs_diff(after_luma))
                .ok_or_else(measurement_overflow)?;
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }

    let compared = u128::from(analysis_pixel_count);
    let changed_u64 = u64::try_from(changed).map_err(|_| measurement_overflow())?;
    let compared_u64 = u64::try_from(compared).map_err(|_| measurement_overflow())?;
    let changed_region_bounds = if changed == 0 {
        None
    } else {
        Some(PixelRect::new(
            min_x,
            min_y,
            max_x
                .checked_sub(min_x)
                .and_then(|value| value.checked_add(1))
                .ok_or_else(measurement_overflow)?,
            max_y
                .checked_sub(min_y)
                .and_then(|value| value.checked_add(1))
                .ok_or_else(measurement_overflow)?,
        )?)
    };
    let mean_luminance_difference =
        u16::try_from(round_ratio(luminance_sum, compared)?).map_err(|_| measurement_overflow())?;
    let color_divisor = compared.checked_mul(3).ok_or_else(measurement_overflow)?;
    let mean_color_difference = u16::try_from(round_ratio(absolute_sum, color_divisor)?)
        .map_err(|_| measurement_overflow())?;
    let perceptual_divisor = u128::from(WEIGHT_SUM)
        .checked_mul(compared)
        .ok_or_else(measurement_overflow)?;
    let mean_weighted_square = weighted_square_sum / perceptual_divisor;
    let perceptual_frame_distance =
        u16::try_from(integer_sqrt(mean_weighted_square)).map_err(|_| measurement_overflow())?;

    Ok(MeasurementVector {
        absolute_pixel_difference: u64::try_from(absolute_sum)
            .map_err(|_| measurement_overflow())?,
        changed_pixel_proportion: ChangedPixelProportion::new(changed_u64, compared_u64)?,
        changed_region_bounds,
        mean_luminance_difference,
        mean_color_difference,
        perceptual_frame_distance,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PixelChange {
    pub(crate) changed: bool,
    pub(crate) weighted_square: u64,
}

/// Canonical thresholded weighted-linear-RGB pixel classifier.
///
/// The per-pixel arithmetic is deliberately unchecked after construction. Each channel delta is
/// at most `u16::MAX`, so the maximum weighted sum is
/// `WEIGHT_SUM * u16::MAX^2` (approximately `2.815e14`), well below
/// `2^63`. Aggregate sums remain checked `u128` at their callers because many changed pixels can
/// exceed `u64`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PixelClassifier {
    threshold: u64,
}

impl PixelClassifier {
    pub(crate) fn new(parameters: MeasurementParameters) -> Result<Self> {
        let threshold = u64::from(parameters.noise_floor)
            .checked_pow(2)
            .and_then(|value| value.checked_mul(WEIGHT_SUM))
            .ok_or_else(measurement_overflow)?;
        Ok(Self { threshold })
    }

    #[inline]
    pub(crate) fn classify(&self, before: &[u16; 3], after: &[u16; 3]) -> PixelChange {
        let weighted_square = weighted_square_u64(before, after);
        debug_assert!(weighted_square < (1_u64 << 63));
        PixelChange {
            changed: weighted_square > self.threshold,
            weighted_square,
        }
    }

    #[inline]
    pub(crate) fn weighted_square(&self, before: &[u16; 3], after: &[u16; 3]) -> u64 {
        let weighted_square = weighted_square_u64(before, after);
        debug_assert!(weighted_square < (1_u64 << 63));
        weighted_square
    }
}

/// Compatibility wrapper for the existing unit-test seam; callers use the hoisted classifier.
#[cfg(test)]
pub(crate) fn classify_pixel_change(
    before: &[u16; 3],
    after: &[u16; 3],
    parameters: MeasurementParameters,
) -> Result<PixelChange> {
    PixelClassifier::new(parameters).map(|classifier| classifier.classify(before, after))
}

/// Count declared gaps intersecting an inclusive comparison interval.
pub(crate) fn intersecting_gap_count(
    gap_ranges: &[crate::TimeRange],
    earlier: crate::Timestamp,
    later: crate::Timestamp,
) -> usize {
    gap_ranges
        .iter()
        .filter(|gap| gap.start() <= later && gap.end() >= earlier)
        .count()
}

#[inline]
fn weighted_square_u64(before: &[u16; 3], after: &[u16; 3]) -> u64 {
    let red = u64::from(before[0].abs_diff(after[0]));
    let green = u64::from(before[1].abs_diff(after[1]));
    let blue = u64::from(before[2].abs_diff(after[2]));
    RED_WEIGHT * red * red + GREEN_WEIGHT * green * green + BLUE_WEIGHT * blue * blue
}

pub(crate) fn linear_luminance(pixel: &[u16]) -> Result<u128> {
    let red = u128::from(RED_WEIGHT)
        .checked_mul(u128::from(pixel[0]))
        .ok_or_else(measurement_overflow)?;
    let green = u128::from(GREEN_WEIGHT)
        .checked_mul(u128::from(pixel[1]))
        .ok_or_else(measurement_overflow)?;
    let blue = u128::from(BLUE_WEIGHT)
        .checked_mul(u128::from(pixel[2]))
        .ok_or_else(measurement_overflow)?;
    let weighted = red
        .checked_add(green)
        .and_then(|value| value.checked_add(blue))
        .and_then(|value| value.checked_add(u128::from(WEIGHT_SUM / 2)))
        .ok_or_else(measurement_overflow)?;
    Ok(weighted / u128::from(WEIGHT_SUM))
}

#[inline]
fn set_bit(bits: &mut [u8], index: usize) {
    bits[index / 8] |= 0x80 >> (index % 8);
}

fn round_ratio(numerator: u128, denominator: u128) -> Result<u128> {
    numerator
        .checked_add(denominator / 2)
        .map(|value| value / denominator)
        .ok_or_else(measurement_overflow)
}

fn integer_sqrt(value: u128) -> u128 {
    let mut remainder = value;
    let mut result = 0_u128;
    let mut bit = 1_u128 << 126;
    while bit > remainder {
        bit >>= 2;
    }
    while bit != 0 {
        if remainder >= result + bit {
            remainder -= result + bit;
            result = (result >> 1) + bit;
        } else {
            result >>= 1;
        }
        bit >>= 2;
    }
    result
}

fn measurement_overflow() -> VisionError {
    VisionError::new(
        ErrorCode::ResourceLimitExceeded,
        "visual measurement exceeds the supported integer representation",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        DeclaredGap, Frame, FrameSequence, IntegerScale, Marker, NormalizationParameters,
        PixelDimensions, PixelFormat, ProcessingLimits, Rgb8, Timestamp, normalize_sequence,
    };

    fn normalized(frames: Vec<(u8, u64, [u8; 4])>) -> NormalizedSequence<u8> {
        let dimensions = PixelDimensions::new(1, 1).unwrap();
        let frames = frames
            .into_iter()
            .map(|(id, timestamp, pixels)| {
                Frame::new(
                    id,
                    Timestamp::from_nanos(timestamp),
                    dimensions,
                    PixelFormat::Rgba8SrgbStraight,
                    pixels.to_vec().into_boxed_slice(),
                )
                .unwrap()
            })
            .collect();
        let sequence = FrameSequence::new(
            frames,
            Vec::<Marker<u8>>::new(),
            Vec::<DeclaredGap<u8>>::new(),
            None,
            None,
        )
        .unwrap();
        normalize_sequence(
            &sequence,
            NormalizationParameters::new(
                Rgb8::new(0, 0, 0),
                None,
                IntegerScale::IDENTITY,
                ProcessingLimits::default(),
            ),
        )
        .unwrap()
    }

    #[test]
    fn integer_square_root_handles_boundaries() {
        assert_eq!(integer_sqrt(0), 0);
        assert_eq!(integer_sqrt(1), 1);
        assert_eq!(integer_sqrt(15), 3);
        assert_eq!(integer_sqrt(16), 4);
        assert_eq!(integer_sqrt(17), 4);
        assert_eq!(integer_sqrt(u128::MAX), u64::MAX as u128);
    }

    #[test]
    fn hoisted_classifier_matches_u128_reference_at_full_u16_bounds() {
        let parameters = MeasurementParameters::new(512);
        let classifier = PixelClassifier::new(parameters).unwrap();
        let threshold = u128::from(parameters.noise_floor()).pow(2) * u128::from(WEIGHT_SUM);
        for deltas in [
            [0_u16, 0, 0],
            [1, 2, 3],
            [u16::MAX, 0, 0],
            [0, u16::MAX, 0],
            [0, 0, u16::MAX],
            [u16::MAX, u16::MAX, u16::MAX],
        ] {
            let before = [0_u16; 3];
            let after = deltas;
            let expected = u128::from(RED_WEIGHT) * u128::from(deltas[0]).pow(2)
                + u128::from(GREEN_WEIGHT) * u128::from(deltas[1]).pow(2)
                + u128::from(BLUE_WEIGHT) * u128::from(deltas[2]).pow(2);
            let actual = classifier.classify(&before, &after);
            assert_eq!(actual.weighted_square, u64::try_from(expected).unwrap());
            assert_eq!(actual.changed, expected > threshold);
        }
    }

    #[test]
    fn shared_analysis_matches_adjacent_comparisons_and_masks() {
        let sequence = normalized(vec![
            (1, 1, [0, 0, 0, 255]),
            (2, 2, [20, 0, 0, 255]),
            (3, 3, [20, 20, 20, 255]),
        ]);
        let parameters = MeasurementParameters::new(0);
        let expected = measure_adjacent(&sequence, parameters).unwrap();
        let shared = analyze_adjacent_pairs(&sequence, parameters, true).unwrap();
        assert_eq!(shared.comparisons(), expected.as_ref());
        let masks = shared.change_masks().unwrap();
        for (pair_index, comparison) in expected.iter().enumerate() {
            let changed = matches!(
                comparison.outcome(),
                ComparisonOutcome::Measured(vector)
                    if vector.changed_pixel_proportion().changed() > 0
            );
            assert_eq!(masks.for_pair(pair_index).unwrap()[0] & 0x80 != 0, changed);
        }
    }

    #[test]
    fn identity_and_threshold_boundary_are_exact() {
        let sequence = normalized(vec![(1, 1, [0, 0, 0, 255]), (2, 1, [0, 0, 0, 255])]);
        let comparison = measure_pair(&sequence, 0, 1, MeasurementParameters::new(0)).unwrap();
        let ComparisonOutcome::Measured(vector) = comparison.outcome() else {
            panic!("unexpected gap")
        };
        assert_eq!(comparison.elapsed_nanos(), 0);
        assert_eq!(vector.absolute_pixel_difference(), 0);
        assert_eq!(vector.changed_pixel_proportion().changed(), 0);
        assert_eq!(vector.changed_pixel_proportion().compared(), 1);
        assert_eq!(vector.changed_region_bounds(), None);

        let at_floor = normalized(vec![(1, 1, [0, 0, 0, 255]), (2, 2, [1, 1, 1, 255])]);
        let delta = at_floor.frames()[1].linear_rgb16()[0];
        let before: &[u16; 3] = at_floor.frames()[0].linear_rgb16().try_into().unwrap();
        let after: &[u16; 3] = at_floor.frames()[1].linear_rgb16().try_into().unwrap();
        let at_threshold = MeasurementParameters::new(delta);
        let one_over_threshold = MeasurementParameters::new(delta - 1);
        assert!(
            !classify_pixel_change(before, after, at_threshold)
                .unwrap()
                .changed
        );
        assert!(
            classify_pixel_change(before, after, one_over_threshold)
                .unwrap()
                .changed
        );
        let ComparisonOutcome::Measured(vector) =
            measure_pair(&at_floor, 0, 1, at_threshold).unwrap().outcome
        else {
            panic!("unexpected gap")
        };
        assert_eq!(vector.changed_pixel_proportion().changed(), 0);
        let ComparisonOutcome::Measured(vector) = measure_pair(&at_floor, 0, 1, one_over_threshold)
            .unwrap()
            .outcome
        else {
            panic!("unexpected gap")
        };
        assert_eq!(vector.changed_pixel_proportion().changed(), 1);

        let gap =
            crate::TimeRange::new(Timestamp::from_nanos(2), Timestamp::from_nanos(3)).unwrap();
        assert_eq!(
            intersecting_gap_count(&[gap], Timestamp::from_nanos(1), Timestamp::from_nanos(2)),
            1
        );
        assert_eq!(
            intersecting_gap_count(&[gap], Timestamp::from_nanos(4), Timestamp::from_nanos(5)),
            0
        );
    }
}
