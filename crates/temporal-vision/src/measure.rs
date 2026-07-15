use std::num::NonZeroUsize;

use serde::{Deserialize, Deserializer, Serialize};

use crate::{
    ErrorCode, NormalizationKind, NormalizationStep, NormalizedSequence, ParameterValue, PixelRect,
    Result, VisionError, normalize::make_parameters,
};

const RED_WEIGHT: u128 = 13_933;
const GREEN_WEIGHT: u128 = 46_871;
const BLUE_WEIGHT: u128 = 4_732;
const WEIGHT_SUM: u128 = 65_536;

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
                        ParameterValue::Unsigned(RED_WEIGHT as u64),
                        ParameterValue::Unsigned(GREEN_WEIGHT as u64),
                        ParameterValue::Unsigned(BLUE_WEIGHT as u64),
                    ]),
                ),
                ("weight_sum", ParameterValue::Unsigned(WEIGHT_SUM as u64)),
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
    pub(crate) const fn from_parts(
        earlier_frame_index: usize,
        later_frame_index: usize,
        elapsed_nanos: u64,
        outcome: ComparisonOutcome,
    ) -> Self {
        Self {
            earlier_frame_index,
            later_frame_index,
            elapsed_nanos,
            outcome,
        }
    }

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
}

/// Measure one ordered pair of normalized frames.
pub fn measure_pair<F>(
    sequence: &NormalizedSequence<F>,
    earlier_frame_index: usize,
    later_frame_index: usize,
    parameters: MeasurementParameters,
) -> Result<FrameComparison> {
    if earlier_frame_index >= later_frame_index || later_frame_index >= sequence.frames().len() {
        return Err(VisionError::new(
            ErrorCode::InvalidParameter,
            "comparison indices must name two existing frames in increasing order",
        ));
    }
    let earlier = &sequence.frames()[earlier_frame_index];
    let later = &sequence.frames()[later_frame_index];
    let elapsed_nanos = later
        .timestamp()
        .as_nanos()
        .checked_sub(earlier.timestamp().as_nanos())
        .ok_or_else(|| {
            VisionError::new(
                ErrorCode::OutOfOrder,
                "comparison timestamps are not in nondecreasing order",
            )
        })?;
    let gap_count = intersecting_gap_count(
        sequence.gap_ranges(),
        earlier.timestamp(),
        later.timestamp(),
    );
    let outcome = if let Some(declared_gap_count) = NonZeroUsize::new(gap_count) {
        ComparisonOutcome::GapBoundary { declared_gap_count }
    } else {
        ComparisonOutcome::Measured(measure_pixels(
            sequence,
            earlier_frame_index,
            later_frame_index,
            parameters,
        )?)
    };
    Ok(FrameComparison::from_parts(
        earlier_frame_index,
        later_frame_index,
        elapsed_nanos,
        outcome,
    ))
}

/// Measure every adjacent captured-frame pair in declaration order.
pub fn measure_adjacent<F>(
    sequence: &NormalizedSequence<F>,
    parameters: MeasurementParameters,
) -> Result<Box<[FrameComparison]>> {
    (1..sequence.frames().len())
        .map(|later| measure_pair(sequence, later - 1, later, parameters))
        .collect::<Result<Vec<_>>>()
        .map(Vec::into_boxed_slice)
}

pub(crate) struct MeasurementAccumulator {
    compared: u128,
    changed: u128,
    absolute_sum: u128,
    luminance_sum: u128,
    weighted_square_sum: u128,
    min_x: u32,
    min_y: u32,
    max_x: u32,
    max_y: u32,
}

impl MeasurementAccumulator {
    pub(crate) fn new(compared: u128) -> Self {
        Self {
            compared,
            changed: 0,
            absolute_sum: 0,
            luminance_sum: 0,
            weighted_square_sum: 0,
            min_x: u32::MAX,
            min_y: u32::MAX,
            max_x: 0,
            max_y: 0,
        }
    }

    pub(crate) fn record(
        &mut self,
        x: u32,
        y: u32,
        before: &[u16; 3],
        after: &[u16; 3],
        change: PixelChange,
    ) -> Result<()> {
        if !change.changed {
            return Ok(());
        }
        let dr = u128::from(before[0].abs_diff(after[0]));
        let dg = u128::from(before[1].abs_diff(after[1]));
        let db = u128::from(before[2].abs_diff(after[2]));
        self.changed = self
            .changed
            .checked_add(1)
            .ok_or_else(measurement_overflow)?;
        let channel_sum = dr
            .checked_add(dg)
            .and_then(|value| value.checked_add(db))
            .ok_or_else(measurement_overflow)?;
        self.absolute_sum = self
            .absolute_sum
            .checked_add(channel_sum)
            .ok_or_else(measurement_overflow)?;
        self.weighted_square_sum = self
            .weighted_square_sum
            .checked_add(change.weighted_square)
            .ok_or_else(measurement_overflow)?;
        let before_luma = linear_luminance(before)?;
        let after_luma = linear_luminance(after)?;
        self.luminance_sum = self
            .luminance_sum
            .checked_add(before_luma.abs_diff(after_luma))
            .ok_or_else(measurement_overflow)?;
        self.min_x = self.min_x.min(x);
        self.min_y = self.min_y.min(y);
        self.max_x = self.max_x.max(x);
        self.max_y = self.max_y.max(y);
        Ok(())
    }

    pub(crate) fn finish(self) -> Result<MeasurementVector> {
        let changed_u64 = u64::try_from(self.changed).map_err(|_| measurement_overflow())?;
        let compared_u64 = u64::try_from(self.compared).map_err(|_| measurement_overflow())?;
        let changed_region_bounds = if self.changed == 0 {
            None
        } else {
            Some(PixelRect::new(
                self.min_x,
                self.min_y,
                self.max_x
                    .checked_sub(self.min_x)
                    .and_then(|value| value.checked_add(1))
                    .ok_or_else(measurement_overflow)?,
                self.max_y
                    .checked_sub(self.min_y)
                    .and_then(|value| value.checked_add(1))
                    .ok_or_else(measurement_overflow)?,
            )?)
        };
        let mean_luminance_difference =
            u16::try_from(round_ratio(self.luminance_sum, self.compared)?)
                .map_err(|_| measurement_overflow())?;
        let color_divisor = self
            .compared
            .checked_mul(3)
            .ok_or_else(measurement_overflow)?;
        let mean_color_difference = u16::try_from(round_ratio(self.absolute_sum, color_divisor)?)
            .map_err(|_| measurement_overflow())?;
        let perceptual_divisor = WEIGHT_SUM
            .checked_mul(self.compared)
            .ok_or_else(measurement_overflow)?;
        let mean_weighted_square = self.weighted_square_sum / perceptual_divisor;
        let perceptual_frame_distance = u16::try_from(integer_sqrt(mean_weighted_square))
            .map_err(|_| measurement_overflow())?;

        Ok(MeasurementVector {
            absolute_pixel_difference: u64::try_from(self.absolute_sum)
                .map_err(|_| measurement_overflow())?,
            changed_pixel_proportion: ChangedPixelProportion::new(changed_u64, compared_u64)?,
            changed_region_bounds,
            mean_luminance_difference,
            mean_color_difference,
            perceptual_frame_distance,
        })
    }
}

fn measure_pixels<F>(
    sequence: &NormalizedSequence<F>,
    earlier_index: usize,
    later_index: usize,
    parameters: MeasurementParameters,
) -> Result<MeasurementVector> {
    let earlier = sequence.frames()[earlier_index].linear_rgb16();
    let later = sequence.frames()[later_index].linear_rgb16();
    let dimensions = sequence.dimensions();
    let mask = sequence.analysis_mask();
    let width = usize::try_from(dimensions.width()).map_err(|_| measurement_overflow())?;
    let mut aggregate = MeasurementAccumulator::new(u128::from(sequence.analysis_pixel_count()));

    for (index, (before, after)) in earlier
        .chunks_exact(3)
        .zip(later.chunks_exact(3))
        .enumerate()
    {
        let x = u32::try_from(index % width).map_err(|_| measurement_overflow())?;
        let y = u32::try_from(index / width).map_err(|_| measurement_overflow())?;
        if mask.is_some_and(|mask| mask.includes(x, y) != Some(true)) {
            continue;
        }

        let before_pixel: &[u16; 3] = before
            .try_into()
            .expect("chunks_exact yields three-channel pixels");
        let after_pixel: &[u16; 3] = after
            .try_into()
            .expect("chunks_exact yields three-channel pixels");
        let change = classify_pixel_change(before_pixel, after_pixel, parameters)?;
        aggregate.record(x, y, before_pixel, after_pixel, change)?;
    }
    aggregate.finish()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PixelChange {
    pub(crate) changed: bool,
    pub(crate) weighted_square: u128,
}

/// Canonical thresholded weighted-linear-RGB pixel classifier.
pub(crate) fn classify_pixel_change(
    before: &[u16; 3],
    after: &[u16; 3],
    parameters: MeasurementParameters,
) -> Result<PixelChange> {
    let threshold = u128::from(parameters.noise_floor)
        .checked_pow(2)
        .and_then(|value| value.checked_mul(WEIGHT_SUM))
        .ok_or_else(measurement_overflow)?;
    let dr = u128::from(before[0].abs_diff(after[0]));
    let dg = u128::from(before[1].abs_diff(after[1]));
    let db = u128::from(before[2].abs_diff(after[2]));
    let red_square = weighted_channel_square(RED_WEIGHT, dr)?;
    let green_square = weighted_channel_square(GREEN_WEIGHT, dg)?;
    let blue_square = weighted_channel_square(BLUE_WEIGHT, db)?;
    let weighted_square = red_square
        .checked_add(green_square)
        .and_then(|value| value.checked_add(blue_square))
        .ok_or_else(measurement_overflow)?;
    Ok(PixelChange {
        changed: weighted_square > threshold,
        weighted_square,
    })
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

fn weighted_channel_square(weight: u128, delta: u128) -> Result<u128> {
    delta
        .checked_mul(delta)
        .and_then(|square| weight.checked_mul(square))
        .ok_or_else(measurement_overflow)
}

pub(crate) fn linear_luminance(pixel: &[u16]) -> Result<u128> {
    let red = RED_WEIGHT
        .checked_mul(u128::from(pixel[0]))
        .ok_or_else(measurement_overflow)?;
    let green = GREEN_WEIGHT
        .checked_mul(u128::from(pixel[1]))
        .ok_or_else(measurement_overflow)?;
    let blue = BLUE_WEIGHT
        .checked_mul(u128::from(pixel[2]))
        .ok_or_else(measurement_overflow)?;
    let weighted = red
        .checked_add(green)
        .and_then(|value| value.checked_add(blue))
        .and_then(|value| value.checked_add(WEIGHT_SUM / 2))
        .ok_or_else(measurement_overflow)?;
    Ok(weighted / WEIGHT_SUM)
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
